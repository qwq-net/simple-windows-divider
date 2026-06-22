//! アプリ本体: メッセージループと全メッセージのディスパッチ（副作用の集約点）。
//!
//! 単一スレッド・単一メッセージループで、`WM_HOTKEY`（機能 B）・`WM_TIMER`（機能 C の遅延リトライと保存
//! デバウンス）・`WM_APP_CONFIG_RELOAD`（設定再読込）を捌き、各ループ末で WinEvent コールバックが積んだ
//! イベント（機能 C のトリガ）を取り出して処理する。[`App`] は状態の所有とディスパッチに徹し、各機能の実処理は
//! サブモジュールへ委譲する:
//! - `store` — 実行中のスロット所属・学習データ・その永続化（機能 B/C 共有）。
//! - `arrange` — 機能 B（矢印ホットキーでのグリッド配置）。
//! - `restore` — 機能 C（生成直後のウィンドウへ学習配置を自動復元）。
//! - `hotkeys` — ホットキー登録レジストリ。
//! - `snap_control` — 機能 A（標準スナップ無効化）と退避ファイルの永続化。
//!
//! 能動的なウィンドウ操作の契機は「①矢印ホットキー（`on_hotkey` → `arrange`）②新規生成時の自動復元
//! （`on_winevents` → `restore`）」の 2 つだけで、いずれも実際の操作の手前で [`guard`] の判定を必ず通す。

mod arrange;
mod hotkeys;
mod restore;
mod snap_control;
mod store;

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostQuitMessage, TranslateMessage, MSG, WM_HOTKEY, WM_TIMER,
};

use crate::action::HotkeyAction;
use crate::config::{self, Config};
use crate::layouts;
use crate::tray::{Tray, TrayCommand, TrayView};
use crate::watcher::ConfigWatcher;
use crate::win::guard::{self, Interventability};
use crate::win::message_window::WM_APP_CONFIG_RELOAD;
use crate::win::winevent::{WinEvent, WinEventHooks};
use crate::win::{autostart, convert, dpi, message_window, winevent, window_ops};
use hotkeys::HotkeyRegistry;
use restore::RestoreManager;
use snap_control::SnapControl;
use store::LayoutStore;

/// アプリ全体の状態。すべて UI スレッドからのみ触れるためロック不要。
pub struct App {
    hwnd: HWND,
    config: Config,
    config_path: PathBuf,
    enabled: bool,
    /// 機能 C: 学習した配置を新規ウィンドウへ自動復元するか。
    auto_restore: bool,
    /// 実行中のスロット所属と学習データ・その永続化（機能 B/C 共有）。
    store: LayoutStore,
    /// 矢印ホットキーの登録・id→アクション対応（機能 B）。
    hotkeys: HotkeyRegistry,
    /// 標準スナップ無効化の状態と退避ファイル管理（機能 A）。
    snap: SnapControl,
    /// 生成直後のウィンドウへ学習配置を適用する遅延リトライ群（機能 C）。
    restore: RestoreManager,
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
    let snap = SnapControl::new(&config_path);

    let mut app = App {
        hwnd,
        enabled: config.general.enabled,
        auto_restore: config.general.auto_restore,
        config,
        config_path,
        store: LayoutStore::new(layouts_path, learned),
        hotkeys: HotkeyRegistry::default(),
        snap,
        restore: RestoreManager::default(),
        tray: None,
        _hooks: None,
        _watcher: None,
    };

    app.snap.recover_if_crashed();
    app.hotkeys.reregister(hwnd, &app.config.hotkeys);
    app.apply_snap_setting();
    app._hooks = Some(winevent::install());
    app.tray = Tray::new(&app.tray_view());
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

    /// トレイメニュー操作を処理する。各腕は状態の変更と必要な反映（永続化・スナップ適用など）だけを行い、
    /// チェック表示の同期は末尾で [`sync_tray`](Self::sync_tray) に一任する（個別のチェック更新は書かない）。
    fn handle_tray_command(&mut self, cmd: TrayCommand) {
        match cmd {
            TrayCommand::ToggleEnabled => {
                self.enabled = !self.enabled;
                self.apply_snap_setting();
                tracing::info!("enabled toggled: {}", self.enabled);
            }
            TrayCommand::ToggleDisableSnap => {
                self.config.general.disable_snap = !self.config.general.disable_snap;
                self.persist_config();
                self.apply_snap_setting();
                tracing::info!("disable_snap toggled: {}", self.config.general.disable_snap);
            }
            TrayCommand::SetColumns(n) => {
                self.config.grid.columns = n;
                self.reset_occupancy();
                self.persist_config();
                tracing::info!("columns set: {n}");
            }
            TrayCommand::SetRows(n) => {
                self.config.grid.rows = n;
                self.reset_occupancy();
                self.persist_config();
                tracing::info!("rows set: {n}");
            }
            TrayCommand::ToggleAutoRestore => {
                self.auto_restore = !self.auto_restore;
                self.config.general.auto_restore = self.auto_restore;
                self.persist_config();
                tracing::info!("auto_restore toggled: {}", self.auto_restore);
            }
            TrayCommand::ToggleAutoAspect => {
                self.config.grid.auto_aspect = !self.config.grid.auto_aspect;
                self.reset_occupancy();
                self.persist_config();
                tracing::info!("auto_aspect toggled: {}", self.config.grid.auto_aspect);
            }
            TrayCommand::OpenSettings => self.open_settings(),
            TrayCommand::ReloadConfig => self.on_config_reload(),
            TrayCommand::ToggleAutostart => {
                let on = autostart::toggle();
                tracing::info!("autostart toggled: {on}");
            }
            TrayCommand::Quit => unsafe { PostQuitMessage(0) },
        }
        self.sync_tray();
    }

    /// 現在の設定をファイルへ保存する（メニュー操作による変更の永続化）。失敗はログに留める。
    fn persist_config(&self) {
        if let Err(e) = config::save(&self.config_path, &self.config) {
            tracing::error!("failed to save config: {e}");
        }
    }

    /// 現在の状態からトレイ表示用のスナップショットを作る。自動起動はレジストリから都度読む。
    fn tray_view(&self) -> TrayView {
        TrayView {
            enabled: self.enabled,
            disable_snap: self.config.general.disable_snap,
            auto_restore: self.auto_restore,
            auto_aspect: self.config.grid.auto_aspect,
            autostart: autostart::is_enabled(),
            columns: self.config.grid.columns,
            rows: self.config.grid.rows,
        }
    }

    /// トレイのチェック表示を現在の状態に合わせる（設定ファイル直接編集やメニュー操作の反映用）。
    fn sync_tray(&self) {
        if let Some(t) = &self.tray {
            t.apply(&self.tray_view());
        }
    }

    /// 分割数が変わる操作で、実行中のスロット所属（非永続）を捨てる。学習データ（永続）は消さない。
    fn reset_occupancy(&mut self) {
        self.store.reset_occupancy();
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
    /// `WM_HOTKEY` を処理する。前景ウィンドウに介入してよいか（[`may_intervene`](Self::may_intervene)）を
    /// ここで判定し、通過したものだけ機能 B の処理（[`arrange::on_arrow`]）へ渡す。
    fn on_hotkey(&mut self, id: i32) {
        let Some(HotkeyAction::Move(family)) = self.hotkeys.action_of(id) else {
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
        arrange::on_arrow(&mut self.store, &self.config, self.hwnd, hwnd, family);
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
        self.store.prune(); // 死んだ窓の所属を掃除してから処理する（WinEvent が来たついでに 1 回）
        if !self.enabled {
            return;
        }
        for ev in events {
            match ev {
                WinEvent::Created(raw) => {
                    if self.auto_restore && !self.store.is_empty() {
                        self.restore.on_created(
                            self.hwnd,
                            convert::u64_to_hwnd(raw),
                            &mut self.store,
                            &self.config,
                        );
                    }
                }
                WinEvent::MoveSizeEnd(raw) => {
                    self.store.release_if_moved(self.hwnd, convert::u64_to_hwnd(raw));
                }
            }
        }
    }

    /// `WM_TIMER` を処理する。保存デバウンスタイマは [`LayoutStore`]、それ以外は復元リトライとして [`RestoreManager`] へ振り分ける。
    fn on_timer(&mut self, timer_id: usize) {
        if timer_id == store::SAVE_TIMER_ID {
            self.store.on_save_timer(self.hwnd);
        } else {
            self.restore.on_timer(self.hwnd, timer_id);
        }
    }

    // ── 設定リロード ─────────────────────────────────────────────────
    fn on_config_reload(&mut self) {
        match config::load(&self.config_path) {
            Ok(cfg) => {
                tracing::info!("config reloaded");
                self.config = cfg;
                self.enabled = self.config.general.enabled;
                self.auto_restore = self.config.general.auto_restore;
                self.reset_occupancy(); // 分割数が変わっている可能性があるため所属をリセット
                self.hotkeys.reregister(self.hwnd, &self.config.hotkeys);
                self.apply_snap_setting();
                self.sync_tray();
            }
            Err(e) => tracing::error!("config reload failed: {e}"),
        }
    }

    // ── 機能 A: スナップ無効化の適用・復元 ───────────────────────────
    /// 「有効かつスナップ無効化設定が ON」のときだけ無効化し、それ以外なら復元する。状態管理は [`SnapControl`] に委譲する。
    fn apply_snap_setting(&mut self) {
        let want_disable = self.enabled && self.config.general.disable_snap;
        self.snap.apply(want_disable, self.config.general.disable_snap_assist);
    }

    fn shutdown(&mut self) {
        self.store.flush_save(); // デバウンス保留中の学習データを取りこぼさない
        self.hotkeys.unregister_all(self.hwnd);
        self.snap.restore_on_exit();
        tracing::info!("windows-divider stopped");
    }
}
