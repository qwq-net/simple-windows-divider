//! 機能 B: 矢印ホットキーによるグリッド配置。
//!
//! [`on_arrow`] が入口で、占有範囲を 1 セル動かす（端では隣モニタへ送る）か、反対方向同時押しで軸をフル化する。
//! 起点の決定は [`super::store::LayoutStore`] に委ね、確定した配置はそこへ学習させる。介入してよいかの判定
//! （[`crate::win::guard`]）は呼び出し側 [`super::App`] が済ませてからここへ入る前提。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

use crate::config::Config;
use crate::layout::geometry::Rect;
use crate::layout::grid::{self, Family, GridSpan};
use crate::layout::placement;
use crate::layouts::Slot;
use crate::win::{convert, monitor, window_info, window_ops};

use super::store::LayoutStore;

// 矢印キーの仮想キーコード（反対方向の同時押し＝最大化を判定するため）。
const VK_LEFT: i32 = 0x25;
const VK_UP: i32 = 0x26;
const VK_RIGHT: i32 = 0x27;
const VK_DOWN: i32 = 0x28;

/// 矢印 1 押下を処理する。押下の瞬間に反対方向キーも押されていれば同時押し＝最大化、そうでなければ単独移動。
///
/// `hwnd` は介入可否判定（[`crate::win::guard::should_intervene`]）を通過済みの対象ウィンドウ、`hwnd_msg` は保存
/// タイマの宛先となるメッセージ専用ウィンドウ。待ち時間を入れず即座に判定するため、単独押しに遅延は乗らない
/// （反対方向は [`GetAsyncKeyState`] で判定）。
pub fn on_arrow(store: &mut LayoutStore, cfg: &Config, hwnd_msg: HWND, hwnd: HWND, family: Family) {
    if opposite_arrow_down(family) {
        apply_axis_full(store, cfg, hwnd_msg, hwnd, family.is_horizontal());
    } else {
        apply_arrow(store, cfg, hwnd_msg, hwnd, family);
    }
}

/// 矢印キー: ウィンドウのグリッド占有範囲を 1 セル動かして再配置する。
///
/// 端でこれ以上動けない（占有が変わらない＝端セルかつその軸が最小幅）とき、操作方向に隣モニタが
/// あればそのモニタへ送る（反対側の端セルに着地）。隣が無ければ従来どおり（実質無変化）。
fn apply_arrow(store: &mut LayoutStore, cfg: &Config, hwnd_msg: HWND, hwnd: HWND, family: Family) {
    let Some((base, work, cols, rows)) = prepare_base(store, cfg, hwnd) else {
        return;
    };
    let next = grid::step(base, family, cols, rows);
    if next == base && move_to_adjacent_monitor(store, hwnd_msg, hwnd, base, family, cols, rows) {
        return;
    }
    set_span(store, hwnd_msg, hwnd, next, cols, rows, work);
}

/// `family` 方向の隣モニタへウィンドウを送る。隣が無ければ何もせず `false`。
///
/// 着地は反対側の端セル（[`grid::cross_edge_span`]）。移動先モニタの作業領域へ適用し、その配置を学習する。
fn move_to_adjacent_monitor(
    store: &mut LayoutStore,
    hwnd_msg: HWND,
    hwnd: HWND,
    base: GridSpan,
    family: Family,
    cols: u32,
    rows: u32,
) -> bool {
    let Some(cur) = monitor::monitor_for_window(hwnd) else {
        return false;
    };
    let monitors = monitor::enumerate();
    let fulls: Vec<Rect> = monitors.iter().map(|m| m.full).collect();
    let Some(adj) = grid::adjacent_monitor(&fulls, cur.full, family) else {
        return false;
    };
    let landing = grid::cross_edge_span(base, family, cols, rows);
    set_span(store, hwnd_msg, hwnd, landing, cols, rows, monitors[adj].work_area);
    true
}

/// 反対方向同時押し: 押した方向の軸だけを全幅にする（←→=横軸フル・行維持／↑↓=縦軸フル・列維持）。
///
/// 現在の占有を起点にもう一方の軸は保つため、横軸フル→縦軸フルの 2 ステップで全画面になる。
fn apply_axis_full(store: &mut LayoutStore, cfg: &Config, hwnd_msg: HWND, hwnd: HWND, horizontal: bool) {
    let Some((base, work, cols, rows)) = prepare_base(store, cfg, hwnd) else {
        return;
    };
    let next = grid::fill_axis(base, horizontal, cols, rows);
    set_span(store, hwnd_msg, hwnd, next, cols, rows, work);
}

/// 操作の起点となる占有範囲・作業領域・分割数を用意する。
///
/// 分割数はそのモニタから決める（[`grid_dims`]。自動判定が有効ならアスペクト比から）。OS 最大化中のウィンドウは
/// 全グリッド占有を起点にし（最大化から ← で `■■□` などになる）、最大化は解除する。
fn prepare_base(store: &LayoutStore, cfg: &Config, hwnd: HWND) -> Option<(GridSpan, Rect, u32, u32)> {
    let mon = monitor::monitor_for_window(hwnd)?;
    let work = mon.work_area;
    let (cols, rows) = grid_dims(cfg, &mon);
    let was_maximized = window_ops::is_maximized(hwnd);
    window_ops::restore_if_maximized(hwnd);
    let base = if was_maximized {
        GridSpan::full(cols, rows)
    } else {
        let current = window_ops::window_visible_rect(hwnd).unwrap_or(work);
        store.span_for(convert::hwnd_to_u64(hwnd), work, current, cols, rows)
    };
    Some((base, work, cols, rows))
}

/// このモニタで使う分割数 `(列数, 行数)`。`auto_aspect` が有効なら解像度アスペクトから自動判定し、無効なら設定値を使う。
fn grid_dims(cfg: &Config, mon: &monitor::MonitorInfo) -> (u32, u32) {
    placement::grid_dims(
        cfg.grid.auto_aspect,
        mon.full,
        (cfg.grid.columns, cfg.grid.rows),
    )
}

/// 占有範囲 `span` を `work` 上の矩形へ適用し、その配置を学習する。能動的なウィンドウ操作はここに集約する。
fn set_span(store: &mut LayoutStore, hwnd_msg: HWND, hwnd: HWND, span: GridSpan, cols: u32, rows: u32, work: Rect) {
    if let Err(e) = window_ops::set_window_rect(hwnd, span.rect(cols, rows, work)) {
        tracing::warn!("set_span: set_window_rect failed: {e}");
    }
    learn(store, hwnd_msg, hwnd, span, cols, rows);
}

/// ユーザー操作で確定した占有範囲を、現在モニタの Slot として学習し所属を更新する。記録と保存は [`LayoutStore`] が担う。
fn learn(store: &mut LayoutStore, hwnd_msg: HWND, hwnd: HWND, span: GridSpan, cols: u32, rows: u32) {
    let Some(key) = window_info::window_key(hwnd) else { return };
    let Some(mon) = monitor::monitor_for_window(hwnd) else { return };
    let slot = Slot { display: mon.display, span, cols, rows };
    store.learn(hwnd_msg, convert::hwnd_to_u64(hwnd), key, slot);
}

/// 押された矢印 `family` の「反対方向キー」が今この瞬間に押されているか。
///
/// 反対方向の同時押し（←+→ / ↑+↓）を待ち時間ゼロで検出するために、[`GetAsyncKeyState`] の最上位ビット
/// （押下中なら i16 が負）を見る。低レベルフックや注入は使わない文書化 API。
fn opposite_arrow_down(family: Family) -> bool {
    let opposite_vk = match family.opposite() {
        Family::Left => VK_LEFT,
        Family::Right => VK_RIGHT,
        Family::Top => VK_UP,
        Family::Bottom => VK_DOWN,
    };
    unsafe { GetAsyncKeyState(opposite_vk) < 0 }
}
