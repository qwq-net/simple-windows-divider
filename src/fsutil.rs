//! ファイル操作の小さな共通ヘルパ（Win32 非依存）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// `path` へ `bytes` を原子的に書き込む。
///
/// 親ディレクトリが無ければ作成し、同じディレクトリの一時ファイル（`path` + `.tmp`）へ書いてから
/// rename で置換する。途中で失敗しても既存ファイルを壊さない。
/// 副作用: ファイルシステムへの書き込み。
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("windows-divider-fsutil-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_bytes_and_creates_missing_parent_dirs() {
        let dir = temp_dir("write");
        let path = dir.join("nested/sub/data.bin");
        atomic_write(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = temp_dir("overwrite");
        let path = dir.join("data.bin");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        let _ = fs::remove_dir_all(&dir);
    }
}
