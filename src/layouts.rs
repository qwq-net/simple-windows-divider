//! 配置の学習データ（Win32 非依存）。
//!
//! ユーザーが矢印操作でウィンドウを配置するたびに `(exe, class) → GridSpan` を記録し（同じキーは上書き＝
//! last-wins）、同じ `(exe, class)` の新規ウィンドウが現れたらその占有範囲へ自動復元する。
//! データは設定（`config.toml`）とは別の `layouts.toml` に保存する（設定監視の再読込ループを避けるため）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::grid::GridSpan;

/// 学習データの識別キー。`exe` は実行ファイル basename を小文字化したもの、`class` はウィンドウクラス名。
///
/// タイトルは含めない（同一アプリの新規ウィンドウへ汎用的に適用するため）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WindowKey {
    pub exe: String,
    pub class: String,
}

/// 学習した `(exe, class) → GridSpan` の集合。TOML には `[[layout]]` の並びとして保存する。
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LearnedLayouts {
    #[serde(default, rename = "layout")]
    entries: Vec<LayoutEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct LayoutEntry {
    exe: String,
    class: String,
    span: GridSpan,
}

impl LearnedLayouts {
    /// 学習データが空か。空なら自動復元の処理を丸ごと省ける。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `key` に占有範囲 `span` を対応づける。同じ `key` が既にあれば上書きする（last-wins）。
    pub fn record(&mut self, key: &WindowKey, span: GridSpan) {
        if let Some(e) = self.find_mut(key) {
            e.span = span;
        } else {
            self.entries.push(LayoutEntry {
                exe: key.exe.clone(),
                class: key.class.clone(),
                span,
            });
        }
    }

    /// `key` に対応する占有範囲を返す。無ければ `None`。
    pub fn lookup(&self, key: &WindowKey) -> Option<GridSpan> {
        self.entries
            .iter()
            .find(|e| e.exe == key.exe && e.class == key.class)
            .map(|e| e.span)
    }

    fn find_mut(&mut self, key: &WindowKey) -> Option<&mut LayoutEntry> {
        self.entries
            .iter_mut()
            .find(|e| e.exe == key.exe && e.class == key.class)
    }
}

/// 既定の学習データファイルのパス。Windows では `%APPDATA%\windows-divider\layouts.toml` 付近。
/// 標準設定ディレクトリを解決できない環境では `None`。
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "windows-divider")
        .map(|dirs| dirs.config_dir().join("layouts.toml"))
}

/// `path` から学習データを読む。ファイルが無い・TOML として壊れている場合は空のデータを返す（寛容）。
///
/// 学習データはアプリが自動生成・更新する補助ファイルのため、読めなくても致命扱いせず空から始める。
pub fn load(path: &Path) -> LearnedLayouts {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// `layouts` を `path` へ原子的に保存する（[`crate::fsutil::atomic_write`] を使う）。
pub fn save(path: &Path, layouts: &LearnedLayouts) -> std::io::Result<()> {
    let text = toml::to_string_pretty(layouts).map_err(std::io::Error::other)?;
    crate::fsutil::atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(exe: &str, class: &str) -> WindowKey {
        WindowKey { exe: exe.to_string(), class: class.to_string() }
    }

    fn span(l: u32, r: u32, t: u32, b: u32) -> GridSpan {
        GridSpan { l, r, t, b }
    }

    #[test]
    fn record_then_lookup_returns_span() {
        let mut store = LearnedLayouts::default();
        assert!(store.is_empty());
        store.record(&key("code.exe", "Chrome_WidgetWin_1"), span(0, 1, 0, 1));
        assert!(!store.is_empty());
        assert_eq!(store.lookup(&key("code.exe", "Chrome_WidgetWin_1")), Some(span(0, 1, 0, 1)));
    }

    #[test]
    fn record_same_key_overwrites_last_wins() {
        let mut store = LearnedLayouts::default();
        let k = key("code.exe", "Chrome_WidgetWin_1");
        store.record(&k, span(0, 0, 0, 0));
        store.record(&k, span(1, 2, 0, 1));
        assert_eq!(store.lookup(&k), Some(span(1, 2, 0, 1)));
    }

    #[test]
    fn lookup_miss_returns_none() {
        let store = LearnedLayouts::default();
        assert_eq!(store.lookup(&key("nope.exe", "X")), None);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = std::env::temp_dir().join("windows-divider-layouts-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("layouts.toml");
        let mut store = LearnedLayouts::default();
        store.record(&key("code.exe", "Chrome_WidgetWin_1"), span(0, 1, 0, 1));
        store.record(&key("wezterm-gui.exe", "org.wezfurlong.wezterm"), span(2, 2, 0, 1));
        save(&path, &store).unwrap();
        assert_eq!(load(&path), store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_or_corrupt_is_empty() {
        let dir = std::env::temp_dir().join("windows-divider-layouts-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 不存在
        assert!(load(&dir.join("none.toml")).is_empty());
        // 壊れた TOML
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, b"this is not = valid = toml = [[[").unwrap();
        assert!(load(&bad).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
