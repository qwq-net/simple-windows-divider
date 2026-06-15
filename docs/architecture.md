# アーキテクチャ

Win32 に依存しない「純ロジック」と、Win32 API を直接呼ぶ「副作用層」を分離した構成です。純ロジックはどのプラットフォームでもユニットテストでき、副作用層は `#[cfg(windows)]` で Windows ターゲットのときだけコンパイルされます。

## レイヤ構成

純ロジック層（`windows` クレートに依存せず、テスト対象）:

- `layout::geometry` — 矩形型 `Rect` と、矩形を割合で切り出す分割プリミティブ。
- `layout::grid` — グリッド占有範囲 `GridSpan` と矢印操作（`step` / `fill_axis` / `estimate_span` / `clamp_to`）。
- `hotkey` — ホットキー文字列のパースと中立型（`Modifiers` / `Hotkey`）。
- `layouts` — 配置の学習データ。識別キー `WindowKey`（exe + class）と `(exe, class) → GridSpan` の記録・参照・保存。
- `action` — ホットキー設定をアクション割り当ての並びへ展開する。
- `window_style` — ウィンドウスタイル（`WS_*`）から「スナップ対象の普通のウィンドウか」を判定する述語。
- `config` — 設定 TOML の型・読み書き・パス解決。
- `fsutil` — ファイルの原子的書き込みなどの共通ヘルパ。

Win32 副作用層（Windows ターゲットのみ）:

- `win` — Win32 を直接呼ぶポート層。モニタ取得・ウィンドウ操作・スナップ無効化・ホットキー登録・イベントフック・介入可否判定・自動起動・単一インスタンス制御などを、用途ごとのファイルに分けています。
- `tray` — タスクトレイのアイコンとメニュー。
- `watcher` — 設定ファイルの変更監視。
- `app` — メッセージループと、すべてのメッセージのディスパッチ。機能 B（矢印操作）と機能 C（学習と自動復元）の処理もここに集約します。

純ロジックは自前の中立型（`Rect` や `Modifiers` など）だけを受け渡しします。`HWND` や `RECT` といった Win32 型との変換は `win::convert` に閉じ込めています。

## メッセージループ

中核は単一スレッド・単一メッセージループです（`app::App::message_loop`）。状態はすべて UI スレッドからのみ触るため、ロックは使いません。

ループは次のメッセージを捌きます。

- `WM_HOTKEY` — 機能 B（矢印キー）。
- `WM_TIMER` — 機能 C の遅延リトライ。
- `WM_APP_CONFIG_RELOAD` — 設定の再読み込み。`watcher` が別スレッドから `PostMessageW` で送ります。

`SetWinEventHook`（out-of-context）のコールバックは、対象のウィンドウハンドルをスレッドローカルのキューに積むだけにしています（コールバックを軽量に保つため）。実際の処理はループの各回末で `winevent::drain_events` から取り出して行います。トレイメニューの操作は、各回末に `Tray::poll` でグローバルチャネルから取り込みます。

## アプリの状態

`app::App` が全体の状態を保持します。主なものは次のとおりです。

- `config` / `config_path` — 現在の設定とそのパス。
- `enabled` / `auto_restore` — 機能全体の有効・無効と、自動復元の有効・無効。
- `spans` — ウィンドウごとのグリッド占有範囲。次回の矢印操作の起点に使う一時状態です。
- `learned` / `layouts_path` — 学習した `(exe, class) → 占有範囲` とその保存先。`spans` とは別物です。
- `actions` / `registered_ids` — 登録済みホットキーの id とアクション。
- `snap_backup` — 機能 A で退避したスナップ設定。
- `restore_jobs` / `next_timer_id` — 機能 C の遅延リトライの管理。

## DPI awareness の宣言

Per-Monitor v2 DPI awareness は、ビルド時にアプリケーションマニフェスト（`build.rs` が埋め込む）で宣言します。マニフェストが効かない実行形態に備え、起動時にも `dpi::set_per_monitor_v2_aware` で設定します。詳細は [build-and-test.md](build-and-test.md) を参照してください。
