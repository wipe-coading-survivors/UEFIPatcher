pub mod cmdline;
pub mod details;
pub mod forms;
pub mod help;
pub mod menu;
pub mod registry;
pub mod status;
pub mod tree;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{App, FormsFocus};

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
    menu::render(f, vertical[2], &app.menu);
    render_hint(f, vertical[3], app);
}

fn render_hint(f: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    let hint: String = match app.mode {
        crate::app::Mode::Normal if app.view == crate::app::View::Forms => {
            if app.forms.show_strings {
                "NORMAL[Forms/Strings]: j/k move · /filter · S/Esc close · Ctrl-hjkl focus · :cmd · ?help · q".into()
            } else if app.forms.focus == FormsFocus::Details {
                "NORMAL[Forms/Details]: j/k вопрос · Enter set-value · Tab image-view · :cmd · ?help · q".into()
            } else {
                "NORMAL[Forms]: j/k move · h/l collapse/expand · v show hidden (unsuppress) · u unlock · a add · T tree/flat · S strings · Tab image-view · :cmd · ?help · q".into()
            }
        }
        crate::app::Mode::Normal => match app.focus {
            crate::app::Focus::Tree => {
                "NORMAL[Tree]: j/k move · h/l collapse/expand · i/r/d · Ctrl-hjkl focus · :cmd · ?help · q".into()
            }
            crate::app::Focus::Details => {
                "NORMAL[Details]: Ctrl-hjkl focus · :cmd · ?help · q".into()
            }
            crate::app::Focus::Registry => {
                "NORMAL[Registry]: j/k select · Enter pick · Ctrl-hjkl focus · ?help · q".into()
            }
        },
        crate::app::Mode::Command | crate::app::Mode::Insert => {
            let mode = if matches!(app.mode, crate::app::Mode::Command) {
                "COMMAND"
            } else {
                "INSERT"
            };
            if app.menu.open {
                format!("{mode}[menu]: ↑↓ select · TAB/→ accept · BackTab back · Esc close · Enter run")
            } else {
                format!("{mode}: TAB compl · ↑↓ hist · Ctrl+←→ word · Ctrl+W/U/K del · Home/End · Enter run · Esc cancel")
            }
        }
    };
    let p = Paragraph::new(hint).block(Block::default().borders(Borders::NONE));
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, MenuItem, Mode};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn hint_of(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(100, 5)).unwrap();
        t.draw(|f| render_hint(f, Rect::new(0, 0, 100, 1), app))
            .unwrap();
        (0..100)
            .map(|x| t.backend().buffer().get(x, 0).symbol().to_string())
            .collect()
    }

    #[test]
    fn command_hint_variants() {
        let mut app = App::new();
        app.mode = Mode::Command;
        let closed = hint_of(&app);
        assert!(closed.contains("TAB compl"));
        assert!(closed.contains("↑↓ hist"));
        assert!(closed.contains("Ctrl+W/U/K"));
        app.menu.open_with(vec![MenuItem {
            display: "x".into(),
            apply: "x".into(),
        }]);
        let open = hint_of(&app);
        assert!(open.contains("[menu]"));
        assert!(open.contains("↑↓ select"));
        assert!(open.contains("Esc close"));
    }

    #[test]
    fn insert_hint_prefix() {
        let mut app = App::new();
        app.mode = Mode::Insert;
        app.insert_cmd = "insert";
        assert!(hint_of(&app).contains("INSERT"));
    }
}
