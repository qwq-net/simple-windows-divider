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
2. 機能 A: ドラッグ端でスナップしない。トレイの「終了」後に元へ戻る。異常終了後の次回起動で復旧する。
3. 機能 B: 各ホットキーで作業領域（タスクバーを除く）どおりに配置される。同時押しで横軸フル・縦軸フル・全画面になる。
4. 混在 DPI（100% / 150%）のマルチモニタで矩形がずれない。
5. 最大化中のウィンドウにホットキーを押すと、いったん復元してからグリッド化される。
6. 機能 C: あるアプリをスナップして配置を学習させ、そのアプリの新規ウィンドウを開くと学習した配置へ復元される（Electron 系もリトライで収束する）。トレイの「覚えた配置を自動復元」を OFF にすると自動配置されない。
7. 設定 TOML を保存すると自動で反映される。トレイの各操作（有効切替・再読込・自動起動・終了）が機能する。
8. アンチチート保守: 全画面ゲーム中・除外 exe・昇格ウィンドウには触れず、クラッシュしない。
