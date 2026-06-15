# windows-divider

Windows 11 向けの常駐型ウィンドウ管理ユーティリティです。標準スナップを無効化し、矢印ホットキーによる独自のグリッド配置と、アプリ別のウィンドウ位置の自動復元を提供します。ゲームと併用してもアンチチートに弾かれないよう、文書化済みのユーザーモード Win32 API だけで構成しています。

## 主な機能

- 標準スナップ（Aero Snap / Snap Assist）の無効化。終了時には元へ戻します。
- 矢印 4 方向のホットキーによるグリッド配置。同時押しで横軸・縦軸フル、組み合わせで全画面まで到達します。
- 配置の学習による自動復元。スナップした配置を覚え、同じアプリの新規ウィンドウへ自動で適用します。

## 動作要件

Windows 11。管理者権限は不要で、非昇格で動かします。

## ダウンロード

[Releases](https://github.com/qwq-net/simple-windows-divider/releases) から zip（`windows-divider-vX.Y.Z-x86_64-pc-windows-msvc.zip`）を取得し、展開して `windows-divider.exe` を実行するとタスクトレイに常駐します。インストールや管理者権限は要りません。

配布物は署名していないため、初回起動時に SmartScreen の警告が出ることがあります。その場合は「詳細情報」→「実行」で起動できます。

zip の完全性は、Release に併置した `.sha256` と照合して確認できます。

```powershell
Get-FileHash .\windows-divider-vX.Y.Z-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

## クイックスタート（ソースからビルド）

```powershell
cargo build --release
```

生成された `target\release\windows-divider.exe` を実行するとタスクトレイに常駐します。

## ドキュメント

使い方・設定・設計の詳細は [docs/](docs/README.md) を参照してください。

## ライセンス

MIT。詳細は [LICENSE](LICENSE) を参照してください。
