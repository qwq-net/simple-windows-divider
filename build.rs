//! ビルドスクリプト: Windows ターゲットのとき Per-Monitor v2 DPI awareness と
//! ロングパス対応を宣言したアプリケーションマニフェストを実行ファイルへ埋め込む。
//!
//! 注意: build.rs はビルドホスト上で動くため、ターゲット OS は `CARGO_CFG_WINDOWS` で判定する
//! （`cfg!(windows)` はホストを指してしまう）。

fn main() {
    // クロス環境で mt.exe（マニフェストツール）が無いとリンクできないため、検証用に埋め込みを
    // 無効化できる逃げ道を用意する（実機 Windows/MSVC では mt.exe があるため通常は埋め込む）。
    let skip_manifest = std::env::var_os("WINDIVIDER_SKIP_MANIFEST").is_some();
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() && !skip_manifest {
        use embed_manifest::manifest::{DpiAwareness, Setting};
        use embed_manifest::{embed_manifest, new_manifest};

        embed_manifest(
            new_manifest("WindowsDivider")
                .dpi_awareness(DpiAwareness::PerMonitorV2)
                .long_path_aware(Setting::Enabled),
        )
        .expect("unable to embed application manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
    // 宣言しないと環境変数の変化で build.rs が再実行されず、キャッシュ済みのマニフェスト埋め込み
    // リンク引数が残って WINDIVIDER_SKIP_MANIFEST の切り替えが効かない。
    println!("cargo:rerun-if-env-changed=WINDIVIDER_SKIP_MANIFEST");
}
