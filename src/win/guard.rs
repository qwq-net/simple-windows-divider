//! アンチチート安全のための介入可否判定（機能 B/C 共通の関門）。

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
/// ゲーム等を壊さないため保守的に倒す。ハンドルを開かずに判定できる条件を先に置き、
/// 最後にだけプロセスを開く（ゲームプロセスへハンドルを開く頻度を下げる）:
/// - 無効ウィンドウ → `SkipInvalid`。
/// - `skip_when_fullscreen` かつフルスクリーン/排他状態 → `SkipFullscreen`。
/// - `skip_non_tileable` かつタイトルバーもリサイズ枠も無いウィンドウ（ボーダーレス全画面・オーバーレイ等）
///   → `SkipNonTileable`。スタイルだけで判定でき、未知のゲームも名前リスト無しに避けられる。
/// - 所有 exe が除外リストにある → `SkipExcluded`（ここで初めて `OpenProcess` を使う）。
///
/// 機能 B（ホットキー時）と機能 C（イベント時）の両方が必ずこれを通す。昇格ウィンドウは
/// ここでは弾かず、`SetWindowPos` の失敗（ACCESS_DENIED）として握り潰す方針（事前判定が不確実なため）。
pub fn should_intervene(hwnd: HWND, exclusions: &Exclusions) -> Interventability {
    if hwnd.0.is_null() {
        return Interventability::SkipInvalid;
    }
    if exclusions.skip_when_fullscreen && is_fullscreen_context(hwnd) {
        return Interventability::SkipFullscreen;
    }
    if exclusions.skip_non_tileable && !crate::window_style::is_tileable(window_ops::window_style_bits(hwnd)) {
        return Interventability::SkipNonTileable;
    }
    if let Some(key) = window_info::window_key(hwnd) {
        if exclusions
            .processes
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&key.exe))
        {
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
        if win.left <= mon.full.left
            && win.top <= mon.full.top
            && win.right >= mon.full.right
            && win.bottom >= mon.full.bottom
        {
            return true;
        }
    }
    false
}
