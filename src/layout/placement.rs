//! 配置の起点決定（Win32 非依存）。
//!
//! ホットキー操作で「そのモニタで使う分割数」を、モニタの矩形と設定値だけから決める。
//! 実際のウィンドウ取得や適用は配線層（`app`）が担い、ここは純粋な判断に徹する。

use super::geometry::Rect;
use super::grid;

/// このモニタで使う分割数 `(列数, 行数)` を決める。
///
/// `auto_aspect` が真なら `full`（モニタ全体の矩形）のアスペクト比から [`grid::grid_for_aspect`] で自動判定し、
/// `configured`（設定の列・行）は無視する。偽なら `configured` をそのまま使う。副作用なし。
pub fn grid_dims(auto_aspect: bool, full: Rect, configured: (u32, u32)) -> (u32, u32) {
    if auto_aspect {
        grid::grid_for_aspect(full.width(), full.height())
    } else {
        configured
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dims_uses_configured_when_manual() {
        // auto_aspect=false → configured をそのまま使う（full は無視）。
        let full = Rect { left: 0, top: 0, right: 3840, bottom: 1080 };
        assert_eq!(grid_dims(false, full, (3, 2)), (3, 2));
    }

    #[test]
    fn grid_dims_auto_overrides_configured() {
        // auto_aspect=true → full のアスペクトから算出し、configured を無視する。
        let full = Rect { left: 0, top: 0, right: 3840, bottom: 1080 }; // 32:9 → 4×2
        assert_eq!(grid_dims(true, full, (1, 1)), (4, 2));
    }

    #[test]
    fn grid_dims_auto_picks_aspect_class() {
        let wide = Rect { left: 0, top: 0, right: 1920, bottom: 1080 }; // 16:9 → 2×2
        assert_eq!(grid_dims(true, wide, (5, 5)), (2, 2));
        let ultra = Rect { left: 0, top: 0, right: 3440, bottom: 1440 }; // 21:9 → 3×2
        assert_eq!(grid_dims(true, ultra, (5, 5)), (3, 2));
    }
}
