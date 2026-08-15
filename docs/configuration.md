# 設定

## 場所と読み込み

- 設定ファイルは `%APPDATA%\windows-divider\config.toml` です。初回起動時に既定値で自動生成します。
- すべての項目は省略でき、省略した項目には既定値が使われます。
- 編集して保存すると自動で再読み込みされます（トレイの「設定を再読み込み」でも反映できます）。
- ログは `%APPDATA%\windows-divider\windows-divider.log` に出力します（日次ローテーション）。

書式の参考として、リポジトリ直下に [config.example.toml](../config.example.toml) を置いています。

## 各セクション

### `[general]`

| キー | 既定 | 内容 |
|---|---|---|
| `enabled` | `true` | 機能全体の有効・無効。false の間は標準スナップを復元し、介入しません。 |
| `disable_snap` | `true` | 機能 A: Windows 標準スナップ（Aero Snap）を無効化するか。 |
| `disable_snap_assist` | `true` | 機能 A: Snap Assist 系のレジストリも無効化するか（best-effort）。 |

### `[grid]`

| キー | 既定 | 内容 |
|---|---|---|
| `columns` | `3` | 列数（横に並ぶセル数）。左右キーが動かす軸。 |
| `rows` | `2` | 行数（縦に並ぶセル数）。上下キーが動かす軸。 |
| `auto_aspect` | `false` | 真なら、各ウィンドウが今いるモニタの解像度アスペクト比で分割数を自動判定する（`columns`/`rows` は使わない）。トレイの「アスペクト比で自動分割」でも切り替えられる。 |

既定はウルトラワイド向けの 3 列 × 2 行です。グリッド操作の詳細は [features.md](features.md) を参照してください。

### `[hotkeys]`

矢印 4 方向のホットキーを文字列で指定します（既定は `Ctrl+Alt+Left` など）。書式は次のとおりです。

- トークンは `+` 区切り。前後の空白は無視し、大文字小文字は区別しません。
- 修飾キー：`Ctrl`（= `Control`）、`Alt`、`Shift`、`Win`（= `Windows` / `Meta` / `Super` / `Cmd`）。順不同です。
- 主キーはちょうど 1 つ必要です。1 文字の英字・数字、`F1`〜`F24`、矢印（`Left` / `Right` / `Up` / `Down`）、`Enter`（= `Return`）、`Space` / `Tab` / `Esc` / `Home` / `End` / `Delete` / `Insert` / `PageUp` / `PageDown` / `Backspace` を解釈します。

### `[exclusions]`

| キー | 既定 | 内容 |
|---|---|---|
| `processes` | 競技系ゲーム数件 | 介入しない実行ファイル名（basename・大文字小文字を区別しない）。 |
| `skip_when_fullscreen` | `true` | フルスクリーン・排他状態のときは介入しないか。 |
| `skip_non_tileable` | `true` | タイトルバーもリサイズ枠も持たないウィンドウ（ボーダーレス全画面ゲーム・オーバーレイ等）に介入しないか。名前リストに無い未知のゲームも避けられる。 |

アンチチート安全性との関係は [anti-cheat.md](anti-cheat.md) を参照してください。

## トレイメニューからの変更

トレイアイコンの右クリックで主要な設定をその場で変更でき、即座に `config.toml` に保存されます。項目は、ウィンドウ管理の有効化・標準スナップ無効化（機能 A）・アスペクト比で自動分割・列数 / 行数（1〜6 から選択）・設定ファイルを開く（除外プロセスなど詳細は notepad で編集）・設定の再読み込み・ログオン時の自動起動・終了です。
