use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let inner = (area.width as usize).saturating_sub(2);
    let content = match app.mode {
        Mode::Command => Line::from(cmdline_spans(":", &app.cmdline, inner)),
        Mode::Insert if app.insert_cmd.is_empty() => {
            Line::from(cmdline_spans("> ", &app.cmdline, inner))
        }
        Mode::Insert => {
            let p = format!("{}> ", app.insert_cmd);
            Line::from(cmdline_spans(&p, &app.cmdline, inner))
        }
        Mode::Normal => Line::from("Press : for commands, i/r/d for insert/replace/remove"),
    };
    let title = match app.mode {
        Mode::Insert => "Insert",
        _ => "Command",
    };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn cmdline_spans(prefix: &str, buf: &crate::line::LineBuffer, inner: usize) -> Vec<Span<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    let text = format!("{}{}", prefix, buf.as_str());
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let cur = prefix.graphemes(true).count() + buf.cursor_grapheme();
    let start = if cur >= inner { cur + 1 - inner } else { 0 };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    for (gi, g) in graphemes.iter().enumerate().skip(start).take(inner) {
        if gi == cur {
            if !run.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut run)));
            }
            spans.push(Span::styled(
                g.to_string(),
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        } else {
            run.push_str(g);
        }
    }
    if !run.is_empty() {
        spans.push(Span::raw(run));
    }
    if cur == graphemes.len() {
        spans.push(Span::styled(
            " ".to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Modifier;

    fn draw(app: &App) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(20, 5)).unwrap();
        t.draw(|f| render(f, Rect::new(0, 0, 20, 3), app)).unwrap();
        t
    }

    #[test]
    fn cursor_reversed_mid_string() {
        let mut app = App::new();
        app.mode = crate::app::Mode::Command;
        app.cmdline.set_str("abc");
        app.cmdline.left();
        let t = draw(&app);
        assert_eq!(t.backend().buffer().get(4, 1).symbol(), "c");
        assert!(
            t.backend()
                .buffer()
                .get(4, 1)
                .modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn cursor_at_end_is_blank_reversed() {
        let mut app = App::new();
        app.mode = crate::app::Mode::Command;
        app.cmdline.set_str("ab");
        let t = draw(&app);
        assert!(
            t.backend()
                .buffer()
                .get(4, 1)
                .modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn long_line_scrolls_to_keep_cursor_visible() {
        let mut app = App::new();
        app.mode = crate::app::Mode::Command;
        app.cmdline.set_str("0123456789abcdefghij");
        let t = draw(&app);
        assert_ne!(t.backend().buffer().get(1, 1).symbol(), "0");
        assert_eq!(t.backend().buffer().get(17, 1).symbol(), "j");
        assert!(
            t.backend()
                .buffer()
                .get(18, 1)
                .modifier
                .contains(Modifier::REVERSED)
        );
    }
}
