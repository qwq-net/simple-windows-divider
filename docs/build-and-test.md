# ビルドとテスト

## 実機 Windows でのビルド（MSVC）

```powershell
rustup toolchain install stable
cargo build --release
```

生成物は `target\release\windows-divider.exe` です。トレイ常駐の GUI アプリで、リリースビルドではコンソールウィンドウを出しません。

## テスト

座標計算・グリッド操作・ホットキーのパース・設定の入出力は Win32 に依存しない純ロジックなので、どの OS でもテストできます。

```bash
cargo test
cargo clippy --all-targets
```

開発では Taskfile（要 [go-task](https://taskfile.dev)）を使えます。

```bash
task test      # cargo test
task lint      # clippy（既定 + windows-gnu）
task ci        # test + lint（CI と同じ）
task version   # Cargo.toml の版を表示
```

## CI とリリース

GitHub Actions に 2 つのワークフローを置いています。

- `.github/workflows/ci.yml`：`main` への push と PR で実行します。ubuntu で `cargo test` と clippy（既定ターゲットと `x86_64-pc-windows-gnu`）を回し、windows で `cargo build --release` を通してマニフェスト埋め込みを含む実 MSVC ビルドの破損を検出します。
- `.github/workflows/release.yml`：`main` への push で `Cargo.toml` の `version` を読み、その版の Release がまだ無ければタグ `vX.Y.Z` を作ってリリースします。`v*.*.*` タグの push でも起動し、その場合はタグと版の一致を検証します。どちらも冪等です（同じ版の Release が既にあれば何もしません）。成果物は zip（`windows-divider-vX.Y.Z-x86_64-pc-windows-msvc.zip`）に SHA256 を併置し、リリースノートは自動生成、ハイフン付きタグ（`v1.1.0-rc.1` 等）は prerelease になります。

通常のリリースは、`Cargo.toml` の `version` を上げて `main` に push するだけです。明示的にタグを打ちたいときは `task release` を使います（作業ツリーがクリーンで同名タグが未存在のときだけ、`v<version>` タグを打って push します）。

配布物は署名していません。利用者向けの注意（SmartScreen の初回警告と SHA256 照合）は [README](../README.md) を参照してください。依存ライブラリのライセンス表示が必要になったら、`cargo about` 等で `THIRD-PARTY-NOTICES.txt` を生成して zip に同梱します（現状は未同梱）。

## WSL2 / Linux からのクロスチェック

Windows 依存のコード（`#[cfg(windows)]`）も、Windows ターゲットを指定すれば Linux から型チェックと lint ができます。

```bash
# 型チェック（リンク不要・mingw 不要）
rustup target add x86_64-pc-windows-gnu
cargo check --target x86_64-pc-windows-gnu

# Windows 依存コードを含めた lint
cargo clippy --target x86_64-pc-windows-gnu

# 実リンク（MSVC、cargo-xwin。SDK を自動取得）
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
cargo xwin build --release --target x86_64-pc-windows-msvc
```

## マニフェストと DPI awareness

`build.rs` は、Windows ターゲットのとき Per-Monitor v2 DPI awareness とロングパス対応を宣言したアプリケーションマニフェストを実行ファイルへ埋め込みます。クロス環境に `mt.exe`（マニフェストツール）が無くリンクが失敗する場合は、環境変数 `WINDIVIDER_SKIP_MANIFEST=1` でマニフェスト無しのリンクを確認できます（DPI awareness は実行時にも設定するため動作はします）。実機の MSVC ビルドでは通常どおり埋め込まれます。

## 実機での確認手順（Windows 11）

純ロジック以外は実機でしか確認できません。変更後は次の項目を確認してください。

1. 二重起動を試み、2 つ目が即座に終了する。
2. 機能 A：ドラッグ端でスナップしない。トレイの「終了」後に元へ戻る。異常終了後の次回起動で復旧する。
3. 機能 B：各ホットキーで作業領域（タスクバーを除く）どおりに配置される。同時押しで横軸フル・縦軸フル・全画面になる。
4. 混在 DPI（100% / 150%）のマルチモニタで矩形がずれない。
5. 最大化中のウィンドウにホットキーを押すと、いったん復元してからグリッド化される。
6. 設定 TOML を保存すると自動で反映される。トレイの各操作（有効切替・再読込・自動起動・終了）が機能する。
7. アンチチート保守：全画面ゲーム中・除外 exe・昇格ウィンドウには触れず、クラッシュしない。
