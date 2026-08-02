use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;
use crate::theme::*;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .tree
        .iter()
        .enumerate()
        .map(|(i, node)| {
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
            let line = Line::from(vec![
                Span::styled(
                    format!("{indent}{expand} {icon} "),
                    Style::default().fg(type_color(node.node_type)),
                ),
                Span::styled(format!("{} ", node.name), Style::default().fg(color)),
                Span::raw(format!("{} {marker}", node.guid.as_deref().unwrap_or(""))),
            ]);
            if i == app.cursor {
                ListItem::new(line).style(Style::default().bg(Color::DarkGray))
            } else {
                ListItem::new(line)
            }
        })
        .collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Tree"));
    f.render_widget(list, area);
}
