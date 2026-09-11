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

fn row_text(row: &FormsRow) -> String {
    match row {
        FormsRow::FormSet { guid, expanded } => {
            let marker = if *expanded { "▾" } else { "▸" };
            format!("{marker} FormSet {}", crate::forms::short_guid(guid))
        }
        FormsRow::Form {
            key,
            visible,
            depth,
            has_children,
            expanded,
            ..
        } => {
            let vis = if *visible { "[V]" } else { "[H]" };
            let marker = if !has_children {
                " "
            } else if *expanded {
                "▾"
            } else {
                "▸"
            };
            format!(
                "{}{marker} {:<6} {:<34} {vis}",
                " ".repeat(2 * depth),
                key.form_id_ifr,
                key.title
            )
        }
        FormsRow::DanglingRef { form_id, depth } => format!(
            "{}  ! {:<6} (dangling REF target)",
            " ".repeat(2 * depth),
            form_id
        ),
    }
}

fn row_item(row: &FormsRow) -> ListItem<'static> {
    ListItem::from(row_text(row))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::FormKey;

    fn fk(id: u32) -> FormKey {
        FormKey {
            target: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: id,
            title: "Main".into(),
        }
    }

    fn form_row(id: u32, depth: usize, has_children: bool, expanded: bool) -> FormsRow {
        FormsRow::Form {
            key: fk(id),
            visible: true,
            depth,
            path: String::new(),
            has_children,
            expanded,
        }
    }

    #[test]
    fn expanded_parent_shows_down_triangle() {
        let row = form_row(1, 1, true, true);
        assert!(row_text(&row).starts_with("  ▾ 1     "));
    }

    #[test]
    fn collapsed_parent_shows_right_triangle() {
        let row = form_row(1, 1, true, false);
        assert!(row_text(&row).starts_with("  ▸ 1     "));
    }

    #[test]
    fn leaf_keeps_two_space_shift() {
        let row = form_row(2, 2, false, false);
        assert!(
            row_text(&row).starts_with("      2     "),
            "лист без маркера, но с тем же +2 сдвигом"
        );
    }

    #[test]
    fn dangling_ref_bang_aligned_with_form_ids() {
        let dangling = FormsRow::DanglingRef {
            form_id: 99,
            depth: 2,
        };
        let form = form_row(99, 2, false, false);
        let d = row_text(&dangling);
        let f = row_text(&form);
        assert_eq!(
            d.find('!'),
            f.find(|c: char| c.is_ascii_digit()),
            "'!' DanglingRef в колонке form id того же depth: {d:?} vs {f:?}"
        );
        assert!(d.starts_with("      ! 99    "));
    }
}
