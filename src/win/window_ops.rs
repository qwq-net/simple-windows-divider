//! ウィンドウの取得・移動・状態変更（文書化された user32 API のみ）。

use std::mem::size_of;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsWindow,
    SetWindowPos, ShowWindow, GWL_STYLE, SW_MAXIMIZE, SW_RESTORE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOZORDER, WINDOWPLACEMENT,
};

use super::convert::{from_rect, to_rect};
use crate::layout::geometry::Rect;

/// 現在のフォアグラウンドウィンドウ。存在しなければ `None`。
pub fn foreground_window() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

/// `hwnd` が現存する有効なウィンドウか。破棄済みハンドル（spans の掃除など）の判定に使う。
pub fn is_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

/// ウィンドウの `GWL_STYLE` ビット（`WS_*`）。取得失敗時は 0（＝どのスタイルも立っていない扱い）。
///
/// 値の意味づけ（スナップ対象か等）は純ロジックの [`crate::window_style`] に委ねる。
pub fn window_style_bits(hwnd: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 }
}

/// ウィンドウの現在の矩形（仮想デスクトップ座標、物理ピクセル）。取得失敗時 `None`。
pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some(from_rect(r))
}

/// ウィンドウを指定矩形へ移動・リサイズする。Z オーダーとアクティブ状態は変更しない。
///
/// 失敗時は `Err`（例: 昇格ウィンドウを非昇格から操作した場合の ACCESS_DENIED）。呼び手はログに留めて続行する。
pub fn set_window_rect(hwnd: HWND, rect: Rect) -> windows::core::Result<()> {
    let r = to_rect(rect);
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    }
}

/// ウィンドウが最大化状態か。
pub fn is_maximized(hwnd: HWND) -> bool {
    let mut wp = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe { GetWindowPlacement(hwnd, &mut wp) }.is_ok() && wp.showCmd == SW_MAXIMIZE.0 as u32
}

/// 最大化されていれば通常状態へ戻す（グリッド適用の前処理。最大化のままだとサイズ指定が効かないため）。
pub fn restore_if_maximized(hwnd: HWND) {
    if is_maximized(hwnd) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }
}
