use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let mode_str = match app.mode {
        crate::app::Mode::Normal => "NORMAL",
        crate::app::Mode::Command => "COMMAND",
        crate::app::Mode::Insert => "INSERT",
    };
    let img = if app.image_loaded {
        "image:loaded"
    } else {
        "image:none"
    };
    let text = format!("[{mode_str}] {img} | {}", app.status_msg);
    let p = Paragraph::new(text).style(Style::default().fg(Color::White).bg(Color::Blue));
    f.render_widget(p, area);
}
