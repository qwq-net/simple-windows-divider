//! 設定 TOML にマップする serde 型。
//!
//! すべて宣言的なデータ型で、`#[serde(default)]` により未指定フィールドは [`Default`] 値で埋まる
//! （= 設定ファイルが部分的でも壊れない）。ホットキーは文字列のまま保持し、解釈は
//! [`crate::hotkey::parse`] が別途行う。学習による自動復元のデータは設定とは別ファイルに持つ
//! （[`crate::layouts`]）。

use serde::{Deserialize, Serialize};

/// 設定全体。`%APPDATA%\windows-divider\config.toml` に対応する。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub grid: GridConfig,
    pub hotkeys: Hotkeys,
    pub exclusions: Exclusions,
}

/// ホットキー用グリッドの分割数。左右キーは列方向、上下キーは行方向に占有範囲を動かす。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridConfig {
    /// 列数（＝横に並ぶセル数 / 垂直線で分割した数）。左右キーが動かす軸。
    pub columns: u32,
    /// 行数（＝縦に並ぶセル数 / 水平線で分割した数）。上下キーが動かす軸。
    pub rows: u32,
    /// 真なら、各ウィンドウが今いるモニタの解像度アスペクト比から分割数を自動判定する（`columns`/`rows` は使わない）。
    pub auto_aspect: bool,
}

impl Default for GridConfig {
    fn default() -> Self {
        // 既定はウルトラワイド向けの 3 列 × 2 行（垂直 3 分割・水平 2 分割）。自動判定は既定で無効。
        GridConfig { columns: 3, rows: 2, auto_aspect: false }
    }
}

/// 全体挙動のスイッチ。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// 機能 B/C の有効・無効（トレイから切替）。false の間は標準スナップを復元し介入しない。
    pub enabled: bool,
    /// 機能 A: Windows 標準スナップ（Aero Snap）を無効化するか。
    pub disable_snap: bool,
    /// 機能 A: Snap Assist 系レジストリも無効化するか（best-effort）。
    pub disable_snap_assist: bool,
    /// 機能 C: 学習した配置を新規ウィンドウへ自動復元するか（トレイから切替）。
    pub auto_restore: bool,
}

impl Default for General {
    fn default() -> Self {
        General {
            enabled: true,
            disable_snap: true,
            disable_snap_assist: true,
            auto_restore: true,
        }
    }
}

/// 矢印 4 方向のホットキー文字列（[`crate::hotkey::parse`] が解釈する表記）。
///
/// 学習コストを最小化するため、割り当てはこの 4 つだけ。四隅・最大化は矢印の組み合わせと同時押しで到達できる。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub left: String,
    pub right: String,
    pub up: String,
    pub down: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            left: "Ctrl+Alt+Left".into(),
            right: "Ctrl+Alt+Right".into(),
            up: "Ctrl+Alt+Up".into(),
            down: "Ctrl+Alt+Down".into(),
        }
    }
}

/// アンチチート安全のための「触らない」設定。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exclusions {
    /// 介入しない実行ファイル名（basename・大小無視）。
    pub processes: Vec<String>,
    /// フルスクリーン/排他検出時に介入しない。
    pub skip_when_fullscreen: bool,
    /// タイトルバーもリサイズ枠も持たない素のウィンドウ（ボーダーレス全画面ゲーム・オーバーレイ等）に介入しない。
    pub skip_non_tileable: bool,
}

impl Exclusions {
    /// `exe`（実行ファイル basename を想定）が除外プロセスに含まれるか。大文字小文字は無視する。
    pub fn excludes(&self, exe: &str) -> bool {
        self.processes.iter().any(|p| p.eq_ignore_ascii_case(exe))
    }
}

impl Default for Exclusions {
    fn default() -> Self {
        Exclusions {
            processes: vec![
                "csgo.exe".into(),
                "cs2.exe".into(),
                "valorant.exe".into(),
                "valorant-win64-shipping.exe".into(),
                "fortniteclient-win64-shipping.exe".into(),
                "r5apex.exe".into(),
            ],
            skip_when_fullscreen: true,
            skip_non_tileable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_toml_yields_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.general, General::default());
        assert_eq!(cfg.hotkeys, Hotkeys::default());
        assert_eq!(cfg.grid, GridConfig::default());
        assert!(cfg.general.enabled);
        assert!(cfg.general.auto_restore);
        assert_eq!(cfg.grid.columns, 3);
        assert_eq!(cfg.grid.rows, 2);
        assert!(!cfg.grid.auto_aspect);
    }

    #[test]
    fn partial_general_overrides_only_given_fields() {
        let cfg: Config = toml::from_str("[general]\ndisable_snap = false\n").unwrap();
        assert!(!cfg.general.disable_snap);
        // 未指定は既定のまま
        assert!(cfg.general.enabled);
        assert!(cfg.general.disable_snap_assist);
        assert!(cfg.general.auto_restore);
    }

    #[test]
    fn exclusions_default_includes_known_games_and_skips() {
        let ex = Exclusions::default();
        assert!(ex.processes.iter().any(|p| p == "valorant.exe"));
        assert!(ex.skip_when_fullscreen);
        assert!(ex.skip_non_tileable);
    }

    #[test]
    fn excludes_matches_case_insensitively() {
        let ex = Exclusions { processes: vec!["Game.exe".into()], ..Default::default() };
        assert!(ex.excludes("game.exe"));
        assert!(ex.excludes("GAME.EXE"));
        assert!(!ex.excludes("other.exe"));
        assert!(!Exclusions { processes: vec![], ..Default::default() }.excludes("game.exe"));
    }

    #[test]
    fn exclusions_partial_keeps_other_defaults() {
        // processes だけ指定しても、skip_* 系は既定（true）のまま残る。
        let cfg: Config = toml::from_str("[exclusions]\nprocesses = [\"foo.exe\"]\n").unwrap();
        assert_eq!(cfg.exclusions.processes, vec!["foo.exe".to_string()]);
        assert!(cfg.exclusions.skip_when_fullscreen);
        assert!(cfg.exclusions.skip_non_tileable);
    }

    #[test]
    fn roundtrip_serialize_then_parse() {
        let cfg = Config::default();
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg, back);
    }

    /// 配布する設定例が常に有効な TOML / スキーマであることを保証する（書式崩れの検出）。
    #[test]
    fn example_config_file_is_valid() {
        let text = include_str!("../../config.example.toml");
        let cfg: Config = toml::from_str(text).expect("config.example.toml must parse");
        assert!(cfg.general.enabled);
        assert_eq!(cfg.hotkeys.left, "Ctrl+Alt+Left");
    }
}
