use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{details_text, App, Focus};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(idx) = app.selected_tree_idx() {
        if let Some(node) = app.tree.get(idx) {
            details_text(node)
        } else {
            "No node selected".into()
        }
    } else {
        "No node selected".into()
    };
    let title = if app.focus == Focus::Details { "Details *" } else { "Details" };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}
