//! Application state: the loaded entries plus the filter/selection view over them.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::history::HistoryEntry;
use crate::text;

/// Columns the selection marker occupies, including its trailing space.
const MARKER_WIDTH: usize = 4;
/// Columns between the entry index and the command text.
const GAP_WIDTH: usize = 2;
/// Minimum width of the entry index column.
const MIN_INDEX_WIDTH: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Navigating the list; selection and delete keys are live.
    Normal,
    /// Extending a range selection from an anchor position.
    Range,
    /// Typing the text search.
    FilterText,
    /// Typing the minimum command length.
    FilterMinLen,
    /// Typing the maximum command length.
    FilterMaxLen,
}

pub struct App {
    /// All loaded entries. Shrinks on delete; indices are positions here.
    pub entries: Vec<HistoryEntry>,
    /// Indices into `entries` matching the current filters, in view order.
    pub filtered: Vec<usize>,
    /// Whether the view runs newest entry first instead of file order.
    pub reversed: bool,
    /// Indices into `entries` selected for deletion.
    pub selected: HashSet<usize>,
    /// Position within `filtered` the active range started at; `None` outside
    /// `Mode::Range`.
    anchor: Option<usize>,
    /// `selected` as it was when the range started. The range is recomputed
    /// from this on every cursor move, so sweeping back and forth over the same
    /// entries neither accumulates nor drops anything.
    base: HashSet<usize>,
    /// Case-insensitive substring match; empty means match all.
    pub text_filter: String,
    /// Minimum command length in characters; `None` means no lower bound.
    pub min_length: Option<usize>,
    /// Maximum command length in characters; `None` means no upper bound.
    pub max_length: Option<usize>,
    /// Position within `filtered` holding the cursor.
    pub cursor: usize,
    /// Position within `filtered` of the entry the window starts at.
    pub top: usize,
    /// Wrapped rows of `filtered[top]` scrolled past, so tall entries can be
    /// partly scrolled through without a cursor position per row.
    pub skip: usize,
    /// Rows the list area can display.
    pub viewport: usize,
    /// Columns the list area has; the command text wraps to the rest.
    pub list_width: usize,
    pub mode: Mode,
    pub history_path: PathBuf,
    /// Write failure shown in the status bar for one frame.
    pub error_msg: Option<String>,
}

impl App {
    pub fn new(entries: Vec<HistoryEntry>, history_path: PathBuf) -> Self {
        let mut app = Self {
            entries,
            filtered: Vec::new(),
            reversed: false,
            selected: HashSet::new(),
            anchor: None,
            base: HashSet::new(),
            text_filter: String::new(),
            min_length: None,
            max_length: None,
            cursor: 0,
            top: 0,
            skip: 0,
            viewport: 20,
            list_width: 80,
            mode: Mode::Normal,
            history_path,
            error_msg: None,
        };
        app.apply_filters();
        app
    }

    /// Recomputes `filtered` and keeps the cursor inside the result.
    pub fn apply_filters(&mut self) {
        // A range is anchored to a position in the previous view, which no
        // longer exists; its entries stay selected.
        self.anchor = None;
        self.base.clear();

        let needle = self.text_filter.to_lowercase();
        let min = self.min_length;
        let max = self.max_length;
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (needle.is_empty() || entry.command.to_lowercase().contains(&needle))
                    && min.is_none_or(|min| entry.char_len() >= min)
                    && max.is_none_or(|max| entry.char_len() <= max)
            })
            .map(|(index, _)| index)
            .collect();

        if self.reversed {
            self.filtered.reverse();
        }

        if self.filtered.is_empty() {
            self.cursor = 0;
            self.top = 0;
            self.skip = 0;
        } else {
            self.cursor = self.cursor.min(self.filtered.len() - 1);
            self.top = self.top.min(self.filtered.len() - 1);
            self.ensure_visible();
        }
    }

    /// Width of the entry index column for the current number of entries.
    pub fn index_width(&self) -> usize {
        self.entries
            .len()
            .saturating_sub(1)
            .to_string()
            .len()
            .max(MIN_INDEX_WIDTH)
    }

    /// Columns of a list row before the command text starts.
    pub fn prefix_width(&self) -> usize {
        MARKER_WIDTH + self.index_width() + GAP_WIDTH
    }

    /// Columns available for command text, which is what wrapping is computed
    /// against.
    pub fn text_width(&self) -> usize {
        self.list_width.saturating_sub(self.prefix_width()).max(1)
    }

    /// Rows `filtered[position]` occupies when wrapped.
    pub fn entry_height(&self, position: usize) -> usize {
        match self.filtered.get(position) {
            Some(&index) => text::height(&self.entries[index].command, self.text_width()),
            None => 0,
        }
    }

    /// Records the list area size and re-fits the window.
    pub fn set_viewport(&mut self, width: usize, height: usize) {
        self.list_width = width;
        self.viewport = height;
        self.ensure_visible();
    }

    /// Scrolls the minimum amount needed to keep the cursor on screen.
    ///
    /// Walks rows from the window anchor instead of counting entries, because an
    /// entry can occupy many rows. The walk stops as soon as the window is full,
    /// so it never costs more than a viewport's worth of wrapped entries.
    pub fn ensure_visible(&mut self) {
        if self.filtered.is_empty() {
            self.top = 0;
            self.skip = 0;
            return;
        }
        let height = self.viewport.max(1);
        self.cursor = self.cursor.min(self.filtered.len() - 1);
        if self.top > self.cursor {
            self.top = self.cursor;
            self.skip = 0;
        }

        // Rows of the window consumed before the cursor entry begins.
        let mut used = 0;
        let mut position = self.top;
        loop {
            let rows = self.entry_height(position);
            let skipped = if position == self.top {
                self.skip.min(rows - 1)
            } else {
                0
            };

            if position == self.cursor {
                if rows > height {
                    // Taller than the window: show its first rows so the
                    // command's opening words, which identify it, stay visible.
                    self.top = self.cursor;
                    self.skip = 0;
                } else if used + rows - skipped > height {
                    self.scroll_down(used + rows - skipped - height);
                }
                return;
            }

            used += rows - skipped;
            if used >= height {
                // The cursor fell past the window: anchor it at the top.
                self.top = self.cursor;
                self.skip = 0;
                return;
            }
            position += 1;
        }
    }

    /// Moves the window anchor `rows` rows further down the list.
    fn scroll_down(&mut self, rows: usize) {
        let mut remaining = rows;
        while remaining > 0 {
            let entry_rows = self.entry_height(self.top);
            let visible = entry_rows - self.skip.min(entry_rows - 1);
            if remaining < visible {
                self.skip += remaining;
                return;
            }
            remaining -= visible;
            if self.top + 1 >= self.filtered.len() {
                return;
            }
            self.top += 1;
            self.skip = 0;
        }
    }

    /// Index into `entries` under the cursor, if the list is non-empty.
    pub fn cursor_entry(&self) -> Option<usize> {
        self.filtered.get(self.cursor).copied()
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let last = self.filtered.len() - 1;
        self.cursor = self.cursor.saturating_add_signed(delta).min(last);
        self.ensure_visible();
        self.extend_range();
    }

    pub fn goto(&mut self, position: usize) {
        if self.filtered.is_empty() {
            return;
        }
        self.cursor = position.min(self.filtered.len() - 1);
        self.ensure_visible();
        self.extend_range();
    }

    /// Starts a range at the cursor. The entry under it is selected right away,
    /// and every move until the range ends extends the selection to the cursor.
    pub fn begin_range(&mut self) {
        if self.cursor_entry().is_none() {
            return;
        }
        self.anchor = Some(self.cursor);
        self.base = self.selected.clone();
        self.mode = Mode::Range;
        self.extend_range();
    }

    /// Ends the range, keeping the selection it produced.
    pub fn finish_range(&mut self) {
        self.anchor = None;
        self.base.clear();
        self.mode = Mode::Normal;
    }

    /// Ends the range, restoring the selection from before it started.
    pub fn cancel_range(&mut self) {
        self.selected = std::mem::take(&mut self.base);
        self.anchor = None;
        self.mode = Mode::Normal;
    }

    /// Re-selects everything between the anchor and the cursor, on top of the
    /// selection the range started from.
    fn extend_range(&mut self) {
        let Some(anchor) = self.anchor else {
            return;
        };
        let [lo, hi] = if anchor <= self.cursor {
            [anchor, self.cursor]
        } else {
            [self.cursor, anchor]
        };
        self.selected = self.base.clone();
        if let Some(range) = self.filtered.get(lo..=hi) {
            self.selected.extend(range.iter().copied());
        }
    }

    /// Flips between file order and newest first, leaving the cursor on the
    /// same entry so the view does not jump off what is being read.
    pub fn toggle_order(&mut self) {
        let sticking = self.cursor_entry();
        self.finish_range();
        self.reversed = !self.reversed;
        self.apply_filters();
        if let Some(index) = sticking
            && let Some(position) = self.filtered.iter().position(|&entry| entry == index)
        {
            self.cursor = position;
            self.ensure_visible();
        }
    }

    /// Selects every entry in the current view, or clears the view's entries
    /// when they are all selected already. Entries filtered out are untouched.
    pub fn toggle_select_all(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        if self.filtered.iter().all(|index| self.selected.contains(index)) {
            for index in &self.filtered {
                self.selected.remove(index);
            }
        } else {
            self.selected.extend(self.filtered.iter().copied());
        }
    }

    pub fn toggle_selection(&mut self) {
        if let Some(index) = self.cursor_entry()
            && !self.selected.remove(&index)
        {
            self.selected.insert(index);
        }
    }

    /// Entries a delete keypress should remove: the whole selection, or just
    /// the entry under the cursor when `cursor_only` is set or nothing is
    /// selected.
    pub fn delete_targets(&self, cursor_only: bool) -> HashSet<usize> {
        if !cursor_only && !self.selected.is_empty() {
            self.selected.clone()
        } else {
            self.cursor_entry().into_iter().collect()
        }
    }

    /// Drops `targets` from `entries`; returns how many were removed.
    pub fn remove(&mut self, targets: &HashSet<usize>) -> usize {
        let removed = targets.len();
        self.entries = std::mem::take(&mut self.entries)
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !targets.contains(index))
            .map(|(_, entry)| entry)
            .collect();
        self.selected.clear();
        // Positions shifted, so an active range has nothing left to extend.
        self.mode = Mode::Normal;
        self.apply_filters();
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(commands: &[&str]) -> App {
        let entries = commands
            .iter()
            .map(|command| HistoryEntry {
                command: command.to_string(),
                raw: command.as_bytes().to_vec(),
            })
            .collect();
        App::new(entries, PathBuf::from("history"))
    }

    #[test]
    fn range_selects_exactly_what_it_sweeps() {
        let mut app = app(&["a", "b", "c", "d", "e"]);
        app.goto(3);
        app.begin_range();
        assert_eq!(app.selected, HashSet::from([3]));

        app.move_cursor(-2);
        assert_eq!(app.selected, HashSet::from([1, 2, 3]));

        // Sweeping back narrows the range instead of keeping what it crossed.
        app.move_cursor(1);
        assert_eq!(app.selected, HashSet::from([2, 3]));
    }

    #[test]
    fn range_starts_from_the_existing_selection() {
        let mut app = app(&["a", "b", "c", "d"]);
        app.goto(0);
        app.toggle_selection();

        app.goto(2);
        app.begin_range();
        app.move_cursor(1);
        app.cancel_range();
        assert_eq!(app.selected, HashSet::from([0]), "Esc puts the range back");
        assert_eq!(app.mode, Mode::Normal);

        app.goto(2);
        app.begin_range();
        app.move_cursor(1);
        app.finish_range();
        assert_eq!(app.selected, HashSet::from([0, 2, 3]));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn range_does_not_survive_a_new_filter() {
        let mut app = app(&["alpha", "beta", "beta two"]);
        app.goto(1);
        app.begin_range();
        app.move_cursor(1);

        app.text_filter = "beta".to_string();
        app.apply_filters();
        assert_eq!(app.selected, HashSet::from([1, 2]));

        // The old anchor positions are gone, so nothing new gets swept in.
        app.move_cursor(-1);
        assert_eq!(app.selected, HashSet::from([1, 2]));
    }

    #[test]
    fn select_all_covers_the_view_and_toggles_back() {
        let mut app = app(&["keep me", "drop one", "drop two"]);
        app.text_filter = "drop".to_string();
        app.apply_filters();

        app.toggle_select_all();
        assert_eq!(app.selected, HashSet::from([1, 2]));

        app.toggle_select_all();
        assert!(app.selected.is_empty());
    }

    #[test]
    fn reversing_flips_the_view_and_holds_the_cursor() {
        let mut app = app(&["a", "b", "c"]);
        app.goto(0);

        app.toggle_order();
        assert_eq!(app.filtered, [2, 1, 0]);
        assert_eq!(app.cursor_entry(), Some(0));

        app.toggle_order();
        assert_eq!(app.filtered, [0, 1, 2]);
        assert_eq!(app.cursor_entry(), Some(0));
    }
}
