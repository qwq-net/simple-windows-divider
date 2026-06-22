//! 標準スナップ無効化の適用・復元と、その退避情報の永続化（機能 A）。
//!
//! 無効化時はレジストリ設定を退避してから書き換え、復元時に書き戻す。退避情報はファイルにも保存し、
//! 異常終了で書き換えたまま残っても次回起動時に元へ戻せるようにする（[`SnapControl::recover_if_crashed`]）。

use std::fs;
use std::path::{Path, PathBuf};

use crate::win::snap::{self, SnapBackup};

/// スナップ設定の現在の退避状態（無効化中なら `Some`）と、退避ファイルのパスを保持する。
pub struct SnapControl {
    backup: Option<SnapBackup>,
    backup_path: PathBuf,
}

impl SnapControl {
    /// 設定ファイルと同じディレクトリに退避ファイル（`snap_backup.toml`）を置く `SnapControl` を作る。
    /// 初期状態では無効化していない（`backup` は `None`）。
    pub fn new(config_path: &Path) -> SnapControl {
        let backup_path = config_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("snap_backup.toml");
        SnapControl { backup: None, backup_path }
    }

    /// 望む状態へ収束させる。`want_disable` が真なら（まだ無効化していなければ）スナップを無効化して退避を保存し、
    /// 偽なら（無効化中なら）元の設定へ復元して退避を消す。`disable_assist` はスナップアシストも無効化するか。
    /// 何度呼んでも現在状態と一致していれば副作用は無い（冪等）。
    pub fn apply(&mut self, want_disable: bool, disable_assist: bool) {
        if want_disable && self.backup.is_none() {
            let backup = snap::disable_snap(disable_assist);
            self.persist(&backup);
            self.backup = Some(backup);
            tracing::info!("native snap disabled");
        } else if !want_disable {
            if let Some(backup) = self.backup.take() {
                snap::restore_snap(&backup);
                self.clear();
                tracing::info!("native snap restored");
            }
        }
    }

    /// 前回の異常終了で退避ファイルが残っていれば、先に元設定へ戻してからファイルを消す。起動時に一度呼ぶ。
    pub fn recover_if_crashed(&mut self) {
        if let Some(backup) = self.load() {
            tracing::warn!("found leftover snap backup; restoring previous settings");
            snap::restore_snap(&backup);
            self.clear();
        }
    }

    /// 無効化中なら元の設定へ復元して退避を消す（終了時）。
    pub fn restore_on_exit(&mut self) {
        if let Some(backup) = self.backup.take() {
            snap::restore_snap(&backup);
            self.clear();
        }
    }

    fn persist(&self, backup: &SnapBackup) {
        if let Ok(text) = toml::to_string(backup) {
            let _ = fs::write(&self.backup_path, text);
        }
    }

    fn load(&self) -> Option<SnapBackup> {
        let text = fs::read_to_string(&self.backup_path).ok()?;
        toml::from_str(&text).ok()
    }

    fn clear(&self) {
        let _ = fs::remove_file(&self.backup_path);
    }
}
