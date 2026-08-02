use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let content = match app.mode {
        Mode::Command => format!(":{}", app.cmdline),
        _ => String::from("Press : for commands"),
    };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title("Command"));
    f.render_widget(p, area);
}
