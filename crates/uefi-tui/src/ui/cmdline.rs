use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let content = match app.mode {
        Mode::Command => format!(":{}▌", app.cmdline),
        Mode::Insert => {
            if app.insert_cmd.is_empty() {
                format!("> {}▌", app.cmdline)
            } else {
                format!("{}> {}▌", app.insert_cmd, app.cmdline)
            }
        }
        Mode::Normal => String::from("Press : for commands, i/r/d for insert/replace/remove"),
    };
    let title = match app.mode {
        Mode::Insert => "Insert",
        _ => "Command",
    };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}
