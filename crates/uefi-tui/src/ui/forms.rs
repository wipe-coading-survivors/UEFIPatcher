use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, FormsFocus};
use crate::forms::FormsRow;
use crate::tree::compute_scrolled_offset;

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
    let list_focus = app.forms.focus == FormsFocus::List;
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Forms")
                .border_style(focus_style(list_focus)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    let total = rows.len();
    let inner_h = cols[0].height.saturating_sub(2) as usize;
    let cursor = if rows.is_empty() {
        0
    } else {
        app.forms.cursor.min(total - 1)
    };
    let prev_off = app.forms_list_state.offset();
    let new_off = compute_scrolled_offset(cursor, prev_off, inner_h, total, FORMS_SCROLL_PAD);
    app.forms_list_state
        .select(if rows.is_empty() { None } else { Some(cursor) });
    *app.forms_list_state.offset_mut() = new_off;
    f.render_stateful_widget(list, cols[0], &mut app.forms_list_state);
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
        app.forms.details_followed = None;
    }
    let inner_h = cols[1].height.saturating_sub(2) as usize;
    let total = fd.text.lines().count();
    let target = fd.info_line.or(fd.marker_line);
    let off = if app.forms.details_followed == target {
        follow_offset(app.forms.details_scroll, None, total, inner_h)
    } else {
        let off = follow_offset(app.forms.details_scroll, target, total, inner_h);
        app.forms.details_followed = target;
        off
    };
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
    let title = if app.forms.strings_filter.is_empty() {
        "Strings".to_string()
    } else {
        format!("Strings (filter: {})", app.forms.strings_filter)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    let total = visible.len();
    let inner_h = area.height.saturating_sub(2) as usize;
    let pos = visible
        .iter()
        .position(|&i| i == app.forms.strings_cursor)
        .unwrap_or(0);
    let cursor = if visible.is_empty() {
        0
    } else {
        pos.min(total - 1)
    };
    let prev_off = app.strings_list_state.offset();
    let new_off = compute_scrolled_offset(cursor, prev_off, inner_h, total, FORMS_SCROLL_PAD);
    app.strings_list_state.select(if visible.is_empty() {
        None
    } else {
        Some(cursor)
    });
    *app.strings_list_state.offset_mut() = new_off;
    f.render_stateful_widget(list, area, &mut app.strings_list_state);
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

    #[test]
    fn forms_details_manual_scroll_survives_until_target_moves() {
        let mut app = crate::app::App::new();
        app.forms.flat_mode = true;
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        app.forms.cursor = 1;
        app.forms.questions_key = Some(fk(1));
        app.forms.questions = (0..30)
            .map(|i| uefi_proto::QuestionSummary {
                question_id: 0x210 + i,
                prompt: format!("q{i}"),
                ..Default::default()
            })
            .collect();
        app.forms.question_cursor = 10;
        app.forms.gates = (0..30)
            .map(|i| uefi_proto::GateInfo {
                gate_kind: "suppress".into(),
                expression: format!("e{i}"),
                ..Default::default()
            })
            .collect();
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let rested = app.forms.details_scroll;
        assert!(
            rested > 0,
            "авто-follow включился: маркер question_cursor=10 ниже окна inner_h=10"
        );
        app.forms.details_scroll = rested.saturating_add(10);
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        assert_eq!(
            app.forms.details_scroll,
            rested + 10,
            "та же цель — ручной PgDn не откатывается (clamp-only)"
        );
        app.forms.question_cursor = 25;
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        assert_ne!(
            app.forms.details_scroll,
            rested + 10,
            "цель сместилась (question_cursor 10→25) — follow догоняет новый маркер"
        );
    }

    #[test]
    fn forms_list_offset_kept_when_cursor_walks_up() {
        let mut app = crate::app::App::new();
        app.forms.flat_mode = true;
        app.forms.forms = (0..30)
            .map(|i| uefi_proto::FormInfo {
                form_id: format!("f{i}"),
                formset_guid: "S".into(),
                form_id_ifr: i,
                title: format!("F{i}"),
                visible: true,
            })
            .collect();
        app.forms.expanded = ["S".into()].into();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 10)).unwrap();
        for _ in 0..20 {
            app.forms_cursor_down();
        }
        terminal
            .draw(|f| super::render(f, f.area(), &mut app))
            .unwrap();
        let off = app.forms_list_state.offset();
        assert!(off > 0, "прокрутка началась");
        app.forms_cursor_up();
        terminal
            .draw(|f| super::render(f, f.area(), &mut app))
            .unwrap();
        assert_eq!(
            app.forms_list_state.offset(),
            off,
            "вверх двигает курсор, не страницу"
        );
        assert!(app.forms.cursor < 20);
    }

    #[test]
    fn strings_list_uses_persistent_state() {
        let mut app = crate::app::App::new();
        app.forms.show_strings = true;
        app.forms.strings = (0..30)
            .map(|i| uefi_proto::StringInfo {
                language: "en".into(),
                string_id: i,
                text: format!("s{i}"),
            })
            .collect();
        for _ in 0..20 {
            app.strings_cursor_down();
        }
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 10)).unwrap();
        terminal
            .draw(|f| super::render(f, f.area(), &mut app))
            .unwrap();
        assert!(app.strings_list_state.offset() > 0);
    }
}
