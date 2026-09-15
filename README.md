# unlog

A terminal browser for shell history: filter it down, select entries, delete them in place.
Deleting rewrites the history file immediately, keeping every surviving entry's original bytes.

## Install

```sh
cargo install --path .
```

## Run

```sh
unlog
```

The file it edits is `$HISTFILE` when that is set and readable, otherwise `~/.zsh_history`,
then `~/.bash_history`.

## Keys

| key | |
|---|---|
| `j` `k`, `↓` `↑` | move the cursor |
| `g` / `G` | first / last entry |
| `/` | filter by case-insensitive substring |
| `m` / `M` | minimum / maximum command length |
| `Space` | select the entry under the cursor |
| `d` | delete the selection, or the entry under the cursor when nothing is selected |
| `Ctrl+d` | delete the entry under the cursor, ignoring the selection |
| `q`, `Esc`, `Ctrl+c` | quit |

`Enter` or `Esc` leaves the filter and length prompts; the filter bar shows the active
filters and how many entries they left.

## Formats

Both zsh extended history (`: <timestamp>:<duration>;<command>`, embedded newlines escaped)
and plain one-command-per-line files (bash, or zsh with `no_extended_history`) are understood.
The history file is read and written as bytes, not text, so the entries unlog leaves alone are
rewritten verbatim even when the file is not valid UTF-8.

## License

MIT
