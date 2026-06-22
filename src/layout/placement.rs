//! 配置の起点決定（Win32 非依存）。
//!
//! ホットキー操作で「どのグリッド占有範囲を起点にするか」「そのモニタで使う分割数」を、ウィンドウ・モニタの
//! 矩形と設定値だけから決める。実際のウィンドウ取得や適用は配線層（`app`）が担い、ここは純粋な判断に徹する。

use super::geometry::Rect;
use super::grid::{self, GridSpan};

/// このウィンドウの起点となる占有範囲を決める。
///
/// `prev`（直前に自分が配置した所属スロットの占有範囲。無ければ `None`）があり、それを `work` 上で矩形化した
/// ものが `current`（ウィンドウの現在矩形）と各辺 `tol` 以内で一致すれば、その `prev` をそのまま返す（自分の
/// 配置が保たれている）。一致しない・`None`（手動移動後や初回）なら `current` から [`grid::estimate_span`] で
/// 推定する。`cols`/`rows` は現在のグリッド分割数、`tol` は再利用とみなす許容差（0 以上）。副作用なし。
pub fn span_for(prev: Option<GridSpan>, work: Rect, current: Rect, cols: u32, rows: u32, tol: i32) -> GridSpan {
    if let Some(span) = prev {
        if span.rect(cols, rows, work).approx_eq(current, tol) {
            return span;
        }
    }
    grid::estimate_span(work, current, cols, rows)
}

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

    const COLS: u32 = 3;
    const ROWS: u32 = 2;
    const TOL: i32 = 6;

    fn span(l: u32, r: u32, t: u32, b: u32) -> GridSpan {
        GridSpan { l, r, t, b }
    }

    #[test]
    fn span_for_reuses_prev_when_rect_matches() {
        // prev を矩形化したものが current と一致 → prev をそのまま返す（estimate しない）。
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        let prev = span(0, 1, 0, 1); // ■■□ 全高
        let current = prev.rect(COLS, ROWS, work);
        assert_eq!(span_for(Some(prev), work, current, COLS, ROWS, TOL), prev);
    }

    #[test]
    fn span_for_estimates_when_no_prev() {
        // prev=None → 現在矩形から推定する。
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        let current = Rect { left: 0, top: 0, right: 400, bottom: 800 }; // 左 1/3 全高
        assert_eq!(span_for(None, work, current, COLS, ROWS, TOL), span(0, 0, 0, 1));
    }

    #[test]
    fn span_for_estimates_when_moved_away() {
        // prev はあるが矩形が許容差を超えてずれている（手動移動後）→ 推定にフォールバック。
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        let prev = span(0, 0, 0, 1); // 左列・全高
        let current = Rect { left: 400, top: 0, right: 800, bottom: 800 }; // 中央列へ動かされた
        assert_eq!(span_for(Some(prev), work, current, COLS, ROWS, TOL), span(1, 1, 0, 1));
    }

    #[test]
    fn span_for_reuses_prev_within_tolerance() {
        // 各辺ちょうど tol px のドリフトは「自分の配置のまま」とみなして prev を再利用する。
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        let prev = span(0, 0, 0, 1); // 左列・全高
        let base = prev.rect(COLS, ROWS, work);
        let drifted = Rect {
            left: base.left + TOL,
            top: base.top - TOL,
            right: base.right + TOL,
            bottom: base.bottom - TOL,
        };
        assert_eq!(span_for(Some(prev), work, drifted, COLS, ROWS, TOL), prev);
    }

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
