//! Loading, parsing and writing shell history files.
//!
//! Two formats are understood:
//!
//! * **zsh extended**: `: <timestamp>:<duration>;<command>` headers, with
//!   embedded newlines escaped by zsh as `<backslash>` + newline. A line is a
//!   continuation of the previous entry exactly when the previous stored line
//!   ends with a backslash.
//! * **plain**: one command per line (bash default, zsh with
//!   `no_extended_history`).
//!
//! The file is read as raw bytes, not `String`: real history files are not
//! always valid UTF-8 (zsh metafies non-ASCII bytes, pasted binary garbage
//! happens), and `save` must reproduce the original bytes for every entry it
//! does not delete. Only the display string is lossily decoded.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    /// Cleaned command text for display, newlines unescaped.
    pub command: String,
    /// Original bytes of this entry, including line terminators.
    pub raw: Vec<u8>,
}

impl HistoryEntry {
    /// Number of characters of the displayed command.
    pub fn char_len(&self) -> usize {
        self.command.chars().count()
    }
}

/// Locates the history file to operate on: an explicit path from the command
/// line first, then `$HISTFILE` if exported and readable, then
/// `~/.zsh_history`, then `~/.bash_history`.
///
/// Only an *exported* `HISTFILE` is visible here. zsh is usually told its
/// history file as a shell parameter (macOS `/etc/zshrc` and oh-my-zsh both set
/// it without exporting), which never reaches another process, so the fallback
/// and the explicit path matter: a shell can be using one file while this
/// process would resolve to another.
pub fn history_path(explicit: Option<&OsStr>) -> Result<PathBuf, String> {
    if let Some(raw) = explicit {
        let raw = raw.to_string_lossy();
        if !raw.trim().is_empty() {
            return Ok(expand_tilde(raw.trim()));
        }
    }
    if let Some(raw) = env::var_os("HISTFILE") {
        let raw = raw.to_string_lossy();
        let raw = raw.trim();
        if !raw.is_empty() {
            let path = expand_tilde(raw);
            if path.is_file() {
                return Ok(path);
            }
        }
    }

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())?;
    for name in [".zsh_history", ".bash_history"] {
        let path = home.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }

    Err("No history file found. Set HISTFILE.".to_string())
}

/// `path` with a leading `$HOME` shortened to `~`, for the status bar.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = env::var_os("HOME")
        && let Ok(rest) = path.strip_prefix(Path::new(&home))
        && !rest.as_os_str().is_empty()
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

fn expand_tilde(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(value)
}

pub fn load(path: &Path) -> io::Result<Vec<HistoryEntry>> {
    Ok(parse(&fs::read(path)?))
}

/// Splits raw history bytes into entries. Grouping is driven by zsh's own
/// rules, so a rewritten file reads back identically.
pub fn parse(data: &[u8]) -> Vec<HistoryEntry> {
    let mut entries: Vec<HistoryEntry> = Vec::new();
    // (raw bytes, unescaped command bytes) of the entry being accumulated.
    let mut pending_raw: Vec<u8> = Vec::new();
    let mut pending_cmd: Vec<u8> = Vec::new();
    let mut open = false;
    // Set by the previous line: a trailing backslash means the next line
    // continues this entry.
    let mut continues = false;

    for line in data.split_inclusive(|&b| b == b'\n') {
        let bare = strip_eol(line);
        match header_command(bare) {
            Some(cmd) => {
                flush(&mut entries, &mut pending_raw, &mut pending_cmd, &mut open);
                pending_raw.extend_from_slice(line);
                pending_cmd.extend_from_slice(cmd);
                open = true;
            }
            None if continues && open => {
                pending_raw.extend_from_slice(line);
                pending_cmd.push(b'\n');
                pending_cmd.extend_from_slice(bare);
            }
            None => {
                flush(&mut entries, &mut pending_raw, &mut pending_cmd, &mut open);
                pending_raw.extend_from_slice(line);
                pending_cmd.extend_from_slice(bare);
                open = true;
            }
        }
        continues = bare.ends_with(b"\\");
    }

    flush(&mut entries, &mut pending_raw, &mut pending_cmd, &mut open);
    entries
}

fn flush(
    entries: &mut Vec<HistoryEntry>,
    raw: &mut Vec<u8>,
    cmd: &mut Vec<u8>,
    open: &mut bool,
) {
    if *open {
        entries.push(HistoryEntry {
            command: display_command(cmd),
            raw: std::mem::take(raw),
        });
        cmd.clear();
        *open = false;
    }
}

fn strip_eol(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// Returns the command bytes of a `: <timestamp>:<duration>;<command>` header.
fn header_command(line: &[u8]) -> Option<&[u8]> {
    let rest = line.strip_prefix(b": ")?;
    let (stamp, rest) = split_digits(rest, b':')?;
    if stamp.is_empty() {
        return None;
    }
    let (_duration, rest) = split_digits(rest, b';')?;
    Some(rest)
}

/// Splits at the first `sep`, requiring everything before it to be digits.
fn split_digits(line: &[u8], sep: u8) -> Option<(&[u8], &[u8])> {
    let at = line.iter().position(|&b| b == sep)?;
    let (head, tail) = line.split_at(at);
    if head.iter().all(u8::is_ascii_digit) {
        Some((head, &tail[1..]))
    } else {
        None
    }
}

/// Turns stored command bytes into a display string: `\` + newline is an
/// escaped newline, and storage-only trailing newlines are dropped.
fn display_command(cmd: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(cmd);
    let mut out = String::with_capacity(decoded.len());
    let mut chars = decoded.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'\n') {
            chars.next();
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Rewrites the file with exactly the raw bytes of the surviving entries.
pub fn save(path: &Path, entries: &[HistoryEntry]) -> io::Result<()> {
    let total: usize = entries.iter().map(|e| e.raw.len()).sum();
    let mut buf = Vec::with_capacity(total);
    for entry in entries {
        buf.extend_from_slice(&entry.raw);
    }
    fs::write(path, buf)
}
