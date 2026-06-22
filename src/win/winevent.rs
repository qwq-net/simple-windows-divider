//! 機能 C: ウィンドウイベント監視（`SetWinEventHook`, out-of-context）。
//!
//! トップレベルウィンドウの「生成」と「ユーザーのドラッグ/リサイズ完了」を監視し、対象イベントを
//! スレッドローカルのキューに積むだけにする（コールバックは超軽量に保ち、再入・大量発火に耐える）。
//! 実処理はメッセージループ本体が [`drain_events`] で取り出して行う。out-of-context なので
//! 他プロセスへ DLL を注入しない。
//!
//! `EVENT_SYSTEM_MOVESIZEEND` はユーザーがモーダルな移動/リサイズ操作を終えたときのみ発火し、
//! 自プロセスの `SetWindowPos` では発火しない。
//!
//! 表示（SHOW）やフォアグラウンド化（FOREGROUND）は監視しない。これらは既存ウィンドウのドラッグや
//! フォーカス移動でも発火し、ユーザーが手で動かしているウィンドウを学習配置へ引き戻してしまうため。

use std::cell::RefCell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, EVENT_SYSTEM_MOVESIZEEND, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use super::convert::hwnd_to_u64;

/// メッセージループへ渡すウィンドウイベント。`u64` は対象ウィンドウ（hwnd）。
pub enum WinEvent {
    /// トップレベルウィンドウの生成。復元の契機。
    Created(u64),
    /// ユーザーによるドラッグ/リサイズの完了。所属解除の判定契機。
    MoveSizeEnd(u64),
}

thread_local! {
    static EVENT_QUEUE: RefCell<Vec<WinEvent>> = const { RefCell::new(Vec::new()) };
}

/// コールバックが積んだイベントを全件取り出し、キューを空にする。メッセージループ各回で呼ぶ。
pub fn drain_events() -> Vec<WinEvent> {
    EVENT_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// 設置したフック群。Drop で全解除する。設置したスレッドでメッセージを回している間だけイベントが届く。
pub struct WinEventHooks {
    hooks: Vec<HWINEVENTHOOK>,
}

impl Drop for WinEventHooks {
    fn drop(&mut self) {
        for h in self.hooks.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(h);
            }
        }
    }
}

/// ウィンドウ生成（`EVENT_OBJECT_CREATE`）と移動/リサイズ完了（`EVENT_SYSTEM_MOVESIZEEND`）の
/// 監視フックを設置する。メインスレッドから呼ぶこと。
///
/// 2 種のイベントはレンジが離れているため（0x8000 と 0x000B）、フックを 2 つ設置する。
/// 監視は生成と MOVESIZEEND だけ。表示・フォアグラウンド化を監視しないのは、既存ウィンドウのドラッグや
/// フォーカス移動でそれらが発火し、手動操作中のウィンドウを学習配置へ引き戻してしまうため。
/// 生成直後のサイズ未確定は、呼び出し側の遅延リトライ（`RESTORE_DELAY_MS` ほか）で吸収する。
pub fn install() -> WinEventHooks {
    // OUTOFCONTEXT: 対象プロセスへ DLL を注入しない（コールバックは自プロセス内で動く）。
    // SKIPOWNPROCESS: 自分のウィンドウ起因のイベントを最初から受け取らない。
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    let mut hooks = Vec::new();
    for (lo, hi) in [
        (EVENT_OBJECT_CREATE, EVENT_OBJECT_CREATE),
        (EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND),
    ] {
        let h = unsafe { SetWinEventHook(lo, hi, None, Some(win_event_proc), 0, 0, flags) };
        if !h.is_invalid() {
            hooks.push(h);
        }
    }
    WinEventHooks { hooks }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // トップレベルウィンドウ自身のイベントのみ対象（子要素・非ウィンドウは無視）。
    if id_object != OBJID_WINDOW.0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    let h = hwnd_to_u64(hwnd);
    let ev = match event {
        EVENT_OBJECT_CREATE => WinEvent::Created(h),
        EVENT_SYSTEM_MOVESIZEEND => WinEvent::MoveSizeEnd(h),
        _ => return,
    };
    EVENT_QUEUE.with(|q| q.borrow_mut().push(ev));
}
