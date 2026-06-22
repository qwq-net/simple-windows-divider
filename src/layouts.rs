//! 配置の学習データ（Win32 非依存）。
//!
//! ホットキー操作でウィンドウを配置するたびに、その占有範囲を「スロット」として識別子
//! `(exe, class, app_id)` ごとに記録する。スロットはディスプレイ名と占有範囲、記録時の分割数の組で、
//! 同じ識別子が複数のモニターや位置に所属できる。新規ウィンドウの復元先選択は [`crate::occupancy`] が担い、
//! ここは永続データの保持に徹する。データは設定（`config.toml`）とは別の `layouts.toml` に保存する。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::geometry::Rect;
use crate::layout::grid::GridSpan;

/// 学習データの識別キー。`exe` は実行ファイル basename を小文字化したもの、`class` はウィンドウクラス名、
/// `app_id` は AppUserModelID（AUMID。取得できない窓では空文字列）。
///
/// `app_id` を分けることで、同じ exe/class を共有する通常ウィンドウと PWA を別物として学習できる。
/// タイトルは含めない（同一アプリの新規ウィンドウへ汎用的に適用するため）。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WindowKey {
    pub exe: String,
    pub class: String,
    pub app_id: String,
}

/// ウィンドウが所属する 1 か所。`display` はディスプレイ名（`\\.\DISPLAYn`）、`span` はそのモニター内の
/// グリッド占有範囲、`cols`/`rows` は記録時の分割数（復元時に分割数が変わっていれば丸めに使う）。
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Slot {
    pub display: String,
    pub span: GridSpan,
    pub cols: u32,
    pub rows: u32,
}

impl Slot {
    /// このスロットを作業領域 `work` 上の実矩形へ変換する。
    ///
    /// 記録時の分割数 `cols`/`rows` でグリッドを解釈し、現在の分割数が変わっていても [`GridSpan::clamp_to`]
    /// で範囲外インデックスを丸めてから矩形化する。復元の適用とドラッグ解除の判定が同じ目標矩形を得るために
    /// 使う（両者が一致することが、解除判定で学習を誤って消さないための前提になる）。`work` は対象ディスプレイの
    /// 作業領域。副作用なし。
    pub fn target_rect(&self, work: Rect) -> Rect {
        self.span.clamp_to(self.cols, self.rows).rect(self.cols, self.rows, work)
    }
}

/// 学習した識別子→スロット集合。TOML には `[[layout]]` の並びで保存する。
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LearnedLayouts {
    #[serde(default, rename = "layout")]
    entries: Vec<LayoutEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct LayoutEntry {
    exe: String,
    class: String,
    #[serde(default)]
    app_id: String,
    slots: Vec<Slot>,
}

impl LearnedLayouts {
    /// 学習データが空か。空なら自動復元の処理を丸ごと省ける。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `key` に占有範囲 `slot` を学習する。`(exe, class, app_id)` の完全一致エントリだけを対象にする。
    ///
    /// - `old_slot` が `Some` なら、同じ窓を動かし直したものとみなし、一致するスロットをその場で `slot` に
    ///   置き換える（スロット数を増やさない）。一致が無ければ追加扱い。
    /// - 置き換えが起きず、`slot` が既存スロットと重複しなければ末尾に追加する。重複するなら何もしない。
    /// - スロット数が `cap` を超えたら最古（先頭）から捨てる。
    ///
    /// ユーザーの矢印操作からのみ呼ばれる（自動復元はこの経路を通らない）。
    pub fn learn(&mut self, key: &WindowKey, slot: Slot, old_slot: Option<Slot>, cap: usize) {
        let entry = self.find_or_create(key);
        let replaced = old_slot
            .and_then(|old| entry.slots.iter_mut().find(|s| **s == old))
            .map(|s| *s = slot.clone())
            .is_some();
        if !replaced && !entry.slots.contains(&slot) {
            entry.slots.push(slot);
        }
        while entry.slots.len() > cap {
            entry.slots.remove(0);
        }
    }

    /// `key` に記録された全スロットを学習順で返す。無ければ空。
    pub fn slots(&self, key: &WindowKey) -> Vec<Slot> {
        self.entries
            .iter()
            .find(|e| e.matches(key))
            .map(|e| e.slots.clone())
            .unwrap_or_default()
    }

    /// `key` から `slot` に一致する記録を消す。スロットが尽きたエントリ自体も取り除く（ドラッグ解除時）。
    pub fn forget(&mut self, key: &WindowKey, slot: &Slot) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.matches(key)) {
            e.slots.retain(|s| s != slot);
        }
        self.entries.retain(|e| !e.slots.is_empty());
    }

    fn find_or_create(&mut self, key: &WindowKey) -> &mut LayoutEntry {
        if let Some(i) = self.entries.iter().position(|e| e.matches(key)) {
            &mut self.entries[i]
        } else {
            self.entries.push(LayoutEntry {
                exe: key.exe.clone(),
                class: key.class.clone(),
                app_id: key.app_id.clone(),
                slots: Vec::new(),
            });
            self.entries.last_mut().unwrap()
        }
    }
}

impl LayoutEntry {
    fn matches(&self, key: &WindowKey) -> bool {
        self.exe == key.exe && self.class == key.class && self.app_id == key.app_id
    }
}

/// 既定の学習データファイルのパス。Windows では `%APPDATA%\windows-divider\layouts.toml` 付近。
/// 標準設定ディレクトリを解決できない環境では `None`。
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "windows-divider")
        .map(|dirs| dirs.config_dir().join("layouts.toml"))
}

/// `path` から学習データを読む。ファイルが無い・TOML として壊れている場合は空のデータを返す（寛容）。
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

    fn key(exe: &str, class: &str, app_id: &str) -> WindowKey {
        WindowKey { exe: exe.into(), class: class.into(), app_id: app_id.into() }
    }
    fn slot(display: &str, l: u32, r: u32, t: u32, b: u32, cols: u32, rows: u32) -> Slot {
        Slot { display: display.into(), span: GridSpan { l, r, t, b }, cols, rows }
    }

    #[test]
    fn target_rect_matches_grid_cell() {
        // 3×2 の左2列・全行（■■□/■■□）= 幅 2/3・全高。
        let work = Rect { left: 0, top: 0, right: 1200, bottom: 800 };
        let s = slot("\\\\.\\DISPLAY1", 0, 1, 0, 1, 3, 2);
        assert_eq!(s.target_rect(work), Rect { left: 0, top: 0, right: 800, bottom: 800 });
    }

    #[test]
    fn target_rect_clamps_shrunk_grid() {
        // 3×2 で記録した右下 (2,2,1,1) を 2×1 グリッドへ → 右端=1・下端=0 に丸めてから矩形化。
        let work = Rect { left: 0, top: 0, right: 1000, bottom: 600 };
        let s = slot("\\\\.\\DISPLAY1", 2, 2, 1, 1, 2, 1);
        // clamp_to(2,1) → (1,1,0,0) = 右半分・全高。
        assert_eq!(s.target_rect(work), Rect { left: 500, top: 0, right: 1000, bottom: 600 });
    }

    #[test]
    fn target_rect_respects_work_origin() {
        // 作業領域が非ゼロ原点（セカンダリモニタ）でもオフセットを保つ。
        let work = Rect { left: 1920, top: 0, right: 1920 + 1280, bottom: 1024 };
        let s = slot("\\\\.\\DISPLAY2", 1, 1, 0, 1, 2, 2); // 右列・全高
        assert_eq!(s.target_rect(work), Rect { left: 1920 + 640, top: 0, right: 1920 + 1280, bottom: 1024 });
    }

    #[test]
    fn learn_then_slots_returns_slot() {
        let mut s = LearnedLayouts::default();
        assert!(s.is_empty());
        s.learn(&key("code.exe", "C", ""), slot("\\\\.\\DISPLAY1", 0, 1, 0, 1, 3, 2), None, 8);
        assert!(!s.is_empty());
        assert_eq!(s.slots(&key("code.exe", "C", "")), vec![slot("\\\\.\\DISPLAY1", 0, 1, 0, 1, 3, 2)]);
    }

    #[test]
    fn learn_distinct_slots_accumulate() {
        let mut s = LearnedLayouts::default();
        let k = key("edge.exe", "C", "");
        s.learn(&k, slot("\\\\.\\DISPLAY1", 2, 2, 1, 1, 3, 2), None, 8);
        s.learn(&k, slot("\\\\.\\DISPLAY2", 0, 1, 0, 1, 2, 2), None, 8);
        assert_eq!(s.slots(&k).len(), 2);
    }

    #[test]
    fn learn_with_old_slot_replaces_in_place() {
        let mut s = LearnedLayouts::default();
        let k = key("brave.exe", "C", "");
        s.learn(&k, slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2), None, 8); // 窓A
        s.learn(&k, slot("\\\\.\\DISPLAY1", 2, 2, 0, 1, 3, 2), None, 8); // 窓B
        let old = slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2);
        s.learn(&k, slot("\\\\.\\DISPLAY1", 1, 1, 0, 1, 3, 2), Some(old), 8); // 窓Aを中央へ
        assert_eq!(s.slots(&k), vec![
            slot("\\\\.\\DISPLAY1", 1, 1, 0, 1, 3, 2),
            slot("\\\\.\\DISPLAY1", 2, 2, 0, 1, 3, 2),
        ]);
    }

    #[test]
    fn learn_same_slot_deduplicated() {
        let mut s = LearnedLayouts::default();
        let k = key("brave.exe", "C", "");
        s.learn(&k, slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2), None, 8);
        s.learn(&k, slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2), None, 8);
        assert_eq!(s.slots(&k).len(), 1);
    }

    #[test]
    fn learn_distinguishes_app_id() {
        let mut s = LearnedLayouts::default();
        s.learn(&key("brave.exe", "C", ""), slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2), None, 8);
        s.learn(&key("brave.exe", "C", "ytm"), slot("\\\\.\\DISPLAY1", 2, 2, 0, 1, 3, 2), None, 8);
        assert_eq!(s.slots(&key("brave.exe", "C", "ytm")), vec![slot("\\\\.\\DISPLAY1", 2, 2, 0, 1, 3, 2)]);
        assert_eq!(s.slots(&key("brave.exe", "C", "")), vec![slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2)]);
    }

    #[test]
    fn learn_evicts_oldest_over_cap() {
        let mut s = LearnedLayouts::default();
        let k = key("brave.exe", "C", "");
        s.learn(&k, slot("\\\\.\\DISPLAY1", 0, 0, 0, 0, 3, 2), None, 2);
        s.learn(&k, slot("\\\\.\\DISPLAY1", 1, 1, 0, 0, 3, 2), None, 2);
        s.learn(&k, slot("\\\\.\\DISPLAY1", 2, 2, 0, 0, 3, 2), None, 2);
        assert_eq!(s.slots(&k), vec![
            slot("\\\\.\\DISPLAY1", 1, 1, 0, 0, 3, 2),
            slot("\\\\.\\DISPLAY1", 2, 2, 0, 0, 3, 2),
        ]);
    }

    #[test]
    fn forget_removes_slot_and_empty_entry() {
        let mut s = LearnedLayouts::default();
        let k = key("brave.exe", "C", "");
        let sl = slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2);
        s.learn(&k, sl.clone(), None, 8);
        s.forget(&k, &sl);
        assert!(s.slots(&k).is_empty());
        assert!(s.is_empty()); // スロットが尽きた entry は消える
    }

    #[test]
    fn slots_miss_returns_empty() {
        let s = LearnedLayouts::default();
        assert!(s.slots(&key("nope.exe", "X", "")).is_empty());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = std::env::temp_dir().join("windows-divider-slot-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("layouts.toml");
        let mut s = LearnedLayouts::default();
        s.learn(&key("brave.exe", "C", ""), slot("\\\\.\\DISPLAY1", 0, 0, 0, 1, 3, 2), None, 8);
        s.learn(&key("brave.exe", "C", ""), slot("\\\\.\\DISPLAY2", 0, 1, 0, 1, 2, 2), None, 8);
        s.learn(&key("ytm.exe", "C", "ytm"), slot("\\\\.\\DISPLAY1", 1, 1, 0, 0, 3, 2), None, 8);
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_or_corrupt_is_empty() {
        let dir = std::env::temp_dir().join("windows-divider-slot-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir.join("none.toml")).is_empty());
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, b"not = valid = [[[").unwrap();
        assert!(load(&bad).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
