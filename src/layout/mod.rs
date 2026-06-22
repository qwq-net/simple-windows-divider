//! ウィンドウ配置の純ロジック（Win32 非依存）。
//!
//! - [`geometry`] — 矩形型 [`geometry::Rect`] と分割プリミティブ。
//! - [`grid`] — グリッド占有範囲 [`grid::GridSpan`] と矢印キー操作 [`grid::step`]。
//! - [`placement`] — 配置の起点となる占有範囲・分割数の決定（[`placement::span_for`] / [`placement::grid_dims`]）。

pub mod geometry;
pub mod grid;
pub mod placement;
