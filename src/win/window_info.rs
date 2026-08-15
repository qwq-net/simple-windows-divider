//! ウィンドウの所有プロセス情報（exe 名）取得。
//!
//! プロセス情報は `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` ＋ `QueryFullProcessImageNameW` の
//! 範囲に留め、`ReadProcessMemory` 等の侵襲的 API は一切使わない（アンチチート安全）。

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// `hwnd` の所有プロセスの実行ファイル名（basename・小文字）。取得不可なら `None`
/// （対象プロセスにアクセスできない／既に消えている等）。除外プロセス判定（[`crate::win::guard`]）に使う。
pub fn window_exe(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    // ロングパス対応環境では exe パスが 260 文字を超えうるため余裕を持たせる（取得漏れによる除外ミスを避ける）。
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
