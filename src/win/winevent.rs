//! 機能 C: ウィンドウイベント監視（`SetWinEventHook`, out-of-context）。
//!
//! 生成/表示/フォアグラウンド化を監視し、対象 HWND を**スレッドローカルのキューに積むだけ**にする
//! （コールバックは超軽量に保ち、再入・大量発火に耐える）。実処理はメッセージループ本体が
//! [`drain_events`] で取り出して行う。out-of-context なので他プロセスへ DLL を注入しない。

use std::cell::RefCell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use super::convert::hwnd_to_u64;

thread_local! {
    static EVENT_QUEUE: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// コールバックが積んだ HWND（u64）を全件取り出し、キューを空にする。メッセージループ各回で呼ぶ。
pub fn drain_events() -> Vec<u64> {
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

/// 生成・表示・フォアグラウンドの監視フックを設置する。メインスレッドから呼ぶこと。
pub fn install() -> WinEventHooks {
    let mut hooks = Vec::new();
    for ev in [
        EVENT_OBJECT_CREATE,
        EVENT_OBJECT_SHOW,
        EVENT_SYSTEM_FOREGROUND,
    ] {
        // OUTOFCONTEXT: 対象プロセスへ DLL を注入しない（コールバックは自プロセス内で動く）。
        // SKIPOWNPROCESS: 自分のウィンドウ起因のイベントを最初から受け取らない。
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let h = unsafe { SetWinEventHook(ev, ev, None, Some(win_event_proc), 0, 0, flags) };
        if !h.is_invalid() {
            hooks.push(h);
        }
    }
    WinEventHooks { hooks }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
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
    EVENT_QUEUE.with(|q| q.borrow_mut().push(h));
}
