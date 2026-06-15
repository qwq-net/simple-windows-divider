//! プロセスの DPI awareness 設定。

use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

/// プロセスを Per-Monitor v2 DPI aware にする。
///
/// 第一の手段はアプリケーションマニフェスト宣言で、本関数はその保険（マニフェストが効かない
/// 実行形態向け）。UI 生成より前に一度だけ呼ぶ。失敗は無視する（best-effort）。
pub fn set_per_monitor_v2_aware() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
