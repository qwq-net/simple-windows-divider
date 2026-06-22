//! モニタ（作業領域）の取得。Per-Monitor v2 aware 前提で物理ピクセル座標を返す。

use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFOEXW,
    MONITOR_DEFAULTTONEAREST,
};

use super::convert::from_rect;
use crate::layout::geometry::Rect;

/// 1 つのモニタの情報。
#[derive(Clone, Debug)]
pub struct MonitorInfo {
    /// タスクバー等を除いた作業領域（ウィンドウ配置の基準）。
    pub work_area: Rect,
    /// モニタ全体の矩形（フルスクリーン判定に使う）。
    pub full: Rect,
    /// ディスプレイ名（`\\.\DISPLAYn`）。学習スロットの同定に使う。
    pub display: String,
}

fn monitor_info(hmon: HMONITOR) -> Option<MonitorInfo> {
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // MONITORINFOEXW の先頭は monitorInfo。cbSize を EXW サイズにしておくと szDevice も埋まる。
    let ok = unsafe { GetMonitorInfoW(hmon, &mut mi.monitorInfo) };
    if !ok.as_bool() {
        return None;
    }
    let display = String::from_utf16_lossy(&mi.szDevice);
    let display = display.trim_end_matches('\0').to_string();
    Some(MonitorInfo {
        work_area: from_rect(mi.monitorInfo.rcWork),
        full: from_rect(mi.monitorInfo.rcMonitor),
        display,
    })
}

/// 指定ウィンドウが属する（最も重なる）モニタの情報。取得失敗時 `None`。
pub fn monitor_for_window(hwnd: HWND) -> Option<MonitorInfo> {
    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitor_info(hmon)
}

/// 接続中の全モニタ情報を列挙する。隣接モニタ判定（モニタ間移動）に使う。情報取得に失敗したモニタは除く。
pub fn enumerate() -> Vec<MonitorInfo> {
    let mut out: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_proc),
            LPARAM(&mut out as *mut Vec<MonitorInfo> as isize),
        );
    }
    out
}

/// ディスプレイ名（`\\.\DISPLAYn`）に一致するモニタを返す。該当が無ければ `None`（復元時に使う）。
pub fn monitor_by_name(name: &str) -> Option<MonitorInfo> {
    enumerate().into_iter().find(|m| m.display == name)
}

unsafe extern "system" fn enum_proc(hmon: HMONITOR, _hdc: HDC, _rc: *mut RECT, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
    if let Some(info) = monitor_info(hmon) {
        out.push(info);
    }
    BOOL(1) // TRUE: 列挙を継続する
}
