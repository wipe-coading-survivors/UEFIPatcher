use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Up,
    Down,
    Quit,
    Tick,
}

pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if !event::poll(timeout).unwrap_or_default() {
        return Some(AppEvent::Tick);
    }
    if let Event::Key(k) = event::read().ok()? {
        if k.kind != KeyEventKind::Press {
            return None;
        }
        return Some(match k.code {
            KeyCode::Char(c) if k.modifiers.contains(KeyModifiers::CONTROL) => AppEvent::Ctrl(c),
            KeyCode::Char(c) => AppEvent::Key(c),
            KeyCode::Enter => AppEvent::Enter,
            KeyCode::Esc => AppEvent::Esc,
            KeyCode::Backspace => AppEvent::Backspace,
            KeyCode::Up => AppEvent::Up,
            KeyCode::Down => AppEvent::Down,
            _ => return None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_on_timeout() {
        let e = poll_event(Duration::from_millis(0));
        assert_eq!(e, Some(AppEvent::Tick));
    }
}
