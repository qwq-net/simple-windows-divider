//! タスクトレイ常駐の UI（アイコン＋右クリックメニュー）。
//!
//! 主要設定はメニュー内で直接変更できる。状態系はチェック式（[`CheckMenuItem`]）、分割数はサブメニューの
//! 選択式（現在値にチェック）。詳細（ルール・除外など）は「設定ファイルを開く」で TOML を編集する。
//! 操作は [`tray_icon::menu::MenuEvent`] のグローバルチャネルへ届き、`app` が [`Tray::poll`] で取り込む。

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// 列数・行数としてメニューに並べる選択肢。
const SPLIT_CHOICES: &[u32] = &[1, 2, 3, 4, 5, 6];

/// トレイメニューから発生する操作。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayCommand {
    ToggleEnabled,
    ToggleDisableSnap,
    ToggleAutoRestore,
    SetColumns(u32),
    SetRows(u32),
    OpenSettings,
    ReloadConfig,
    ToggleAutostart,
    Quit,
}

/// トレイアイコン本体とメニュー項目を保持する。Drop でアイコンが消える。
pub struct Tray {
    _tray: TrayIcon,
    enabled_item: CheckMenuItem,
    disable_snap_item: CheckMenuItem,
    auto_restore_item: CheckMenuItem,
    autostart_item: CheckMenuItem,
    columns_items: Vec<(u32, CheckMenuItem)>,
    rows_items: Vec<(u32, CheckMenuItem)>,
    id_enabled: MenuId,
    id_disable_snap: MenuId,
    id_auto_restore: MenuId,
    id_autostart: MenuId,
    id_open: MenuId,
    id_reload: MenuId,
    id_quit: MenuId,
}

impl Tray {
    /// トレイを生成する。現在の状態（有効・スナップ無効・自動復元・自動起動・列数・行数）を初期表示に反映する。
    pub fn new(
        enabled: bool,
        autostart: bool,
        disable_snap: bool,
        auto_restore: bool,
        columns: u32,
        rows: u32,
    ) -> Option<Tray> {
        let menu = Menu::new();
        let enabled_item = CheckMenuItem::new("ウィンドウ管理を有効化", true, enabled, None);
        let disable_snap_item = CheckMenuItem::new("標準スナップを無効化", true, disable_snap, None);
        let auto_restore_item = CheckMenuItem::new("覚えた配置を自動復元", true, auto_restore, None);
        let columns_menu = Submenu::new("列数（横の分割）", true);
        let columns_items = append_choices(&columns_menu, columns);
        let rows_menu = Submenu::new("行数（縦の分割）", true);
        let rows_items = append_choices(&rows_menu, rows);
        let open_item = MenuItem::new("設定ファイルを開く", true, None);
        let reload_item = MenuItem::new("設定を再読み込み", true, None);
        let autostart_item = CheckMenuItem::new("ログオン時に自動起動", true, autostart, None);
        let quit_item = MenuItem::new("終了", true, None);

        menu.append(&enabled_item).ok()?;
        menu.append(&disable_snap_item).ok()?;
        menu.append(&auto_restore_item).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&columns_menu).ok()?;
        menu.append(&rows_menu).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&open_item).ok()?;
        menu.append(&reload_item).ok()?;
        menu.append(&autostart_item).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&quit_item).ok()?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("windows-divider")
            .with_icon(default_icon())
            .build()
            .ok()?;

        Some(Tray {
            id_enabled: enabled_item.id().clone(),
            id_disable_snap: disable_snap_item.id().clone(),
            id_auto_restore: auto_restore_item.id().clone(),
            id_autostart: autostart_item.id().clone(),
            id_open: open_item.id().clone(),
            id_reload: reload_item.id().clone(),
            id_quit: quit_item.id().clone(),
            enabled_item,
            disable_snap_item,
            auto_restore_item,
            autostart_item,
            columns_items,
            rows_items,
            _tray: tray,
        })
    }

    /// 保留中のメニューイベントを 1 件取り出してコマンド化する。無ければ `None`。
    pub fn poll(&self) -> Option<TrayCommand> {
        let ev = MenuEvent::receiver().try_recv().ok()?;
        if ev.id == self.id_enabled {
            Some(TrayCommand::ToggleEnabled)
        } else if ev.id == self.id_disable_snap {
            Some(TrayCommand::ToggleDisableSnap)
        } else if ev.id == self.id_auto_restore {
            Some(TrayCommand::ToggleAutoRestore)
        } else if ev.id == self.id_autostart {
            Some(TrayCommand::ToggleAutostart)
        } else if ev.id == self.id_open {
            Some(TrayCommand::OpenSettings)
        } else if ev.id == self.id_reload {
            Some(TrayCommand::ReloadConfig)
        } else if ev.id == self.id_quit {
            Some(TrayCommand::Quit)
        } else if let Some((v, _)) = self.columns_items.iter().find(|(_, it)| *it.id() == ev.id) {
            Some(TrayCommand::SetColumns(*v))
        } else if let Some((v, _)) = self.rows_items.iter().find(|(_, it)| *it.id() == ev.id) {
            Some(TrayCommand::SetRows(*v))
        } else {
            None
        }
    }

    pub fn set_enabled_checked(&self, enabled: bool) {
        self.enabled_item.set_checked(enabled);
    }

    pub fn set_disable_snap_checked(&self, on: bool) {
        self.disable_snap_item.set_checked(on);
    }

    pub fn set_auto_restore_checked(&self, on: bool) {
        self.auto_restore_item.set_checked(on);
    }

    pub fn set_autostart_checked(&self, autostart: bool) {
        self.autostart_item.set_checked(autostart);
    }

    /// 列数の選択チェックを `value` に合わせる（他は外す）。
    pub fn set_columns_checked(&self, value: u32) {
        check_only(&self.columns_items, value);
    }

    /// 行数の選択チェックを `value` に合わせる。
    pub fn set_rows_checked(&self, value: u32) {
        check_only(&self.rows_items, value);
    }
}

/// サブメニューに 1..=6 のチェック項目を追加し、`current` にだけチェックを付ける。`(値, 項目)` を返す。
fn append_choices(submenu: &Submenu, current: u32) -> Vec<(u32, CheckMenuItem)> {
    let mut items = Vec::new();
    for &v in SPLIT_CHOICES {
        let item = CheckMenuItem::new(v.to_string(), true, v == current, None);
        let _ = submenu.append(&item);
        items.push((v, item));
    }
    items
}

/// `value` の項目だけチェックし、他は外す（ラジオボタン的な相互排他）。
fn check_only(items: &[(u32, CheckMenuItem)], value: u32) {
    for (v, item) in items {
        item.set_checked(*v == value);
    }
}

/// 32x32 の簡易アイコン（左右 2 分割を表す。左=青系・右=灰系・枠線あり）。
fn default_icon() -> Icon {
    const S: u32 = 32;
    let mut rgba = Vec::with_capacity((S * S * 4) as usize);
    for y in 0..S {
        for x in 0..S {
            let border = x == 0 || y == 0 || x == S - 1 || y == S - 1 || x == S / 2;
            let (r, g, b) = if border {
                (40, 40, 40)
            } else if x < S / 2 {
                (70, 130, 200)
            } else {
                (200, 200, 205)
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Icon::from_rgba(rgba, S, S).expect("32x32 RGBA is a valid icon")
}
