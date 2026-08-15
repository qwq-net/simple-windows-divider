//! 純ロジックの中立型と Win32 型の相互変換。

use windows::Win32::Foundation::RECT;

use crate::layout::geometry::Rect;

/// 中立 [`Rect`] を Win32 `RECT` へ。フィールドは同順・同型（i32）。
pub fn to_rect(r: Rect) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/// Win32 `RECT` を中立 [`Rect`] へ。
pub fn from_rect(r: RECT) -> Rect {
    Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}
