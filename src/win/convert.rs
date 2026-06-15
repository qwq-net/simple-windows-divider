//! 純ロジックの中立型と Win32 型の相互変換。

use windows::Win32::Foundation::{HWND, RECT};

use crate::layout::geometry::Rect;

/// 中立 [`Rect`] を Win32 `RECT` へ。フィールドは同順・同型（i32）。
pub fn to_rect(r: Rect) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/// Win32 `RECT` を中立 [`Rect`] へ。
pub fn from_rect(r: RECT) -> Rect {
    Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/// `HWND` を純ロジック用の不透明な `u64` 識別子へ。占有範囲の記録や復元の重複抑止のキーに使う。
pub fn hwnd_to_u64(hwnd: HWND) -> u64 {
    hwnd.0 as usize as u64
}

/// [`hwnd_to_u64`] で得た値から `HWND` を復元する。
///
/// 値は実在したウィンドウ由来であること。復元後のウィンドウが既に破棄されている可能性はあるため、
/// 利用側は後続の Win32 呼び出しの失敗を許容する（破棄済みなら情報取得が失敗するだけ）。
pub fn u64_to_hwnd(v: u64) -> HWND {
    HWND(v as usize as *mut core::ffi::c_void)
}
