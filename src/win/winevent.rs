//! 機能 C: ウィンドウイベント監視（`SetWinEventHook`, out-of-context）。
//!
//! トップレベルウィンドウの「生成」だけを監視し、対象 HWND をスレッドローカルのキューに積むだけにする
//! （コールバックは超軽量に保ち、再入・大量発火に耐える）。実処理はメッセージループ本体が
//! [`drain_events`] で取り出して行う。out-of-context なので他プロセスへ DLL を注入しない。
//!
//! 表示（SHOW）やフォアグラウンド化（FOREGROUND）は監視しない。これらは既存ウィンドウのドラッグや
//! フォーカス移動でも発火し、ユーザーが手で動かしているウィンドウを学習配置へ引き戻してしまうため。
//! 自動配置の対象は「新しく作られたウィンドウ」だけに限る。

use std::cell::RefCell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
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

/// ウィンドウ生成（`EVENT_OBJECT_CREATE`）の監視フックを設置する。メインスレッドから呼ぶこと。
///
/// 監視するのは生成だけ。表示・フォアグラウンド化を監視しないのは、既存ウィンドウのドラッグや
/// フォーカス移動でそれらが発火し、手動操作中のウィンドウを学習配置へ引き戻してしまうため。
/// 生成直後のサイズ未確定は、呼び出し側の遅延リトライ（`RESTORE_DELAY_MS` ほか）で吸収する。
pub fn install() -> WinEventHooks {
    // OUTOFCONTEXT: 対象プロセスへ DLL を注入しない（コールバックは自プロセス内で動く）。
    // SKIPOWNPROCESS: 自分のウィンドウ起因のイベントを最初から受け取らない。
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    let h = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_CREATE,
            None,
            Some(win_event_proc),
            0,
            0,
            flags,
        )
    };
    let mut hooks = Vec::new();
    if !h.is_invalid() {
        hooks.push(h);
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
