//! Turning stored command text into printable, wrapped rows.
//!
//! Commands are stored bytes decoded lossily, so they can contain anything:
//! control characters from pasted escapes, embedded newlines from multi-line
//! entries, and characters wider than one column. Rendering and the scroll math
//! in `App` must agree on how many rows a command occupies, so both go through
//! here.

use unicode_width::UnicodeWidthChar;

/// Replaces control characters with printable stand-ins.
pub fn sanitize(command: &str) -> String {
    command
        .chars()
        .map(|c| match c {
            '\n' => '\u{23ce}',
            '\t' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect()
}

/// Hard-wraps `text` into rows of at most `width` columns, cutting on character
/// boundaries. Always returns at least one row, so an empty command still
/// occupies a line.
pub fn rows(text: &str, width: usize) -> Vec<&str> {
    let width = width.max(1);
    let mut out = Vec::with_capacity(text.len() / width + 1);
    let mut start = 0;
    let mut used = 0;

    for (index, c) in text.char_indices() {
        let cells = cell_width(c);
        // A zero-width character never needs a row of its own; otherwise the row
        // is full once the next character would not fit.
        if used + cells > width && index > start {
            out.push(&text[start..index]);
            start = index;
            used = 0;
        }
        used += cells;
    }

    out.push(&text[start..]);
    out
}

/// Rows `command` occupies when rendered `width` columns wide.
pub fn height(command: &str, width: usize) -> usize {
    rows(&sanitize(command), width).len()
}

fn cell_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{height, rows, sanitize};

    #[test]
    fn wraps_on_boundaries() {
        assert_eq!(rows("", 5), [""], "an empty command still occupies a row");
        assert_eq!(rows("abcde", 5), ["abcde"], "exactly filling the width is one row");
        assert_eq!(rows("abcdef", 5), ["abcde", "f"]);
        assert_eq!(rows(&"x".repeat(121), 49).len(), 3);
    }

    #[test]
    fn counts_columns_not_characters() {
        // Wide characters take two columns, so "日本" fills a four column row.
        assert_eq!(rows("日本語", 4), ["日本", "語"]);
        // A wide character that cannot fit still gets its own row.
        assert_eq!(rows("a日", 2), ["a", "日"]);
    }

    #[test]
    fn zero_width_characters_never_split() {
        assert_eq!(rows("e\u{301}\u{301}", 1), ["e\u{301}\u{301}"]);
    }

    #[test]
    fn sanitized_commands_stay_single_line() {
        assert_eq!(sanitize("a\nb\tc\u{0}d"), "a\u{23ce}b c d");
        assert_eq!(height("a\nb", 10), 1);
        assert_eq!(height(&"y".repeat(10), 4), 3);
    }
}
