use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Focus, details_text};

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
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
