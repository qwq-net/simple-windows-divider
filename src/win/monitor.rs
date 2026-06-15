//! モニタ（作業領域）の取得。Per-Monitor v2 aware 前提で物理ピクセル座標を返す。

use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};

use super::convert::from_rect;
use crate::layout::geometry::Rect;

/// 1 つのモニタの情報。
#[derive(Clone, Copy, Debug)]
pub struct MonitorInfo {
    /// タスクバー等を除いた作業領域（ウィンドウ配置の基準）。
    pub work_area: Rect,
    /// モニタ全体の矩形（フルスクリーン判定に使う）。
    pub full: Rect,
}

fn monitor_info(hmon: HMONITOR) -> Option<MonitorInfo> {
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GetMonitorInfoW(hmon, &mut mi) };
    if !ok.as_bool() {
        return None;
    }
    Some(MonitorInfo {
        work_area: from_rect(mi.rcWork),
        full: from_rect(mi.rcMonitor),
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

unsafe extern "system" fn enum_proc(hmon: HMONITOR, _hdc: HDC, _rc: *mut RECT, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
    if let Some(info) = monitor_info(hmon) {
        out.push(info);
    }
    BOOL(1) // TRUE: 列挙を継続する
}
