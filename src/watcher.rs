//! 設定ファイルの変更監視（デバウンス付き）。
//!
//! 変更を検知したら UI スレッドのメッセージウィンドウへ [`WM_APP_CONFIG_RELOAD`] を `PostMessageW` する。
//! 監視は親ディレクトリに対して行い、変更パスが設定ファイル名と一致するときだけ通知する
//! （ログや退避ファイルの書き込みで誤発火しないようにする）。

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::win::message_window::WM_APP_CONFIG_RELOAD;

/// 設定監視ハンドル。保持し続ける限り監視が継続し、drop で停止する。
pub type ConfigWatcher = Debouncer<RecommendedWatcher, RecommendedCache>;

/// `config_path` の変更を監視し、検知時に `hwnd` へ再読込メッセージを送る監視を開始する。
///
/// 戻り値（[`ConfigWatcher`]）を保持し続けること（drop すると監視が止まる）。開始に失敗したら `None`。
pub fn watch_config(config_path: &Path, hwnd: HWND) -> Option<ConfigWatcher> {
    // HWND は Send でないため生のポインタ値で渡し、コールバック側で復元する（同一プロセス内宛て）。
    let hwnd_raw = hwnd.0 as isize;
    let cfg_name: Option<OsString> = config_path.file_name().map(|s| s.to_os_string());

    let mut debouncer = new_debouncer(
        Duration::from_millis(400),
        None,
        move |res: DebounceEventResult| {
            let Ok(events) = res else {
                return;
            };
            let target = cfg_name.as_deref();
            let hit = events
                .iter()
                .any(|e| e.paths.iter().any(|p| p.file_name() == target));
            if hit {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_raw as *mut core::ffi::c_void)),
                        WM_APP_CONFIG_RELOAD,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        },
    )
    .ok()?;

    let watch_target = config_path.parent().unwrap_or(config_path);
    debouncer
        .watch(watch_target, RecursiveMode::NonRecursive)
        .ok()?;
    Some(debouncer)
}
