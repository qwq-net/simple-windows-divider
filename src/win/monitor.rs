//! モニタ（作業領域）の取得。Per-Monitor v2 aware 前提で物理ピクセル座標を返す。

use std::mem::size_of;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
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
