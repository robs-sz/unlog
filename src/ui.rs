//! Rendering: filter bar, entry list, status bar.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

use crate::app::{App, Mode};

/// Vertical split of the terminal: filter bar, list, status bar.
pub struct Areas {
    pub filter: Rect,
    pub list: Rect,
    pub help: Rect,
}

pub fn layout(area: Rect) -> Areas {
    let [filter, list, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    Areas { filter, list, help }
}

pub fn render(frame: &mut Frame, app: &App) {
    let areas = layout(frame.area());

    frame.render_widget(Paragraph::new(filter_line(app)), areas.filter);
    frame.render_widget(Paragraph::new(status_line(app)), areas.help);

    // Only the visible window is materialised: formatting every entry of a
    // large history on each frame is wasteful. `App` owns the scroll offset,
    // so the window is already sliced and must be rendered without an
    // additional `ListState` offset.
    let height = areas.list.height as usize;
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(height)
        .map(|(position, &index)| row(app, position, index))
        .collect();
    frame.render_widget(List::new(items), areas.list);
}

fn filter_line(app: &App) -> Line<'_> {
    let picked = Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let idle = Style::default().fg(Color::DarkGray);

    let text_style = if app.mode == Mode::FilterText {
        picked
    } else {
        idle
    };
    let min_style = if app.mode == Mode::FilterMinLen {
        picked
    } else if app.min_length.is_some() {
        Style::default().fg(Color::Cyan)
    } else {
        idle
    };
    let max_style = if app.mode == Mode::FilterMaxLen {
        picked
    } else if app.max_length.is_some() {
        Style::default().fg(Color::Cyan)
    } else {
        idle
    };

    let mut spans = vec![
        Span::styled(format!("Text: \"{}\" ", app.text_filter), text_style),
        Span::styled("| Len: ", idle),
        Span::styled(
            app.min_length
                .map_or("\u{2265}-".to_string(), |min| format!("\u{2265}{min}")),
            min_style,
        ),
        Span::styled(" ", idle),
        Span::styled(
            app.max_length
                .map_or("\u{2264}-".to_string(), |max| format!("\u{2264}{max}")),
            max_style,
        ),
        Span::styled(" | ", idle),
        Span::styled(
            format!("Entries: {}/{}", app.filtered.len(), app.entries.len()),
            Style::default().fg(Color::White),
        ),
    ];
    if !app.selected.is_empty() {
        spans.push(Span::styled(
            format!(" | Sel: {}", app.selected.len()),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn status_line(app: &App) -> Line<'_> {
    match &app.error_msg {
        Some(message) => Line::styled(
            format!(" {message}"),
            Style::default().fg(Color::Black).bg(Color::Red),
        ),
        None => Line::styled(
            " / filter  m minlen  M maxlen  Space select  d delete  Ctrl+d delete one  q quit",
            Style::default().fg(Color::DarkGray),
        ),
    }
}

fn row(app: &App, position: usize, index: usize) -> ListItem<'_> {
    let entry = &app.entries[index];
    let selected = app.selected.contains(&index);
    let cursor = position == app.cursor;

    let mut style = Style::default();
    if selected {
        style = style.fg(Color::Green).add_modifier(Modifier::BOLD);
    }
    if cursor {
        // The cursor wins over the selection colour, but keeps it.
        style = style.add_modifier(Modifier::REVERSED);
    }

    let marker = if selected { "[\u{2022}]" } else { "[ ]" };
    ListItem::new(Line::styled(
        format!("{marker} {index:>5}  {}", sanitize(&entry.command)),
        style,
    ))
}

/// Commands can span lines; the list shows one row per entry.
fn sanitize(command: &str) -> String {
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
