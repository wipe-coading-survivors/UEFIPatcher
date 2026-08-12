use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::{App, Focus};

fn header(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    )))
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let active = app.active_image_id.as_deref();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selectable_items_idx: Vec<usize> = Vec::new();

    items.push(header("Images"));
    for im in &app.registry.images {
        selectable_items_idx.push(items.len());
        let marker = if active == Some(im.image_id.as_str()) {
            "► "
        } else {
            "  "
        };
        items.push(ListItem::new(Line::from(format!(
            "{marker}{}  {}  {}",
            short(&im.image_id),
            im.name,
            fmt_size(im.size),
        ))));
    }
    items.push(header("Artifacts"));
    for ar in &app.registry.artifacts {
        selectable_items_idx.push(items.len());
        items.push(ListItem::new(Line::from(format!(
            "   {}  {}  {}  {}",
            short(&ar.artifact_id),
            ar.kind,
            fmt_size(ar.size),
            ar.source,
        ))));
    }

    let mut state = ListState::default();
    let sel = if selectable_items_idx.is_empty() {
        None
    } else {
        let c = app.registry.cursor.min(selectable_items_idx.len() - 1);
        Some(selectable_items_idx[c])
    };
    state.select(sel);
    let title = if app.focus == Focus::Registry {
        "Registry *"
    } else {
        "Registry"
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut state);
}

fn short(id: &str) -> String {
    let len = id.len().min(8);
    id[..len].to_string()
}

fn fmt_size(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fmt_size_units() {
        assert_eq!(fmt_size(512), "512 B");
        assert_eq!(fmt_size(2048), "2.0 KB");
        assert_eq!(fmt_size(16 * 1024 * 1024), "16.0 MB");
    }
    #[test]
    fn short_truncates_to_8() {
        assert_eq!(short("abcdefghijklmnop"), "abcdefgh");
        assert_eq!(short("ab"), "ab");
    }
}
