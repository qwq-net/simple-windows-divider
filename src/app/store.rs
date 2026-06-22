//! 配置の実行時状態とその永続化（機能 B/C 共有のデータ層）。
//!
//! 「実行中のスロット所属（[`Occupancy`]・非永続）」と「学習済み配置（[`LearnedLayouts`]・永続）」をまとめて
//! 所有し、起点の占有範囲決定・学習・所属の予約/解除・デバウンス保存を提供する。機能 B（矢印）と機能 C（自動復元）
//! の双方がこの 1 つのストアを介して状態を読み書きする。純粋な判断は [`crate::layout`] / [`crate::occupancy`] /
//! [`crate::layouts`] に委ね、ここはそれらと Win32 取得・タイマを束ねる配線に徹する。

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::layout::geometry::Rect;
use crate::layout::grid::GridSpan;
use crate::layout::placement;
use crate::layouts::{self, LearnedLayouts, Slot, WindowKey};
use crate::occupancy::Occupancy;
use crate::win::{convert, monitor, window_ops};

/// 学習データ保存のデバウンス用タイマ ID。復元タイマ（`restore` の `RESTORE_TIMER_BASE` 以上）と衝突しない値にする。
pub const SAVE_TIMER_ID: usize = 1;
/// 連続操作中はインメモリ更新のみとし、最後の変更から保存までこの時間だけ待つ。
const SAVE_DEBOUNCE_MS: u32 = 500;
/// 1 つの識別キー `(exe, class, app_id)` に貯める学習スロットの上限。超過分は最古から捨てる。
const LEARNED_SLOTS_PER_KEY: usize = 8;
/// 保存済み占有範囲を「直前に自分が配置したもの」とみなして再利用する際の、現在矩形との許容差。
const SPAN_REUSE_TOLERANCE_PX: i32 = 6;

/// 実行中のスロット所属・学習データ・保存状態を束ねたストア。
pub struct LayoutStore {
    /// 実行中のスロット所属（hwnd→スロット）。起点決定・空き判定・解除に使う（非永続）。
    occupancy: Occupancy,
    /// `(exe, class, app_id)` ごとに学習した占有スロット（永続）。
    learned: LearnedLayouts,
    path: PathBuf,
    /// 学習データに未保存の変更があるか（デバウンス保存用）。
    dirty: bool,
}

impl LayoutStore {
    /// 既存の学習データ `learned` を読み込んだ状態で作る。所属は空、未保存フラグは偽から始める。
    pub fn new(path: PathBuf, learned: LearnedLayouts) -> LayoutStore {
        LayoutStore { occupancy: Occupancy::default(), learned, path, dirty: false }
    }

    /// 学習データが空か。空なら自動復元の処理を丸ごと省ける。
    pub fn is_empty(&self) -> bool {
        self.learned.is_empty()
    }

    /// `id` のウィンドウの起点となる占有範囲を、所属スロット（あれば）と現在矩形から決める。
    ///
    /// 直前に自分が配置した所属スロットの矩形が現在矩形とほぼ一致すればそれを再利用し、一致しなければ現在位置から
    /// 推定する（[`placement::span_for`]）。`work` は対象モニタの作業領域、`current` はウィンドウの現在矩形。
    pub fn span_for(&self, id: u64, work: Rect, current: Rect, cols: u32, rows: u32) -> GridSpan {
        let prev = self.occupancy.entry_of(id).map(|(_, s)| s.span);
        placement::span_for(prev, work, current, cols, rows, SPAN_REUSE_TOLERANCE_PX)
    }

    /// ユーザー操作で確定した占有 `slot` を `key` の学習に記録し、`id` の所属を更新する。保存はデバウンス予約する。
    ///
    /// 同じ窓を動かし直したときは記録中の該当スロットをその場で置き換える（[`LearnedLayouts::learn`] が判定）。
    /// `hwnd_msg` は保存タイマの宛先となるメッセージ専用ウィンドウ。
    pub fn learn(&mut self, hwnd_msg: HWND, id: u64, key: WindowKey, slot: Slot) {
        let old = self.occupancy.entry_of(id).map(|(_, s)| s);
        self.learned.learn(&key, slot.clone(), old, LEARNED_SLOTS_PER_KEY);
        self.occupancy.on_placed(id, key, slot);
        self.schedule_save(hwnd_msg);
    }

    /// `key` に記録された全スロット。
    pub fn slots(&self, key: &WindowKey) -> Vec<Slot> {
        self.learned.slots(key)
    }

    /// `recorded` のうち、いまどの生きた所属窓にも占有されていない先頭スロット。空きが無ければ `None`。
    pub fn pick_slot(&self, key: &WindowKey, recorded: &[Slot]) -> Option<Slot> {
        self.occupancy.pick_slot(key, recorded)
    }

    /// 復元のため `id` に `slot` を所属として予約する（学習も保存もしない）。続けて生成される同識別子の窓が
    /// 同じスロットを選ばないようにするために使う。
    pub fn reserve(&mut self, id: u64, key: WindowKey, slot: Slot) {
        self.occupancy.on_placed(id, key, slot);
    }

    /// `hwnd_target` がドラッグ/リサイズで所属スロットから外れていれば、所属と記録の双方から外す。
    ///
    /// ウィンドウは動かさない（内部状態と記録の更新のみ）。記録ディスプレイが現存しないときは判定しない
    /// （復元側 [`crate::app`] の apply と対称。切断中に別モニタの作業領域で誤比較して学習を消すのを防ぐ）。
    /// 記録を消した場合は保存をデバウンス予約する。`hwnd_msg` は保存タイマの宛先。
    pub fn release_if_moved(&mut self, hwnd_msg: HWND, hwnd_target: HWND) {
        let id = convert::hwnd_to_u64(hwnd_target);
        let Some((key, slot)) = self.occupancy.entry_of(id) else { return };
        let Some(mon) = monitor::monitor_by_name(&slot.display) else { return };
        let target = slot.target_rect(mon.work_area);
        let Some(cur) = window_ops::window_visible_rect(hwnd_target) else { return };
        if !cur.approx_eq(target, SPAN_REUSE_TOLERANCE_PX) {
            self.occupancy.on_released(id);
            self.learned.forget(&key, &slot);
            self.schedule_save(hwnd_msg);
        }
    }

    /// 既に存在しないウィンドウの所属を掃除する（長期常駐での単調増加を防ぐ）。
    pub fn prune(&mut self) {
        self.occupancy
            .prune(|id| window_ops::is_window(convert::u64_to_hwnd(id)));
    }

    /// 実行中のスロット所属（非永続）をすべて捨てる。学習データ（永続）は消さない。分割数が変わる操作で使う。
    pub fn reset_occupancy(&mut self) {
        self.occupancy = Occupancy::default();
    }

    /// 学習データのディスク保存をデバウンス予約する。保存タイマを毎回張り直し、最後の変更から
    /// `SAVE_DEBOUNCE_MS` 後に 1 回だけ書き出す。
    fn schedule_save(&mut self, hwnd_msg: HWND) {
        self.dirty = true;
        unsafe {
            SetTimer(Some(hwnd_msg), SAVE_TIMER_ID, SAVE_DEBOUNCE_MS, None);
        }
    }

    /// 保存タイマ発火時の処理。タイマを止めてから未保存分を書き出す。
    pub fn on_save_timer(&mut self, hwnd_msg: HWND) {
        unsafe {
            let _ = KillTimer(Some(hwnd_msg), SAVE_TIMER_ID);
        }
        self.flush_save();
    }

    /// 未保存の学習データがあれば `layouts.toml` へ書き出す。失敗はログに留める。終了時の取りこぼし防止にも使う。
    pub fn flush_save(&mut self) {
        if !self.dirty {
            return;
        }
        if let Err(e) = layouts::save(&self.path, &self.learned) {
            tracing::warn!("failed to save layouts: {e}");
        }
        self.dirty = false;
    }
}
