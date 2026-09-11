use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, FormsFocus};
use crate::forms::FormsRow;

fn focus_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn row_item(row: &FormsRow) -> ListItem<'static> {
    match row {
        FormsRow::FormSet { guid, expanded } => {
            let marker = if *expanded { "▾" } else { "▸" };
            ListItem::from(format!(
                "{marker} FormSet {}",
                crate::forms::short_guid(guid)
            ))
        }
        FormsRow::Form {
            key,
            visible,
            depth,
            ..
        } => {
            let vis = if *visible { "[V]" } else { "[H]" };
            ListItem::from(format!(
                "{}{:<6} {:<34} {vis}",
                " ".repeat(2 * depth),
                key.form_id_ifr,
                key.title
            ))
        }
        FormsRow::DanglingRef { form_id, depth } => ListItem::from(format!(
            "{}! {:<6} (dangling REF target)",
            " ".repeat(2 * depth),
            form_id
        )),
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.forms.show_strings {
        render_strings(f, area, app);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    let rows = app.forms_rows();
    let items: Vec<ListItem> = rows.iter().map(row_item).collect();
    let mut state = ListState::default();
    if rows.is_empty() {
        state.select(None);
    } else {
        state.select(Some(app.forms.cursor.min(rows.len() - 1)));
    }
    let list_focus = app.forms.focus == FormsFocus::List;
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Forms")
                    .border_style(focus_style(list_focus)),
            )
            .highlight_style(Style::default().bg(Color::DarkGray)),
        cols[0],
        &mut state,
    );
    let text = crate::forms::form_details_text(&app.forms, &rows, app.forms.cursor);
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Form")
                .border_style(focus_style(!list_focus)),
        ),
        cols[1],
    );
}

fn render_strings(f: &mut Frame, area: Rect, app: &mut App) {
    let visible = app.strings_visible();
    let items: Vec<ListItem> = visible
        .iter()
        .filter_map(|&i| app.forms.strings.get(i))
        .map(|s| ListItem::from(format!("{}  #{:<5} {}", s.language, s.string_id, s.text)))
        .collect();
    let mut state = ListState::default();
    if visible.is_empty() {
        state.select(None);
    } else {
        let pos = visible
            .iter()
            .position(|&i| i == app.forms.strings_cursor)
            .unwrap_or(0);
        state.select(Some(pos));
    }
    let title = if app.forms.strings_filter.is_empty() {
        "Strings".to_string()
    } else {
        format!("Strings (filter: {})", app.forms.strings_filter)
    };
    f.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::DarkGray)),
        area,
        &mut state,
    );
}
