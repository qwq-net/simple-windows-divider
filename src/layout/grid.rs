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

impl Family {
    /// 水平軸（左右）の方向か。反対方向同時押し時に横軸フル/縦軸フルのどちらにするかを選ぶのに使う。
    pub fn is_horizontal(self) -> bool {
        matches!(self, Family::Left | Family::Right)
    }

    /// 反対方向（←↔→、↑↔↓）。反対方向の同時押し（最大化トリガ）を検出するのに使う。
    pub fn opposite(self) -> Family {
        match self {
            Family::Left => Family::Right,
            Family::Right => Family::Left,
            Family::Top => Family::Bottom,
            Family::Bottom => Family::Top,
        }
    }
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

/// 端モニタへ送るときの「着地」占有範囲。`family` の反対側の端へ幅/高さ 1 セルで置き、もう一方の軸は維持する。
///
/// 右へ越える → 隣の左端（`l=r=0`）、左へ → 右端、下へ → 上端（`t=b=0`）、上へ → 下端。行/列のうち
/// 操作軸でない側（右左なら行、上下なら列）は `span` の値をそのまま使う。
pub fn cross_edge_span(span: GridSpan, family: Family, columns: u32, rows: u32) -> GridSpan {
    let max_c = columns.max(1) - 1;
    let max_r = rows.max(1) - 1;
    match family {
        Family::Right => GridSpan { l: 0, r: 0, t: span.t, b: span.b },
        Family::Left => GridSpan { l: max_c, r: max_c, t: span.t, b: span.b },
        Family::Bottom => GridSpan { l: span.l, r: span.r, t: 0, b: 0 },
        Family::Top => GridSpan { l: span.l, r: span.r, t: max_r, b: max_r },
    }
}

/// `current`（モニタ矩形）から見て `family` 方向に隣接するモニタの添字を返す。無ければ `None`。
///
/// 操作方向にあり（右なら `left >= current.right` など）、かつ垂直/水平に重なるモニタのうち最も近いものを選ぶ。
/// `current` 自身は方向条件で除外される。重なりが無いモニタは隣接とみなさない。
pub fn adjacent_monitor(monitors: &[Rect], current: Rect, family: Family) -> Option<usize> {
    let v_overlap = |m: Rect| m.top < current.bottom && m.bottom > current.top;
    let h_overlap = |m: Rect| m.left < current.right && m.right > current.left;
    let mut best: Option<(usize, i32)> = None;
    for (i, &m) in monitors.iter().enumerate() {
        let (in_dir, dist) = match family {
            Family::Right => (m.left >= current.right && v_overlap(m), m.left - current.right),
            Family::Left => (m.right <= current.left && v_overlap(m), current.left - m.right),
            Family::Bottom => (m.top >= current.bottom && h_overlap(m), m.top - current.bottom),
            Family::Top => (m.bottom <= current.top && h_overlap(m), current.top - m.bottom),
        };
        if in_dir && dist >= 0 && best.is_none_or(|(_, bd)| dist < bd) {
            best = Some((i, dist));
        }
    }
    best.map(|(i, _)| i)
}

/// モニタ解像度のアスペクト比（`width / height`）から分割数 `(列, 行)` を選ぶ（自動分割モード用のプリセット）。
///
/// 帯で判定して近似アスペクトを吸収する: `>=3.0` → 4×2（32:9 等）、`>=2.0` → 3×2（21:9 等）、
/// `>=1.0` → 2×2（16:9 / 16:10 / 4:3 等）、`<1.0`（縦長）→ 1×2。`height` が 0 以下でも破綻しない（最低 1 として扱う）。
pub fn grid_for_aspect(width: i32, height: i32) -> (u32, u32) {
    let aspect = width.max(1) as f64 / height.max(1) as f64;
    if aspect >= 3.0 {
        (4, 2)
    } else if aspect >= 2.0 {
        (3, 2)
    } else if aspect >= 1.0 {
        (2, 2)
    } else {
        (1, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLS: u32 = 3;
    const ROWS: u32 = 2;

    #[test]
    fn grid_for_aspect_maps_classes() {
        assert_eq!(grid_for_aspect(3840, 1080), (4, 2)); // 32:9
        assert_eq!(grid_for_aspect(3440, 1440), (3, 2)); // 21:9
        assert_eq!(grid_for_aspect(2560, 1080), (3, 2)); // ~21:9
        assert_eq!(grid_for_aspect(1920, 1080), (2, 2)); // 16:9
        assert_eq!(grid_for_aspect(1920, 1200), (2, 2)); // 16:10
        assert_eq!(grid_for_aspect(1024, 768), (2, 2)); // 4:3
        assert_eq!(grid_for_aspect(1080, 1920), (1, 2)); // 縦長
    }

    #[test]
    fn grid_for_aspect_boundaries() {
        assert_eq!(grid_for_aspect(3000, 1000), (4, 2)); // ちょうど 3.0
        assert_eq!(grid_for_aspect(2000, 1000), (3, 2)); // ちょうど 2.0
        assert_eq!(grid_for_aspect(1000, 1000), (2, 2)); // ちょうど 1.0（正方形）
        assert_eq!(grid_for_aspect(999, 1000), (1, 2)); // 1.0 未満
        assert_eq!(grid_for_aspect(1920, 0), (4, 2)); // height=0 でもパニックしない
    }

    #[test]
    fn family_is_horizontal_classifies() {
        assert!(Family::Left.is_horizontal());
        assert!(Family::Right.is_horizontal());
        assert!(!Family::Top.is_horizontal());
        assert!(!Family::Bottom.is_horizontal());
    }

    #[test]
    fn family_opposite_pairs_arrows() {
        assert_eq!(Family::Left.opposite(), Family::Right);
        assert_eq!(Family::Right.opposite(), Family::Left);
        assert_eq!(Family::Top.opposite(), Family::Bottom);
        assert_eq!(Family::Bottom.opposite(), Family::Top);
    }

    fn mon(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }

    #[test]
    fn cross_edge_span_lands_on_opposite_edge() {
        // 右へ越える: 行は維持、列は左端 1 セル
        assert_eq!(cross_edge_span(span(2, 2, 0, 1), Family::Right, COLS, ROWS), span(0, 0, 0, 1));
        // 左へ越える: 右端 1 セル
        assert_eq!(cross_edge_span(span(0, 0, 1, 1), Family::Left, COLS, ROWS), span(2, 2, 1, 1));
        // 下へ越える: 上端 1 セル・列維持
        assert_eq!(cross_edge_span(span(1, 1, 1, 1), Family::Bottom, COLS, ROWS), span(1, 1, 0, 0));
        // 上へ越える: 下端 1 セル・列維持
        assert_eq!(cross_edge_span(span(0, 1, 0, 0), Family::Top, COLS, ROWS), span(0, 1, 1, 1));
    }

    #[test]
    fn adjacent_monitor_left_right() {
        let a = mon(0, 0, 1920, 1080);
        let b = mon(1920, 0, 3840, 1080);
        let mons = [a, b];
        assert_eq!(adjacent_monitor(&mons, a, Family::Right), Some(1));
        assert_eq!(adjacent_monitor(&mons, a, Family::Left), None);
        assert_eq!(adjacent_monitor(&mons, b, Family::Left), Some(0));
        assert_eq!(adjacent_monitor(&mons, b, Family::Right), None);
    }

    #[test]
    fn adjacent_monitor_vertical_and_requires_overlap() {
        let top = mon(0, 0, 1920, 1080);
        let bottom = mon(0, 1080, 1920, 2160);
        let mons = [top, bottom];
        assert_eq!(adjacent_monitor(&mons, top, Family::Bottom), Some(1));
        assert_eq!(adjacent_monitor(&mons, top, Family::Top), None);
        // 右にあるが縦に重ならない → 隣接なし
        let far = mon(1920, 5000, 3840, 6080);
        assert_eq!(adjacent_monitor(&[top, far], top, Family::Right), None);
    }

    #[test]
    fn adjacent_monitor_picks_nearest() {
        let a = mon(0, 0, 1920, 1080);
        let b = mon(1920, 0, 3840, 1080);
        let c = mon(3840, 0, 5760, 1080);
        let mons = [a, b, c];
        assert_eq!(adjacent_monitor(&mons, a, Family::Right), Some(1)); // 最も近い b
    }

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
