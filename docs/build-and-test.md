# ビルドとテスト

## 実機 Windows でのビルド（MSVC）

```powershell
rustup toolchain install stable
cargo build --release
```

生成物は `target\release\windows-divider.exe` です。トレイ常駐の GUI アプリで、リリースビルドではコンソールウィンドウを出しません。

## テスト

座標計算・グリッド操作・配置の学習データ・ホットキーのパース・設定の入出力は Win32 に依存しない純ロジックなので、どの OS でもテストできます。

```bash
cargo test
cargo clippy --all-targets
```

## CI とリリース

GitHub Actions に 2 つのワークフローを置いています。

- `.github/workflows/ci.yml`：`main` への push と PR で実行します。ubuntu で `cargo test` と clippy（既定ターゲットと `x86_64-pc-windows-gnu`）を回し、windows で `cargo build --release` を通してマニフェスト埋め込みを含む実 MSVC ビルドの破損を検出します。
- `.github/workflows/release.yml`：リリースを行います。起動方法は次の 2 つで、どちらも冪等です（同じ版の Release が既にあれば何もしません）。
  - `main` への push で `Cargo.toml` の `version` を読み、その版の Release がまだ無ければ自動でリリースします（タグ `vX.Y.Z` の作成も行います）。
  - `v*.*.*` 形式のタグ push でもリリースします。この場合はタグと `Cargo.toml` の版の一致を検証します。

  まず ubuntu の軽いジョブでリリース要否を判定し、必要なときだけ windows でビルドします。成果物は zip（`windows-divider-vX.Y.Z-x86_64-pc-windows-msvc.zip`）に固め、SHA256 を併置して GitHub Release を作成します。リリースノートは自動生成、ハイフン付きタグ（`v1.1.0-rc.1` 等）は prerelease になります。

### リリースのやり方

通常は、`Cargo.toml` の `version` を上げて `main` に push するだけです。版が新しければ Release が自動で作られます。

明示的にタグを打ちたいときは Taskfile を使います（要 [go-task](https://taskfile.dev)）。

```bash
task release   # Cargo.toml の版で v<version> タグを打って push
```

`task release` は、作業ツリーがクリーンで、同名タグが未存在のときだけ実行されます。

### 開発で使う Taskfile

```bash
task test      # cargo test
task lint      # clippy（既定 + windows-gnu）
task ci        # test + lint（CI と同じ）
task version   # Cargo.toml の版を表示
```

配布物は署名していません。利用者向けの注意（SmartScreen の初回警告と SHA256 照合）は [README](../README.md) を参照してください。依存ライブラリのライセンス表示が必要になったら、`cargo about` 等で `THIRD-PARTY-NOTICES.txt` を生成して zip に同梱します（現状は未同梱）。

## WSL2 / Linux からのクロスチェック

開発時の型チェックやリンティングは、Linux からでも行えます。Windows 依存のコード（`#[cfg(windows)]`）は、Windows ターゲットを指定したときにコンパイルされます。

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

`build.rs` は、Windows ターゲットのとき Per-Monitor v2 DPI awareness とロングパス対応を宣言したアプリケーションマニフェストを実行ファイルへ埋め込みます。

クロス環境に `mt.exe`（マニフェストツール）が無い場合、マニフェスト埋め込みの段階でリンクが失敗します。その際は環境変数 `WINDIVIDER_SKIP_MANIFEST=1` を付けると、マニフェスト無しでリンクを確認できます。DPI awareness は実行時にも `SetProcessDpiAwarenessContext` で設定するため、動作はします。実機の MSVC ビルドでは `mt.exe` があるため、通常どおり埋め込まれます。

## 実機での確認手順（Windows 11）

純ロジック以外は実機でしか確認できません。変更後は次の項目を確認してください。

1. 二重起動を試み、2 つ目が即座に終了する。
2. 機能 A：ドラッグ端でスナップしない。トレイの「終了」後に元へ戻る。異常終了後の次回起動で復旧する。
3. 機能 B：各ホットキーで作業領域（タスクバーを除く）どおりに配置される。同時押しで横軸フル・縦軸フル・全画面になる。
4. 混在 DPI（100% / 150%）のマルチモニタで矩形がずれない。
5. 最大化中のウィンドウにホットキーを押すと、いったん復元してからグリッド化される。
6. 機能 C：あるアプリをスナップして配置を学習させ、そのアプリの新規ウィンドウを開くと学習した配置へ復元される（Electron 系もリトライで収束する）。トレイの「覚えた配置を自動復元」を OFF にすると自動配置されない。
7. 設定 TOML を保存すると自動で反映される。トレイの各操作（有効切替・再読込・自動起動・終了）が機能する。
8. アンチチート保守：全画面ゲーム中・除外 exe・昇格ウィンドウには触れず、クラッシュしない。
