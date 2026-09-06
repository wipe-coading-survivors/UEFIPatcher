use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
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
    let engine = if app.engine_online {
        ("engine:online", Color::Green)
    } else {
        ("ENGINE OFFLINE", Color::Black)
    };
    let line = Line::from(vec![
        Span::styled(format!("[{mode_str}] "), Style::default().fg(Color::White)),
        Span::styled(format!("{} ", engine.0), Style::default().fg(engine.1)),
        Span::raw(format!("{img} | ")),
        Span::raw(app.status_msg.clone()),
    ]);
    let bg = if app.engine_online {
        Color::Blue
    } else {
        Color::Red
    };
    let p = Paragraph::new(line).style(Style::default().bg(bg));
    f.render_widget(p, area);
}
