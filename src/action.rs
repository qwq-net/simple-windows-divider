//! ホットキー → アクションの対応（Win32 非依存）。
//!
//! 学習コスト最小化のため、ホットキーは矢印 4 方向のみ。すべて [`HotkeyAction::Move`]（占有範囲を 1 セル動かす）。
//! 反対方向の同時押し（←→ / ↑↓）による軸フル化は、これらの組み合わせとして `app` 側が検出する。

use crate::config::schema::Hotkeys;
use crate::layout::grid::Family;

/// ホットキー押下時に行う操作。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HotkeyAction {
    /// 矢印方向にグリッド占有範囲を 1 セル動かす（左右=列軸・上下=行軸）。
    Move(Family),
}

/// 1 つのホットキー割り当て（ログ用の名前・キーチョード・アクション）。
pub struct Binding {
    pub name: &'static str,
    pub chord: String,
    pub action: HotkeyAction,
}

/// 設定の [`Hotkeys`]（矢印 4 方向）を割り当ての並びへ展開する。順序は登録 id の割り当て順を兼ねる。
pub fn bindings(h: &Hotkeys) -> Vec<Binding> {
    use Family as F;
    use HotkeyAction::Move;
    vec![
        Binding { name: "left", chord: h.left.clone(), action: Move(F::Left) },
        Binding { name: "right", chord: h.right.clone(), action: Move(F::Right) },
        Binding { name: "up", chord: h.up.clone(), action: Move(F::Top) },
        Binding { name: "down", chord: h.down.clone(), action: Move(F::Bottom) },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_four_arrows() {
        assert_eq!(bindings(&Hotkeys::default()).len(), 4);
    }

    #[test]
    fn maps_arrows_to_families() {
        let b = bindings(&Hotkeys::default());
        let find = |name: &str| b.iter().find(|x| x.name == name).unwrap().action;
        assert_eq!(find("left"), HotkeyAction::Move(Family::Left));
        assert_eq!(find("right"), HotkeyAction::Move(Family::Right));
        assert_eq!(find("up"), HotkeyAction::Move(Family::Top));
        assert_eq!(find("down"), HotkeyAction::Move(Family::Bottom));
    }

    #[test]
    fn carries_chord_strings_from_config() {
        let b = bindings(&Hotkeys::default());
        assert_eq!(b.iter().find(|x| x.name == "left").unwrap().chord, "Ctrl+Alt+Left");
    }
}
