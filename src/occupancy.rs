//! 実行中のスロット所属管理（Win32 非依存）。
//!
//! どの生きたウィンドウ（hwnd）が、どの識別子のどのスロットを占有しているかを保持する。新規ウィンドウへ
//! 割り当てる空きスロットの選択、所属の確定・解除、死んだウィンドウの掃除を担う。永続化しない
//! （起動ごとに空から始まる）。hwnd は `u64` として扱い、Win32 型には依存しない。

use std::collections::HashMap;

use crate::layouts::{Slot, WindowKey};

/// hwnd → (識別子, 占有スロット) の対応。
#[derive(Default)]
pub struct Occupancy {
    assigned: HashMap<u64, (WindowKey, Slot)>,
}

impl Occupancy {
    /// `recorded`（`key` の記録スロット全件）のうち、いまどの生きた所属窓にも占有されていない先頭スロットを
    /// 返す。並び順はディスプレイ名→span の上端 `t`→左端 `l`。空きが無ければ `None`。
    ///
    /// 占有判定は同じ `key` の所属だけを見る（別アプリが同位置に居ても空きとみなす）。新規窓の復元先選択に使う。
    pub fn pick_slot(&self, key: &WindowKey, recorded: &[Slot]) -> Option<Slot> {
        let occupied: Vec<&Slot> = self
            .assigned
            .values()
            .filter(|(k, _)| k == key)
            .map(|(_, s)| s)
            .collect();
        let mut free: Vec<&Slot> = recorded
            .iter()
            .filter(|s| !occupied.contains(s))
            .collect();
        free.sort_by(|a, b| {
            a.display
                .cmp(&b.display)
                .then(a.span.t.cmp(&b.span.t))
                .then(a.span.l.cmp(&b.span.l))
        });
        free.first().map(|s| (*s).clone())
    }

    /// `hwnd` の所属を確定・更新する。ホットキー配置と復元適用の双方から呼ぶ。
    pub fn on_placed(&mut self, hwnd: u64, key: WindowKey, slot: Slot) {
        self.assigned.insert(hwnd, (key, slot));
    }

    /// `hwnd` の所属を解除する（ドラッグ離脱・クローズ）。未所属なら何もしない。
    pub fn on_released(&mut self, hwnd: u64) {
        self.assigned.remove(&hwnd);
    }

    /// `hwnd` の現所属（識別子とスロット）。記録時の `old_slot` と解除時の `layouts.forget` 引数に使う。
    pub fn entry_of(&self, hwnd: u64) -> Option<(WindowKey, Slot)> {
        self.assigned.get(&hwnd).cloned()
    }

    /// `is_alive` が偽を返す hwnd の所属を一括除去する。生存判定は呼び出し側（app）が `is_window` で注入する。
    pub fn prune(&mut self, is_alive: impl Fn(u64) -> bool) {
        self.assigned.retain(|&hwnd, _| is_alive(hwnd));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::GridSpan;

    fn key(app_id: &str) -> WindowKey {
        WindowKey { exe: "edge.exe".into(), class: "C".into(), app_id: app_id.into() }
    }
    fn slot(display: &str, l: u32, t: u32) -> Slot {
        Slot { display: display.into(), span: GridSpan { l, r: l, t, b: t }, cols: 3, rows: 2 }
    }

    #[test]
    fn pick_slot_returns_first_unoccupied_in_order() {
        let occ = Occupancy::default();
        let recorded = vec![slot("\\\\.\\DISPLAY2", 0, 0), slot("\\\\.\\DISPLAY1", 2, 0)];
        assert_eq!(occ.pick_slot(&key(""), &recorded), Some(slot("\\\\.\\DISPLAY1", 2, 0)));
    }

    #[test]
    fn pick_slot_skips_occupied() {
        let mut occ = Occupancy::default();
        occ.on_placed(1, key(""), slot("\\\\.\\DISPLAY1", 2, 0));
        let recorded = vec![slot("\\\\.\\DISPLAY1", 2, 0), slot("\\\\.\\DISPLAY2", 0, 0)];
        assert_eq!(occ.pick_slot(&key(""), &recorded), Some(slot("\\\\.\\DISPLAY2", 0, 0)));
    }

    #[test]
    fn pick_slot_none_when_all_occupied() {
        let mut occ = Occupancy::default();
        occ.on_placed(1, key(""), slot("\\\\.\\DISPLAY1", 2, 0));
        let recorded = vec![slot("\\\\.\\DISPLAY1", 2, 0)];
        assert_eq!(occ.pick_slot(&key(""), &recorded), None);
    }

    #[test]
    fn pick_slot_occupancy_is_per_key() {
        let mut occ = Occupancy::default();
        occ.on_placed(1, key("ytm"), slot("\\\\.\\DISPLAY1", 2, 0));
        let recorded = vec![slot("\\\\.\\DISPLAY1", 2, 0)];
        assert_eq!(occ.pick_slot(&key(""), &recorded), Some(slot("\\\\.\\DISPLAY1", 2, 0)));
    }

    #[test]
    fn pick_slot_none_when_recorded_empty() {
        let occ = Occupancy::default();
        assert_eq!(occ.pick_slot(&key(""), &[]), None);
    }

    #[test]
    fn entry_of_returns_assignment() {
        let mut occ = Occupancy::default();
        occ.on_placed(7, key(""), slot("\\\\.\\DISPLAY1", 1, 0));
        assert_eq!(occ.entry_of(7), Some((key(""), slot("\\\\.\\DISPLAY1", 1, 0))));
        assert_eq!(occ.entry_of(8), None);
    }

    #[test]
    fn released_slot_becomes_available_again() {
        let mut occ = Occupancy::default();
        occ.on_placed(1, key(""), slot("\\\\.\\DISPLAY1", 2, 0));
        occ.on_released(1);
        let recorded = vec![slot("\\\\.\\DISPLAY1", 2, 0)];
        assert_eq!(occ.pick_slot(&key(""), &recorded), Some(slot("\\\\.\\DISPLAY1", 2, 0)));
    }

    #[test]
    fn prune_drops_dead_windows() {
        let mut occ = Occupancy::default();
        occ.on_placed(1, key(""), slot("\\\\.\\DISPLAY1", 2, 0));
        occ.on_placed(2, key(""), slot("\\\\.\\DISPLAY2", 0, 0));
        occ.prune(|hwnd| hwnd == 2);
        assert_eq!(occ.entry_of(1), None);
        assert!(occ.entry_of(2).is_some());
    }
}
