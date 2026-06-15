//! グローバルホットキーの登録・解除（`RegisterHotKey`）。
//!
//! 低レベル入力フックを使わず `RegisterHotKey` のみで実現する（アンチチート表面を最小化）。
//! 矢印の連続押下で占有範囲を 1 セルずつ動かすため `MOD_NOREPEAT` は付けない（押下ごとに `WM_HOTKEY` を受ける）。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};

use crate::hotkey::Hotkey;

/// ホットキー `hk` を識別子 `id` で `hwnd` に登録する。
///
/// 失敗時は `Err`（多くは他常駐ソフトとのキー競合）。呼び手はログ＋通知して起動を継続する。
pub fn register(hwnd: HWND, id: i32, hk: Hotkey) -> windows::core::Result<()> {
    unsafe {
        RegisterHotKey(
            Some(hwnd),
            id,
            HOT_KEY_MODIFIERS(hk.mods.win32_bits()),
            hk.vk as u32,
        )
    }
}

/// `id` のホットキー登録を解除する。失敗は無視する。
pub fn unregister(hwnd: HWND, id: i32) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), id);
    }
}
