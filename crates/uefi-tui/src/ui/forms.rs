use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, FormsFocus};
use crate::forms::FormsRow;
use crate::ui::scroll;

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
    if app.forms.show_varstores {
        render_varstores(f, area, app);
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
    scroll::render_scrolled_list(
        f,
        list,
        cols[0],
        &mut app.forms_list_state,
        app.forms.cursor,
        rows.len(),
        scroll::SCROLL_PAD,
    );

    let panel = crate::forms::form_panel(&app.forms, &rows, app.forms.cursor);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Form")
        .border_style(focus_style(!list_focus));
    let inner = block.inner(cols[1]);
    f.render_widget(block, cols[1]);
    let cap = 3.max(inner.height as usize * 45 / 100) as u16;
    let mut header_h = (panel.header.len() as u16).min(inner.height);
    let mut bottom_h = (panel.bottom.len() as u16).min(cap);
    if inner.height < header_h + bottom_h + 1 {
        bottom_h = bottom_h.min(2);
    }
    if inner.height < header_h + bottom_h + 1 {
        header_h = header_h.min(2);
    }
    if inner.height < header_h + bottom_h + 1 {
        bottom_h = bottom_h.min(inner.height.saturating_sub(header_h + 1));
    }
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Min(1),
            Constraint::Length(bottom_h),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(panel.header.join("\n")), zones[0]);
    render_middle(f, zones[1], app, &panel);
    f.render_widget(Paragraph::new(panel.bottom.join("\n")), zones[2]);
}

fn render_middle(f: &mut Frame, mid: Rect, app: &mut App, panel: &crate::forms::FormPanel) {
    if !panel.questions_ready {
        return;
    }
    if panel.questions.is_empty() {
        let mut lines = vec!["(no questions)".to_string()];
        lines.extend(panel.gates.iter().cloned());
        f.render_widget(Paragraph::new(lines.join("\n")), mid);
        return;
    }
    let mut items: Vec<ListItem> = panel
        .questions
        .iter()
        .map(|s| ListItem::new(s.clone()))
        .collect();
    items.extend(panel.gates.iter().map(|s| {
        ListItem::new(Line::styled(
            s.clone(),
            Style::default().add_modifier(Modifier::DIM),
        ))
    }));
    let total = items.len();
    let cursor = app.forms.question_cursor.min(panel.questions.len() - 1);
    let list = List::new(items).highlight_style(Style::default().bg(Color::DarkGray));
    let inner_h = mid.height as usize;
    scroll::sync_list_state(
        &mut app.forms.questions_state,
        cursor,
        total,
        inner_h,
        scroll::SCROLL_PAD,
        Some(cursor),
    );
    app.forms.questions_viewport = inner_h;
    f.render_stateful_widget(list, mid, &mut app.forms.questions_state);
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
    let pos = visible
        .iter()
        .position(|&i| i == app.forms.strings_cursor)
        .unwrap_or(0);
    scroll::render_scrolled_list(
        f,
        list,
        area,
        &mut app.strings_list_state,
        pos,
        total,
        scroll::SCROLL_PAD,
    );
}

/// Панель varstores ('V'): карта деклараций формсета выбранной формы;
/// строка 0 — target, подсветка смещена на +1. Спека
/// varstore-contract §5.
pub fn render_varstores(f: &mut Frame, area: Rect, app: &mut App) {
    let target = app.forms.varstores_target.clone().unwrap_or_default();
    let rows: Vec<ListItem> = std::iter::once(ListItem::new(format!("Varstores: {target}")))
        .chain(app.forms.varstores.iter().map(|v| {
            ListItem::new(format!(
                "id={:#06x} guid={} size={:#06x} \"{}\"",
                v.id,
                if v.guid.is_empty() {
                    "-".into()
                } else {
                    v.guid.clone()
                },
                v.size,
                v.name
            ))
        }))
        .collect();
    let list = List::new(rows)
        .block(Block::default().borders(Borders::ALL).title("Varstores"))
        .highlight_style(Style::default().bg(Color::DarkGray));
    scroll::render_scrolled_list(
        f,
        list,
        area,
        &mut app.varstores_list_state,
        app.forms.varstores_cursor + 1,
        app.forms.varstores.len() + 1,
        scroll::SCROLL_PAD,
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

    fn row_of(t: &ratatui::Terminal<ratatui::backend::TestBackend>, y: u16) -> String {
        (0..80)
            .map(|x| t.backend().buffer().get(x, y).symbol().to_string())
            .collect()
    }

    fn panel_text(t: &ratatui::Terminal<ratatui::backend::TestBackend>, y_max: u16) -> String {
        (0..y_max)
            .map(|y| row_of(t, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app_with_form(n_questions: usize) -> crate::app::App {
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
        app.forms.questions_key = Some(fk(1));
        app.forms.questions = (0..n_questions)
            .map(|i| uefi_proto::QuestionSummary {
                question_id: 0x210 + i as u32,
                prompt: format!("q{i}"),
                kind: "numeric".into(),
                ..Default::default()
            })
            .collect();
        app
    }

    #[test]
    fn three_zone_header_stays_put_on_last_question() {
        let mut app = app_with_form(30);
        app.forms.question_cursor = 29;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(text.contains("Form:    Main"), "шапка формы на месте");
        assert!(
            text.contains("Questions (30): prompt · qid · kind"),
            "колонки-хедер на месте"
        );
        assert!(text.contains("q29"), "последний вопрос виден");
    }

    #[test]
    fn tiny_terminal_keeps_question_row_visible() {
        let mut app = app_with_form(30);
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 8)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 8);
        assert!(text.contains("Form:    Main"), "шапка сжата, но жива");
        assert!(text.contains("q0x210"), "середина ≥1 строки — вопрос виден");
    }

    #[test]
    fn loading_state_shows_header_line_only() {
        let mut app = app_with_form(0);
        app.forms.questions_key = None;
        *app.forms.questions_state.offset_mut() = 4;
        app.forms.questions_state.select(Some(3));
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(text.contains("Questions: loading…"));
        assert_eq!(
            app.forms.questions_state.selected(),
            Some(3),
            "loading: список не рендерится — состояние нетронуто"
        );
        assert_eq!(app.forms.questions_state.offset(), 4);
    }

    #[test]
    fn bottom_zone_shows_selected_question_and_gates_tail_reachable() {
        let mut app = app_with_form(30);
        app.forms.question_cursor = 29;
        app.forms.gates = vec![
            uefi_proto::GateInfo {
                gate_kind: "suppress".into(),
                expression: "e0".into(),
                flippable: true,
                ..Default::default()
            },
            uefi_proto::GateInfo {
                gate_kind: "suppress".into(),
                expression: "e1".into(),
                ..Default::default()
            },
        ];
        app.forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x22d,
            kind: "numeric".into(),
            var_store_id: 2,
            var_offset: 0x37,
            width: 1,
            min: 1,
            max: 8,
            step: 1,
            ..Default::default()
        });
        app.forms.question_info_key = Some((fk(1), 0x22d));
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(
            text.contains("Question q0x22d"),
            "низ — детали выбранного вопроса"
        );
        assert!(text.contains("Gates (2):"), "гейт-хвост достижим скроллом");
        let sel = app.forms.questions_state.selected();
        assert_eq!(sel, Some(29), "курсор на последнем вопросе, не на гейтах");
    }

    #[test]
    fn bottom_cap_keeps_questions_visible() {
        let mut app = app_with_form(6);
        app.forms.question_cursor = 0;
        let mut options = Vec::new();
        for i in 0..12u64 {
            options.push(uefi_proto::OptionEntry {
                value: i,
                string_id: 0,
                text: format!("opt{i}"),
                ..Default::default()
            });
        }
        app.forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x210,
            kind: "one_of".into(),
            var_store_id: 2,
            var_offset: 0x40,
            width: 1,
            options,
            ..Default::default()
        });
        app.forms.question_info_key = Some((fk(1), 0x210));
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 20);
        assert!(text.contains("options: 0x0"), "начало options видно");
        assert!(
            text.contains("q0x210"),
            "вопросы не выдавлены низом-переростком"
        );
        assert!(text.contains("Form:    Main"), "шапка на месте");
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
