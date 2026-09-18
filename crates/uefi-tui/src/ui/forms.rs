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

const FORMS_SCROLL_PAD: usize = 3;

/// Скролл details «Form»: follow info/marker-строки с гистерезисом дерева,
/// без цели — clamp. Спека R8.
pub fn follow_offset(prev: u16, target: Option<usize>, total: usize, inner_h: usize) -> u16 {
    match target {
        Some(t) => {
            crate::tree::compute_scrolled_offset(t, prev as usize, inner_h, total, FORMS_SCROLL_PAD)
                as u16
        }
        None => (prev as usize).min(total.saturating_sub(inner_h)) as u16,
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
    let fd = crate::forms::form_details(&app.forms, &rows, app.forms.cursor);
    let anchor = match rows.get(app.forms.cursor) {
        Some(FormsRow::Form { key, .. }) => Some(format!(
            "{}|{}|{}",
            key.target, key.formset_guid, key.form_id_ifr
        )),
        _ => None,
    };
    if app.forms.details_anchor != anchor {
        app.forms.details_scroll = 0;
        app.forms.details_anchor = anchor;
    }
    let inner_h = cols[1].height.saturating_sub(2) as usize;
    let total = fd.text.lines().count();
    let off = follow_offset(
        app.forms.details_scroll,
        fd.info_line.or(fd.marker_line),
        total,
        inner_h,
    );
    app.forms.details_scroll = off;
    f.render_widget(
        Paragraph::new(fd.text)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((off, 0))
            .block(
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

    #[test]
    fn follow_offset_follows_target_and_keeps_window() {
        assert_eq!(
            follow_offset(0, Some(50), 100, 10),
            44,
            "цель ниже окна — докрутка с pad"
        );
        assert_eq!(
            follow_offset(44, Some(50), 100, 10),
            44,
            "цель в окне — офсет на месте"
        );
        assert_eq!(follow_offset(90, None, 100, 10), 90);
        assert_eq!(follow_offset(90, None, 20, 10), 10, "clamp по total");
        assert_eq!(follow_offset(0, Some(0), 100, 10), 0);
    }

    #[test]
    fn details_anchor_reset_on_form_change() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        app.forms.cursor = 1;
        app.forms.details_scroll = 7;
        app.forms.details_anchor = Some("old|S|1".into());
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        assert_eq!(
            app.forms.details_scroll, 0,
            "anchor сменился (другая форма) — скролл сброшен"
        );
    }
}
