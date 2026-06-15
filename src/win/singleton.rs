//! 単一インスタンス制御（名前付きミューテックス）。

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// 保持している間だけ単一インスタンスを保証するガード。Drop でミューテックスを解放する
/// （= プロセス終了時）。
pub struct SingleInstance(HANDLE);

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// 単一インスタンスの取得を試みる。
///
/// 既に別プロセスが起動していれば `None`（呼び手は速やかに終了すべき）。取得できれば `Some(guard)` を返し、
/// `guard` が生きる間ミューテックスを保持する。
///
/// 注意: `CreateMutexW` は既存でも `Ok` を返すため、戻り値だけでは二重起動を判定できない。
/// 直後の `GetLastError() == ERROR_ALREADY_EXISTS` で判定する。
pub fn acquire() -> Option<SingleInstance> {
    let handle =
        unsafe { CreateMutexW(None, false, w!(r"Global\windows-divider-singleton-9f3a")) }.ok()?;
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already_running {
        unsafe {
            let _ = CloseHandle(handle);
        }
        None
    } else {
        Some(SingleInstance(handle))
    }
}
