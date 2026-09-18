use crossterm::event::{self, Event};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    PageUp,
    PageDown,
    Left,
    Right,
    Home,
    End,
    Delete,
    WordLeft,
    WordRight,
    Quit,
    Tick,
}

pub fn map_key(k: crossterm::event::KeyEvent) -> Option<AppEvent> {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    if k.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    Some(match k.code {
        KeyCode::Char(c) if ctrl => AppEvent::Ctrl(c),
        KeyCode::Char(c) => AppEvent::Key(c),
        KeyCode::Enter => AppEvent::Enter,
        KeyCode::Esc => AppEvent::Esc,
        KeyCode::Backspace => AppEvent::Backspace,
        KeyCode::Tab => AppEvent::Tab,
        KeyCode::BackTab => AppEvent::BackTab,
        KeyCode::Up => AppEvent::Up,
        KeyCode::Down => AppEvent::Down,
        KeyCode::PageUp => AppEvent::PageUp,
        KeyCode::PageDown => AppEvent::PageDown,
        KeyCode::Left if ctrl => AppEvent::WordLeft,
        KeyCode::Right if ctrl => AppEvent::WordRight,
        KeyCode::Left => AppEvent::Left,
        KeyCode::Right => AppEvent::Right,
        KeyCode::Home => AppEvent::Home,
        KeyCode::End => AppEvent::End,
        KeyCode::Delete => AppEvent::Delete,
        _ => return None,
    })
}

pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if !event::poll(timeout).unwrap_or_default() {
        return Some(AppEvent::Tick);
    }
    if let Event::Key(k) = event::read().ok()? {
        return map_key(k);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, m: KeyModifiers) -> Option<AppEvent> {
        map_key(KeyEvent::new(code, m))
    }

    #[test]
    fn tick_on_timeout() {
        let e = poll_event(Duration::from_millis(0));
        assert_eq!(e, Some(AppEvent::Tick));
    }

    #[test]
    fn arrows_and_words() {
        assert_eq!(key(KeyCode::Left, KeyModifiers::NONE), Some(AppEvent::Left));
        assert_eq!(
            key(KeyCode::Right, KeyModifiers::NONE),
            Some(AppEvent::Right)
        );
        assert_eq!(
            key(KeyCode::Left, KeyModifiers::CONTROL),
            Some(AppEvent::WordLeft)
        );
        assert_eq!(
            key(KeyCode::Right, KeyModifiers::CONTROL),
            Some(AppEvent::WordRight)
        );
    }

    #[test]
    fn home_end_delete() {
        assert_eq!(key(KeyCode::Home, KeyModifiers::NONE), Some(AppEvent::Home));
        assert_eq!(key(KeyCode::End, KeyModifiers::NONE), Some(AppEvent::End));
        assert_eq!(
            key(KeyCode::Delete, KeyModifiers::NONE),
            Some(AppEvent::Delete)
        );
    }

    #[test]
    fn ctrl_char_still_mapped() {
        assert_eq!(
            key(KeyCode::Char('u'), KeyModifiers::CONTROL),
            Some(AppEvent::Ctrl('u'))
        );
        assert_eq!(
            key(KeyCode::Char('x'), KeyModifiers::NONE),
            Some(AppEvent::Key('x'))
        );
    }
}
