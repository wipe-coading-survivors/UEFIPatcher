use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, Focus, details_text};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.selected_is_nvar() {
        render_nvar(f, area, app);
        return;
    }
    let content = if let Some(idx) = app.selected_tree_idx() {
        if let Some(node) = app.tree.get(idx) {
            details_text(node)
        } else {
            "No node selected".into()
        }
    } else {
        "No node selected".into()
    };
    let title = if app.focus == Focus::Details {
        "Details *"
    } else {
        "Details"
    };
    let path = app.selected_path();
    if app.details_anchor != path {
        app.details_scroll = 0;
        app.details_anchor = path;
    }
    let total = content.lines().count();
    let inner_h = area.height.saturating_sub(2) as usize;
    app.details_scroll = (app.details_scroll as usize).min(total.saturating_sub(inner_h)) as u16;
    let p = Paragraph::new(content)
        .scroll((app.details_scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

/// Трёхсекционная панель NVAR-стора (спека nvar-op §7): (1) мета как
/// сейчас, кап по высоте; (2) список переменных (курсор j/k); (3) hex
/// выбранной переменной (скролл PgUp/PgDn).
fn render_nvar(f: &mut Frame, area: Rect, app: &mut App) {
    let title = if app.focus == Focus::Details {
        "Details *"
    } else {
        "Details"
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let node_text = app
        .selected_tree_idx()
        .and_then(|i| app.tree.get(i))
        .map(details_text)
        .unwrap_or_else(|| "No node selected".into());
    let meta_lines = node_text.lines().count() as u16;
    let h = inner.height;
    let meta_h = meta_lines.min((h * 2 / 5).max(3));
    let hex_rows = hex_lines(app).len() as u16;
    let hex_h = hex_rows.min((h / 4).max(2));
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(meta_h),
            Constraint::Min(3),
            Constraint::Length(hex_h),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(node_text), zones[0]);
    render_vars(f, zones[1], app);
    let hex = if app.nvar.stores.is_empty() {
        "loading…".to_string()
    } else {
        hex_lines(app).join("\n")
    };
    app.nvar.hex_viewport = zones[2].height as usize;
    let max_scroll = hex_rows.saturating_sub(zones[2].height);
    app.nvar.hex_scroll = app.nvar.hex_scroll.min(max_scroll);
    f.render_widget(
        Paragraph::new(hex).scroll((app.nvar.hex_scroll, 0)),
        zones[2],
    );
}

fn render_vars(f: &mut Frame, mid: Rect, app: &mut App) {
    if app.nvar.stores.is_empty() {
        f.render_widget(Paragraph::new("loading…"), mid);
        return;
    }
    let items: Vec<ListItem> = app
        .nvar_vars()
        .iter()
        .map(|v| {
            ListItem::new(format!(
                "{:<20} {:<36} {:#010x} {:>7} {:#04x}",
                v.name,
                if v.guid.is_empty() {
                    "-".to_string()
                } else {
                    v.guid.clone()
                },
                v.offset,
                format!("{:#x}", v.size),
                v.attributes
            ))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(ratatui::style::Style::default().bg(ratatui::style::Color::DarkGray));
    let total = app.nvar_vars().len();
    let inner_h = mid.height as usize;
    crate::ui::scroll::sync_list_state(
        &mut app.nvar.list_state,
        app.nvar.cursor,
        total,
        inner_h,
        crate::ui::scroll::SCROLL_PAD,
        (total > 0).then_some(app.nvar.cursor),
    );
    f.render_stateful_widget(list, mid, &mut app.nvar.list_state);
}

/// Hex-строки данных выбранной переменной: 16 байт/строку с офсетами
/// относительно данных переменной (спека nvar-op §7).
fn hex_lines(app: &App) -> Vec<String> {
    let Some(v) = app.nvar_vars().get(app.nvar.cursor) else {
        return vec!["(no data)".into()];
    };
    if v.data.is_empty() {
        return vec!["(no data)".into()];
    }
    v.data
        .chunks(16)
        .enumerate()
        .map(|(i, c)| {
            let bytes = c
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("+{:04x}  {bytes}", i * 16)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_nvar_selected() -> crate::app::App {
        let mut app = crate::app::App::new();
        app.tree = vec![crate::app::TreeNode {
            path: "0/0".into(),
            depth: 1,
            node_type: 66,
            subtype: 0x01,
            guid: None,
            name: "NVRAM store".into(),
            region: String::new(),
            action: 50,
            expanded: false,
            has_children: false,
            is_nvar: true,
        }];
        app.cursor = 0;
        app.focus = crate::app::Focus::Details;
        app
    }

    #[test]
    fn nvar_store_renders_three_sections() {
        let mut app = app_with_nvar_selected();
        app.nvar.stores = vec![uefi_proto::NvarStoreInfo {
            path: "0/0".into(),
            vars: vec![var_row("Setup", 0x500088, &[0xAA; 40])],
            ..Default::default()
        }];
        app.nvar.key = Some("i:0/0".into());
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text: String = (0..30)
            .map(|y| {
                (0..100)
                    .map(|x| t.backend().buffer().get(x, y).symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Path:"), "мета-секция на месте");
        assert!(text.contains("Setup"), "строка переменной видна");
        assert!(text.contains("aa aa"), "hex-данные видны");
        assert!(text.contains("+0010"), "hex-строка с офсетом 16");
    }

    #[test]
    fn nvar_pane_loading_state() {
        let mut app = app_with_nvar_selected();
        app.nvar.key = None;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text: String = (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| t.backend().buffer().get(x, y).symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("loading…"));
    }

    #[test]
    fn non_store_node_renders_as_before() {
        let mut app = crate::app::App::new();
        app.tree = vec![crate::app::TreeNode {
            path: "0".into(),
            depth: 0,
            node_type: 62,
            subtype: 0,
            guid: None,
            name: String::new(),
            region: String::new(),
            action: 50,
            expanded: true,
            has_children: true,
            is_nvar: false,
        }];
        app.cursor = 0;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text: String = (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| t.backend().buffer().get(x, y).symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Children:"));
        assert!(!text.contains("loading…"));
    }

    fn var_row(name: &str, offset: u64, data: &[u8]) -> uefi_proto::NvarVarInfo {
        uefi_proto::NvarVarInfo {
            name: name.into(),
            guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
            offset,
            size: data.len() as u32,
            attributes: 0x82,
            depth: 1,
            data: data.to_vec(),
        }
    }
}
