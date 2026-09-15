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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigating the list; selection and delete keys are live.
    Normal,
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
    /// Indices into `entries` matching the current filters, in file order.
    pub filtered: Vec<usize>,
    /// Indices into `entries` selected for deletion.
    pub selected: HashSet<usize>,
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
            selected: HashSet::new(),
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
    }

    pub fn goto(&mut self, position: usize) {
        if self.filtered.is_empty() {
            return;
        }
        self.cursor = position.min(self.filtered.len() - 1);
        self.ensure_visible();
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
        self.apply_filters();
        removed
    }
}
