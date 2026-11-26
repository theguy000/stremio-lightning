//! Pure Logs Screen multi-row selection, keyboard step, and pointer-drag state.
//! Zero locks, zero disk I/O, zero Slint types.

use std::collections::HashSet;

pub use super::log_filter::Filter;
use crate::plumbing::logs::{Entry, Level, Source};

#[derive(Clone, Debug, Default)]
struct DragState {
    start_id: u64,
    pressed_id: u64,
    moved: bool,
    toggle_off_if_unmoved: bool,
    ctrl_base: Option<(HashSet<u64>, bool)>,
}

/// Interactive state of the Logs page: active filter, open detail row, multi-select set, Shift
/// anchor, and active pointer drag.
#[derive(Clone, Debug, Default)]
pub struct LogSelection {
    filter: Filter,
    detail: u64,
    selected: HashSet<u64>,
    anchor: u64,
    drag: DragState,
}

impl LogSelection {
    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    pub fn set_source(&mut self, source: &str) {
        self.filter.source =
            Source::from_id(source).map_or_else(String::new, |s| s.id().to_owned());
    }

    pub fn set_level(&mut self, level: &str) {
        self.filter.level = Level::from_id(level).map_or_else(String::new, |l| l.id().to_owned());
    }

    pub fn set_search(&mut self, search: &str) {
        self.filter.search = search.to_owned();
    }

    pub fn reset_filter(&mut self) {
        self.filter = Filter::default();
    }

    /// Matching entries in newest-first visible order under `filter`.
    pub fn matching(entries: &[Entry], filter: &Filter) -> Vec<Entry> {
        entries
            .iter()
            .rev()
            .filter(|e| filter.keeps(e))
            .cloned()
            .collect()
    }

    fn first_selected(&self, visible: &[u64]) -> u64 {
        visible
            .iter()
            .copied()
            .find(|vid| self.selected.contains(vid))
            .unwrap_or(0)
    }

    fn visible_ids(&self, entries: &[Entry]) -> Vec<u64> {
        entries
            .iter()
            .rev()
            .filter(|e| self.filter.keeps(e))
            .map(|e| e.id)
            .collect()
    }

    pub fn selected_ids(&self) -> &HashSet<u64> {
        &self.selected
    }

    pub fn selected_in(&self, entries: &[Entry], filter: &Filter) -> Vec<Entry> {
        if self.selected.is_empty() {
            return Vec::new();
        }
        entries
            .iter()
            .rev()
            .filter(|e| filter.keeps(e) && self.selected.contains(&e.id))
            .cloned()
            .collect()
    }

    pub fn detail(&self, entries: &[Entry]) -> Option<Entry> {
        if self.detail == 0 {
            return None;
        }
        entries
            .iter()
            .find(|entry| entry.id == self.detail)
            .cloned()
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = 0;
        self.detail = 0;
        self.drag = DragState::default();
    }

    pub fn select_all(&mut self, entries: &[Entry]) {
        let visible = self.visible_ids(entries);
        if visible.is_empty() {
            self.clear_selection();
            return;
        }
        let all: HashSet<u64> = visible.iter().copied().collect();
        let focus = if self.detail != 0 && all.contains(&self.detail) {
            self.detail
        } else {
            visible[0]
        };
        self.selected = all;
        self.anchor = focus;
        self.detail = focus;
        self.drag = DragState::default();
    }

    pub fn step_record(&mut self, entries: &[Entry], delta: isize, shift: bool) -> Option<u64> {
        let visible = self.visible_ids(entries);
        if visible.is_empty() {
            return None;
        }
        let first = self.first_selected(&visible);
        let current = if self.detail != 0 && visible.contains(&self.detail) {
            Some(self.detail)
        } else {
            (first != 0).then_some(first)
        };
        let target_pos = match current.and_then(|id| visible.iter().position(|vid| *vid == id)) {
            Some(pos) => (pos as isize + delta).clamp(0, visible.len() as isize - 1) as usize,
            None if delta < 0 => visible.len() - 1,
            None => 0,
        };
        let target = visible[target_pos];
        if shift {
            let start_id = if self.anchor != 0 && visible.contains(&self.anchor) {
                self.anchor
            } else {
                let fallback = current.unwrap_or(target);
                self.anchor = fallback;
                fallback
            };
            self.selected = range_selection(&visible, start_id, target, None, false);
            self.detail = target;
        } else {
            self.selected = HashSet::from([target]);
            self.anchor = target;
            self.detail = target;
        }
        self.drag = DragState::default();
        Some(target)
    }

    pub fn press_record(&mut self, entries: &[Entry], id: u64, shift: bool, ctrl: bool) {
        let visible = self.visible_ids(entries);
        if !visible.contains(&id) {
            self.drag = DragState::default();
            return;
        }
        let detail_open = self.detail != 0;
        if shift {
            let prev = self.selected.clone();
            let start_id = if self.anchor != 0 && visible.contains(&self.anchor) {
                self.anchor
            } else {
                self.anchor = id;
                id
            };
            self.selected = range_selection(&visible, start_id, id, ctrl.then_some(&prev), false);
            if detail_open {
                self.detail = id;
            }
            self.drag = DragState {
                start_id,
                pressed_id: id,
                ctrl_base: ctrl.then_some((prev, false)),
                ..DragState::default()
            };
        } else if ctrl {
            let prev = self.selected.clone();
            let removing = prev.contains(&id);
            if removing {
                self.selected.remove(&id);
            } else {
                self.selected.insert(id);
            }
            self.anchor = id;
            if detail_open {
                if !removing {
                    self.detail = id;
                } else if self.detail == id {
                    self.detail = self.first_selected(&visible);
                }
            }
            self.drag = DragState {
                start_id: id,
                pressed_id: id,
                ctrl_base: Some((prev, removing)),
                ..DragState::default()
            };
        } else {
            let only_this = self.selected.contains(&id)
                && visible
                    .iter()
                    .all(|vid| *vid == id || !self.selected.contains(vid));
            if !only_this {
                self.selected = HashSet::from([id]);
                self.anchor = id;
                if detail_open {
                    self.detail = id;
                }
            }
            self.drag = DragState {
                start_id: id,
                pressed_id: id,
                toggle_off_if_unmoved: only_this,
                ..DragState::default()
            };
        }
    }

    pub fn drag_record(&mut self, entries: &[Entry], id: u64) {
        let visible = self.visible_ids(entries);
        if !visible.contains(&id) || self.drag.start_id == 0 {
            return;
        }
        if id != self.drag.pressed_id {
            self.drag.moved = true;
            self.drag.toggle_off_if_unmoved = false;
        }
        let (base, remove) = match &self.drag.ctrl_base {
            Some((b, r)) => (Some(b), *r),
            None => (None, false),
        };
        self.selected = range_selection(&visible, self.drag.start_id, id, base, remove);
        if self.detail != 0 {
            self.detail = if self.selected.contains(&id) {
                id
            } else {
                self.first_selected(&visible)
            };
        }
    }

    pub fn release_record(&mut self, entries: &[Entry], id: u64) {
        let drag = std::mem::take(&mut self.drag);
        if drag.toggle_off_if_unmoved && !drag.moved {
            self.clear_selection();
            return;
        }
        let visible = self.visible_ids(entries);
        self.detail = if id != 0 && self.selected.contains(&id) && visible.contains(&id) {
            id
        } else {
            self.first_selected(&visible)
        };
    }
}

fn range_selection(
    visible: &[u64],
    from_id: u64,
    to_id: u64,
    base: Option<&HashSet<u64>>,
    remove: bool,
) -> HashSet<u64> {
    let mut out = base.cloned().unwrap_or_default();
    let Some(to_pos) = visible.iter().position(|id| *id == to_id) else {
        return out;
    };
    let from_pos = visible
        .iter()
        .position(|id| *id == from_id)
        .unwrap_or(to_pos);
    let (lo, hi) = (from_pos.min(to_pos), from_pos.max(to_pos));
    for &id in &visible[lo..=hi] {
        if remove {
            out.remove(&id);
        } else {
            out.insert(id);
        }
    }
    out
}

#[cfg(test)]
#[path = "log_selection_tests.rs"]
mod tests;
