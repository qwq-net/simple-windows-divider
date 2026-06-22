//! 矩形型と分割プリミティブ。
//!
//! 座標は Win32 と同じ「仮想デスクトップ上の物理ピクセル」を想定する（左上原点・右下方向が正）。
//! ただし本モジュールは Win32 に依存せず、純粋な整数/割合計算のみを行う。

use serde::{Deserialize, Serialize};

/// 軸平行な矩形。`right`/`bottom` は排他的境界として扱う（幅 = `right - left`）。
///
/// Win32 の `RECT` と同じ並びなので、Windows 側で相互変換しやすい。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    /// 幅（`right - left`）。負にはならない前提（正規化済みの矩形を渡すこと）。
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    /// 高さ（`bottom - top`）。
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// 左上 `(x, y)` と幅・高さから生成する（`right = x + w`, `bottom = y + h`）。
    pub fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { left: x, top: y, right: x + w, bottom: y + h }
    }

    /// 自身を割合区間 `[x0, x1] × [y0, y1]`（各 0.0..=1.0、`self` の幅・高さに対する比）で
    /// 切り出した部分矩形を返す。
    ///
    /// 境界は `start + round(span * f)` で算出するため、**隣接する区間どうしの境界がちょうど一致し、
    /// 隙間も重なりも生じない**（例: `[0,1/3]` の右端と `[1/3,2/3]` の左端は同値）。
    /// `f == 1.0` のときは丸め誤差なく `self` の右端・下端に一致する。`self` が非ゼロ原点
    /// （セカンダリモニタ等）でもオフセットは保たれる。
    ///
    /// `x0 <= x1`, `y0 <= y1` を前提とし、範囲外や逆転は検査しない（呼び手が妥当な値を渡す）。副作用なし。
    ///
    /// ```
    /// use windows_divider::layout::geometry::Rect;
    /// let work = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
    /// assert_eq!(work.sub(0.0, 0.5, 0.0, 1.0), Rect { left: 0, top: 0, right: 960, bottom: 1080 });
    /// assert_eq!(work.sub(0.0, 1.0, 0.0, 1.0), work); // 全域は self に一致
    /// ```
    pub fn sub(&self, x0: f64, x1: f64, y0: f64, y1: f64) -> Rect {
        let w = self.width() as f64;
        let h = self.height() as f64;
        let bx = |f: f64| self.left + (w * f).round() as i32;
        let by = |f: f64| self.top + (h * f).round() as i32;
        Rect { left: bx(x0), top: by(y0), right: bx(x1), bottom: by(y1) }
    }

    /// `other` と各辺（left/top/right/bottom）の差の絶対値がすべて `tol` 以下なら `true`。
    ///
    /// `tol` は 0 以上を想定する（負だと常に `false`）。配置後の収束判定や、保存済み占有範囲が
    /// 現在のウィンドウ矩形と一致するかの再利用判定に使う。副作用なし。
    pub fn approx_eq(&self, other: Rect, tol: i32) -> bool {
        (self.left - other.left).abs() <= tol
            && (self.top - other.top).abs() <= tol
            && (self.right - other.right).abs() <= tol
            && (self.bottom - other.bottom).abs() <= tol
    }

    /// 自身が `outer` を四辺すべてで覆う（完全に含む）か。辺がちょうど一致する場合も覆うとみなす。
    ///
    /// 各辺で `self.left <= outer.left`・`self.top <= outer.top`・`self.right >= outer.right`・
    /// `self.bottom >= outer.bottom` がすべて成り立てば `true`。ウィンドウ矩形がモニタ全体を覆うか
    /// （フルスクリーン相当か）を見るために使う。各辺は独立に判定するため、1 辺でも内側に収まれば `false`。
    /// 副作用なし。
    pub fn covers(&self, outer: Rect) -> bool {
        self.left <= outer.left
            && self.top <= outer.top
            && self.right >= outer.right
            && self.bottom >= outer.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_and_height() {
        let r = Rect { left: 10, top: 20, right: 110, bottom: 220 };
        assert_eq!(r.width(), 100);
        assert_eq!(r.height(), 200);
    }

    #[test]
    fn from_xywh_sets_edges() {
        let r = Rect::from_xywh(10, 20, 100, 200);
        assert_eq!(r, Rect { left: 10, top: 20, right: 110, bottom: 220 });
    }

    #[test]
    fn sub_full_returns_self() {
        let work = Rect { left: 5, top: 7, right: 1925, bottom: 1087 };
        assert_eq!(work.sub(0.0, 1.0, 0.0, 1.0), work);
    }

    #[test]
    fn sub_left_half() {
        let work = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        assert_eq!(
            work.sub(0.0, 0.5, 0.0, 1.0),
            Rect { left: 0, top: 0, right: 960, bottom: 1080 }
        );
    }

    #[test]
    fn sub_respects_nonzero_origin() {
        // セカンダリモニタ（原点 x=1920）の右半分
        let work = Rect { left: 1920, top: 0, right: 1920 + 1280, bottom: 1024 };
        assert_eq!(
            work.sub(0.5, 1.0, 0.0, 1.0),
            Rect { left: 1920 + 640, top: 0, right: 1920 + 1280, bottom: 1024 }
        );
    }

    #[test]
    fn adjacent_thirds_have_no_gap_or_overlap() {
        let work = Rect { left: 0, top: 0, right: 1000, bottom: 1000 };
        let l = work.sub(0.0, 1.0 / 3.0, 0.0, 1.0);
        let c = work.sub(1.0 / 3.0, 2.0 / 3.0, 0.0, 1.0);
        let r = work.sub(2.0 / 3.0, 1.0, 0.0, 1.0);
        // 隣接境界が一致（隙間も重なりもない）
        assert_eq!(l.right, c.left);
        assert_eq!(c.right, r.left);
        // 端は work にちょうど一致
        assert_eq!(l.left, work.left);
        assert_eq!(r.right, work.right);
        // 幅の合計が work 幅と一致
        assert_eq!(l.width() + c.width() + r.width(), work.width());
    }

    #[test]
    fn sub_vertical_bottom_half() {
        let work = Rect { left: 0, top: 0, right: 800, bottom: 600 };
        assert_eq!(
            work.sub(0.0, 1.0, 0.5, 1.0),
            Rect { left: 0, top: 300, right: 800, bottom: 600 }
        );
    }

    #[test]
    fn approx_eq_respects_tolerance_on_every_edge() {
        let a = Rect { left: 0, top: 0, right: 100, bottom: 100 };
        // 全辺が許容内（差 0〜2、tol=2）。
        assert!(a.approx_eq(Rect { left: 2, top: -2, right: 98, bottom: 102 }, 2));
        // 1 辺でも許容を超えれば不一致（bottom の差 3 > tol=2）。
        assert!(!a.approx_eq(Rect { left: 0, top: 0, right: 100, bottom: 103 }, 2));
    }

    #[test]
    fn covers_exact_is_true() {
        // 全辺がちょうど一致（等号境界）→ 覆うとみなす。フルスクリーン判定の要。
        let mon = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        assert!(mon.covers(mon));
    }

    #[test]
    fn covers_larger_window_is_true() {
        let mon = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        // モニタからはみ出すウィンドウ（枠が外側）→ 覆う。
        let win = Rect { left: -8, top: -8, right: 1928, bottom: 1088 };
        assert!(win.covers(mon));
    }

    #[test]
    fn covers_smaller_window_is_false() {
        let mon = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        // 内側に収まるウィンドウ → 覆わない。
        let win = Rect { left: 100, top: 100, right: 800, bottom: 600 };
        assert!(!win.covers(mon));
    }

    #[test]
    fn covers_one_edge_short_is_false() {
        let mon = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
        // 3 辺は覆うが bottom が 1px 足りない → 覆わない（各辺は独立判定）。
        let win = Rect { left: 0, top: 0, right: 1920, bottom: 1079 };
        assert!(!win.covers(mon));
    }

    #[test]
    fn covers_on_nonzero_origin_monitor() {
        // セカンダリモニタ（原点 x=1920）でも正しく判定する。
        let mon = Rect { left: 1920, top: 0, right: 1920 + 2560, bottom: 1440 };
        assert!(mon.covers(mon));
        let inside = Rect { left: 2000, top: 100, right: 3000, bottom: 1000 };
        assert!(!inside.covers(mon));
    }
}
