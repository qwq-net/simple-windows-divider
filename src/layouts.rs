//! 配置の学習データ（Win32 非依存）。
//!
//! ユーザーが矢印操作でウィンドウを配置するたびに `(exe, class, app_id) → 占有範囲の集合` を学習し、
//! 同じキーの新規ウィンドウが現れたらその範囲へ自動復元する。1 キーに複数の配置（スロット）を貯められる
//! ため、同じアプリの窓を別々の位置へ覚えさせられる。データは設定（`config.toml`）とは別の
//! `layouts.toml` に保存する（設定監視の再読込ループを避けるため）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::grid::GridSpan;

/// 学習データの識別キー。`exe` は実行ファイル basename を小文字化したもの、`class` はウィンドウクラス名、
/// `app_id` は AppUserModelID（AUMID）。AUMID を取得できないウィンドウでは空文字列を入れる。
///
/// `app_id` を分けることで、同じ exe/class を共有する通常ウィンドウと PWA（インストール済み Web アプリ）を
/// 別々に学習できる。タイトルは含めない（同一アプリの新規ウィンドウへ汎用的に適用するため）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WindowKey {
    pub exe: String,
    pub class: String,
    pub app_id: String,
}

/// 学習した `(exe, class, app_id) → 占有範囲スロット列` の集合。TOML には `[[layout]]` の並びとして保存する。
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LearnedLayouts {
    #[serde(default, rename = "layout")]
    entries: Vec<LayoutEntry>,
}

/// 1 つの識別キーに紐づく学習エントリ。`spans` は学習順（先頭が最古）に並ぶ占有範囲のスロット列。
///
/// 旧 `layouts.toml` は 1 行 1 範囲（`span` 単数・`app_id` 無し）だったため、読み込み時の後方互換として
/// `span`（単数）も受け付ける。[`normalize`] が `span` を `spans` へ畳み込む。保存時は `spans` のみ出力する。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct LayoutEntry {
    exe: String,
    class: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    app_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    spans: Vec<GridSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    span: Option<GridSpan>,
}

impl LearnedLayouts {
    /// 学習データが空か。空なら自動復元の処理を丸ごと省ける。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `key` に占有範囲 `span` を学習する。`(exe, class, app_id)` が完全一致するエントリだけを対象にし、
    /// app_id="" へのフォールバックはしない（[`slots`](Self::slots) の読み取り時のみ降りる）。
    ///
    /// - `old_span` が `Some` のときは、同じ窓を動かし直したものとみなし、その値に一致するスロットを
    ///   その場で `span` に置き換える（スロット数は増えない）。一致するスロットが無ければ追加扱い。
    /// - `old_span` が `None`、または置き換え先が見つからないとき、`span` が既存スロットと重複しなければ
    ///   末尾に追加する。重複するなら何もしない（同じ配置を二重に貯めない）。
    /// - スロット数が `cap` を超えたら最古（先頭）から捨てる（1 キーあたりの無限増加を防ぐ）。
    ///
    /// ユーザーの矢印操作からのみ呼ばれる（自動復元はこの経路を通らないため、復元が学習を上書きしない）。
    pub fn learn(&mut self, key: &WindowKey, span: GridSpan, old_span: Option<GridSpan>, cap: usize) {
        let entry = self.find_or_create(key);
        let replaced = old_span
            .and_then(|old| entry.spans.iter_mut().find(|s| **s == old))
            .map(|slot| *slot = span)
            .is_some();
        if !replaced && !entry.spans.contains(&span) {
            entry.spans.push(span);
        }
        while entry.spans.len() > cap {
            entry.spans.remove(0);
        }
    }

    /// `key` に学習済みのスロット列を返す（学習順）。該当が無ければ空。
    ///
    /// `(exe, class, app_id)` の完全一致を最優先する。完全一致が無く `key.app_id` が非空のときに限り、
    /// 同じ `(exe, class)` で `app_id=""` の旧エントリへフォールバックする（AUMID 導入前に貯めた学習を
    /// アップデート後も活かすため）。`key.app_id` が空ならフォールバックはせず、一致のみを見る。
    pub fn slots(&self, key: &WindowKey) -> Vec<GridSpan> {
        if let Some(e) = self.entries.iter().find(|e| e.matches(key)) {
            return e.spans.clone();
        }
        if !key.app_id.is_empty() {
            if let Some(e) = self
                .entries
                .iter()
                .find(|e| e.exe == key.exe && e.class == key.class && e.app_id.is_empty())
            {
                return e.spans.clone();
            }
        }
        Vec::new()
    }

    fn find_or_create(&mut self, key: &WindowKey) -> &mut LayoutEntry {
        if let Some(i) = self.entries.iter().position(|e| e.matches(key)) {
            &mut self.entries[i]
        } else {
            self.entries.push(LayoutEntry {
                exe: key.exe.clone(),
                class: key.class.clone(),
                app_id: key.app_id.clone(),
                spans: Vec::new(),
                span: None,
            });
            self.entries.last_mut().unwrap()
        }
    }
}

impl LayoutEntry {
    fn matches(&self, key: &WindowKey) -> bool {
        self.exe == key.exe && self.class == key.class && self.app_id == key.app_id
    }

    /// 旧形式の単数 `span` を `spans` の先頭へ畳み込み、`span` を消す。読み込み直後に 1 度だけ呼ぶ。
    fn normalize(&mut self) {
        if let Some(s) = self.span.take() {
            self.spans.insert(0, s);
        }
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
    let mut layouts: LearnedLayouts = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    for e in &mut layouts.entries {
        e.normalize();
    }
    layouts
}

/// `layouts` を `path` へ原子的に保存する（[`crate::fsutil::atomic_write`] を使う）。
pub fn save(path: &Path, layouts: &LearnedLayouts) -> std::io::Result<()> {
    let text = toml::to_string_pretty(layouts).map_err(std::io::Error::other)?;
    crate::fsutil::atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(exe: &str, class: &str, app_id: &str) -> WindowKey {
        WindowKey { exe: exe.to_string(), class: class.to_string(), app_id: app_id.to_string() }
    }

    fn span(l: u32, r: u32, t: u32, b: u32) -> GridSpan {
        GridSpan { l, r, t, b }
    }

    #[test]
    fn learn_then_slots_returns_span() {
        let mut store = LearnedLayouts::default();
        assert!(store.is_empty());
        store.learn(&key("code.exe", "Chrome_WidgetWin_1", ""), span(0, 1, 0, 1), None, 6);
        assert!(!store.is_empty());
        assert_eq!(store.slots(&key("code.exe", "Chrome_WidgetWin_1", "")), vec![span(0, 1, 0, 1)]);
    }

    #[test]
    fn learn_distinct_spans_accumulates_slots() {
        // 同一キーに異なる配置を学習すると、上書き（last-wins）せず別スロットとして貯まる。
        let mut store = LearnedLayouts::default();
        let k = key("brave.exe", "Chrome_WidgetWin_1", "");
        store.learn(&k, span(0, 0, 0, 1), None, 6);
        store.learn(&k, span(2, 2, 0, 1), None, 6);
        assert_eq!(store.slots(&k), vec![span(0, 0, 0, 1), span(2, 2, 0, 1)]);
    }

    #[test]
    fn learn_with_old_span_updates_that_slot() {
        // 同じ窓の動かし直しは old_span でスロットを特定し、置き換える（スロット数は増えない）。
        let mut store = LearnedLayouts::default();
        let k = key("brave.exe", "Chrome_WidgetWin_1", "");
        store.learn(&k, span(0, 0, 0, 1), None, 6); // 窓 A を左へ
        store.learn(&k, span(2, 2, 0, 1), None, 6); // 窓 B を右へ
        store.learn(&k, span(1, 1, 0, 1), Some(span(0, 0, 0, 1)), 6); // 窓 A を中央へ
        assert_eq!(store.slots(&k), vec![span(1, 1, 0, 1), span(2, 2, 0, 1)]);
    }

    #[test]
    fn learn_same_span_is_deduplicated() {
        // 既存スロットと同じ配置を学習しても重複させない。
        let mut store = LearnedLayouts::default();
        let k = key("brave.exe", "Chrome_WidgetWin_1", "");
        store.learn(&k, span(0, 0, 0, 1), None, 6);
        store.learn(&k, span(0, 0, 0, 1), None, 6);
        assert_eq!(store.slots(&k), vec![span(0, 0, 0, 1)]);
    }

    #[test]
    fn slots_distinguishes_by_app_id() {
        // exe/class が同じでも app_id（AUMID）が違えば別キー。PWA と通常ブラウザを分離できる。
        let mut store = LearnedLayouts::default();
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", ""), span(0, 0, 0, 1), None, 6);
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", "app.ytm"), span(2, 2, 0, 1), None, 6);
        assert_eq!(store.slots(&key("brave.exe", "Chrome_WidgetWin_1", "app.ytm")), vec![span(2, 2, 0, 1)]);
    }

    #[test]
    fn slots_falls_back_to_empty_app_id_when_no_exact_match() {
        // 旧データ（app_id="")しか無いとき、AUMID 付き新規窓でも旧スロットを拾う（アップデート後の互換）。
        let mut store = LearnedLayouts::default();
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", ""), span(0, 1, 0, 1), None, 6);
        let with_aumid = key("brave.exe", "Chrome_WidgetWin_1", "Brave.ABC123");
        assert_eq!(store.slots(&with_aumid), vec![span(0, 1, 0, 1)]);
    }

    #[test]
    fn slots_prefers_exact_match_over_fallback() {
        // 完全一致があるなら app_id="" のフォールバックには降りない。
        let mut store = LearnedLayouts::default();
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", ""), span(0, 0, 0, 1), None, 6);
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", "Brave.ABC123"), span(2, 2, 0, 1), None, 6);
        assert_eq!(store.slots(&key("brave.exe", "Chrome_WidgetWin_1", "Brave.ABC123")), vec![span(2, 2, 0, 1)]);
    }

    #[test]
    fn slots_miss_returns_empty() {
        let store = LearnedLayouts::default();
        assert!(store.slots(&key("nope.exe", "X", "")).is_empty());
    }

    #[test]
    fn learn_evicts_oldest_when_over_capacity() {
        // 同一キーのスロットが上限を超えたら最古を捨てる（無限増加の防止）。
        let mut store = LearnedLayouts::default();
        let k = key("brave.exe", "Chrome_WidgetWin_1", "");
        store.learn(&k, span(0, 0, 0, 0), None, 2);
        store.learn(&k, span(1, 1, 0, 0), None, 2);
        store.learn(&k, span(2, 2, 0, 0), None, 2); // 上限 2 を超過 → 最古 (0,0,0,0) を捨てる
        assert_eq!(store.slots(&k), vec![span(1, 1, 0, 0), span(2, 2, 0, 0)]);
    }

    #[test]
    fn save_then_load_roundtrips_multiple_slots() {
        let dir = std::env::temp_dir().join("windows-divider-layouts-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("layouts.toml");
        let mut store = LearnedLayouts::default();
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", ""), span(0, 0, 0, 1), None, 6);
        store.learn(&key("brave.exe", "Chrome_WidgetWin_1", ""), span(2, 2, 0, 1), None, 6);
        store.learn(&key("ytmusic.exe", "Chrome_WidgetWin_1", "app.ytm"), span(1, 1, 0, 0), None, 6);
        save(&path, &store).unwrap();
        assert_eq!(load(&path), store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_legacy_format_without_app_id() {
        // 旧 layouts.toml（app_id 無し・1 キー 1 行）を読めること（後方互換）。
        let dir = std::env::temp_dir().join("windows-divider-layouts-legacy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("legacy.toml");
        let legacy = "[[layout]]\nexe = \"code.exe\"\nclass = \"Chrome_WidgetWin_1\"\nspan = { l = 0, r = 1, t = 0, b = 1 }\n";
        std::fs::write(&p, legacy).unwrap();
        let store = load(&p);
        assert_eq!(store.slots(&key("code.exe", "Chrome_WidgetWin_1", "")), vec![span(0, 1, 0, 1)]);
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
