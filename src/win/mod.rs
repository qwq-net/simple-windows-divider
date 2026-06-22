//! Win32 を直接叩く副作用ポート層（Windows ターゲット専用）。
//!
//! 純ロジック（[`crate::layout`] / [`crate::hotkey`] / [`crate::layouts`]）の中立型と Win32 型の橋渡しを行う。
//! 使う API は **文書化済みのユーザーモード関数のみ**（`RegisterHotKey` / `SetWinEventHook`(out-of-context) /
//! `SetWindowPos` / `EnumWindows` 等）。他プロセスへの DLL 注入・メモリ操作・低レベル入力フックは使わない。

pub mod autostart;
pub mod convert;
pub mod dpi;
pub mod guard;
pub mod hotkey;
pub mod message_window;
pub mod monitor;
pub mod singleton;
pub mod snap;
pub mod winevent;
pub mod window_info;
pub mod window_ops;
