//! Application state: the loaded entries plus the filter/selection view over them.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::history::HistoryEntry;

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
    /// First visible row of the list.
    pub scroll: usize,
    /// Position within `filtered`.
    pub cursor: usize,
    /// Rows the list can display; kept up to date by the render loop.
    pub viewport: usize,
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
            scroll: 0,
            cursor: 0,
            viewport: 20,
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
            self.scroll = 0;
        } else {
            self.cursor = self.cursor.min(self.filtered.len() - 1);
            self.ensure_visible();
        }
    }

    /// Scrolls the minimum amount needed to keep the cursor on screen.
    pub fn ensure_visible(&mut self) {
        let height = self.viewport.max(1);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + height {
            self.scroll = self.cursor + 1 - height;
        }
        let max_scroll = self.filtered.len().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll);
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
