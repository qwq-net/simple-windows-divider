# CLAUDE.md

windows-divider は Windows 11 常駐型のウィンドウ管理ツールです。設計と仕様の詳細は [docs/](docs/README.md) にまとめています。このファイルには、変更時に守る運用ルールだけを記載します。

## コマンド

```bash
cargo test                                  # 純ロジックのユニットテスト + doctest（どの OS でも可）
cargo clippy --all-targets                  # 既定ターゲットの lint
cargo check  --target x86_64-pc-windows-gnu # Windows 依存コードの型チェック
cargo clippy --target x86_64-pc-windows-gnu # Windows 依存コードの lint
cargo build  --release                      # 実機 Windows（MSVC）でのビルド
```

ビルドの詳細（クロスビルド・マニフェスト・実機確認手順）は [docs/build-and-test.md](docs/build-and-test.md) を参照してください。

## 守る不変条件

- 純ロジック層（`action` / `config` / `fsutil` / `hotkey` / `layout` / `layouts`）に `windows` クレート依存を持ち込まない。Win32 型との変換は `win::convert` に閉じ込める。
- Win32 を直接呼ぶコードは `win`（および配線の `app` / `tray` / `watcher`）に置き、`#[cfg(windows)]` でゲートする。
- アンチチート安全性を壊す API を追加しない（低レベル入力フック・DLL インジェクション・他プロセスのメモリ操作・カーネルドライバ）。能動的なウィンドウ操作は必ず `win::guard::should_intervene` を通す。詳細は [docs/anti-cheat.md](docs/anti-cheat.md)。
- 非昇格で動かす前提を保つ。
- コメントは `behavioral-comments` 方針（挙動＝契約を書き、実装をなぞらない）で書く。

## コードスタイル

- 構造体リテラルを 1 行にまとめるなど、既存のコンパクトな記法に合わせる。`cargo fmt` の既定設定とは差異があるため、一括整形はかけない。
- 文章（ドキュメント・コメント）は です・ます調で平易に書く。過度な強調や誇張は避ける。

## ドキュメント

- [docs/README.md](docs/README.md) — 索引
- [docs/architecture.md](docs/architecture.md) — 設計とモジュール構成
- [docs/features.md](docs/features.md) — 機能 A/B/C の仕様
- [docs/anti-cheat.md](docs/anti-cheat.md) — アンチチート安全性
- [docs/configuration.md](docs/configuration.md) — 設定リファレンス
- [docs/build-and-test.md](docs/build-and-test.md) — ビルドとテスト
