//! ホットキー登録レジストリ（機能 B の入口）。
//!
//! 設定の矢印 4 方向を `RegisterHotKey` で登録し、`WM_HOTKEY` の id から対応するアクションを引けるようにする。
//! 低レベル入力フックは使わない（[`crate::win::hotkey`] 経由）。

use std::collections::HashMap;

use windows::Win32::Foundation::HWND;

use crate::action::{bindings, HotkeyAction};
use crate::config::schema::Hotkeys;
use crate::hotkey::parse;
use crate::win::hotkey as winhotkey;

/// 登録済みホットキー id → アクションの対応と、解除用の id 一覧を保持する。
#[derive(Default)]
pub struct HotkeyRegistry {
    actions: HashMap<i32, HotkeyAction>,
    registered_ids: Vec<i32>,
}

impl HotkeyRegistry {
    /// 既存の登録をすべて解除してから、`hotkeys`（矢印 4 方向）を `hwnd` へ登録し直す。
    ///
    /// 初期登録と設定リロードの両方から呼ぶ。チョード文字列のパース失敗・登録失敗（他常駐ソフトとの競合など）は
    /// その割り当てだけ読み飛ばし、ログに残して継続する。登録 id は割り当て順に 1 から振る。
    pub fn reregister(&mut self, hwnd: HWND, hotkeys: &Hotkeys) {
        self.unregister_all(hwnd);
        for (i, b) in bindings(hotkeys).into_iter().enumerate() {
            let id = i as i32 + 1;
            match parse(&b.chord) {
                Ok(hk) => match winhotkey::register(hwnd, id, hk) {
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

    /// `WM_HOTKEY` の id に対応するアクション。未登録の id なら `None`。
    pub fn action_of(&self, id: i32) -> Option<HotkeyAction> {
        self.actions.get(&id).copied()
    }

    /// 登録済みホットキーをすべて解除し、対応表も空にする（リロード前・終了時）。
    pub fn unregister_all(&mut self, hwnd: HWND) {
        for id in self.registered_ids.drain(..) {
            winhotkey::unregister(hwnd, id);
        }
        self.actions.clear();
    }
}
