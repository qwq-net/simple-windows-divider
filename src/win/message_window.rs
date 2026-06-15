//! 非表示のメッセージ専用ウィンドウ（HWND_MESSAGE）。
//!
//! ホットキー（`WM_HOTKEY`）・タイマ（`WM_TIMER`）・スレッド間通知（`WM_APP_*`）の宛先となる HWND を提供する。
//! 主要メッセージはメッセージループ本体が `msg.message` を見て直接捌くため、ここの WndProc は最小限
//! （破棄時にループを終わらせるだけ）に留める。

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, PostQuitMessage, RegisterClassW, HWND_MESSAGE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_DESTROY, WNDCLASSW,
};

/// 設定ファイル変更を別スレッドから UI スレッドへ伝えるためのメッセージ。
pub const WM_APP_CONFIG_RELOAD: u32 = WM_APP + 1;

/// メッセージ専用ウィンドウを生成して HWND を返す。
///
/// 同一クラス名で二重登録しても `RegisterClassW` の失敗は無視する（再登録は無害）。
/// 生成自体に失敗したら `Err`。
pub fn create() -> windows::core::Result<HWND> {
    unsafe {
        let hmodule = GetModuleHandleW(None)?;
        let hinstance = HINSTANCE(hmodule.0);
        let class_name = w!("windows_divider_msg_window");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);

        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("windows-divider"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        )
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
