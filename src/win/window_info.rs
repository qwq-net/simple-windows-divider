//! ウィンドウの識別情報（exe / class / AUMID）取得。
//!
//! プロセス情報は `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` ＋ `QueryFullProcessImageNameW` の
//! 範囲に留め、`ReadProcessMemory` 等の侵襲的 API は一切使わない（アンチチート安全）。
//! AUMID はシェルのプロパティストア（`SHGetPropertyStoreForWindow`）経由で取得するため
//! 対象プロセスを開かない。

use windows::core::{GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, PROPERTYKEY};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowThreadProcessId};

use crate::layouts::WindowKey;

/// System.AppUserModel.ID（{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, pid=5）
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
    pid: 5,
};

/// ウィンドウの識別情報を取得する。exe を解決できなければ `None`
/// （対象プロセスにアクセスできない／既に消えている等）。`exe` は basename を小文字化して返す。
///
/// `app_id` は AUMID（AppUserModelID）。取得できないウィンドウでは空文字列を返す
/// （多くの通常ウィンドウは AUMID を持たないため空になる）。
pub fn window_key(hwnd: HWND) -> Option<WindowKey> {
    let exe = process_exe_basename(hwnd)?;
    Some(WindowKey { exe, class: class_name(hwnd), app_id: window_app_id(hwnd) })
}

/// 指定ウィンドウの AUMID（AppUserModelID）。取得できなければ空文字列を返す。
///
/// シェルのプロパティストアを読むだけで、対象プロセスは開かない（注入・メモリアクセスなし）。
/// API 失敗・プロパティ未設定・型不一致のいずれでも空文字列を返す。AUMID はログに出さない。
fn window_app_id(hwnd: HWND) -> String {
    unsafe {
        let store: IPropertyStore = match SHGetPropertyStoreForWindow(hwnd) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };
        let value = match store.GetValue(&PKEY_APP_USER_MODEL_ID) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        // PROPVARIANT の Drop が PropVariantClear を呼ぶため、value はここでスコープを抜けると解放される。
        // PropVariantToStringAlloc は VT_LPWSTR / VT_BSTR 等を統一的に文字列化する。
        // 戻り値の PWSTR は CoTaskMemFree で解放する必要がある。
        let pwstr: PWSTR = match PropVariantToStringAlloc(&value) {
            Ok(p) => p,
            Err(_) => return String::new(),
        };
        let result = pwstr.to_string().unwrap_or_default();
        CoTaskMemFree(Some(pwstr.as_ptr() as *const _));
        result
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// 所有プロセスの実行ファイル名（basename・小文字）。取得不可なら `None`。
fn process_exe_basename(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    // ロングパス対応環境では exe パスが 260 文字を超えうるため余裕を持たせる（取得漏れによる
    // 除外・学習照合のミスを避ける）。
    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let res = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size)
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    res.ok()?;
    let full = String::from_utf16_lossy(&buf[..size as usize]);
    let base = full.rsplit(['\\', '/']).next().unwrap_or(&full);
    Some(base.to_ascii_lowercase())
}
