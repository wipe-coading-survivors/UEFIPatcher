# Трёхзонная панель «Form» + унификация скролл-механики — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Панель «Form» в Forms View становится трёхзонной (фикс-шапка / скроллируемый список вопросов с гейт-хвостом / фикс-низ с деталями выбранного вопроса), а механика «список с гистерезисом» унифицирована в `ui/scroll.rs`.

**Architecture:** Чистая сборка контента (`forms.rs::form_panel`) → рендер тремя зонами (`ui/forms.rs`); вся скролл-механика списков TUI (clamp/select/офсет/пейджинг) сводится в новый модуль `ui/scroll.rs` поверх нетронутого примитива `tree::compute_scrolled_offset`. Источник сегодняшних артефактов — `Paragraph` с `.wrap()`+`.scroll()` (визуальные vs логические строки) — удаляется целиком.

**Tech Stack:** Rust workspace (edition 2024), ratatui 0.28 (List/ListState/Paragraph/Layout, TestBackend), TDD.

**Spec:** `docs/superpowers/specs/2026-09-19-forms-panel-three-zone-design.md` — план аргументирует от спеки; исполнители читают обе.

## Global Constraints

- Спека (выше) — обязательное чтение перед каждой задачей; её решения владельца: gates = хвост средней зоны; низ = авто-рост до `cap = max(3, 45% высоты панели)`; PgUp/PgDn на Details = страница вопросов.
- **Module-first (критично):** сразу после создания `ui/scroll.rs` добавить `pub mod scroll;` в `crates/uefi-tui/src/ui/mod.rs` В ТОМ ЖЕ ШАГЕ, до `cargo test`.
- **Комментарии в коде — нет** (кроме коротких rustdoc-контрактов `///` на pub/pub(crate)-функциях).
- Миграция существующих списков — 1:1 без изменения поведения: registry остаётся на pad **2** (локальная константа), tree/forms/strings/questions — pad **3** (`scroll::SCROLL_PAD`).
- После каждой задачи: `cargo test -p uefi-tui` (все зелёные, вывод чистый) и `cargo clippy -p uefi-tui -- -D warnings`; перед коммитом `cargo fmt --all`.
- Сообщения коммитов — из плана. Один коммит на задачу.
- Референсы поведения: `compute_scrolled_offset` — `crates/uefi-tui/src/tree.rs:53-77`; существующие рендер-тесты — `crates/uefi-tui/src/ui/forms.rs` (tests), `crates/uefi-tui/src/ui/registry.rs:95-146` (паттерн чтения буфера `row()`).

## File Structure

| Файл | Отвечает за | Задачи |
|------|-------------|--------|
| `crates/uefi-tui/src/ui/scroll.rs` *(create)* | Единая механика: `SCROLL_PAD`, `sync_list_state`, `render_scrolled_list`, `page_down`, `page_up` | T1 |
| `crates/uefi-tui/src/ui/mod.rs` | `pub mod scroll;` + хинт Forms/Details + тест хинта | T1, T4 |
| `crates/uefi-tui/src/ui/tree.rs` | Миграция главного списка на `render_scrolled_list` | T2 |
| `crates/uefi-tui/src/ui/registry.rs` | Миграция на `sync_list_state` (select-маппинг остаётся) | T2 |
| `crates/uefi-tui/src/app.rs` | Пейджеры на `scroll::page_*`; `FormsData.questions_viewport`; `forms_question_page_*`; −`details_scroll/anchor/followed`, +`questions_state` | T2, T4, T5 |
| `crates/uefi-tui/src/forms.rs` | `FormPanel` + `form_panel` + `question_bottom`; смерть `form_details`/`FormDetails`/`form_details_text` | T3, T5 |
| `crates/uefi-tui/src/main.rs` | PgUp/PgDn на Details → `forms_question_page_down/up` + refresh | T4 |
| `crates/uefi-tui/src/ui/help.rs` | HELP: строка Details focus + тест | T4 |
| `crates/uefi-tui/src/ui/forms.rs` | Трёхзонный рендер; миграция forms/strings списков; смерть `follow_offset`/`FORMS_SCROLL_PAD`; тесты | T5 |
| `crates/uefi-tui/src/commands.rs` | `"open" | "o"` в flags-match + тест | T6 |

---

### Task 1: `ui/scroll.rs` — единая механика списков

**Files:**
- Create: `crates/uefi-tui/src/ui/scroll.rs`
- Modify: `crates/uefi-tui/src/ui/mod.rs` (добавить `pub mod scroll;` рядом с существующими `pub mod …`)

**Interfaces:**
- Produces (используют T2/T4/T5):
  - `pub const SCROLL_PAD: usize = 3;`
  - `pub fn sync_list_state(state: &mut ListState, cursor: usize, total: usize, inner_h: usize, pad: usize, select: Option<usize>)`
  - `pub fn render_scrolled_list(f: &mut Frame, list: List<'_>, area: Rect, state: &mut ListState, cursor: usize, total: usize, pad: usize)`
  - `pub fn page_down(cursor: usize, total: usize, page: usize) -> usize`
  - `pub fn page_up(cursor: usize, page: usize) -> usize`

- [ ] **Step 1: Создать модуль с тестами (module-first) и объявить его**

`crates/uefi-tui/src/ui/mod.rs` — в блок `pub mod …` добавить:

```rust
pub mod scroll;
```

`crates/uefi-tui/src/ui/scroll.rs`:

```rust
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
        sync_list_state(&mut st, 44, 100, 10, SCROLL_PAD, Some(44));
        assert_eq!(st.offset(), 44, "цель в окне — офсет на месте (гистерезис)");
    }

    #[test]
    fn render_scrolled_list_renders_and_follows() {
        let items: Vec<String> = (0..30).map(|i| format!("row{i}")).collect();
        let mut st = ratatui::widgets::ListState::default();
        let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
        t.draw(|f| {
            let list = List::new(items.clone());
            super::render_scrolled_list(
                f,
                list,
                f.area(),
                &mut st,
                20,
                30,
                SCROLL_PAD,
            );
        })
        .unwrap();
        assert_eq!(st.selected(), Some(20));
        assert!(st.offset() > 0, "курсор 20 в окне 10 — прокрутка началась");
    }
}
```

- [ ] **Step 2: Запустить — убедиться в RED**

Run: `cargo test -p uefi-tui scroll`
Expected: FAIL (compile error: `page_down`, `sync_list_state`, `render_scrolled_list` не определены).

- [ ] **Step 3: Реализация**

В `crates/uefi-tui/src/ui/scroll.rs` перед `mod tests`:

```rust
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
```

- [ ] **Step 4: Запустить — GREEN**

Run: `cargo test -p uefi-tui scroll`
Expected: PASS, 4 теста.

- [ ] **Step 5: Полный прогон + lint**

Run: `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all`
Expected: все тесты зелёные, clippy чистый.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-tui/src/ui/scroll.rs crates/uefi-tui/src/ui/mod.rs
git commit -m "feat(tui): ui/scroll.rs — единая механика списков (sync_list_state/render_scrolled_list/page_down/page_up)"
```

---

### Task 2: Миграция tree/registry/App-пейджеров на `ui::scroll` (поведение 1:1)

Рефакторинг без изменения поведения: гейт — существующие тесты остаются зелёными. TDD-цикла нет, новые тесты не пишутся.

**Files:**
- Modify: `crates/uefi-tui/src/ui/tree.rs:49-71`
- Modify: `crates/uefi-tui/src/ui/registry.rs:52-78`
- Modify: `crates/uefi-tui/src/app.rs:289-299`

**Interfaces:**
- Consumes: `crate::ui::scroll::{render_scrolled_list, sync_list_state, page_down, page_up, SCROLL_PAD}` из Task 1.

- [ ] **Step 1: `ui/tree.rs` — заменить ручной блок на `render_scrolled_list`**

Удалить `use crate::tree::compute_scrolled_offset;` (строка 9) и `const SCROLL_PAD: usize = 3;` (строка 11), добавить `use crate::ui::scroll;`. Тело `render` после сбора `items`:

```rust
    let total = visible.len();
    app.tree_viewport_rows = area.height.saturating_sub(2) as usize;

    let title = if app.focus == Focus::Tree {
        "Tree *"
    } else {
        "Tree"
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    scroll::render_scrolled_list(
        f,
        list,
        area,
        &mut app.tree_state,
        app.cursor,
        total,
        scroll::SCROLL_PAD,
    );
```

- [ ] **Step 2: `ui/registry.rs` — заменить ручной блок на `sync_list_state`**

Локальную `const SCROLL_PAD: usize = 2;` (строка 10) оставить, `use crate::tree::compute_scrolled_offset;` удалить, добавить `use crate::ui::scroll;`. Блок строк 52-68 заменить на:

```rust
    let total = selectable_items_idx.len();
    let inner_h = area.height.saturating_sub(2) as usize;
    let row_count = items.len();
    let cursor = if total == 0 {
        0
    } else {
        app.registry.cursor.min(total - 1)
    };
    let sel = if selectable_items_idx.is_empty() {
        None
    } else {
        Some(selectable_items_idx[cursor])
    };
    scroll::sync_list_state(
        &mut app.registry_state,
        cursor,
        row_count,
        inner_h,
        SCROLL_PAD,
        sel,
    );
```

Строки с `title`/`list`/`f.render_stateful_widget(...)` — без изменений.

- [ ] **Step 3: `app.rs` — пейджеры на `scroll::page_*`**

`cursor_page_down`/`cursor_page_up` (app.rs:289-299) заменить на:

```rust
    pub fn cursor_page_down(&mut self) {
        let n = self.visible().len();
        if n == 0 {
            return;
        }
        self.cursor = crate::ui::scroll::page_down(self.cursor, n, self.page_size());
    }

    pub fn cursor_page_up(&mut self) {
        self.cursor = crate::ui::scroll::page_up(self.cursor, self.page_size());
    }
```

- [ ] **Step 4: Прогон — все существующие тесты зелёные**

Run: `cargo test -p uefi-tui`
Expected: PASS (включая `registry::*`, `app::tests::*` пейджеры, `forms_list_offset_kept_when_cursor_walks_up`).

- [ ] **Step 5: Lint + commit**

Run: `cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all`

```bash
git add crates/uefi-tui/src/ui/tree.rs crates/uefi-tui/src/ui/registry.rs crates/uefi-tui/src/app.rs
git commit -m "refactor(tui): tree/registry/App-пейджеры -> ui/scroll (поведение 1:1)"
```

---

### Task 3: `form_panel` — чистая трёхзонная сборка

**Files:**
- Modify: `crates/uefi-tui/src/forms.rs` (новые `FormPanel`, `form_panel`, `question_bottom` + тесты; `form_details`/`FormDetails`/`form_details_text` пока НЕ трогаем — рендер ещё на них, смерть в Task 5)

**Interfaces:**
- Consumes: `FormsData` (app.rs:69-89: `questions_key: Option<FormKey>`, `questions: Vec<QuestionSummary>`, `gates: Vec<GateInfo>`, `question_info: Option<QuestionInfo>`, `question_info_key: Option<(FormKey, u32)>`), `FormsRow::Form { key, path, .. }`, `short_guid`, `theme::question_icon`.
- Produces (использует Task 5):
  - `pub struct FormPanel { pub header: Vec<String>, pub questions: Vec<String>, pub gates: Vec<String>, pub bottom: Vec<String>, pub questions_ready: bool }`
  - `pub fn form_panel(forms: &FormsData, rows: &[FormsRow], cursor: usize) -> FormPanel`

- [ ] **Step 1: Написать failing-тесты**

В `mod tests` файла `crates/uefi-tui/src/forms.rs` (рядом с существующим тестом `form_details_reports_marker_and_info_lines`):

```rust
    #[test]
    fn form_panel_builds_zones() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        let rows = app.forms_rows();
        let key = FormKey {
            target: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
        };
        app.forms.questions_key = Some(key.clone());
        app.forms.questions = vec![
            uefi_proto::QuestionSummary {
                question_id: 0x210,
                prompt: "Cores".into(),
                kind: "numeric".into(),
                ..Default::default()
            },
            uefi_proto::QuestionSummary {
                question_id: 0x211,
                prompt: String::new(),
                kind: "one_of".into(),
                ..Default::default()
            },
        ];
        app.forms.gates = vec![uefi_proto::GateInfo {
            gate_kind: "suppress".into(),
            expression: "e0".into(),
            flippable: true,
            ..Default::default()
        }];
        app.forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x210,
            kind: "numeric".into(),
            var_store_id: 2,
            var_offset: 0x37,
            width: 1,
            min: 1,
            max: 8,
            step: 1,
            ..Default::default()
        });
        app.forms.question_info_key = Some((key, 0x210));
        let p = form_panel(&app.forms, &rows, 1);
        assert!(p.questions_ready);
        assert_eq!(
            p.header.last().unwrap(),
            "Questions (2): prompt · qid · kind"
        );
        assert_eq!(p.questions.len(), 2);
        assert!(p.questions[0].contains("Cores"));
        assert!(p.questions[0].contains("q0x210"));
        assert!(p.questions[1].contains('-'), "пустой промпт — дефис");
        assert_eq!(p.gates[1], "Gates (1):");
        assert!(p.bottom[0].contains("Question q0x210"));
        assert!(p.bottom.iter().any(|l| l.contains("range 1..=8 step 1")));
    }

    #[test]
    fn form_panel_loading_and_no_form() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        let rows = app.forms_rows();
        let p = form_panel(&app.forms, &rows, 1);
        assert!(!p.questions_ready);
        assert_eq!(p.header.last().unwrap(), "Questions: loading…");
        assert!(p.questions.is_empty());
        assert!(p.gates.is_empty());
        assert_eq!(p.bottom, vec!["(loading…)".to_string()]);
        let p0 = form_panel(&app.forms, &rows, 0);
        assert_eq!(p0.header, vec!["no form selected".to_string()]);
        assert!(!p0.questions_ready);
    }
```

- [ ] **Step 2: RED**

Run: `cargo test -p uefi-tui form_panel`
Expected: FAIL (compile error: `form_panel`/`FormPanel` не определены).

- [ ] **Step 3: Реализация**

В `crates/uefi-tui/src/forms.rs` (после `form_details`, до `all_formset_guids`):

```rust
pub struct FormPanel {
    pub header: Vec<String>,
    pub questions: Vec<String>,
    pub gates: Vec<String>,
    pub bottom: Vec<String>,
    pub questions_ready: bool,
}

/// Трёхзонная сборка правой панели Forms-view (спека
/// forms-panel-three-zone §2-§3): шапка формы + колонки-хедер, строки
/// вопросов, хвост гейтов, низ — детали выбранного вопроса. Рендерит
/// ui/forms.rs. НЕ скроллит — скролл списка делает рендер.
pub fn form_panel(forms: &FormsData, rows: &[FormsRow], cursor: usize) -> FormPanel {
    let Some(FormsRow::Form { key, path, .. }) = rows.get(cursor) else {
        return FormPanel {
            header: vec!["no form selected".into()],
            questions: vec![],
            gates: vec![],
            bottom: vec![],
            questions_ready: false,
        };
    };
    let mut header = vec![
        format!("Form:    {}", key.title),
        format!("Form ID: {}", key.form_id_ifr),
        format!("FormSet: {}", short_guid(&key.formset_guid)),
        format!("Target:  {}", key.target),
    ];
    if !path.is_empty() {
        header.push(format!("Path:    {path}"));
    }
    let ready = forms.questions_key.as_ref() == Some(key);
    header.push(String::new());
    header.push(if ready {
        format!(
            "Questions ({}): prompt · qid · kind",
            forms.questions.len()
        )
    } else {
        "Questions: loading…".into()
    });

    let questions: Vec<String> = if ready {
        forms
            .questions
            .iter()
            .map(|q| {
                let prompt = if q.prompt.is_empty() { "-" } else { &q.prompt };
                format!(
                    "{} {prompt:<28} q{:#x} {}",
                    crate::theme::question_icon(&q.kind),
                    q.question_id,
                    q.kind
                )
            })
            .collect()
    } else {
        vec![]
    };

    let mut gates = Vec::new();
    if ready && !forms.gates.is_empty() {
        gates.push(String::new());
        gates.push(format!("Gates ({}):", forms.gates.len()));
        for g in &forms.gates {
            let flip = if g.flippable { "flippable" } else { "-" };
            let src = if g.source_target.is_empty() {
                String::new()
            } else {
                format!(" @{}", g.source_target)
            };
            gates.push(format!(
                "  {:<8} {:<24} {}{}",
                g.gate_kind, g.expression, flip, src
            ));
        }
    }

    let info_for_this = forms
        .question_info_key
        .as_ref()
        .is_some_and(|(k, _)| k == key);
    let bottom = if info_for_this {
        forms
            .question_info
            .as_ref()
            .map(question_bottom)
            .unwrap_or_else(|| vec!["(loading…)".into()])
    } else {
        vec!["(loading…)".into()]
    };

    FormPanel {
        header,
        questions,
        gates,
        bottom,
        questions_ready: ready,
    }
}

fn question_bottom(qi: &uefi_proto::QuestionInfo) -> Vec<String> {
    let icon = crate::theme::question_icon(&qi.kind);
    let mut lines = vec![
        format!("{icon} Question q{:#x} ({}):", qi.question_id, qi.kind),
        format!(
            "  store {} · offset {:#x} · width {}",
            qi.var_store_id, qi.var_offset, qi.width
        ),
    ];
    match qi.kind.as_str() {
        "numeric" => {
            lines.push(format!(
                "  range {}..={} step {}",
                qi.min, qi.max, qi.step
            ));
        }
        "one_of" => {
            if qi.options.is_empty() {
                lines.push("  options: (none)".into());
            }
            for (i, o) in qi.options.iter().enumerate() {
                let item = if o.text.is_empty() {
                    format!("{:#x}(sid {})", o.value, o.string_id)
                } else {
                    format!("{:#x} \"{}\"", o.value, o.text)
                };
                let prefix = if i == 0 {
                    "  options: ".to_string()
                } else {
                    " ".repeat(11)
                };
                lines.push(format!("{prefix}{item}"));
            }
        }
        _ => {}
    }
    lines
}
```

Если тестам не хватает импорта `FormKey` в `mod tests` — добавить `use super::FormKey;` (или полное имя), по образцу существующих тестов файла.

- [ ] **Step 4: GREEN**

Run: `cargo test -p uefi-tui form_panel`
Expected: PASS, 2 теста.

- [ ] **Step 5: Полный прогон + lint + commit**

Run: `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all`

```bash
git add crates/uefi-tui/src/forms.rs
git commit -m "feat(tui): form_panel — чистая трёхзонная сборка панели Form"
```

---

### Task 4: PgUp/PgDn — страница вопросов (+viewport, хинт, help)

**Files:**
- Modify: `crates/uefi-tui/src/app.rs:69-89` (FormsData) и блок методов рядом с `forms_question_cursor_down/up` (app.rs:435-446)
- Modify: `crates/uefi-tui/src/main.rs:273-278`
- Modify: `crates/uefi-tui/src/ui/mod.rs:55`
- Modify: `crates/uefi-tui/src/ui/help.rs:41` (+тест)

**Interfaces:**
- Consumes: `scroll::page_down/page_up` (Task 1); `commands::refresh_question_info_if_needed(app, c)` — уже вызывается из j/k-веток (main.rs:262, 270).
- Produces: `FormsData.questions_viewport: usize` (пишет рендер в Task 5, фолбэк 10); `App::forms_question_page_down/forms_question_page_up/forms_question_page_size`.

- [ ] **Step 1: Failing-тесты**

`app.rs`, `mod tests` (рядом с `question_cursor_clamps_and_selected_question`, app.rs:1194):

```rust
    #[test]
    fn forms_question_page_moves_clamp() {
        let mut app = App::new();
        app.forms.questions = (0..30)
            .map(|i| uefi_proto::QuestionSummary {
                question_id: 0x210 + i,
                prompt: format!("q{i}"),
                ..Default::default()
            })
            .collect();
        app.forms.questions_viewport = 10;
        app.forms_question_page_down();
        assert_eq!(app.forms.question_cursor, 10);
        app.forms_question_page_down();
        assert_eq!(app.forms.question_cursor, 20);
        app.forms_question_page_down();
        assert_eq!(app.forms.question_cursor, 29, "clamp по последнему вопросу");
        app.forms_question_page_up();
        assert_eq!(app.forms.question_cursor, 19);
        app.forms.question_cursor = 2;
        app.forms_question_page_up();
        assert_eq!(app.forms.question_cursor, 0, "saturating");
    }

    #[test]
    fn forms_question_page_size_defaults_to_10() {
        let app = App::new();
        assert_eq!(app.forms_question_page_size(), 10);
    }
```

`ui/help.rs`, `mod tests`:

```rust
    #[test]
    fn help_documents_forms_details_paging() {
        assert!(HELP.contains(
            "Details focus: j/k выбор вопроса · PgUp/PgDn страница · Enter → :hii set-value <item> <value>"
        ));
    }
```

`ui/mod.rs`, `mod tests`:

```rust
    #[test]
    fn forms_details_hint_documents_page_keys() {
        let mut app = crate::app::App::new();
        app.view = crate::app::View::Forms;
        app.forms.focus = crate::app::FormsFocus::Details;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
        t.draw(|f| super::render(f, f.area(), &mut app)).unwrap();
        let text: String = (0..24)
            .map(|y| {
                (0..100)
                    .map(|x| t.backend().buffer().get(x, y).symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("PgUp/PgDn страница"));
    }
```

- [ ] **Step 2: RED**

Run: `cargo test -p uefi-tui forms_question_page && cargo test -p uefi-tui help_documents_forms_details_paging && cargo test -p uefi-tui forms_details_hint`
Expected: FAIL (методов/поля нет — compile error; help-строки нет — assertion fail; хинта нет — assertion fail).

- [ ] **Step 3: Реализация**

`app.rs`, в `FormsData` после `pub question_info_key: …` (app.rs:81) добавить:

```rust
    pub questions_viewport: usize,
```

и в `App::new()` (рядом с инициализацией остальных полей forms, после `strings_cursor`-блока, где сегодня `details_scroll: 0, details_anchor: None, details_followed: None,`) добавить:

```rust
            questions_viewport: 0,
```

Методы рядом с `forms_question_cursor_up` (app.rs:442-446):

```rust
    pub fn forms_question_page_size(&self) -> usize {
        if self.forms.questions_viewport == 0 {
            10
        } else {
            self.forms.questions_viewport
        }
    }

    pub fn forms_question_page_down(&mut self) {
        let n = self.forms.questions.len();
        if n == 0 {
            return;
        }
        self.forms.question_cursor = crate::ui::scroll::page_down(
            self.forms.question_cursor,
            n,
            self.forms_question_page_size(),
        );
    }

    pub fn forms_question_page_up(&mut self) {
        self.forms.question_cursor =
            crate::ui::scroll::page_up(self.forms.question_cursor, self.forms_question_page_size());
    }
```

`main.rs:273-278` — ветки PageDown/PageUp заменить на:

```rust
        AppEvent::PageDown if app.forms.focus == FormsFocus::Details && !app.forms.show_strings => {
            app.forms_question_page_down();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_question_info_if_needed(app, c).await;
            }
        }
        AppEvent::PageUp if app.forms.focus == FormsFocus::Details && !app.forms.show_strings => {
            app.forms_question_page_up();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_question_info_if_needed(app, c).await;
            }
        }
```

`ui/mod.rs:55` — заменить строку хинта на:

```rust
                "NORMAL[Forms/Details]: j/k вопрос · PgUp/PgDn страница · Enter set-value · Tab image-view · :cmd · ?help · q".into()
```

`ui/help.rs:41` — заменить строку на:

```rust
  Details focus: j/k выбор вопроса · PgUp/PgDn страница · Enter → :hii set-value <item> <value>
```

- [ ] **Step 4: GREEN**

Run: `cargo test -p uefi-tui`
Expected: PASS (включая новые 3 теста).

- [ ] **Step 5: Lint + commit**

Run: `cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all`

```bash
git add crates/uefi-tui/src/app.rs crates/uefi-tui/src/main.rs crates/uefi-tui/src/ui/mod.rs crates/uefi-tui/src/ui/help.rs
git commit -m "feat(tui): PgUp/PgDn на Forms/Details — страница вопросов (+viewport, hint, help)"
```

---

### Task 5: Трёхзонный рендер панели «Form» + смерть wrap+scroll Paragraph

**Files:**
- Modify: `crates/uefi-tui/src/ui/forms.rs` (render, render_strings, смерть `follow_offset`/`FORMS_SCROLL_PAD`, тесты)
- Modify: `crates/uefi-tui/src/forms.rs` (удалить `form_details`/`FormDetails`/`form_details_text` + их тесты)
- Modify: `crates/uefi-tui/src/app.rs` (FormsData: −`details_scroll`/`details_anchor`/`details_followed`, +`questions_state: ListState`; правка doc-комментария у `selected_question_qid` ~app.rs:448-450, где упомянут `form_details_text`)

**Interfaces:**
- Consumes: `form_panel`/`FormPanel` (Task 3), `sync_list_state`/`render_scrolled_list`/`SCROLL_PAD` (Task 1), `FormsData.questions_viewport` (Task 4).
- Produces: рендер-контракт панели: шапка = `panel.header`, середина = `panel.questions`+`panel.gates` (List, cursor `forms.question_cursor`), низ = `panel.bottom`.

- [ ] **Step 1: Failing-тесты (новое поведение — шапка не уезжает)**

В `mod tests` файла `ui/forms.rs`. Хелпер чтения строки буфера — по образцу `ui/registry.rs:101-105`; добавить локально:

```rust
    fn row_of(t: &ratatui::Terminal<ratatui::backend::TestBackend>, y: u16) -> String {
        (0..80)
            .map(|x| t.backend().buffer().get(x, y).symbol().to_string())
            .collect()
    }

    fn panel_text(t: &ratatui::Terminal<ratatui::backend::TestBackend>, y_max: u16) -> String {
        (0..y_max)
            .map(|y| row_of(t, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app_with_form(n_questions: usize) -> crate::app::App {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        app.forms.questions_key = Some(fk(1));
        app.forms.questions = (0..n_questions)
            .map(|i| uefi_proto::QuestionSummary {
                question_id: 0x210 + i,
                prompt: format!("q{i}"),
                kind: "numeric".into(),
                ..Default::default()
            })
            .collect();
        app
    }
```

(`fk` уже есть в этом `mod tests` — ui/forms.rs:186-193.)

Тесты:

```rust
    #[test]
    fn three_zone_header_stays_put_on_last_question() {
        let mut app = app_with_form(30);
        app.forms.question_cursor = 29;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(text.contains("Form:    Main"), "шапка формы на месте");
        assert!(
            text.contains("Questions (30): prompt · qid · kind"),
            "колонки-хедер на месте"
        );
        assert!(text.contains("q29"), "последний вопрос виден");
    }

    #[test]
    fn loading_state_shows_header_line_only() {
        let mut app = app_with_form(0);
        app.forms.questions_key = None;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(text.contains("Questions: loading…"));
        assert_eq!(app.forms.questions_state.selected(), None);
    }

    #[test]
    fn bottom_zone_shows_selected_question_and_gates_tail_reachable() {
        let mut app = app_with_form(30);
        app.forms.question_cursor = 29;
        app.forms.gates = vec![
            uefi_proto::GateInfo {
                gate_kind: "suppress".into(),
                expression: "e0".into(),
                flippable: true,
                ..Default::default()
            },
            uefi_proto::GateInfo {
                gate_kind: "suppress".into(),
                expression: "e1".into(),
                ..Default::default()
            },
        ];
        app.forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x22d,
            kind: "numeric".into(),
            var_store_id: 2,
            var_offset: 0x37,
            width: 1,
            min: 1,
            max: 8,
            step: 1,
            ..Default::default()
        });
        app.forms.question_info_key = Some((fk(1), 0x22d));
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(
            text.contains("Question q0x22d"),
            "низ — детали выбранного вопроса"
        );
        assert!(text.contains("Gates (2):"), "гейт-хвост достижим скроллом");
        let sel = app.forms.questions_state.selected();
        assert_eq!(sel, Some(29), "курсор на последнем вопросе, не на гейтах");
    }

    #[test]
    fn bottom_cap_keeps_questions_visible() {
        let mut app = app_with_form(6);
        app.forms.question_cursor = 0;
        let mut options = Vec::new();
        for i in 0..12u64 {
            options.push(uefi_proto::OptionEntry {
                value: i,
                string_id: 0,
                text: format!("opt{i}"),
                ..Default::default()
            });
        }
        app.forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x210,
            kind: "one_of".into(),
            var_store_id: 2,
            var_offset: 0x40,
            width: 1,
            options,
            ..Default::default()
        });
        app.forms.question_info_key = Some((fk(1), 0x210));
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 20);
        assert!(text.contains("options: 0x0"), "начало options видно");
        assert!(text.contains("q0x210"), "вопросы не выдавлены низом-переростком");
        assert!(text.contains("Form:    Main"), "шапка на месте");
    }
```

Тип элемента `options` — `uefi_proto::OptionEntry` (proto `engine.proto:267`: `string_id: u32`, `value: u64`, `flags: u32`, `text: String`; prost-derive `Default`).

- [ ] **Step 2: RED**

Run: `cargo test -p uefi-tui three_zone && cargo test -p uefi-tui loading_state && cargo test -p uefi-tui bottom_`
Expected: FAIL (`questions_state` поля нет — compile error; после добавления поля — шапка уезжает на старом рендере).

- [ ] **Step 3: Реализация**

**app.rs** — в `FormsData` (app.rs:86-88) заменить три поля:

```rust
    pub details_scroll: u16,
    pub details_anchor: Option<String>,
    pub details_followed: Option<usize>,
```

на:

```rust
    pub questions_state: ratatui::widgets::ListState,
```

В `App::new()` инициализацию `details_scroll: 0, details_anchor: None, details_followed: None,` заменить на `questions_state: ListState::default(),`. Doc-комментарий у `selected_question_qid` (app.rs:448-450): упоминание `form_details_text` заменить на `form_panel`.

**ui/forms.rs** — новый рендер. Импорты: убрать `use crate::tree::compute_scrolled_offset;`, добавить `use crate::ui::scroll;` и в ratatui-импортах `Line`, `Modifier` (`ratatui::text::Line`, `ratatui::style::Modifier`). Удалить `const FORMS_SCROLL_PAD` (строка 59) и `pub fn follow_offset` (строки 61-71) с их тестами (`follow_offset_follows_target_and_keeps_window`, `details_anchor_reset_on_form_change`, `forms_details_manual_scroll_survives_until_target_moves`).

`render` целиком:

```rust
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if app.forms.show_strings {
        render_strings(f, area, app);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    let rows = app.forms_rows();
    let items: Vec<ListItem> = rows.iter().map(row_item).collect();
    let list_focus = app.forms.focus == FormsFocus::List;
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Forms")
                .border_style(focus_style(list_focus)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    scroll::render_scrolled_list(
        f,
        list,
        cols[0],
        &mut app.forms_list_state,
        app.forms.cursor,
        rows.len(),
        scroll::SCROLL_PAD,
    );

    let panel = crate::forms::form_panel(&app.forms, &rows, app.forms.cursor);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Form")
        .border_style(focus_style(!list_focus));
    let inner = block.inner(cols[1]);
    f.render_widget(block, cols[1]);
    let cap = 3.max(inner.height as usize * 45 / 100) as u16;
    let mut header_h = (panel.header.len() as u16).min(inner.height);
    let mut bottom_h = (panel.bottom.len() as u16).min(cap);
    if inner.height < header_h + bottom_h + 1 {
        bottom_h = bottom_h.min(2);
    }
    if inner.height < header_h + bottom_h + 1 {
        header_h = header_h.min(2);
    }
    if inner.height < header_h + bottom_h + 1 {
        bottom_h = bottom_h.min(inner.height.saturating_sub(header_h + 1));
    }
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Min(1),
            Constraint::Length(bottom_h),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(panel.header.join("\n")), zones[0]);
    render_middle(f, zones[1], app, &panel);
    f.render_widget(Paragraph::new(panel.bottom.join("\n")), zones[2]);
}

fn render_middle(f: &mut Frame, mid: Rect, app: &mut App, panel: &crate::forms::FormPanel) {
    if !panel.questions_ready {
        return;
    }
    if panel.questions.is_empty() {
        let mut lines = vec!["(no questions)".to_string()];
        lines.extend(panel.gates.iter().cloned());
        f.render_widget(Paragraph::new(lines.join("\n")), mid);
        return;
    }
    let mut items: Vec<ListItem> = panel
        .questions
        .iter()
        .map(|s| ListItem::new(s.clone()))
        .collect();
    items.extend(panel.gates.iter().map(|s| {
        ListItem::new(Line::styled(
            s.clone(),
            Style::default().add_modifier(Modifier::DIM),
        ))
    }));
    let total = items.len();
    let cursor = app.forms.question_cursor.min(panel.questions.len() - 1);
    let list = List::new(items).highlight_style(Style::default().bg(Color::DarkGray));
    let inner_h = mid.height as usize;
    scroll::sync_list_state(
        &mut app.forms.questions_state,
        cursor,
        total,
        inner_h,
        scroll::SCROLL_PAD,
        Some(cursor),
    );
    app.forms.questions_viewport = inner_h;
    f.render_stateful_widget(list, mid, &mut app.forms.questions_state);
}
```

`render_strings` — блок строк 159-178 (от `let total = visible.len();` до `f.render_stateful_widget(...)`) заменить на:

```rust
    let total = visible.len();
    let pos = visible
        .iter()
        .position(|&i| i == app.forms.strings_cursor)
        .unwrap_or(0);
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    scroll::render_scrolled_list(
        f,
        list,
        area,
        &mut app.strings_list_state,
        pos,
        total,
        scroll::SCROLL_PAD,
    );
```

**forms.rs** — удалить `form_details_text` (строки 285-287), `FormDetails` (289-293), `form_details` (295-399) и тест `form_details_reports_marker_and_info_lines` (783+). Проверить: других потребителей `form_details`/`form_details_text` нет (`grep -rn "form_details" crates/uefi-tui/src` — только ui/forms.rs:106, который удалён выше, и doc-комментарий app.rs:450, поправлен выше).

**main.rs** — проверить, что упоминаний `details_scroll` не осталось (`grep -n details_scroll crates/uefi-tui/src/main.rs` — пусто после Task 4).

- [ ] **Step 4: GREEN + мигрировавшие тесты живы**

Run: `cargo test -p uefi-tui`
Expected: PASS — новые 4 теста + `forms_list_offset_kept_when_cursor_walks_up` + `strings_list_uses_persistent_state` + `form_panel_*` зелёные; `follow_offset`/`details_*`-тестов в выводе нет.

- [ ] **Step 5: Lint + commit**

Run: `cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all && cargo test --all`

```bash
git add crates/uefi-tui/src/ui/forms.rs crates/uefi-tui/src/forms.rs crates/uefi-tui/src/app.rs
git commit -m "feat(tui): панель Form — трёхзонный рендер (список вопросов с гейт-хвостом, авто-рост низа); смерть wrap+scroll Paragraph"
```

---

### Task 6: `:o` получает `--mode` (минор из спеки §7)

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs:1086` (flags-match в `complete`)
- Test: `crates/uefi-tui/src/commands.rs` (`mod tests`)

**Interfaces:**
- Consumes: `complete(&App, &str) -> Completion` (контракт single-candidate: единственный кандидат → `common: Some(префикс+кандидат+" ")`, `items` пуст).

- [ ] **Step 1: Failing-тест**

В `mod tests` файла commands.rs (рядом с `complete_extract_offers_body_only_flag`):

```rust
    #[test]
    fn complete_o_alias_offers_mode_flag() {
        let app = crate::app::App::new();
        let c = complete(&app, "o p.bin --mo");
        assert_eq!(
            c.common.as_deref(),
            Some("o p.bin --mode "),
            "алиас :o получает флаги :open (single-candidate: common, items пуст)"
        );
        let c = complete(&app, "o p.bin --");
        assert_eq!(c.common.as_deref(), Some("o p.bin --mode "));
    }
```

- [ ] **Step 2: RED**

Run: `cargo test -p uefi-tui complete_o_alias`
Expected: FAIL (`common == None` — алиас не матчится на флаги).

- [ ] **Step 3: Реализация**

`commands.rs:1086`:

```rust
            "open" | "o" => &["--mode"],
```

- [ ] **Step 4: GREEN**

Run: `cargo test -p uefi-tui complete_o_alias`
Expected: PASS.

- [ ] **Step 5: Полный прогон + lint + commit**

Run: `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all`

```bash
git add crates/uefi-tui/src/commands.rs
git commit -m "fix(tui): :o --mode completion (алиас open в flags-match)"
```

---

## Финализация цикла (после Task 6)

1. `cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check` — весь workspace.
2. TODO.md: закрыть позицию «Форма-панель: трёхзонный layout» (3413) с ссылкой на цикл; minor `:o` из той же позиции закрыт Task 6.
3. Спека: приложить §Вердикт после живого гейта владельцем (как в цикле registry-polish).
4. PR в master, live-gейт владельцем: IntelRCSetup → Processor Configuration — шапка на месте, вопросы скроллятся, низ с деталями выбранного, гейты в хвосте, PgUp/PgDn прыгают страницей, `:o --mo<TAB>`.
