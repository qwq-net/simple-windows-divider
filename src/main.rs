//! windows-divider 実行バイナリのエントリポイント。
//!
//! Windows ターゲットでのみ実体が動作する（単一インスタンス確認 → ログ初期化 → メッセージループ）。
//! 非 Windows 環境では純ロジックの `cargo test` 用にビルドでき、起動すると Windows 専用である旨を表示する。

// トレイ常駐アプリのため、リリースビルドではコンソールウィンドウを出さない。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    windows_main();

    #[cfg(not(windows))]
    {
        eprintln!(
            "windows-divider is a Windows-only application.\n\
             Pure-logic unit tests run on any platform via `cargo test`."
        );
    }
}

#[cfg(windows)]
fn windows_main() {
    use windows_divider::win::singleton;

    // 二重起動を防ぐ。既に動いていれば即終了。
    let Some(_guard) = singleton::acquire() else {
        return;
    };

    init_logging();

    // 戻り値を局所変数に束縛してから判定する。一時値のまま `if let` に置くと、`_guard`（単一インスタンス
    // ガード）との drop 順が edition によって変わる（実害はないが、順序を明示しておく）。
    let result = windows_divider::app::run();
    if let Err(e) = result {
        tracing::error!("fatal: {e}");
    }
}

/// ファイルへのローテーションログを初期化する（トレイ常駐で stderr が見えないため）。
#[cfg(windows)]
fn init_logging() {
    use tracing_subscriber::EnvFilter;

    let log_dir = windows_divider::config::default_path()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "windows-divider.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(file_appender)
        .with_ansi(false)
        .init();
}
