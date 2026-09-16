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
| `v` | start a range selection at the cursor |
| `a` | select everything in the current view; again to clear it |
| `r` | newest first / file order |
| `d` | delete the selection, or the entry under the cursor when nothing is selected |
| `Ctrl+d` | delete the entry under the cursor, ignoring the selection |
| `q`, `Esc`, `Ctrl+c` | quit |

`Enter` or `Esc` leaves the filter and length prompts; the filter bar shows the active
filters and how many entries they left.

**Range selection.** `v` anchors a range at the cursor and every move (`j`/`k`, `g`/`G`,
arrows) extends the selection to the cursor, so sweeping back narrows it again. `v`,
`Space` or `Enter` keeps the range and leaves the mode, `Esc` abandons it and restores
the selection as it was before. `d` works from inside the mode, so `v`, `jjj`, `d`
deletes three entries.

**Select all.** `a` selects every entry in the current view, which with a filter active is
exactly the filtered set: `/`, `a`, `d` clears matching entries. Pressing `a` again
removes the view's entries from the selection. Entries outside the view are never touched,
so a narrower filter never clears selection work done under a wider one.

**Order.** `r` flips between file order (oldest first) and newest first, keeping the
cursor on the entry it was on; the view scrolls to follow it, and `g`/`G` jump to either
end of the view. The filter bar shows `Order: newest first` while it is flipped.

## Ctrl+R still finds deleted commands

The file is rewritten the moment you delete, but a shell answers Ctrl+R from its own
in-memory copy of the history, and nothing another process writes can reach into that copy.
Entries therefore stay findable in an open shell until it reloads its list; unlog reminds
you on exit when it deleted something. In zsh:

```sh
fc -p $HISTFILE   # push a fresh list and read the file into it
```

`fc -R` does not help — it adds the file's entries to the list instead of replacing it.
In bash, `history -c && history -r`. Opening a new shell always works.

Reloading can be automatic for the shell unlog was launched from:

```sh
# in your .zshrc
unlog() { command unlog "$@"; fc -P 2>/dev/null; fc -p $HISTFILE }
```

The list equals the file again the moment the pruning session ends, so Ctrl+R stops
offering what was deleted; the cost is that the `unlog` command itself is not kept in
history, since it went into the list that was pushed away. A `precmd` hook that reloads
when the file's size shrinks covers deletions made from another terminal, but it can drop
commands the shell has not written out yet, so the wrapper is the safer half of the pair.

Because a rewrite makes the file shorter, a shell with `SHARE_HISTORY` can also re-read it
from a stale offset and import entries the list already holds, so a running list ends up
with duplicates as well as the deleted entries; that is a second reason for `fc -p`. In a
three entry file, deleting one entry left the shell listing six events, one of them twice,
while the file held two.

The reverse lag is worth knowing too: with `SHARE_HISTORY` or `INC_APPEND_HISTORY`, zsh
writes a command to the file at the next prompt, so the newest command may not be on disk
yet when unlog loads it, and unlog reads the file once at startup.

## Formats

Both zsh extended history (`: <timestamp>:<duration>;<command>`, embedded newlines escaped)
and plain one-command-per-line files (bash, or zsh with `no_extended_history`) are understood.
The history file is read and written as bytes, not text, so the entries unlog leaves alone are
rewritten verbatim even when the file is not valid UTF-8.

## License

MIT
