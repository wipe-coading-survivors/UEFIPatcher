use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{List, ListState};

use crate::tree::compute_scrolled_offset;

pub const SCROLL_PAD: usize = 3;

/// Единая механика TUI-списков: clamp курса по total, select, офсет
/// гистерезиса (tree::compute_scrolled_offset). select — что подсветить
/// (None — подсветки нет); для регистра с неселектабельными хедерами
/// маппинг делает вызывающий. Спека forms-panel-three-zone §1.
pub fn sync_list_state(
    state: &mut ListState,
    cursor: usize,
    total: usize,
    inner_h: usize,
    pad: usize,
    select: Option<usize>,
) {
    let cursor = if total == 0 { 0 } else { cursor.min(total - 1) };
    let off = compute_scrolled_offset(cursor, state.offset(), inner_h, total, pad);
    state.select(select);
    *state.offset_mut() = off;
}

/// Общий случай списка в рамке: подсвечен clamp-нутый курсор,
/// inner_h = area.height - 2 (бордюр). Спека forms-panel-three-zone §1.
pub fn render_scrolled_list(
    f: &mut Frame,
    list: List<'_>,
    area: Rect,
    state: &mut ListState,
    cursor: usize,
    total: usize,
    pad: usize,
) {
    let inner_h = area.height.saturating_sub(2) as usize;
    let cursor = if total == 0 { 0 } else { cursor.min(total - 1) };
    let select = (total > 0).then_some(cursor);
    sync_list_state(state, cursor, total, inner_h, pad, select);
    f.render_stateful_widget(list, area, state);
}

/// Постраничный ход вниз: cursor + page, clamp по total-1; пустой список — 0.
pub fn page_down(cursor: usize, total: usize, page: usize) -> usize {
    if total == 0 {
        0
    } else {
        cursor.saturating_add(page).min(total - 1)
    }
}

/// Постраничный ход вверх: cursor - page, saturating.
pub fn page_up(cursor: usize, page: usize) -> usize {
    cursor.saturating_sub(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::List;

    #[test]
    fn page_down_clamps_and_empty() {
        assert_eq!(page_down(0, 0, 10), 0, "пустой список — курсор 0");
        assert_eq!(page_down(5, 20, 10), 15);
        assert_eq!(page_down(15, 20, 10), 19, "clamp по последней строке");
    }

    #[test]
    fn page_up_saturates() {
        assert_eq!(page_up(5, 10), 0);
        assert_eq!(page_up(15, 10), 5);
    }

    #[test]
    fn sync_selects_none_on_empty_and_follows_hysteresis() {
        let mut st = ratatui::widgets::ListState::default();
        sync_list_state(&mut st, 0, 0, 10, SCROLL_PAD, None);
        assert_eq!(st.selected(), None);
        assert_eq!(st.offset(), 0);
        sync_list_state(&mut st, 50, 100, 10, SCROLL_PAD, Some(50));
        assert_eq!(st.selected(), Some(50));
        assert_eq!(st.offset(), 44, "цель ниже окна — докрутка с pad");
        sync_list_state(&mut st, 47, 100, 10, SCROLL_PAD, Some(47));
        assert_eq!(st.offset(), 44, "цель в окне — офсет на месте (гистерезис)");
    }

    #[test]
    fn render_scrolled_list_renders_and_follows() {
        let items: Vec<String> = (0..30).map(|i| format!("row{i}")).collect();
        let mut st = ratatui::widgets::ListState::default();
        let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
        t.draw(|f| {
            let list = List::new(items.clone());
            super::render_scrolled_list(f, list, f.area(), &mut st, 20, 30, SCROLL_PAD);
        })
        .unwrap();
        assert_eq!(st.selected(), Some(20));
        assert!(st.offset() > 0, "курсор 20 в окне 10 — прокрутка началась");
    }
}
