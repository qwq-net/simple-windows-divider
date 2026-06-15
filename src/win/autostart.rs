//! ログオン時自動起動（HKCU の Run キー）。

use windows_registry::CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "windows-divider";

/// 自動起動が登録済みか。
pub fn is_enabled() -> bool {
    CURRENT_USER
        .open(RUN_KEY)
        .ok()
        .and_then(|k| k.get_string(VALUE_NAME).ok())
        .is_some()
}

/// 自動起動を有効/無効にする。有効時は現在の実行ファイルのフルパスを Run キーに書く。失敗は無視。
fn set(enabled: bool) {
    let Ok(key) = CURRENT_USER.create(RUN_KEY) else {
        return;
    };
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            let _ = key.set_string(VALUE_NAME, format!("\"{}\"", exe.display()));
        }
    } else {
        let _ = key.remove_value(VALUE_NAME);
    }
}

/// 自動起動の有効/無効を反転し、反転後の状態を返す。
pub fn toggle() -> bool {
    let next = !is_enabled();
    set(next);
    next
}
