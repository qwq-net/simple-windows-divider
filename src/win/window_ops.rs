//! ウィンドウの取得・移動・状態変更（文書化された user32 API のみ）。

use core::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
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

/// ウィンドウの現在の矩形（仮想デスクトップ座標、物理ピクセル）。`GetWindowRect` の外周で、見えない
/// リサイズ枠を含む。取得失敗時 `None`。
pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some(from_rect(r))
}

/// 実際に見えているウィンドウ矩形（DWM 拡張フレーム境界）。取得不可なら [`window_rect`] にフォールバック。
///
/// Windows 10/11 のトップレベルウィンドウは [`window_rect`] の外周に見えない余白（リサイズ枠・影領域）を
/// 含む。配置の比較・推定は「見えている矩形」で行うため、こちらを使う。
pub fn window_visible_rect(hwnd: HWND) -> Option<Rect> {
    dwm_frame(hwnd).or_else(|| window_rect(hwnd))
}

/// DWM 拡張フレーム境界（見える矩形）。取得失敗時 `None`。
fn dwm_frame(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    Some(from_rect(r))
}

/// 外周（[`window_rect`]）と可視矩形（[`dwm_frame`]）の差＝見えない外周余白 `(左, 上, 右, 下)`。
///
/// いずれも 0 以上。取得不可なら全て 0（余白なし扱い＝補正しない）。
fn frame_insets(hwnd: HWND) -> (i32, i32, i32, i32) {
    match (window_rect(hwnd), dwm_frame(hwnd)) {
        (Some(outer), Some(vis)) => (
            (vis.left - outer.left).max(0),
            (vis.top - outer.top).max(0),
            (outer.right - vis.right).max(0),
            (outer.bottom - vis.bottom).max(0),
        ),
        _ => (0, 0, 0, 0),
    }
}

/// ウィンドウの**見えるフレーム**が `rect` にぴったり収まるよう移動・リサイズする。Z オーダーとアクティブ状態は変えない。
///
/// 見えない外周余白（DWM のリサイズ枠・影領域）の分だけ広げて `SetWindowPos` するため、隣のウィンドウや
/// 画面端との間に隙間ができない。失敗時は `Err`（例: 昇格ウィンドウへの ACCESS_DENIED）。呼び手はログに留めて続行する。
pub fn set_window_rect(hwnd: HWND, rect: Rect) -> windows::core::Result<()> {
    let (il, it, ir, ib) = frame_insets(hwnd);
    let r = to_rect(rect);
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            r.left - il,
            r.top - it,
            (r.right - r.left) + il + ir,
            (r.bottom - r.top) + it + ib,
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
