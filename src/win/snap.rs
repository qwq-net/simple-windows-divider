//! 機能 A: Windows 標準スナップ（Aero Snap / Snap Assist）の無効化と復元。
//!
//! レジストリ（HKCU）の現値を退避してから 0 に書き換え、`SystemParametersInfoW(SPI_SETWINARRANGING)`
//! で実行中システムへも反映・ブロードキャストする。復元は退避値へ戻す（冪等）。
//! [`SnapBackup`] は serde 可能で、クラッシュ復旧用にファイルへ永続化できる。

use serde::{Deserialize, Serialize};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETWINARRANGING,
};
use windows_registry::CURRENT_USER;

const DESKTOP: &str = r"Control Panel\Desktop";
const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const ARRANGING: &str = "WindowArrangementActive";
/// Snap Assist 系の DWORD 値（Win11 ビルドにより一部存在しないことがある＝ best-effort）。
const ASSIST_VALUES: &[&str] = &["SnapAssist", "EnableSnapAssistFlyout", "SnapFill", "JointResize"];

/// 無効化前の元の値。復元に必要な分だけ持つ。
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct SnapBackup {
    /// `WindowArrangementActive`（"0"/"1"）の元値。キーが無ければ `None`。
    pub window_arranging: Option<String>,
    /// Snap Assist 系 DWORD の (名前, 元値)。元値が無い（キー未存在）なら `None`。
    pub snap_assist: Vec<(String, Option<u32>)>,
}

/// 標準スナップを無効化し、元の値を退避して返す。`disable_assist` が真なら Snap Assist 系も 0 にする。
pub fn disable_snap(disable_assist: bool) -> SnapBackup {
    let mut backup = SnapBackup::default();

    if let Ok(key) = CURRENT_USER.create(DESKTOP) {
        backup.window_arranging = key.get_string(ARRANGING).ok();
        let _ = key.set_string(ARRANGING, "0");
    }
    broadcast_winarranging(false);

    if disable_assist {
        if let Ok(key) = CURRENT_USER.create(ADVANCED) {
            for name in ASSIST_VALUES {
                let prev = key.get_u32(name).ok();
                backup.snap_assist.push(((*name).to_string(), prev));
                let _ = key.set_u32(name, 0);
            }
        }
    }
    backup
}

/// 退避値からスナップ設定を元に戻す（冪等）。元が無かった値は削除する。
pub fn restore_snap(backup: &SnapBackup) {
    if let Ok(key) = CURRENT_USER.create(DESKTOP) {
        match &backup.window_arranging {
            Some(v) => {
                let _ = key.set_string(ARRANGING, v);
            }
            None => {
                let _ = key.remove_value(ARRANGING);
            }
        }
    }
    // 元値が "0"（ユーザーが元から無効化）以外なら有効へ戻す。
    let restore_enabled = backup.window_arranging.as_deref() != Some("0");
    broadcast_winarranging(restore_enabled);

    if let Ok(key) = CURRENT_USER.create(ADVANCED) {
        for (name, prev) in &backup.snap_assist {
            match prev {
                Some(v) => {
                    let _ = key.set_u32(name, *v);
                }
                None => {
                    let _ = key.remove_value(name);
                }
            }
        }
    }
}

/// 実行中システムの「ウィンドウ配置（Aero Snap）」設定を切り替えてブロードキャストする。
///
/// `pvParam` がブール値そのもの（FALSE=null, TRUE=非0）を表す SPI のイディオムに従う。
/// `SPIF_UPDATEINIFILE` でレジストリにも反映、`SPIF_SENDCHANGE` で `WM_SETTINGCHANGE` を配信する。
fn broadcast_winarranging(enabled: bool) {
    // ここでの 1 はダングリングポインタではなく「値 TRUE」を PVOID へ符号化したもの（上記の SPI イディオム）。
    // clippy の dangling_mut 提案は c_void の alignment が 1 である偶然に依存し意図を曖昧にするため使わない。
    #[allow(clippy::manual_dangling_ptr)]
    let pv: *mut core::ffi::c_void = if enabled {
        1usize as *mut core::ffi::c_void
    } else {
        core::ptr::null_mut()
    };
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_SETWINARRANGING,
            0,
            Some(pv),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        );
    }
}
