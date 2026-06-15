//! 設定（TOML）の型・読み書き・パス解決。
//!
//! [`schema`] は serde 型のみで Win32 非依存。読み書き（[`load`] / [`save`]）も標準ライブラリの
//! ファイル操作で実装し、プラットフォーム非依存に保つ。保存は一時ファイル＋ rename による原子的置換で、
//! 書き込み途中のクラッシュでも既存設定が壊れないようにする。

pub mod schema;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use schema::Config;

/// 設定の読み書きで起こりうる失敗。
#[derive(Debug)]
pub enum ConfigError {
    /// ファイル入出力エラー（不存在・権限など）。`kind()` で `NotFound` を判別できる。
    Io(io::Error),
    /// TOML として解釈できない。
    Parse(toml::de::Error),
    /// 設定を TOML へ直列化できない。
    Serialize(toml::ser::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config io error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
            ConfigError::Serialize(e) => write!(f, "config serialize error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// 既定の設定ファイルパス。Windows では `%APPDATA%\windows-divider\config.toml` 付近を指す。
///
/// プラットフォームの標準設定ディレクトリを解決できない環境では `None`。
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "windows-divider")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

/// `path` の TOML を [`Config`] として読み込む。
///
/// ファイルが無ければ [`ConfigError::Io`]（`NotFound`）、TOML 不正なら [`ConfigError::Parse`]。
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(ConfigError::Parse)
}

/// `cfg` を `path` へ原子的に保存する（[`crate::fsutil::atomic_write`] を使う）。
///
/// 親ディレクトリが無ければ作成し、途中で失敗しても既存ファイルを壊さない。
/// 副作用: ファイルシステムへの書き込み。
pub fn save(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    let text = toml::to_string_pretty(cfg).map_err(ConfigError::Serialize)?;
    crate::fsutil::atomic_write(path, text.as_bytes())?;
    Ok(())
}

/// `path` を読み込む。存在しなければ既定値を書き出してからそれを返す。
///
/// 初回起動時に既定の設定ファイルを生成する用途。`NotFound` 以外の入出力エラーや解釈エラーは
/// そのまま返す（壊れた設定を勝手に上書きしない）。
pub fn load_or_init(path: &Path) -> Result<Config, ConfigError> {
    match load(path) {
        Ok(cfg) => Ok(cfg),
        Err(ConfigError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
            let cfg = Config::default();
            save(path, &cfg)?;
            Ok(cfg)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト毎に固有の一時ディレクトリを用意し、Drop で後始末する。
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("windows-divider-test-{tag}"));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = TempDir::new("roundtrip");
        let path = tmp.file("config.toml");
        let mut cfg = Config::default();
        cfg.general.disable_snap = false;
        save(&path, &cfg).unwrap();
        assert_eq!(load(&path).unwrap(), cfg);
    }

    #[test]
    fn save_overwrites_existing() {
        let tmp = TempDir::new("overwrite");
        let path = tmp.file("config.toml");
        save(&path, &Config::default()).unwrap();
        let mut cfg2 = Config::default();
        cfg2.general.disable_snap_assist = false;
        save(&path, &cfg2).unwrap();
        assert!(!load(&path).unwrap().general.disable_snap_assist);
    }

    #[test]
    fn save_creates_missing_parent_dirs() {
        let tmp = TempDir::new("mkdir");
        let path = tmp.file("nested/sub/config.toml");
        save(&path, &Config::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn load_missing_file_is_notfound() {
        let tmp = TempDir::new("missing");
        let path = tmp.file("does-not-exist.toml");
        match load(&path) {
            Err(ConfigError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn load_or_init_creates_then_reads() {
        let tmp = TempDir::new("init");
        let path = tmp.file("config.toml");
        assert!(!path.exists());
        let created = load_or_init(&path).unwrap();
        assert_eq!(created, Config::default());
        assert!(path.exists());
        // 2 回目は既存を読む
        assert_eq!(load_or_init(&path).unwrap(), Config::default());
    }
}
