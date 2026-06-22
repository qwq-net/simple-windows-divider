//! windows-divider
//!
//! Windows 11 常駐型ウィンドウ管理ユーティリティのライブラリクレート。
//!
//! 設計の要は「純ロジック」と「Win32 副作用」の分離である:
//! - 純ロジック（座標計算・グリッド占有範囲の操作・配置の学習データ・ホットキー文字列パース）は
//!   `windows` クレートに一切依存せず、どのプラットフォームでも `cargo test` できる。
//! - Win32 を直接叩くポート層（`win` 以下）とそれを使う配線（`app` / `tray` / `watcher`）は
//!   `#[cfg(windows)]` でゲートし、Windows ターゲットでのみコンパイルされる。
//!
//! 純ロジックは自前の中立ドメイン型（[`layout::geometry::Rect`] や
//! [`hotkey::Modifiers`] など）だけを受け渡しし、`HWND` や `HOT_KEY_MODIFIERS` といった
//! Win32 型は Windows 側の薄いアダプタでのみ変換する。

// ── 純ロジック（全プラットフォーム共通・ユニットテスト対象） ──
pub mod action;
pub mod config;
pub mod fsutil;
pub mod hotkey;
pub mod layout;
pub mod layouts;
pub mod occupancy;
pub mod window_style;

// ── Win32 副作用ポート層・配線（Windows ターゲットでのみコンパイル） ──
#[cfg(windows)]
pub mod app;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod watcher;
#[cfg(windows)]
pub mod win;
