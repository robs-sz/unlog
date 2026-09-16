//! unlog: browse, filter and prune shell history entries.

mod app;
mod history;
mod text;
mod ui;

use std::env;
use std::io::{self, Stdout};
use std::process::exit;
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use crossterm::execute;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use app::{App, Mode};

fn main() {
    let explicit = env::args_os().nth(1);
    let path = match history::history_path(explicit.as_deref()) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("unlog: {message}");
            exit(1);
        }
    };
    let entries = match history::load(&path) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("unlog: cannot read {}: {error}", path.display());
            exit(1);
        }
    };

    let mut app = App::new(entries, path);
    if let Err(error) = run(&mut app) {
        restore_terminal();
        eprintln!("unlog: {error}");
        exit(1);
    }
    if app.deleted > 0 {
        eprintln!("{}", deletion_notice(app.deleted, &app.history_path));
    }
}

/// Tells the user how to make already-open shells forget the deleted entries.
///
/// Deleting rewrites the history file, but a shell keeps its own in-memory list
/// and searches that for Ctrl+R, so the entries stay findable until the shell
/// reloads. zsh only imports new lines into a running list, and `fc -R` adds to
/// it rather than replacing it, so the list has to be pushed and re-read from
/// the file: that is what `fc -p $HISTFILE` does.
fn deletion_notice(deleted: usize, path: &std::path::Path) -> String {
    let entries = if deleted == 1 { "entry" } else { "entries" };
    format!(
        "unlog: removed {deleted} {entries} from {}. Shells that were already open still hold them \
         in memory, which is where Ctrl+R searches: run `fc -p $HISTFILE` in zsh (or `history -c && \
         history -r` in bash), or open a new shell, to drop them.",
        path.display()
    )
}

fn run(app: &mut App) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let result = event_loop(&mut terminal, app);
    restore_terminal();
    result
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        let size = terminal.size()?;
        let list = ui::layout(Rect::new(0, 0, size.width, size.height)).list;
        app.set_viewport(list.width as usize, list.height as usize);

        terminal.draw(|frame| ui::render(frame, app))?;
        // A write failure is reported for exactly one frame.
        app.error_msg = None;

        if !event::poll(Duration::from_millis(16))? {
            continue;
        }
        if let Event::Key(key) = event::read()?
            && key.kind != KeyEventKind::Release
            && handle_key(app, key)
        {
            return Ok(());
        }
    }
}

/// Returns `true` when the application should quit.
fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    match app.mode {
        Mode::Normal => return handle_normal(app, key),
        Mode::Range => handle_range(app, key),
        Mode::FilterText => handle_text_filter(app, key),
        Mode::FilterMinLen => handle_length_filter(app, key, LengthFilter::Min),
        Mode::FilterMaxLen => handle_length_filter(app, key, LengthFilter::Max),
    }
    false
}

fn handle_normal(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Char('j') | KeyCode::Down => app.move_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_cursor(-1),
        KeyCode::Char('g') => app.goto(0),
        KeyCode::Char('G') => app.goto(usize::MAX),
        KeyCode::Char(' ') => app.toggle_selection(),
        KeyCode::Char('v') => app.begin_range(),
        KeyCode::Char('a') => app.toggle_select_all(),
        KeyCode::Char('r') => app.toggle_order(),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            do_delete(app, true);
        }
        KeyCode::Char('d') | KeyCode::Delete => do_delete(app, false),
        KeyCode::Char('/') => {
            app.text_filter.clear();
            app.apply_filters();
            app.mode = Mode::FilterText;
        }
        KeyCode::Char('m') => app.mode = Mode::FilterMinLen,
        KeyCode::Char('M') => app.mode = Mode::FilterMaxLen,
        _ => {}
    }
    false
}

/// Range mode: navigation extends the range, so the only new keys are the ways
/// to end it. `Esc` puts back the selection the range started from.
fn handle_range(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_cursor(-1),
        KeyCode::Char('g') => app.goto(0),
        KeyCode::Char('G') => app.goto(usize::MAX),
        KeyCode::Char('v') | KeyCode::Char(' ') | KeyCode::Enter => app.finish_range(),
        KeyCode::Esc => app.cancel_range(),
        KeyCode::Char('a') => {
            app.finish_range();
            app.toggle_select_all();
        }
        KeyCode::Char('d') | KeyCode::Delete => do_delete(app, false),
        _ => {}
    }
}

/// Removes entries from memory and immediately rewrites the history file.
fn do_delete(app: &mut App, cursor_only: bool) {
    let targets = app.delete_targets(cursor_only);
    if targets.is_empty() {
        return;
    }
    let removed = app.remove(&targets);
    if let Err(error) = history::save(&app.history_path, &app.entries) {
        app.error_msg = Some(format!(
            "could not write {}: {error} ({removed} entries removed in memory only)",
            app.history_path.display()
        ));
    } else {
        app.deleted += removed;
    }
}

fn handle_text_filter(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Backspace => {
            app.text_filter.pop();
            app.apply_filters();
        }
        KeyCode::Char(c) if !c.is_control() => {
            app.text_filter.push(c);
            app.apply_filters();
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum LengthFilter {
    Min,
    Max,
}

fn handle_length_filter(app: &mut App, key: KeyEvent, filter: LengthFilter) {
    let current = match filter {
        LengthFilter::Min => app.min_length,
        LengthFilter::Max => app.max_length,
    };
    let updated = match key.code {
        KeyCode::Enter | KeyCode::Esc => {
            app.mode = Mode::Normal;
            return;
        }
        KeyCode::Backspace => pop_digit(current),
        KeyCode::Char(c) if c.is_ascii_digit() => match push_digit(current, c) {
            Some(value) => Some(value),
            None => return,
        },
        _ => return,
    };
    match filter {
        LengthFilter::Min => app.min_length = updated,
        LengthFilter::Max => app.max_length = updated,
    }
    app.apply_filters();
}

fn push_digit(current: Option<usize>, digit: char) -> Option<usize> {
    let mut text = current.map(|value| value.to_string()).unwrap_or_default();
    text.push(digit);
    text.parse().ok()
}

fn pop_digit(current: Option<usize>) -> Option<usize> {
    let text = current.map(|value| value.to_string()).unwrap_or_default();
    let text = &text[..text.len().saturating_sub(1)];
    if text.is_empty() {
        None
    } else {
        text.parse().ok()
    }
}
