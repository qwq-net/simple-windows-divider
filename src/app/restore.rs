//! 機能 C: 生成直後のウィンドウへ学習配置を自動復元する遅延リトライ群の管理。
//!
//! 新規ウィンドウの生成イベントを受けて復元対象かを判定し（[`RestoreManager::on_created`]）、対象なら遅延後に
//! 学習スロットを適用する。生成直後はサイズが未確定なことがあるため、収束するまで数回リトライする。所属の予約や
//! 学習データの参照は [`LayoutStore`] を介し、ウィンドウを動かす能動操作はこの [`apply_learned_slot`] に集約する。

use std::collections::HashMap;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::config::Config;
use crate::layouts::Slot;
use crate::win::guard::{self, Interventability};
use crate::win::{convert, monitor, window_info, window_ops};

use super::store::LayoutStore;

/// 復元リトライ用タイマ ID の起点。保存タイマ（[`super::store::SAVE_TIMER_ID`]）と衝突しない値にする。
const RESTORE_TIMER_BASE: usize = 1000;
/// 生成直後のウィンドウへ復元を適用するまでの遅延と、収束までのリトライ回数（内部固定）。
const RESTORE_DELAY_MS: u32 = 150;
const RESTORE_MAX_ATTEMPTS: u32 = 3;
/// 適用後の矩形が目標とこの差以内なら収束とみなす。
const CONVERGE_TOLERANCE_PX: i32 = 4;

/// 遅延リトライ 1 件分の状態。学習した占有範囲を生成直後のウィンドウへ適用する。
struct RestoreJob {
    hwnd: HWND,
    slot: Slot,
    attempts_left: u32,
}

/// 復元の遅延リトライ群を管理する。各ジョブはタイマ ID で識別する。
pub struct RestoreManager {
    jobs: HashMap<usize, RestoreJob>,
    next_timer_id: usize,
}

impl Default for RestoreManager {
    fn default() -> RestoreManager {
        RestoreManager { jobs: HashMap::new(), next_timer_id: RESTORE_TIMER_BASE }
    }
}

impl RestoreManager {
    /// 生成イベントを受け、復元対象なら所属を予約して遅延適用のタイマを張る。
    ///
    /// まず安価な関門（[`guard::cheap_interventability`]・`OpenProcess` を伴わない）で一過性・子ウィンドウを弾き、
    /// 通過したものだけ exe を取得して除外判定と学習照合を行う。記録に無い識別子や空きスロットが無いものは動かさない。
    /// `hwnd_msg` はタイマの宛先となるメッセージ専用ウィンドウ、`hwnd` は生成された対象ウィンドウ。
    pub fn on_created(&mut self, hwnd_msg: HWND, hwnd: HWND, store: &mut LayoutStore, cfg: &Config) {
        // 生成イベントは大量に発火するため、その大半を GetWindowLongPtr だけで落として CPU を抑える。
        if guard::cheap_interventability(hwnd, &cfg.exclusions) != Interventability::Ok {
            return;
        }
        // ここで初めて OpenProcess（exe 取得）。除外判定と学習照合で同じ key を使い回す（二重取得しない）。
        let Some(key) = window_info::window_key(hwnd) else { return };
        if cfg.exclusions.excludes(&key.exe) {
            return;
        }
        let recorded = store.slots(&key);
        if recorded.is_empty() {
            return; // 記録に無い識別子は動かさない
        }
        let Some(slot) = store.pick_slot(&key, &recorded) else {
            return; // 空きスロットなし → 自由
        };
        // 復元で占有することを予約し、続けて生成される同識別子の窓が同じスロットを選ばないようにする。
        store.reserve(convert::hwnd_to_u64(hwnd), key, slot.clone());
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        unsafe {
            SetTimer(Some(hwnd_msg), id, RESTORE_DELAY_MS, None);
        }
        self.jobs.insert(id, RestoreJob { hwnd, slot, attempts_left: RESTORE_MAX_ATTEMPTS });
    }

    /// 復元タイマ発火時の処理。学習スロットを 1 回適用し、収束またはリトライ尽きでタイマとジョブを片付ける。
    /// `timer_id` が自分のジョブでなければ何もしない。
    pub fn on_timer(&mut self, hwnd_msg: HWND, timer_id: usize) {
        let Some(job) = self.jobs.get_mut(&timer_id) else {
            return;
        };
        let hwnd = job.hwnd;
        let slot = job.slot.clone();
        job.attempts_left = job.attempts_left.saturating_sub(1);
        let attempts_left = job.attempts_left;

        let converged = apply_learned_slot(hwnd, &slot);

        if converged || attempts_left == 0 {
            unsafe {
                let _ = KillTimer(Some(hwnd_msg), timer_id);
            }
            self.jobs.remove(&timer_id);
        }
    }
}

/// 学習スロットを、そのスロットのディスプレイの作業領域へ適用する。
///
/// 記録時の分割数でグリッドを解釈し、現在その分割数が変わっていても [`Slot::target_rect`] が clamp で丸める。
/// 対象ディスプレイが現存しなければ何もしない（収束扱いで打ち切り）。適用後に目標とほぼ一致すれば `true`。
fn apply_learned_slot(hwnd: HWND, slot: &Slot) -> bool {
    let Some(mon) = monitor::monitor_by_name(&slot.display) else {
        return true; // ディスプレイが無い → 復元しない（打ち切り）
    };
    let target = slot.target_rect(mon.work_area);
    window_ops::restore_if_maximized(hwnd);
    if let Err(e) = window_ops::set_window_rect(hwnd, target) {
        tracing::warn!("apply_learned_slot: set_window_rect failed: {e}");
    }
    window_ops::window_visible_rect(hwnd)
        .map(|cur| cur.approx_eq(target, CONVERGE_TOLERANCE_PX))
        .unwrap_or(false)
}
