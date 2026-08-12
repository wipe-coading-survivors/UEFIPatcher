use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::{App, Focus};
use crate::theme::*;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&idx| {
            let node = &app.tree[idx];
            let indent = "  ".repeat(node.depth);
            let icon = type_icon(node.node_type, node.subtype);
            let expand = if !node.has_children {
                " "
            } else if node.expanded {
                "\u{EAB4}"
            } else {
                "\u{EAB6}"
            };
            let marker = action_marker(node.action);
            let color = action_color(node.action);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{indent}{expand} {icon} "),
                    Style::default().fg(type_color(node.node_type)),
                ),
                Span::styled(format!("{} ", node.name), Style::default().fg(color)),
                Span::raw(format!("{} {marker}", node.guid.as_deref().unwrap_or(""))),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    let sel = if visible.is_empty() { None } else { Some(app.cursor.min(visible.len() - 1)) };
    state.select(sel);
    let title = if app.focus == Focus::Tree { "Tree *" } else { "Tree" };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list, area, &mut state);
}
