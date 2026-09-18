# Спека: трёхзонная панель «Form» + унификация скролл-механики списков

Дата: 2026-09-19. Ветка: `forms-panel-three-zone`.
Первоисточник: TODO.md «Форма-панель: трёхзонный layout» (дефект живого гейта цикла
registry-polish, §Вердикт `2026-09-18-tui-registry-polish-design.md`).

## Контекст и проблема

Правая панель «Form» Forms-view — один скроллируемый `Paragraph` с `.wrap()` +
`.scroll((off,0))`: шапка формы + все вопросы + question_info + гейты одним
текстом (`crates/uefi-tui/src/forms.rs:form_details`). На живом гейте
(IntelRCSetup → Processor Configuration):

1. на первом вопросе детали ниже сгиба — не видны без доскролла;
2. при скролле строки деталей накладываются на вопросы (визуальные артефакты);
3. на последнем вопросе детали появляются, шапка панели уезжает.

Корень артефактов: scroll у `Paragraph` работает по визуальным строкам после
переноса, а `marker_line`/`info_line` (цели авто-follow) считаются по
логическим строкам до переноса. Любая длинная строка (промпт, Path, options)
сдвигает нумерацию — офсет срезает строки посередине.

Решение владельца: три зоны — фиксированная шапка ~7 строк (детали формы +
колонки-хедер вопросов), скроллируемая середина (вопросы, паттерн
Image/Forms списков: гистерезис, курсор за 3 строки до края), фиксированный
низ ~5 строк (детали выбранного вопроса).

Решения владельца по развилкам (брейншторм 2026-09-19):

- **Gates** — хвост средней зоны (после вопросов, с пустой строкой), один
  скролл на панель, низ строго про выбранный вопрос.
- **Переполнение низа** (one_of с длинным списком options) — авто-рост до
  cap ≈ 45% высоты панели, не второй скролл и не жёсткая обрезка.
- **PgUp/PgDn на Details** — постраничный ход курсора вопросов (паритет с
  Tree основного вида), а не скролл Paragraph (механика умирает).

Попутное требование владельца: **унифицировать механику списков** — сегодня
блок «clamp → select → `compute_scrolled_offset` → `offset_mut` → render»
скопирован вручную 4 раза; новая средняя зона — пятый потребитель, будущие
списки должны подключаться одной строкой.

## §1 Унификация: `crates/uefi-tui/src/ui/scroll.rs` (новый модуль)

- `pub const SCROLL_PAD: usize = 3;`
- `pub fn render_scrolled_list(f: &mut Frame, list: List, area: Rect, state: &mut ListState, cursor: usize, total: usize, pad: usize)`:
  вычисляет `inner_h = area.height - 2`, clamp курсора по `total` (0 →
  `select(None)`, офсет 0), select, офсет через `tree::compute_scrolled_offset`
  c переданным `pad`, `render_stateful_widget`. Ровно та арифметика, что
  сегодня размножена по сайтам — поведение существующих списков не меняется.
- `pub fn page_down(cursor: usize, total: usize, page: usize) -> usize` —
  `(cursor + page).min(total - 1)` при `total > 0`, иначе 0.
- `pub fn page_up(cursor: usize, page: usize) -> usize` — `cursor.saturating_sub(page)`.
- Примитив `compute_scrolled_offset` остаётся в `tree.rs` нетронутым (вместе со
  своими тестами).

**Пад параметризован**, не захардкожен: сегодня registry использует pad **2**
(`ui/registry.rs:10`), остальные — pad **3** (`ui/tree.rs:11`,
`FORMS_SCROLL_PAD` в `ui/forms.rs:59`). Миграция — перенос без изменения
поведения: registry передаёт 2 (локальная константа остаётся), tree/forms/
strings/questions — `SCROLL_PAD` (3).

Миграция четырёх сайтов: `ui/tree.rs:52-62`, `ui/registry.rs:55-69`,
`ui/forms.rs` (forms-список и strings). `follow_offset`, `FORMS_SCROLL_PAD` и
обёртка `crate::tree::compute_scrolled_offset` в `ui/forms.rs` умирают вместе
с однопанельным Paragraph. `App::cursor_page_down/up` (`app.rs:289-299`)
переписываются на `page_down/page_up` (арифметика та же).

## §2 Layout панели

Внешний блок «Form» (рамка + тайтл + focus-стиль) — как сегодня
(`ui/forms.rs:render`, правая колонка 60%). Внутренняя область режется
`Layout::vertical([Length(header_h), Min(0), Length(bottom_h)])`. Внутренних
рамок нет: разделители — пустая строка в шапке и строка-заголовок вверху низа.

- **Шапка (а), 5–7 строк, всегда видна:** `Form:` / `Form ID:` / `FormSet:` /
  `Target:` / `Path:` (если непуст) / пустая / последняя строка —
  `Questions (N): prompt · qid · kind` при `questions_ready` (N =
  `questions.len()`), иначе `Questions: loading…`. `Paragraph` без wrap:
  длинный Path обрезается краем зоны.
- **Середина (б):** `List` на всё свободное место (см. §3).
- **Низ (в):** `bottom_h = bottom_lines.len().min(cap)`, где
  `cap = max(3, inner_h * 45 / 100)`, `inner_h` — высота панели внутри рамки.
  `Paragraph` без wrap, хвост сверх cap обрезается.
- **Крошечные терминалы:** середине гарантируется ≥ 1 строка. Если места не
  хватает — по порядку: низ сжимается до 2 строк, шапка до 2 (title + колонки-
  хедер), середина floor 1. Рендер не паникует ни при какой высоте.

## §3 Контент зон: `forms.rs::form_panel`

`form_details` / `FormDetails` / `form_details_text` (и `marker_line` /
`info_line`) умирают. Вместо них одна чистая сборка:

```rust
pub struct FormPanel {
    pub header: Vec<String>,    // зона а, включая пустую строку и колонки-хедер
    pub questions: Vec<String>, // только строки-вопросов
    pub gates: Vec<String>,     // "" + "Gates (N):" + строки; пуст, если гейтов нет
    pub bottom: Vec<String>,    // детали выбранного вопроса / плейсхолдеры
    pub questions_ready: bool,  // false => кэш не под этот key
}
pub fn form_panel(forms: &FormsData, rows: &[FormsRow], cursor: usize) -> FormPanel;
```

- **Строка вопроса:** `{icon} {prompt:<28} q{question_id:#x} {kind}` (icon —
  `theme::question_icon`, пустой промпт — `-`). Курсор — штатная подсветка
  DarkGray (`highlight_style`), маркер `>` умирает.
- **Средняя зона, `questions.len() > 0`:** items = question-строки + хвост
  гейтов (items гейтов — Dim-стиль; курсор на них не заходит — clamp по
  `questions.len() - 1`, гарантирован `forms_question_cursor_down/up`,
  `app.rs:435-446`). `total` для гистерезиса = items.len() — доскроллил
  список до конца, увидел гейты. Рендер через `render_scrolled_list` c
  `questions_state`, pad `SCROLL_PAD`.
- **Средняя зона, `!questions_ready`:** пустая зона (loading живёт в шапке,
  см. §2), List не рендерится. **При ready и `questions.len() == 0`:**
  статическая строка `(no questions)` + хвост гейтов как `Paragraph`.
- **Низ:** из `forms.question_info` — `✔ Question q{id:#x} ({kind}):`,
  `  store {s} · offset {off:#x} · width {w}`, `  range {min}..={max} step {st}`
  (numeric) или `  options: …` с переносом хвоста на строки с паддингом
  (one_of; формат как в текущем `form_details`). `question_info` отсутствует
  или `question_info_key` не совпадает с (форма, вопрос) — одна строка
  `(loading…)`.

## §4 Состояние

`FormsData` (`app.rs:69-89`):

- умирают: `details_scroll`, `details_anchor`, `details_followed`;
- добавляются: `questions_state: ListState` (персистентный офсет между
  кадрами), `questions_viewport: usize` (рендер пишет высоту средней зоны);
- остальное (`question_cursor`, `questions`, `gates`, `question_info`,
  `question_info_key`, …) без изменений.

`App::forms_question_page_size()` — `questions_viewport`, фолбэк 10 (зеркало
`page_size`/`tree_viewport_rows`, `app.rs:282-287`, `ui/tree.rs:51`).

## §5 Клавиши и хинты

Фокус Details (без изменений): `j/k` — вопрос ±1 c
`refresh_question_info_if_needed`, `Enter` — set-value prefill, `Ctrl-hjkl` —
фокус. Изменения:

- `PgDn/PgUp` на Details (`main.rs:273-278`, сегодня `details_scroll ± 10`) →
  `App::forms_question_page_down/up()` (через `scroll::page_down/up` и
  `forms_question_page_size`) + `refresh_question_info_if_needed` — как у j/k.
- Хинт `ui/mod.rs:55`:
  `NORMAL[Forms/Details]: j/k вопрос · PgUp/PgDn страница · Enter set-value · Tab image-view · :cmd · ?help · q`.
- `ui/help.rs` HELP, строка 41:
  `Details focus: j/k выбор вопроса · PgUp/PgDn страница · Enter → :hii set-value <item> <value>`.

## §6 Тесты

TDD по задачам плана. Ключевые:

- чистые: `form_panel` — содержимое зон, `questions_ready`, гейты отдельным
  полем; `page_down/page_up` (clamp, saturating); `render_scrolled_list`
  (через TestBackend-рендер существующих сценариев).
- рендер-тесты (TestBackend, как существующие в `ui/forms.rs`): шапка на
  месте при курсоре на последнем вопросе длинной формы; низ меняется со сменой
  `question_cursor`; авто-рост низа до cap и не выше; гейты видны при
  доскролле середины; курсор не заходит на гейт-строки; PgDn прыгает
  страницей.
- миграция: тесты `follow_offset*`, `details_anchor_reset_on_form_change`,
  `forms_details_manual_scroll_survives_until_target_moves` умирают вместе с
  механикой; `forms_list_offset_kept_when_cursor_walks_up` и
  `strings_list_uses_persistent_state` остаются зелёными на
  `render_scrolled_list`.
- help/хинт: assertion-тесты на новые строки (`ui/help.rs`, `ui/mod.rs`).

## §7 Минор: `:o` получает `--mode`

`commands.rs:1086`: `"open" => &["--mode"]` → `"open" | "o" => &["--mode"]`
(алиас уже исполняется там же, где `open`, — `commands.rs:183`).
Тест: `:o --mo<TAB>` → дополняет `--mode`.

## За рамками

- WebUI (Forms-интерфейс шлюза) — отдельные циклы.
- Кастомные виджеты-таблицы: колонки — `format!`-выравнивание, норма репо.
- Изменение tuning'а registry-скролла (pad 2 сохраняется).
- Скролл шапки/низа: зоны короткие, обрезка краем — принято.

## §Вердикт (живой гейт владельца, 2026-09-19)

Сценарий: IntelRCSetup → Processor Configuration (тот же, где вскрылся
дефект R8 цикла registry-polish) + общий обход.

1. **Три зоны:** на первом вопросе детали видны в низу сразу; шапка не
   уезжает на последнем вопросе; наложений при скролле нет — ✅.
   UX-заметка владельца: непривычно, что низ (детали вопроса) не отделён
   визуально от списка вопросов (безрамочное решение §2); разделитель —
   кандидат в мелкую правку, не блокирует.
2. **Скролл вопросов** (гистерезис, подсветка) — ✅.
3. **PgUp/PgDn** — страницы, низ обновляется; Enter — set-value prefill — ✅.
4. **Крошечный терминал** (сжат «вообще ничего не видно») — паники нет — ✅.
5. **`:o --mode`** — флаг и значения (read/write) — ✅.

Цикл признан прошедшим гейт. PR — merge по решению владельца.
