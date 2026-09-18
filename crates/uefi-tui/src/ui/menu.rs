use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::{MENU_ROWS, MenuState};

pub fn render(f: &mut Frame, anchor: Rect, menu: &MenuState) {
    if !menu.open || menu.items.is_empty() {
        return;
    }
    let h = menu.items.len().min(MENU_ROWS) as u16;
    let rows = h + 2;
    let y = anchor.y.saturating_sub(rows);
    let area = Rect {
        x: anchor.x,
        y,
        width: anchor.width,
        height: rows,
    };
    f.render_widget(Clear, area);
    let visible: Vec<ListItem> = menu.items[menu.offset..menu.offset + h as usize]
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let idx = menu.offset + i;
            let style = if idx == menu.selected {
                Style::default()
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(item.display.clone()).style(style)
        })
        .collect();
    let title = format!("{}/{}", menu.selected + 1, menu.items.len());
    let list = List::new(visible).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{MenuItem, MenuState};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
        (0..40)
            .map(|x| terminal.backend().buffer().get(x, y).symbol().to_string())
            .collect()
    }

    #[test]
    fn popup_renders_items_and_counter() {
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        let mut menu = MenuState::default();
        menu.open_with(vec![
            MenuItem {
                display: "alpha".into(),
                apply: "x alpha".into(),
            },
            MenuItem {
                display: "beta".into(),
                apply: "x beta".into(),
            },
        ]);
        terminal
            .draw(|f| render(f, Rect::new(0, 10, 40, 3), &menu))
            .unwrap();
        assert!(row(&terminal, 6).contains("1/2"));
        assert!(row(&terminal, 7).contains("alpha"));
        assert!(row(&terminal, 8).contains("beta"));
    }

    #[test]
    fn closed_menu_renders_nothing() {
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal
            .draw(|f| render(f, Rect::new(0, 10, 40, 3), &MenuState::default()))
            .unwrap();
        assert!(row(&terminal, 6).trim().is_empty());
    }

    #[test]
    fn height_clamped_to_menu_rows_and_scrolls() {
        let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
        let mut menu = MenuState::default();
        let items: Vec<MenuItem> = (0..10)
            .map(|i| MenuItem {
                display: format!("i{i}"),
                apply: format!("a{i}"),
            })
            .collect();
        menu.open_with(items);
        for _ in 0..9 {
            menu.down();
        }
        terminal
            .draw(|f| render(f, Rect::new(0, 20, 40, 3), &menu))
            .unwrap();
        assert!(row(&terminal, 10).contains("10/10"));
        assert!(row(&terminal, 11).contains("i2"));
        assert!(row(&terminal, 18).contains("i9"));
    }
}
