//! グリッド占有範囲と、矢印キーによるその操作（Win32 非依存）。
//!
//! ウィンドウは `columns × rows` のグリッド上で連続したセル範囲 [`GridSpan`]（列 `l..=r`、行 `t..=b`）を
//! 占有する。矢印キーは [`step`] で範囲を 1 セルずつ動かす:
//! - 左/右キー（[`Family::Left`]/[`Family::Right`]）は列方向のみ、上/下キーは行方向のみを変え、もう一方の軸は保つ。
//! - 各軸の規則（[`axis_toward_start`] / [`axis_toward_end`]）: 幅が 2 以上なら手前/奥の辺を削って寄せ、
//!   幅 1 ならその方向へ 1 セル広げる（端で停止）。
//!
//! 学習した占有範囲を別グリッドへ適用するときは [`GridSpan::clamp_to`] で範囲外インデックスを丸める。

use serde::{Deserialize, Serialize};

use super::geometry::Rect;

/// 矢印キーの方向。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Left,
    Right,
    Top,
    Bottom,
}

/// グリッド上の占有範囲。列 `l..=r`、行 `t..=b`（いずれも 0 始まり・両端含む、`l<=r`・`t<=b`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GridSpan {
    pub l: u32,
    pub r: u32,
    pub t: u32,
    pub b: u32,
}

impl GridSpan {
    /// グリッド全体を占有する範囲（最大化相当）。`columns`/`rows` は最低 1 として扱う。
    pub fn full(columns: u32, rows: u32) -> GridSpan {
        GridSpan { l: 0, r: columns.max(1) - 1, t: 0, b: rows.max(1) - 1 }
    }

    /// 占有範囲を `columns × rows` のグリッドに収まるようクランプする。
    ///
    /// 学習時より分割数が小さくなった場合に、範囲外のインデックスを最大インデックス（`columns-1` /
    /// `rows-1`）へ丸める。`columns`/`rows` は最低 1 として扱う。`l<=r`・`t<=b` の関係は保たれる。
    pub fn clamp_to(self, columns: u32, rows: u32) -> GridSpan {
        let max_c = columns.max(1) - 1;
        let max_r = rows.max(1) - 1;
        GridSpan {
            l: self.l.min(max_c),
            r: self.r.min(max_c),
            t: self.t.min(max_r),
            b: self.b.min(max_r),
        }
    }

    /// この占有範囲を作業領域 `work` 上の実矩形へ変換する。隣接セルの境界は [`Rect::sub`] で一致する。
    pub fn rect(&self, columns: u32, rows: u32, work: Rect) -> Rect {
        let c = columns.max(1) as f64;
        let rw = rows.max(1) as f64;
        work.sub(
            self.l as f64 / c,
            (self.r as f64 + 1.0) / c,
            self.t as f64 / rw,
            (self.b as f64 + 1.0) / rw,
        )
    }
}

/// 軸の `(start, end)` に「手前方向（←／↑）」を適用する。
///
/// 幅 2 以上（`start < end`）なら終端を 1 手前へ縮め（寄せて細く）、幅 1（`start == end`）なら始端を
/// 1 手前へ広げる（0 で停止）。
pub fn axis_toward_start(start: u32, end: u32) -> (u32, u32) {
    if start == end {
        (start.saturating_sub(1), end)
    } else {
        (start, end - 1)
    }
}

/// 軸の `(start, end)` に「奥方向（→／↓）」を適用する。`max` は軸の最大インデックス。
///
/// 幅 2 以上なら始端を 1 奥へ縮め、幅 1 なら終端を 1 奥へ広げる（`max` で停止）。
pub fn axis_toward_end(start: u32, end: u32, max: u32) -> (u32, u32) {
    if start == end {
        (start, (end + 1).min(max))
    } else {
        (start + 1, end)
    }
}

/// 占有範囲 `span` に方向 `family` の操作を 1 回適用する。左右は列軸のみ、上下は行軸のみを変える。
pub fn step(span: GridSpan, family: Family, columns: u32, rows: u32) -> GridSpan {
    let mut s = span;
    match family {
        Family::Left => {
            let (l, r) = axis_toward_start(s.l, s.r);
            s.l = l;
            s.r = r;
        }
        Family::Right => {
            let (l, r) = axis_toward_end(s.l, s.r, columns.max(1) - 1);
            s.l = l;
            s.r = r;
        }
        Family::Top => {
            let (t, b) = axis_toward_start(s.t, s.b);
            s.t = t;
            s.b = b;
        }
        Family::Bottom => {
            let (t, b) = axis_toward_end(s.t, s.b, rows.max(1) - 1);
            s.t = t;
            s.b = b;
        }
    }
    s
}

/// 占有範囲の一方の軸だけを全幅にする。`horizontal` が真なら列を全占有（行は維持）、偽なら行を全占有（列は維持）。
///
/// 反対方向の同時押し（←→ で横軸フル、↑↓ で縦軸フル）に対応する。横軸フルと縦軸フルの両方を行えば全画面になる。
pub fn fill_axis(span: GridSpan, horizontal: bool, columns: u32, rows: u32) -> GridSpan {
    if horizontal {
        GridSpan { l: 0, r: columns.max(1) - 1, t: span.t, b: span.b }
    } else {
        GridSpan { l: span.l, r: span.r, t: 0, b: rows.max(1) - 1 }
    }
}

/// `current`（ウィンドウの現在矩形）を `work` 上のグリッドに丸めて占有範囲を推定する。
///
/// 保持している占有範囲が無い／ユーザーが手動で動かした後の初回操作で、現在位置から最も近い格子を起点に
/// するために使う。各辺をセル境界に丸め、`l<=r`・`t<=b` を保証する。ほぼ全画面なら全体を占有とみなす。
pub fn estimate_span(work: Rect, current: Rect, columns: u32, rows: u32) -> GridSpan {
    let c = columns.max(1);
    let r = rows.max(1);
    let cell_w = (work.width().max(1) as f64) / c as f64;
    let cell_h = (work.height().max(1) as f64) / r as f64;

    let to_index = |pos: i32, origin: i32, cell: f64| -> i64 { (((pos - origin) as f64) / cell).round() as i64 };

    let l = to_index(current.left, work.left, cell_w).clamp(0, c as i64 - 1) as u32;
    let r_end = (to_index(current.right, work.left, cell_w) - 1).clamp(0, c as i64 - 1) as u32;
    let t = to_index(current.top, work.top, cell_h).clamp(0, r as i64 - 1) as u32;
    let b_end = (to_index(current.bottom, work.top, cell_h) - 1).clamp(0, r as i64 - 1) as u32;

    GridSpan {
        l: l.min(r_end),
        r: l.max(r_end),
        t: t.min(b_end),
        b: t.max(b_end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLS: u32 = 3;
    const ROWS: u32 = 2;

    fn span(l: u32, r: u32, t: u32, b: u32) -> GridSpan {
        GridSpan { l, r, t, b }
    }

    #[test]
    fn full_span_is_whole_grid() {
        assert_eq!(GridSpan::full(COLS, ROWS), span(0, 2, 0, 1));
    }

    /// ユーザー提示の左右シーケンス: 全幅 →←→ ■■□ →←→ ■□□ →→→ ■■□ →→→ □■□
    #[test]
    fn user_left_right_sequence() {
        let s0 = GridSpan::full(COLS, ROWS); // (0,2,0,1) 全幅
        let s1 = step(s0, Family::Left, COLS, ROWS);
        assert_eq!(s1, span(0, 1, 0, 1)); // ■■□

        let s2 = step(s1, Family::Left, COLS, ROWS);
        assert_eq!(s2, span(0, 0, 0, 1)); // ■□□

        let s3 = step(s2, Family::Right, COLS, ROWS);
        assert_eq!(s3, span(0, 1, 0, 1)); // ■■□

        let s4 = step(s3, Family::Right, COLS, ROWS);
        assert_eq!(s4, span(1, 1, 0, 1)); // □■□
    }

    #[test]
    fn right_edge_behaviour_mirror() {
        // 右からの対称: 全幅 →→→ □■■ →→→ □□■
        let s0 = GridSpan::full(COLS, ROWS);
        assert_eq!(step(s0, Family::Right, COLS, ROWS), span(1, 2, 0, 1)); // □■■
        let s1 = span(1, 2, 0, 1);
        assert_eq!(step(s1, Family::Right, COLS, ROWS), span(2, 2, 0, 1)); // □□■
    }

    /// ユーザー提示の上下: 横の占有を保ったまま、両行 → 上行 になる。
    #[test]
    fn user_up_preserves_columns() {
        // ■■□/■■□ から ↑ → ■■□/□□□
        assert_eq!(step(span(0, 1, 0, 1), Family::Top, COLS, ROWS), span(0, 1, 0, 0));
        // ■□□/■□□ から ↑ → ■□□/□□□
        assert_eq!(step(span(0, 0, 0, 1), Family::Top, COLS, ROWS), span(0, 0, 0, 0));
        // □■□/□■□ から ↑ → □■□/□□□
        assert_eq!(step(span(1, 1, 0, 1), Family::Top, COLS, ROWS), span(1, 1, 0, 0));
    }

    #[test]
    fn width_one_widens_then_stops_at_edge() {
        // 幅1で ← は左へ広がる、左端では停止
        assert_eq!(step(span(1, 1, 0, 1), Family::Left, COLS, ROWS), span(0, 1, 0, 1));
        assert_eq!(step(span(0, 0, 0, 1), Family::Left, COLS, ROWS), span(0, 0, 0, 1)); // 左端停止
        // 幅1で → は右へ広がる、右端では停止
        assert_eq!(step(span(1, 1, 0, 1), Family::Right, COLS, ROWS), span(1, 2, 0, 1));
        assert_eq!(step(span(2, 2, 0, 1), Family::Right, COLS, ROWS), span(2, 2, 0, 1)); // 右端停止
    }

    #[test]
    fn span_rect_matches_grid_cells() {
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        // ■■□/■■□ = 左2列・全行 = 幅 2/3, 全高
        assert_eq!(span(0, 1, 0, 1).rect(COLS, ROWS, work), Rect { left: 0, top: 0, right: 800, bottom: 800 });
        // □■□（中央1列・全行）= x=[1/3,2/3]
        assert_eq!(span(1, 1, 0, 1).rect(COLS, ROWS, work), Rect { left: 400, top: 0, right: 800, bottom: 800 });
        // 左上セル ■□□/□□□ = x=[0,1/3] y=[0,1/2]
        assert_eq!(span(0, 0, 0, 0).rect(COLS, ROWS, work), Rect { left: 0, top: 0, right: 400, bottom: 400 });
    }

    #[test]
    fn estimate_full_screen_is_whole_grid() {
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        assert_eq!(estimate_span(work, work, COLS, ROWS), span(0, 2, 0, 1));
    }

    #[test]
    fn estimate_left_column() {
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        let cur = Rect { left: 0, top: 0, right: 400, bottom: 800 }; // 左1/3 全高
        assert_eq!(estimate_span(work, cur, COLS, ROWS), span(0, 0, 0, 1));
    }

    #[test]
    fn clamp_to_shrinks_out_of_range_indices() {
        // 3×2 で学習した右下 (2,2,1,1) を 2×1 グリッドへ → 右端=1・下端=0 に丸める。
        assert_eq!(span(2, 2, 1, 1).clamp_to(2, 1), span(1, 1, 0, 0));
        // 収まっている範囲はそのまま。
        assert_eq!(span(0, 1, 0, 1).clamp_to(3, 2), span(0, 1, 0, 1));
    }

    #[test]
    fn fill_axis_maximizes_one_axis() {
        // 下段左セル □□□/■□□ (0,0,1,1) を横軸フル → 下段全幅 □□□/■■■ (0,2,1,1)
        assert_eq!(fill_axis(span(0, 0, 1, 1), true, COLS, ROWS), span(0, 2, 1, 1));
        // 同セルを縦軸フル → 左列全高 ■□□/■□□ (0,0,0,1)
        assert_eq!(fill_axis(span(0, 0, 1, 1), false, COLS, ROWS), span(0, 0, 0, 1));
        // 横軸フル → 縦軸フルで全画面
        let h = fill_axis(span(0, 0, 1, 1), true, COLS, ROWS);
        assert_eq!(fill_axis(h, false, COLS, ROWS), GridSpan::full(COLS, ROWS));
    }
}
