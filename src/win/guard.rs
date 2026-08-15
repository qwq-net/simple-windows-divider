//! アンチチート安全のための介入可否判定（能動的なウィンドウ操作の関門）。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN,
};

use super::{monitor, window_info, window_ops};
use crate::config::schema::Exclusions;

/// 介入可否の判定結果。`Ok` 以外はそのウィンドウに一切触れない。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Interventability {
    Ok,
    SkipInvalid,
    SkipFullscreen,
    SkipNonTileable,
    SkipExcluded,
}

/// `hwnd` にウィンドウ操作を行ってよいか判定する。
///
/// 安価な順（無効 → スタイル → 全画面）にプロセスを開かない判定を先に通し、通過したものだけ
/// `OpenProcess` を伴うプロセス名の除外判定にかける（ゲームプロセスへハンドルを開く頻度を最小化する）:
/// - 無効ウィンドウ → `SkipInvalid`。
/// - `skip_non_tileable` かつタイトルバーもリサイズ枠も無いウィンドウ（ボーダーレス全画面・オーバーレイ等）
///   → `SkipNonTileable`。`GetWindowLongPtr` だけで判定でき最も安い。未知のゲームも名前リスト無しに避けられる。
/// - `skip_when_fullscreen` かつフルスクリーン/排他状態 → `SkipFullscreen`。
/// - 所有 exe が除外リストにある → `SkipExcluded`。
///
/// `Ok` 以外はそのウィンドウに一切触れない。能動的なウィンドウ操作（矢印ホットキー）は必ずこれを通す。
/// 昇格ウィンドウはここでは弾かず、`SetWindowPos` の失敗（ACCESS_DENIED）として握り潰す方針
/// （事前判定が不確実なため）。
pub fn should_intervene(hwnd: HWND, exclusions: &Exclusions) -> Interventability {
    if hwnd.0.is_null() {
        return Interventability::SkipInvalid;
    }
    if exclusions.skip_non_tileable
        && !crate::window_style::is_tileable(window_ops::window_style_bits(hwnd))
    {
        return Interventability::SkipNonTileable;
    }
    if exclusions.skip_when_fullscreen && is_fullscreen_context(hwnd) {
        return Interventability::SkipFullscreen;
    }
    if let Some(exe) = window_info::window_exe(hwnd) {
        if exclusions.excludes(&exe) {
            return Interventability::SkipExcluded;
        }
    }
    Interventability::Ok
}

/// フルスクリーン/排他状態か。システム通知状態と「モニタ全体を覆う矩形」の両面で判定する。
fn is_fullscreen_context(hwnd: HWND) -> bool {
    if let Ok(state) = unsafe { SHQueryUserNotificationState() } {
        // 全画面 D3D に加え、全画面ゲームが返しうる BUSY も介入回避とする。
        if state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_BUSY {
            return true;
        }
    }
    if let (Some(win), Some(mon)) = (
        window_ops::window_rect(hwnd),
        monitor::monitor_for_window(hwnd),
    ) {
        if win.covers(mon.full) {
            return true;
        }
    }
    false
}
