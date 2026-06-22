//! アプリ本体: メッセージループと全メッセージのディスパッチ（副作用の集約点）。
//!
//! 単一スレッド・単一メッセージループで、`WM_HOTKEY`（機能 B）・`WM_TIMER`（機能 C の遅延リトライ）・
//! `WM_APP_CONFIG_RELOAD`（設定再読込）を捌き、各ループ末で WinEvent コールバックが積んだイベント
//! （機能 C のトリガ）を取り出して処理する。アンチチート安全のため、能動的なウィンドウ操作はすべて
//! [`guard::should_intervene`] を通す。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, PostQuitMessage, SetTimer, TranslateMessage, MSG,
    WM_HOTKEY, WM_TIMER,
};

use crate::action::{bindings, HotkeyAction};
use crate::config::{self, Config};
use crate::hotkey::parse;
use crate::layout::geometry::Rect;
use crate::layout::grid::{self, Family, GridSpan};
use crate::layouts::{self, LearnedLayouts, Slot};
use crate::occupancy::Occupancy;
use crate::tray::{Tray, TrayCommand};
use crate::watcher::ConfigWatcher;
use crate::win::guard::Interventability;
use crate::win::message_window::WM_APP_CONFIG_RELOAD;
use crate::win::snap::SnapBackup;
use crate::win::winevent::{WinEvent, WinEventHooks};
use crate::win::{
    autostart, convert, dpi, guard, hotkey as winhotkey, message_window, monitor, snap, winevent,
    window_info, window_ops,
};

const RESTORE_TIMER_BASE: usize = 1000;
const CONVERGE_TOLERANCE_PX: i32 = 4;
/// 保存済み占有範囲を「直前に自分が配置したもの」とみなして再利用する際の、現在矩形との許容差。
const SPAN_REUSE_TOLERANCE_PX: i32 = 6;
/// 機能 C: 生成直後のウィンドウへ復元を適用するまでの遅延と、収束までのリトライ回数（内部固定）。
const RESTORE_DELAY_MS: u32 = 150;
const RESTORE_MAX_ATTEMPTS: u32 = 3;
/// 学習データ保存のデバウンス用タイマ ID と遅延。復元タイマ（`RESTORE_TIMER_BASE` 以上）と衝突しない値にする。
const SAVE_TIMER_ID: usize = 1;
const SAVE_DEBOUNCE_MS: u32 = 500;
/// 1 つの識別キー `(exe, class, app_id)` に貯める学習スロットの上限。超過分は最古から捨てる。
const LEARNED_SLOTS_PER_KEY: usize = 8;

// 矢印キーの仮想キーコード（反対方向の同時押し＝最大化を判定するため）。
const VK_LEFT: i32 = 0x25;
const VK_UP: i32 = 0x26;
const VK_RIGHT: i32 = 0x27;
const VK_DOWN: i32 = 0x28;

/// 機能 C の遅延リトライ 1 件分の状態。学習した占有範囲を生成直後のウィンドウへ適用する。
struct RestoreJob {
    hwnd: HWND,
    slot: Slot,
    attempts_left: u32,
}

/// アプリ全体の状態。すべて UI スレッドからのみ触れるためロック不要。
pub struct App {
    hwnd: HWND,
    config: Config,
    config_path: PathBuf,
    enabled: bool,
    /// 機能 C: 学習した配置を新規ウィンドウへ自動復元するか。
    auto_restore: bool,
    /// 実行中のスロット所属（hwnd→スロット）。起点決定・空き判定・解除に使う（非永続）。
    occupancy: Occupancy,
    /// `(exe, class, app_id)` ごとに学習した占有スロット（永続）。
    learned: LearnedLayouts,
    layouts_path: PathBuf,
    /// 登録済みホットキー id → アクション。
    actions: HashMap<i32, HotkeyAction>,
    registered_ids: Vec<i32>,
    snap_backup: Option<SnapBackup>,
    restore_jobs: HashMap<usize, RestoreJob>,
    next_timer_id: usize,
    /// 学習データに未保存の変更があるか（デバウンス保存用）。
    layouts_dirty: bool,
    tray: Option<Tray>,
    _hooks: Option<WinEventHooks>,
    _watcher: Option<ConfigWatcher>,
}

/// アプリを起動してメッセージループを回す。WM_QUIT で正常終了する。
pub fn run() -> windows::core::Result<()> {
    dpi::set_per_monitor_v2_aware();
    let hwnd = message_window::create()?;
    let config_path = config::default_path().unwrap_or_else(|| PathBuf::from("config.toml"));
    let config = config::load_or_init(&config_path).unwrap_or_else(|e| {
        tracing::error!("config load failed, using defaults: {e}");
        Config::default()
    });

    let layouts_path = layouts::default_path().unwrap_or_else(|| PathBuf::from("layouts.toml"));
    let learned = layouts::load(&layouts_path);

    let mut app = App {
        hwnd,
        enabled: config.general.enabled,
        auto_restore: config.general.auto_restore,
        config,
        config_path,
        occupancy: Occupancy::default(),
        learned,
        layouts_path,
        actions: HashMap::new(),
        registered_ids: Vec::new(),
        snap_backup: None,
        restore_jobs: HashMap::new(),
        next_timer_id: RESTORE_TIMER_BASE,
        layouts_dirty: false,
        tray: None,
        _hooks: None,
        _watcher: None,
    };

    app.recover_snap_if_crashed();
    app.register_hotkeys();
    app.apply_snap_setting();
    app._hooks = Some(winevent::install());
    app.tray = Tray::new(
        app.enabled,
        autostart::is_enabled(),
        app.config.general.disable_snap,
        app.auto_restore,
        app.config.grid.auto_aspect,
        app.config.grid.columns,
        app.config.grid.rows,
    );
    app._watcher = crate::watcher::watch_config(&app.config_path, hwnd);
    tracing::info!("windows-divider started (enabled={})", app.enabled);

    app.message_loop();
    app.shutdown();
    Ok(())
}

impl App {
    fn message_loop(&mut self) {
        let mut msg = MSG::default();
        loop {
            let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if r.0 == 0 {
                break; // WM_QUIT
            }
            if r.0 == -1 {
                tracing::error!("GetMessageW failed");
                break;
            }
            match msg.message {
                WM_HOTKEY => self.on_hotkey(msg.wParam.0 as i32),
                WM_TIMER => self.on_timer(msg.wParam.0),
                WM_APP_CONFIG_RELOAD => self.on_config_reload(),
                _ => {}
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // OUTOFCONTEXT の WinEvent コールバックは GetMessage 処理中に積まれる。各回末で取り出す。
            self.on_winevents();
            // トレイメニュー操作（muda のグローバルチャネル）を取り込む。
            if let Some(cmd) = self.tray.as_ref().and_then(Tray::poll) {
                self.handle_tray_command(cmd);
            }
        }
    }

    fn handle_tray_command(&mut self, cmd: TrayCommand) {
        match cmd {
            TrayCommand::ToggleEnabled => {
                self.enabled = !self.enabled;
                self.apply_snap_setting();
                if let Some(t) = &self.tray {
                    t.set_enabled_checked(self.enabled);
                }
                tracing::info!("enabled toggled: {}", self.enabled);
            }
            TrayCommand::ToggleDisableSnap => {
                self.config.general.disable_snap = !self.config.general.disable_snap;
                self.persist_config();
                self.apply_snap_setting();
                if let Some(t) = &self.tray {
                    t.set_disable_snap_checked(self.config.general.disable_snap);
                }
                tracing::info!("disable_snap toggled: {}", self.config.general.disable_snap);
            }
            TrayCommand::SetColumns(n) => {
                self.config.grid.columns = n;
                self.occupancy = Occupancy::default(); // 分割数変更で旧占有範囲は無効になるためリセット
                self.persist_config();
                if let Some(t) = &self.tray {
                    t.set_columns_checked(n);
                }
                tracing::info!("columns set: {n}");
            }
            TrayCommand::SetRows(n) => {
                self.config.grid.rows = n;
                self.occupancy = Occupancy::default();
                self.persist_config();
                if let Some(t) = &self.tray {
                    t.set_rows_checked(n);
                }
                tracing::info!("rows set: {n}");
            }
            TrayCommand::ToggleAutoRestore => {
                self.auto_restore = !self.auto_restore;
                self.config.general.auto_restore = self.auto_restore;
                self.persist_config();
                if let Some(t) = &self.tray {
                    t.set_auto_restore_checked(self.auto_restore);
                }
                tracing::info!("auto_restore toggled: {}", self.auto_restore);
            }
            TrayCommand::ToggleAutoAspect => {
                self.config.grid.auto_aspect = !self.config.grid.auto_aspect;
                self.occupancy = Occupancy::default(); // 分割数が変わるため旧占有範囲をリセット
                self.persist_config();
                if let Some(t) = &self.tray {
                    t.set_auto_aspect_checked(self.config.grid.auto_aspect);
                }
                tracing::info!("auto_aspect toggled: {}", self.config.grid.auto_aspect);
            }
            TrayCommand::OpenSettings => self.open_settings(),
            TrayCommand::ReloadConfig => self.on_config_reload(),
            TrayCommand::ToggleAutostart => {
                let on = autostart::toggle();
                if let Some(t) = &self.tray {
                    t.set_autostart_checked(on);
                }
                tracing::info!("autostart toggled: {on}");
            }
            TrayCommand::Quit => unsafe { PostQuitMessage(0) },
        }
    }

    /// 現在の設定をファイルへ保存する（メニュー操作による変更の永続化）。失敗はログに留める。
    fn persist_config(&self) {
        if let Err(e) = config::save(&self.config_path, &self.config) {
            tracing::error!("failed to save config: {e}");
        }
    }

    /// トレイのチェック表示を現在の設定に合わせる（設定ファイル直接編集の反映用）。
    fn sync_tray(&self) {
        if let Some(t) = &self.tray {
            t.set_enabled_checked(self.enabled);
            t.set_disable_snap_checked(self.config.general.disable_snap);
            t.set_auto_restore_checked(self.auto_restore);
            t.set_auto_aspect_checked(self.config.grid.auto_aspect);
            t.set_columns_checked(self.config.grid.columns);
            t.set_rows_checked(self.config.grid.rows);
            t.set_autostart_checked(autostart::is_enabled());
        }
    }

    /// 設定ファイルを既定のテキストエディタ（notepad）で開く。存在しなければ既定値で作成してから開く。
    fn open_settings(&self) {
        if !self.config_path.exists() {
            let _ = config::save(&self.config_path, &self.config);
        }
        match std::process::Command::new("notepad.exe")
            .arg(&self.config_path)
            .spawn()
        {
            Ok(_) => tracing::info!("opened settings in notepad"),
            Err(e) => tracing::warn!("failed to open settings: {e}"),
        }
    }

    // ── 機能 B: 矢印ホットキー ──────────────────────────────────────────
    fn on_hotkey(&mut self, id: i32) {
        let Some(&HotkeyAction::Move(family)) = self.actions.get(&id) else {
            return;
        };
        if !self.enabled {
            return;
        }
        let Some(hwnd) = window_ops::foreground_window() else {
            return;
        };
        if !self.may_intervene(hwnd) {
            return;
        }
        self.on_arrow(hwnd, family);
    }

    /// 矢印 1 押下を処理する。押下の瞬間に反対方向キーも押されていれば同時押し＝最大化、そうでなければ単独移動。
    ///
    /// 待ち時間を入れず即座に判定するため、単独押しに遅延は乗らない（反対方向は [`GetAsyncKeyState`] で判定）。
    fn on_arrow(&mut self, hwnd: HWND, family: Family) {
        if opposite_arrow_down(family) {
            self.apply_axis_full(hwnd, is_horizontal(family));
        } else {
            self.apply_arrow(hwnd, family);
        }
    }

    /// 矢印キー: ウィンドウのグリッド占有範囲を 1 セル動かして再配置する。
    ///
    /// 端でこれ以上動けない（占有が変わらない＝端セルかつその軸が最小幅）とき、操作方向に隣モニタが
    /// あればそのモニタへ送る（反対側の端セルに着地）。隣が無ければ従来どおり（実質無変化）。
    fn apply_arrow(&mut self, hwnd: HWND, family: Family) {
        let Some((base, work, cols, rows)) = self.prepare_base(hwnd) else {
            return;
        };
        let next = grid::step(base, family, cols, rows);
        if next == base && self.move_to_adjacent_monitor(hwnd, base, family, cols, rows) {
            return;
        }
        self.set_span(hwnd, next, cols, rows, work);
    }

    /// `family` 方向の隣モニタへウィンドウを送る。隣が無ければ何もせず `false`。
    ///
    /// 着地は反対側の端セル（[`grid::cross_edge_span`]）。移動先モニタの作業領域へ適用し、`(exe, class)` を学習する。
    fn move_to_adjacent_monitor(
        &mut self,
        hwnd: HWND,
        base: GridSpan,
        family: Family,
        cols: u32,
        rows: u32,
    ) -> bool {
        let Some(cur) = monitor::monitor_for_window(hwnd) else {
            return false;
        };
        let monitors = monitor::enumerate();
        let fulls: Vec<Rect> = monitors.iter().map(|m| m.full).collect();
        let Some(adj) = grid::adjacent_monitor(&fulls, cur.full, family) else {
            return false;
        };
        let landing = grid::cross_edge_span(base, family, cols, rows);
        self.set_span(hwnd, landing, cols, rows, monitors[adj].work_area);
        true
    }

    /// 反対方向同時押し: 押した方向の軸だけを全幅にする（←→=横軸フル・行維持／↑↓=縦軸フル・列維持）。
    ///
    /// 現在の占有を起点にもう一方の軸は保つため、横軸フル→縦軸フルの 2 ステップで全画面になる。
    fn apply_axis_full(&mut self, hwnd: HWND, horizontal: bool) {
        let Some((base, work, cols, rows)) = self.prepare_base(hwnd) else {
            return;
        };
        let next = grid::fill_axis(base, horizontal, cols, rows);
        self.set_span(hwnd, next, cols, rows, work);
    }

    /// 操作の起点となる占有範囲・作業領域・分割数を用意する。
    ///
    /// 分割数はそのモニタから決める（[`grid_dims_for`](Self::grid_dims_for)。自動判定が有効ならアスペクト比から）。
    /// OS 最大化中のウィンドウは全グリッド占有を起点にし（最大化から ← で `■■□` などになる）、最大化は解除する。
    fn prepare_base(&self, hwnd: HWND) -> Option<(GridSpan, Rect, u32, u32)> {
        let mon = monitor::monitor_for_window(hwnd)?;
        let work = mon.work_area;
        let (cols, rows) = self.grid_dims_for(&mon);
        let was_maximized = window_ops::is_maximized(hwnd);
        window_ops::restore_if_maximized(hwnd);
        let base = if was_maximized {
            GridSpan::full(cols, rows)
        } else {
            let current = window_ops::window_visible_rect(hwnd).unwrap_or(work);
            self.span_for(hwnd, work, current, cols, rows)
        };
        Some((base, work, cols, rows))
    }

    fn set_span(&mut self, hwnd: HWND, span: GridSpan, cols: u32, rows: u32, work: Rect) {
        if let Err(e) = window_ops::set_window_rect(hwnd, span.rect(cols, rows, work)) {
            tracing::warn!("set_span: set_window_rect failed: {e}");
        }
        self.learn(hwnd, span, cols, rows);
    }

    /// ユーザー操作で確定した占有範囲を、現在モニタの Slot として学習し所属を更新する。保存はデバウンス。
    fn learn(&mut self, hwnd: HWND, span: GridSpan, cols: u32, rows: u32) {
        let Some(key) = window_info::window_key(hwnd) else { return };
        let Some(mon) = monitor::monitor_for_window(hwnd) else { return };
        let slot = Slot { display: mon.display, span, cols, rows };
        let id = convert::hwnd_to_u64(hwnd);
        let old = self.occupancy.entry_of(id).map(|(_, s)| s);
        self.learned.learn(&key, slot.clone(), old, LEARNED_SLOTS_PER_KEY);
        self.occupancy.on_placed(id, key, slot);
        self.schedule_layouts_save();
    }

    /// 学習データのディスク保存をデバウンス予約する。保存タイマを毎回張り直し、最後の変更から
    /// `SAVE_DEBOUNCE_MS` 後に 1 回だけ書き出す（連続操作中はインメモリ更新のみ）。
    fn schedule_layouts_save(&mut self) {
        self.layouts_dirty = true;
        unsafe {
            SetTimer(Some(self.hwnd), SAVE_TIMER_ID, SAVE_DEBOUNCE_MS, None);
        }
    }

    /// 未保存の学習データがあれば `layouts.toml` へ書き出す。失敗はログに留める。
    fn flush_layouts_save(&mut self) {
        if !self.layouts_dirty {
            return;
        }
        if let Err(e) = layouts::save(&self.layouts_path, &self.learned) {
            tracing::warn!("failed to save layouts: {e}");
        }
        self.layouts_dirty = false;
    }

    /// このウィンドウの起点となる占有範囲を決める。
    ///
    /// 直前に自分が配置した所属スロットがあり、その矩形が現在矩形とほぼ一致すればそれを再利用する。
    /// 手動で動かされた・初回などで一致しなければ、現在位置から推定する。
    fn span_for(&self, hwnd: HWND, work: Rect, current: Rect, cols: u32, rows: u32) -> GridSpan {
        let id = convert::hwnd_to_u64(hwnd);
        if let Some((_, slot)) = self.occupancy.entry_of(id) {
            if slot.span.rect(cols, rows, work).approx_eq(current, SPAN_REUSE_TOLERANCE_PX) {
                return slot.span;
            }
        }
        grid::estimate_span(work, current, cols, rows)
    }

    /// このモニタで使う分割数 `(列数, 行数)`。`auto_aspect` が有効なら解像度アスペクトから自動判定し、無効なら設定値を使う。
    fn grid_dims_for(&self, mon: &monitor::MonitorInfo) -> (u32, u32) {
        if self.config.grid.auto_aspect {
            grid::grid_for_aspect(mon.full.width(), mon.full.height())
        } else {
            (self.config.grid.columns, self.config.grid.rows)
        }
    }

    fn may_intervene(&self, hwnd: HWND) -> bool {
        match guard::should_intervene(hwnd, &self.config.exclusions) {
            Interventability::Ok => true,
            other => {
                tracing::debug!("skip intervention: {other:?}");
                false
            }
        }
    }

    // ── 機能 C: ウィンドウイベント → 学習配置の自動復元 ─────────────────
    fn on_winevents(&mut self) {
        let events = winevent::drain_events();
        if events.is_empty() {
            return;
        }
        self.prune_occupancy(); // 死んだ窓の所属を掃除してから処理する
        if !self.enabled {
            return;
        }
        for ev in events {
            match ev {
                WinEvent::Created(raw) => {
                    if self.auto_restore && !self.learned.is_empty() {
                        self.maybe_schedule_restore(convert::u64_to_hwnd(raw));
                    }
                }
                WinEvent::MoveSizeEnd(raw) => self.maybe_release(convert::u64_to_hwnd(raw)),
            }
        }
    }

    fn maybe_schedule_restore(&mut self, hwnd: HWND) {
        // 安価な関門（OpenProcess を伴わない）を先に通し、一過性・子ウィンドウをここで弾く。
        // 生成イベントは大量に発火するため、その大半を GetWindowLongPtr だけで落として CPU を抑える。
        if guard::cheap_interventability(hwnd, &self.config.exclusions) != Interventability::Ok {
            return;
        }
        // ここで初めて OpenProcess（exe 取得）。除外判定と学習照合で同じ key を使い回す（二重取得しない）。
        let Some(key) = window_info::window_key(hwnd) else { return };
        if self.config.exclusions.excludes(&key.exe) {
            return;
        }
        let recorded = self.learned.slots(&key);
        if recorded.is_empty() {
            return; // 記録に無い識別子は動かさない
        }
        let Some(slot) = self.occupancy.pick_slot(&key, &recorded) else {
            return; // 空きスロットなし → 自由
        };
        // 復元で占有することを予約し、続けて生成される同識別子の窓が同じスロットを選ばないようにする。
        self.occupancy.on_placed(convert::hwnd_to_u64(hwnd), key, slot.clone());
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        unsafe {
            SetTimer(Some(self.hwnd), id, RESTORE_DELAY_MS, None);
        }
        self.restore_jobs.insert(id, RestoreJob { hwnd, slot, attempts_left: RESTORE_MAX_ATTEMPTS });
    }

    /// ユーザーがドラッグ/リサイズで所属スロットから外したら、所属と記録の双方から外す。
    ///
    /// ウィンドウは動かさない（内部状態と記録の更新のみ）。
    fn maybe_release(&mut self, hwnd: HWND) {
        let id = convert::hwnd_to_u64(hwnd);
        let Some((key, slot)) = self.occupancy.entry_of(id) else { return };
        // 記録ディスプレイが現存しないときは解除判定をしない（復元側 apply_learned_slot と対称）。
        // 別モニタの作業領域で誤比較し、切断中に学習を消してしまうのを防ぐ。
        let Some(mon) = monitor::monitor_by_name(&slot.display) else { return };
        let target = slot.span.clamp_to(slot.cols, slot.rows).rect(slot.cols, slot.rows, mon.work_area);
        let Some(cur) = window_ops::window_visible_rect(hwnd) else { return };
        if !cur.approx_eq(target, SPAN_REUSE_TOLERANCE_PX) {
            self.occupancy.on_released(id);
            self.learned.forget(&key, &slot);
            self.schedule_layouts_save();
        }
    }

    /// 既に存在しないウィンドウの所属を掃除する（長期常駐での単調増加を防ぐ）。
    /// 掃除は `on_winevents` 先頭の 1 か所に集約する（WinEvent が来たついでに回す）。
    fn prune_occupancy(&mut self) {
        self.occupancy
            .prune(|id| window_ops::is_window(convert::u64_to_hwnd(id)));
    }

    fn on_timer(&mut self, timer_id: usize) {
        if timer_id == SAVE_TIMER_ID {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), SAVE_TIMER_ID);
            }
            self.flush_layouts_save();
            return;
        }
        let Some(job) = self.restore_jobs.get_mut(&timer_id) else {
            return;
        };
        let hwnd = job.hwnd;
        let slot = job.slot.clone();
        job.attempts_left = job.attempts_left.saturating_sub(1);
        let attempts_left = job.attempts_left;

        let converged = self.apply_learned_slot(hwnd, &slot);

        if converged || attempts_left == 0 {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), timer_id);
            }
            self.restore_jobs.remove(&timer_id);
        }
    }

    /// 学習スロットを、そのスロットのディスプレイの作業領域へ適用する。
    ///
    /// 記録時の分割数でグリッドを解釈し、現在その分割数が変わっていても clamp_to で丸める。対象ディスプレイが
    /// 現存しなければ何もしない（収束扱いで打ち切り）。適用後に目標とほぼ一致すれば `true`。
    fn apply_learned_slot(&self, hwnd: HWND, slot: &Slot) -> bool {
        let Some(mon) = monitor::monitor_by_name(&slot.display) else {
            return true; // ディスプレイが無い → 復元しない（打ち切り）
        };
        let target = slot.span.clamp_to(slot.cols, slot.rows).rect(slot.cols, slot.rows, mon.work_area);
        window_ops::restore_if_maximized(hwnd);
        if let Err(e) = window_ops::set_window_rect(hwnd, target) {
            tracing::warn!("apply_learned_slot: set_window_rect failed: {e}");
        }
        window_ops::window_visible_rect(hwnd)
            .map(|cur| cur.approx_eq(target, CONVERGE_TOLERANCE_PX))
            .unwrap_or(false)
    }

    // ── 設定リロード ─────────────────────────────────────────────────
    fn on_config_reload(&mut self) {
        match config::load(&self.config_path) {
            Ok(cfg) => {
                tracing::info!("config reloaded");
                self.config = cfg;
                self.enabled = self.config.general.enabled;
                self.auto_restore = self.config.general.auto_restore;
                self.occupancy = Occupancy::default(); // 分割数が変わっている可能性があるため所属をリセット
                self.register_hotkeys();
                self.apply_snap_setting();
                self.sync_tray();
            }
            Err(e) => tracing::error!("config reload failed: {e}"),
        }
    }

    // ── ホットキー登録 ───────────────────────────────────────────────
    fn register_hotkeys(&mut self) {
        for id in self.registered_ids.drain(..) {
            winhotkey::unregister(self.hwnd, id);
        }
        self.actions.clear();
        for (i, b) in bindings(&self.config.hotkeys).into_iter().enumerate() {
            let id = i as i32 + 1;
            match parse(&b.chord) {
                Ok(hk) => match winhotkey::register(self.hwnd, id, hk) {
                    Ok(()) => {
                        self.actions.insert(id, b.action);
                        self.registered_ids.push(id);
                    }
                    Err(e) => tracing::warn!("hotkey '{}' ({}) register failed: {e}", b.name, b.chord),
                },
                Err(e) => tracing::warn!("hotkey '{}' parse failed: {e}", b.name),
            }
        }
        tracing::info!("registered {} hotkeys", self.registered_ids.len());
    }

    // ── 機能 A: スナップ無効化の適用・復元 ───────────────────────────
    fn apply_snap_setting(&mut self) {
        let want_disable = self.enabled && self.config.general.disable_snap;
        if want_disable && self.snap_backup.is_none() {
            let backup = snap::disable_snap(self.config.general.disable_snap_assist);
            self.persist_snap_backup(&backup);
            self.snap_backup = Some(backup);
            tracing::info!("native snap disabled");
        } else if !want_disable {
            if let Some(backup) = self.snap_backup.take() {
                snap::restore_snap(&backup);
                self.clear_persisted_backup();
                tracing::info!("native snap restored");
            }
        }
    }

    /// 前回の異常終了で退避ファイルが残っていれば、先に元設定へ戻してからファイルを消す。
    fn recover_snap_if_crashed(&mut self) {
        if let Some(backup) = self.load_persisted_backup() {
            tracing::warn!("found leftover snap backup; restoring previous settings");
            snap::restore_snap(&backup);
            self.clear_persisted_backup();
        }
    }

    fn shutdown(&mut self) {
        self.flush_layouts_save(); // デバウンス保留中の学習データを取りこぼさない
        for id in self.registered_ids.drain(..) {
            winhotkey::unregister(self.hwnd, id);
        }
        if let Some(backup) = self.snap_backup.take() {
            snap::restore_snap(&backup);
            self.clear_persisted_backup();
        }
        tracing::info!("windows-divider stopped");
    }

    // ── スナップ退避ファイルの永続化（クラッシュ復旧用） ─────────────
    fn snap_backup_path(&self) -> PathBuf {
        self.config_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("snap_backup.toml")
    }

    fn persist_snap_backup(&self, backup: &SnapBackup) {
        if let Ok(text) = toml::to_string(backup) {
            let _ = fs::write(self.snap_backup_path(), text);
        }
    }

    fn load_persisted_backup(&self) -> Option<SnapBackup> {
        let text = fs::read_to_string(self.snap_backup_path()).ok()?;
        toml::from_str(&text).ok()
    }

    fn clear_persisted_backup(&self) {
        let _ = fs::remove_file(self.snap_backup_path());
    }
}

/// 押された矢印 `family` の「反対方向キー」が今この瞬間に押されているか。
///
/// 反対方向の同時押し（←+→ / ↑+↓）を待ち時間ゼロで検出するために、`GetAsyncKeyState` の
/// 最上位ビット（押下中なら i16 が負）を見る。低レベルフックや注入は使わない文書化 API。
fn opposite_arrow_down(family: Family) -> bool {
    let opposite_vk = match family {
        Family::Left => VK_RIGHT,
        Family::Right => VK_LEFT,
        Family::Top => VK_DOWN,
        Family::Bottom => VK_UP,
    };
    unsafe { GetAsyncKeyState(opposite_vk) < 0 }
}

/// 水平軸（左右）の方向か。同時押し時に横軸フル/縦軸フルを選ぶのに使う。
fn is_horizontal(family: Family) -> bool {
    matches!(family, Family::Left | Family::Right)
}
