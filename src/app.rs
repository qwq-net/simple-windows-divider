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
use crate::layouts::{self, LearnedLayouts};
use crate::tray::{Tray, TrayCommand};
use crate::watcher::ConfigWatcher;
use crate::win::guard::Interventability;
use crate::win::message_window::WM_APP_CONFIG_RELOAD;
use crate::win::snap::SnapBackup;
use crate::win::winevent::WinEventHooks;
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
/// `spans` が無効エントリで膨らむのを防ぐための掃除しきい値（これを超えたら無効分を除去）。
const SPANS_PRUNE_THRESHOLD: usize = 64;

// 矢印キーの仮想キーコード（反対方向の同時押し＝最大化を判定するため）。
const VK_LEFT: i32 = 0x25;
const VK_UP: i32 = 0x26;
const VK_RIGHT: i32 = 0x27;
const VK_DOWN: i32 = 0x28;

/// 機能 C の遅延リトライ 1 件分の状態。学習した占有範囲を生成直後のウィンドウへ適用する。
struct RestoreJob {
    hwnd: HWND,
    span: GridSpan,
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
    /// ウィンドウ(u64)ごとのグリッド占有範囲。矢印キーで更新し、次回操作の起点に使う（一時状態）。
    spans: HashMap<u64, GridSpan>,
    /// `(exe, class)` ごとに学習した占有範囲（永続）。`spans` とは別物。
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
        spans: HashMap::new(),
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
                self.spans.clear(); // 分割数変更で旧占有範囲は無効になるためリセット
                self.persist_config();
                if let Some(t) = &self.tray {
                    t.set_columns_checked(n);
                }
                tracing::info!("columns set: {n}");
            }
            TrayCommand::SetRows(n) => {
                self.config.grid.rows = n;
                self.spans.clear();
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
        let (cols, rows) = self.grid_dims();
        let Some((base, work)) = self.prepare_base(hwnd, cols, rows) else {
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
        let (cols, rows) = self.grid_dims();
        let Some((base, work)) = self.prepare_base(hwnd, cols, rows) else {
            return;
        };
        let next = grid::fill_axis(base, horizontal, cols, rows);
        self.set_span(hwnd, next, cols, rows, work);
    }

    /// 操作の起点となる占有範囲と作業領域を用意する。
    ///
    /// OS 最大化中のウィンドウは全グリッド占有を起点にし（最大化から ← で `■■□` などになる）、最大化は解除する。
    fn prepare_base(&self, hwnd: HWND, cols: u32, rows: u32) -> Option<(GridSpan, Rect)> {
        let mon = monitor::monitor_for_window(hwnd)?;
        let work = mon.work_area;
        let was_maximized = window_ops::is_maximized(hwnd);
        window_ops::restore_if_maximized(hwnd);
        let base = if was_maximized {
            GridSpan::full(cols, rows)
        } else {
            let current = window_ops::window_rect(hwnd).unwrap_or(work);
            self.span_for(hwnd, work, current, cols, rows)
        };
        Some((base, work))
    }

    /// 占有範囲を保存しつつウィンドウへ適用し、`(exe, class)` 単位で学習する。
    ///
    /// ユーザーの矢印操作からのみ呼ばれる（自動復元はこの経路を通らないため、復元が学習を上書きしない）。
    fn set_span(&mut self, hwnd: HWND, span: GridSpan, cols: u32, rows: u32, work: Rect) {
        self.spans.insert(convert::hwnd_to_u64(hwnd), span);
        if self.spans.len() > SPANS_PRUNE_THRESHOLD {
            self.prune_spans();
        }
        if let Err(e) = window_ops::set_window_rect(hwnd, span.rect(cols, rows, work)) {
            tracing::warn!("set_span: set_window_rect failed: {e}");
        }
        self.learn(hwnd, span);
    }

    /// 既に存在しないウィンドウの `spans` エントリを掃除する（長期常駐での単調増加を防ぐ）。
    fn prune_spans(&mut self) {
        self.spans
            .retain(|&id, _| window_ops::is_window(convert::u64_to_hwnd(id)));
    }

    /// ユーザー操作で確定した占有範囲を `(exe, class)` 単位で学習し、保存を予約する。
    ///
    /// 保存はデバウンスする（矢印連打で `layouts.toml` 書き込みが多発しないよう、最後の 1 回に合流させる）。
    fn learn(&mut self, hwnd: HWND, span: GridSpan) {
        let Some(key) = window_info::window_key(hwnd) else {
            return;
        };
        self.learned.record(&key, span);
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
    /// 保存済みの範囲があり、その矩形が現在のウィンドウ矩形とほぼ一致すれば（＝直前に自分が配置した）
    /// それを使う。手動で動かされた／初回などで一致しなければ、現在位置から推定する。
    fn span_for(&self, hwnd: HWND, work: Rect, current: Rect, cols: u32, rows: u32) -> GridSpan {
        let key = convert::hwnd_to_u64(hwnd);
        if let Some(&saved) = self.spans.get(&key) {
            if saved.rect(cols, rows, work).approx_eq(current, SPAN_REUSE_TOLERANCE_PX) {
                return saved;
            }
        }
        grid::estimate_span(work, current, cols, rows)
    }

    /// 設定のグリッド分割数 `(列数, 行数)`。
    fn grid_dims(&self) -> (u32, u32) {
        (self.config.grid.columns, self.config.grid.rows)
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
        // キューは常に drain して滞留を防ぐ。自動復元が無効／学習データが空なら以降の重い処理（window_key 等）は省く。
        let events = winevent::drain_events();
        if events.is_empty() || !self.enabled || !self.auto_restore || self.learned.is_empty() {
            return;
        }
        for raw in events {
            let hwnd = convert::u64_to_hwnd(raw);
            self.maybe_schedule_restore(hwnd);
        }
    }

    fn maybe_schedule_restore(&mut self, hwnd: HWND) {
        // 安価な関門（OpenProcess を伴わない）を先に通し、一過性・子ウィンドウをここで弾く。
        // 生成イベントは大量に発火するため、その大半を GetWindowLongPtr だけで落として CPU を抑える。
        if guard::cheap_interventability(hwnd, &self.config.exclusions) != Interventability::Ok {
            return;
        }
        // ここで初めて OpenProcess（exe 取得）。除外判定と学習照合で同じ key を使い回す（二重取得しない）。
        let Some(key) = window_info::window_key(hwnd) else {
            return;
        };
        if self.config.exclusions.excludes(&key.exe) {
            return;
        }
        let Some(span) = self.learned.lookup(&key) else {
            return;
        };
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        unsafe {
            SetTimer(Some(self.hwnd), id, RESTORE_DELAY_MS, None);
        }
        self.restore_jobs.insert(
            id,
            RestoreJob {
                hwnd,
                span,
                attempts_left: RESTORE_MAX_ATTEMPTS,
            },
        );
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
        let span = job.span;
        job.attempts_left = job.attempts_left.saturating_sub(1);
        let attempts_left = job.attempts_left;

        let converged = self.apply_learned_span(hwnd, span);

        if converged || attempts_left == 0 {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), timer_id);
            }
            self.restore_jobs.remove(&timer_id);
        }
    }

    /// 学習した占有範囲を、ウィンドウの現在モニタの作業領域へ適用する。
    ///
    /// 現在のグリッド分割数に合わせて範囲外インデックスをクランプする（分割数が学習時から変わっていても破綻しない）。
    /// 適用後に目標とほぼ一致すれば `true`（収束＝リトライ終了）。対象モニタを取得できなければ `true`（打ち切り）。
    fn apply_learned_span(&self, hwnd: HWND, span: GridSpan) -> bool {
        let (cols, rows) = self.grid_dims();
        let Some(mon) = monitor::monitor_for_window(hwnd) else {
            return true;
        };
        let target = span.clamp_to(cols, rows).rect(cols, rows, mon.work_area);
        window_ops::restore_if_maximized(hwnd);
        if let Err(e) = window_ops::set_window_rect(hwnd, target) {
            tracing::warn!("apply_learned_span: set_window_rect failed: {e}");
        }
        window_ops::window_rect(hwnd)
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
                self.spans.clear(); // 分割数が変わっている可能性があるため占有範囲をリセット
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
