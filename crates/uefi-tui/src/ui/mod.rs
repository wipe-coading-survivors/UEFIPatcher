pub mod cmdline;
pub mod details;
pub mod forms;
pub mod help;
pub mod registry;
pub mod status;
pub mod tree;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;

pub fn render(f: &mut Frame, app: &mut App) {
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
    match app.view {
        crate::app::View::Image => {
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(vertical[1]);
            tree::render(f, horizontal[0], app);
            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(horizontal[1]);
            details::render(f, right[0], app);
            registry::render(f, right[1], app);
        }
        crate::app::View::Forms => forms::render(f, vertical[1], app),
    }
    cmdline::render(f, vertical[2], app);
    render_hint(f, vertical[3], app);
}

fn render_hint(f: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    let hint = match app.mode {
        crate::app::Mode::Normal if app.view == crate::app::View::Forms => {
            if app.forms.show_strings {
                "NORMAL[Forms/Strings]: j/k move · /filter · S/Esc close · Ctrl-hjkl focus · :cmd · ?help · q"
            } else {
                "NORMAL[Forms]: j/k move · h/l collapse/expand · S strings · Tab image-view · :cmd · ?help · q"
            }
        }
        crate::app::Mode::Normal => match app.focus {
            crate::app::Focus::Tree => {
                "NORMAL[Tree]: j/k move · h/l collapse/expand · i/r/d · Ctrl-hjkl focus · :cmd · ?help · q"
            }
            crate::app::Focus::Details => "NORMAL[Details]: Ctrl-hjkl focus · :cmd · ?help · q",
            crate::app::Focus::Registry => {
                "NORMAL[Registry]: j/k select · Enter pick · Ctrl-hjkl focus · ?help · q"
            }
        },
        crate::app::Mode::Command => "COMMAND: Enter execute · Esc cancel · Backspace",
        crate::app::Mode::Insert => "INSERT: Enter execute · Esc cancel · Backspace",
    };
    let p = Paragraph::new(hint).block(Block::default().borders(Borders::NONE));
    f.render_widget(p, area);
}
