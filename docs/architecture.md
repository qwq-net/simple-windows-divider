# アーキテクチャ

Win32 に依存しない「純ロジック」と、Win32 API を直接呼ぶ「副作用層」を分離した構成です。純ロジックはどの OS でもユニットテストでき、副作用層は `#[cfg(windows)]` で Windows ターゲットのときだけコンパイルされます。

## レイヤ構成

純ロジック層（`windows` クレートに依存せず、テスト対象）:

- `layout::geometry`：矩形型 `Rect`（割合切り出し `sub`・包含判定 `covers`）と分割プリミティブ。
- `layout::grid`：グリッド占有範囲 `GridSpan` と矢印操作（`step` / `fill_axis` / `estimate_span`）、方向 `Family` の軸・反対方向。
- `layout::placement`：そのモニタで使う分割数（設定値かアスペクト比自動か）の決定。
- `hotkey`：ホットキー文字列のパースと中立型（`Modifiers` / `Hotkey`）。
- `action`：ホットキー設定をアクション割り当ての並びへ展開する。
- `window_style`：ウィンドウスタイル（`WS_*`）から「スナップ対象の普通のウィンドウか」を判定する述語。
- `config`：設定 TOML の型・読み書き・パス解決。
- `fsutil`：ファイルの原子的書き込みなどの共通ヘルパ。

Win32 副作用層（Windows ターゲットのみ）:

- `win`：Win32 を直接呼ぶポート層。モニタ取得・ウィンドウ操作・スナップ無効化・ホットキー登録・介入可否判定・自動起動・単一インスタンス制御を、用途ごとのファイルに分けています。
- `tray`：タスクトレイのアイコンとメニュー。
- `watcher`：設定ファイルの変更監視。
- `app`：メッセージループと全メッセージのディスパッチ。`app::App` は状態の所有とディスパッチに徹し、実処理はサブモジュール（`arrange`＝機能 B、`hotkeys`＝ホットキー登録レジストリ、`snap_control`＝機能 A と退避ファイルの永続化）へ委譲します。

純ロジックは自前の中立型（`Rect` や `Modifiers` など）だけを受け渡しします。`HWND` や `RECT` といった Win32 型との変換は `win::convert` に閉じ込めています。

## メッセージループ

中核は単一スレッド・単一メッセージループです（`app::App::message_loop`）。状態はすべて UI スレッドからのみ触るため、ロックは使いません。捌くメッセージは `WM_HOTKEY`（機能 B）と `WM_APP_CONFIG_RELOAD`（設定の再読み込み。`watcher` が別スレッドから `PostMessageW` で送ります）の 2 つで、トレイメニューの操作は各回末に `Tray::poll` でグローバルチャネルから取り込みます。

## DPI awareness の宣言

Per-Monitor v2 DPI awareness は、ビルド時にアプリケーションマニフェスト（`build.rs` が埋め込む）で宣言します。マニフェストが効かない実行形態に備え、起動時にも `dpi::set_per_monitor_v2_aware` で設定します。詳細は [build-and-test.md](build-and-test.md) を参照してください。
