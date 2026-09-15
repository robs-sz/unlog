//! Rendering: filter bar, wrapped entry list, status bar.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Mode};
use crate::text;

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
    frame.render_widget(Paragraph::new(list_lines(app, areas.list)), areas.list);
}

/// Builds the visible rows, wrapping each entry's command across as many rows as
/// it needs. Entries are wrapped only for the window being drawn, so a large
/// history costs no more than a screenful of work per frame.
fn list_lines<'a>(app: &'a App, area: Rect) -> Vec<Line<'a>> {
    let height = area.height as usize;
    let width = app.text_width();
    let digits = app.index_width();
    let indent = " ".repeat(app.prefix_width());
    let mut lines = Vec::with_capacity(height);

    for position in app.top..app.filtered.len() {
        if lines.len() >= height {
            break;
        }
        let index = app.filtered[position];
        let marker = if app.selected.contains(&index) {
            "[\u{2022}]"
        } else {
            "[ ]"
        };
        let style = row_style(app, position);
        let command = text::sanitize(&app.entries[index].command);

        for (row, chunk) in text::rows(&command, width)
            .into_iter()
            .enumerate()
            .skip(app.skip)
        {
            if lines.len() >= height {
                break;
            }
            if row == 0 {
                lines.push(Line::styled(
                    format!("{marker} {index:>digits$}  {chunk}"),
                    style,
                ));
            } else {
                lines.push(Line::styled(format!("{indent}{chunk}"), style));
            }
        }
    }

    lines
}

/// The cursor wins over the selection colour, but keeps it.
fn row_style(app: &App, position: usize) -> Style {
    let mut style = Style::default();
    if app
        .filtered
        .get(position)
        .is_some_and(|index| app.selected.contains(index))
    {
        style = style.fg(Color::Green).add_modifier(Modifier::BOLD);
    }
    if position == app.cursor {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
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
