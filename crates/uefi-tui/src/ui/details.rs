use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(node) = app.tree.get(app.cursor) {
        format!(
            "Path:    {}\nType:    {}\nSubtype: {:#04X}\nGUID:    {}\nName:    {}\nAction:  {}\nExpanded: {}",
            node.path,
            node.node_type,
            node.subtype,
            node.guid.as_deref().unwrap_or("(none)"),
            node.name,
            node.action,
            node.expanded
        )
    } else {
        "No node selected".into()
    };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title("Details"));
    f.render_widget(p, area);
}
