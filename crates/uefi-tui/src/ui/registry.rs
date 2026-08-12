use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::{App, Focus};
use crate::tree::compute_scrolled_offset;

const SCROLL_PAD: usize = 2;

fn header(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    )))
}

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
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

    let total = selectable_items_idx.len();
    let inner_h = area.height.saturating_sub(2) as usize;
    let row_count = items.len();
    let cursor = if total == 0 {
        0
    } else {
        app.registry.cursor.min(total - 1)
    };
    let prev_off = app.registry_state.offset();
    let new_off = compute_scrolled_offset(cursor, prev_off, inner_h, row_count, SCROLL_PAD);
    let sel = if selectable_items_idx.is_empty() {
        None
    } else {
        Some(selectable_items_idx[cursor])
    };
    app.registry_state.select(sel);
    *app.registry_state.offset_mut() = new_off;

    let title = if app.focus == Focus::Registry {
        "Registry *"
    } else {
        "Registry"
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut app.registry_state);
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
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
