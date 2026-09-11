use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::{App, Focus};
use crate::theme::*;
use crate::tree::compute_scrolled_offset;

const SCROLL_PAD: usize = 3;

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
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
            let immutable = node.node_type == TYPE_REGION;
            let icon_style = if immutable {
                immutable_style()
            } else {
                Style::default().fg(type_color(node.node_type))
            };
            let name_style = if immutable {
                immutable_style()
            } else {
                Style::default().fg(color)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{indent}{expand} {icon} "), icon_style),
                Span::styled(format!("{} ", app.node_label(node)), name_style),
                Span::raw(format!("{} {marker}", node.guid.as_deref().unwrap_or(""))),
            ]))
        })
        .collect();

    let total = visible.len();
    let inner_h = area.height.saturating_sub(2) as usize;
    app.tree_viewport_rows = inner_h;
    let cursor = if total == 0 {
        0
    } else {
        app.cursor.min(total - 1)
    };
    let prev_off = app.tree_state.offset();
    let new_off = compute_scrolled_offset(cursor, prev_off, inner_h, total, SCROLL_PAD);
    app.tree_state
        .select(if total == 0 { None } else { Some(cursor) });
    *app.tree_state.offset_mut() = new_off;

    let title = if app.focus == Focus::Tree {
        "Tree *"
    } else {
        "Tree"
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list, area, &mut app.tree_state);
}
