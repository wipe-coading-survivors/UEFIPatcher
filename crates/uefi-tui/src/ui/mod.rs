pub mod cmdline;
pub mod details;
pub mod help;
pub mod status;
pub mod tree;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App) {
    let full = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(full);
    status::render(f, vertical[0], app);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(vertical[1]);
    tree::render(f, horizontal[0], app);
    details::render(f, horizontal[1], app);
    cmdline::render(f, vertical[2], app);
    render_hint(f, vertical[3], app);
}

fn render_hint(f: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    let hint = match app.mode {
        crate::app::Mode::Normal => "NORMAL: j/k move · Space expand · :commands · ?help · q quit",
        crate::app::Mode::Command => "COMMAND: Enter execute · Esc cancel",
        crate::app::Mode::Insert => "INSERT: type text · Esc back to Normal",
    };
    let p = Paragraph::new(hint).block(Block::default().borders(Borders::NONE));
    f.render_widget(p, area);
}
