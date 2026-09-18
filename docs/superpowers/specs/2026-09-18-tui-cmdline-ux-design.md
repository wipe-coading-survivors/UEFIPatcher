# TUI cmdline UX — Design

**Date:** 2026-09-18
**Scope:** `uefi-tui`, точечная правка `uefi-common::state`
**Related:** TODO-раздел «Отложенные после V3» (completion-меню, V3-мелочи,
`TODO.md:3325-3378`); цикл TUI Revival A (A5 tab-completion — цикл надстраивается
над его грамматикой); будущий HII extract/export (переиспользует эту
cmdline-инфраструктуру без переделки — анализ брейнсторма 2026-09-18: все
компоненты generic-слоя, новые команды лишь добавят кандидатов).

## Motivation

Cmdline сегодня append-only: `App.cmdline: String` (`app.rs:124`), символы пушатся
в конец, Backspace режет хвост, курсора нет вовсе; рендер ставит блок-каретку всегда
в конец (`ui/cmdline.rs:9`). TAB-completion (Revival A5) умеет позиционную
грамматику и доводку по общему префиксу, но список кандидатов выбрасывается: всё,
что осталось от него, — одна строка в status_msg (`main.rs:166`). При ~180
item-кандидатах (`hii question add <TAB>`) доводка упирается в общий префикс и
дальше владелец работает вслепую («не выпадает», 2026-09-12, `TODO.md:3350`).
Истории команд нет. Фичесет согласован с владельцем в брейнсторме 2026-09-18
(см. «Решения владельца»).

## Goals

| # | Задача | Сторона | Суть |
|---|---|---|---|
| C1 | LineBuffer | TUI | Grapheme-корректный буфер с курсором: посимвольные/пословные прыжки, края, kill-опы |
| C2 | История команд | TUI+common | Персист в state-dir (cap 500, дедуп подряд) + prefix-recall |
| C3 | Completion-меню | TUI | Popup над cmdline: навигация, приём кандидата, live-фильтрация |
| C4 | Рендер cmdline | TUI | Курсор внутри строки, горизонтальный скролл длинных строк |
| C5 | Мелочи complete() | TUI | item-format-хелпер, `--ffs` guard по позиции токена, симлинки в complete_path |
| C6 | Контекстный hint-бар | TUI | Подсказка по клавишам ввода в hint-строке при активном Command/Insert, с учётом состояния меню |

## Non-goals

- Ctrl+R-поиск по истории — prefix-recall закрывает потребность; при необходимости
  добавляется аддитивно (решение владельца 2026-09-18).
- Live-popup (fish-style) без нажатия TAB.
- Vi-режим внутри cmdline (Esc занят выходом из Command-режима; вложенные режимы
  не заводим).
- PageUp/PageDown в меню (окно 8 строк, стрелок достаточно).
- История для uefi-cli (rustyline — отдельная тема, если понадобится).
- Прочие V3-мелочи вне C5: `fmt_u32_ids`, ассерт пустого forms-списка
  (`TODO.md:3325`).
- Расширение грамматики/кандидатов `complete()` (кроме C5-фиксов) — кандидаты
  HII extract/export добавятся позже аддитивно, позиционная грамматика этого не
  требует.

## Решения владельца (брейнсторм 2026-09-18)

1. История: персист + prefix-recall (Up/Down листают записи с набранным
   префиксом); Ctrl+R — YAGNI сейчас.
2. Набор клавиш редактирования: база + пословные прыжки + kill-опы
   (vim-cmdline/bash-паритет, таблица в секции handle_command).
3. Меню: вариант «Меню + навигация» — Up/Down/j/k листают открытое меню,
   Enter при открытом меню принимает выбранного кандидата в буфер (как
   TAB/→), исполнение — Enter при закрытом меню, Esc двухступенчатый.
   [правки живого гейта, см. § Вердикт]
4. Ядро line-editor: собственный `LineBuffer` на `unicode-segmentation`
   (tui-input/tui-textarea отклонены: их event-модель конфликтует с нашим
   AppEvent-пайплайном).

## Design

### C1: `src/line.rs` — LineBuffer

Курсор — byte-индекс на grapheme-границе; все операции поддерживают инвариант.
Разбиение — крейт `unicode-segmentation` (новая зависимость uefi-tui): graphemes
для посимвольных операций и позиционирования, `split_word_bounds` для пословных.

```rust
pub struct LineBuffer { s: String, cursor: usize }

impl LineBuffer {
    pub fn new() -> Self;                    // "", курсор 0
    pub fn from_str(s: &str) -> Self;        // курсор в конец
    pub fn as_str(&self) -> &str;
    pub fn set_str(&mut self, s: &str);      // TAB-доводка / recall / prefill; курсор в конец
    pub fn cursor_grapheme(&self) -> usize;  // позиция для рендера/hscroll
    pub fn insert(&mut self, c: char);
    pub fn backspace(&mut self);             // grapheme перед курсором
    pub fn delete(&mut self);                // grapheme под курсором
    pub fn left(&mut self);
    pub fn right(&mut self);
    pub fn word_left(&mut self);
    pub fn word_right(&mut self);
    pub fn home(&mut self);
    pub fn end(&mut self);
    pub fn kill_word(&mut self);             // от начала предыдущего слова до курсора
    pub fn kill_to_start(&mut self);
    pub fn kill_to_end(&mut self);
    pub fn is_empty(&self) -> bool;
}
```

Word-семантика — Unicode word bounds: `/`, `:`, пробел — разделители; Ctrl+Right
по пути прыгает по сегментам пути (как readline default). Спец-кейсов для путей
не заводим.

### C2: `src/history.rs` — History + state_dir в uefi-common

`uefi-common::state` получает хелпер `pub fn state_dir() -> PathBuf` (XDG state
dir через `directories::BaseDirs`, fallback как в `default_sock`;
`default_sock()` рефакторится на `state_dir().join("uefipatcher.sock")`).
Файл истории: `state_dir()/cmdline_history`, одна команда на строку, UTF-8.

```rust
pub struct History {
    entries: Vec<String>,   // старые → новые, cap 500
    recall: Option<usize>,  // текущая позиция листания
    saved: Option<String>,  // строка, набранная до первого prev
}

impl History {
    pub fn load() -> Self;                // нет файла — пусто; cap: последние 500; дедуп подряд
    pub fn submit(&mut self, line: &str); // непустая и != последней; сброс recall/saved
    pub fn save(&self);                   // fs::write + create_dir_all; история некритична
    pub fn prev(&mut self, prefix: &str) -> Option<String>;
    pub fn next(&mut self, prefix: &str) -> Option<String>; // за концом — saved, recall=None
}
```

- Prefix-recall: `prev`/`next` листают только записи с префиксом `prefix`
  (пустой префикс — вся история). Первый `prev` фиксирует `saved = prefix`.
- Сброс позиции: любая мутация буфера (вставка/удаление/kill/`set_str` из
  TAB-доводки или prefill) — `recall = None, saved = None`. Исключение:
  подстановка самой recall-строки (`set_str` из `prev`/`next`) позицию не
  сбрасывает, иначе листание было бы невозможно. Повторный Up после правки
  начинает с головы.
- Persist: `save()` вызывается после каждого исполненного Enter — переживает
  kill процесса, bash-style.

### C3: MenuState + `src/ui/menu.rs` — completion-меню

`App` получает `menu: MenuState`:

```rust
pub struct MenuState {
    open: bool,
    items: Vec<MenuItem>,
    selected: usize,
    offset: usize,   // скролл-окно
}
pub struct MenuItem {
    pub display: String, // строка в меню
    pub apply: String,   // полный cmdline после подстановки кандидата
}
```

`complete()` в commands.rs меняет возвращаемое на структуру (внутренний API;
мигрируют вызов в `main.rs::handle_command` и тесты commands.rs):

```rust
pub struct Completion {
    pub common: Option<String>, // доводка общим префиксом (сегодняшний rep)
    pub items: Vec<MenuItem>,
}
pub fn complete(app: &App, cmdline: &str) -> Completion;
```

`items[i].apply` строится тем же правилом, что и `common`, но с конкретным
кандидатом i.

Поведение:

- Открытие: TAB при `!open` → complete(); `common` применяется к буферу
  (`set_str`); `items` непусты → `open=true, selected=0, offset=0`.
- Навигация: Up/Down/j/k → `selected` ± 1 с wrap-around (как vim wildmenu);
  окно 8 строк, `offset` скроллится. j/k работают только при `open`
  (закрытое меню — обычный ввод символов).
- Приём: TAB/Right при `open` → `set_str(items[selected].apply)`, затем
  re-complete по новой строке (items опустели → закрыть).
- BackTab — цикл назад.
- Live-фильтрация: любая мутация буфера при `open` → re-complete, сброс
  `selected=0, offset=0`; items пусты → закрыть.
- Esc: `open` → закрыть; иначе — выход из Command-режима (как сегодня).
- Enter: `open` → принять выбранного кандидата (`accept_menu_selection`,
  как TAB/→); `!open` → исполнить набранное; после исполнения —
  `history.submit`, menu close. [правки живого гейта, см. § Вердикт]
- Up/Down при `!open` — prefix-recall истории (префикс = текущий буфер).
- Меню работает и в Insert-режиме (handle_command общий).

Рендер `ui/menu.rs`: popup (Clear + List) над cmdline-панелью, прижат к её
верхней кромке, ширина — ширина cmdline-панели; title = `n/N`; выделенная
строка — reversed; max высота 8, прокрутка по offset.

### C4: рендер cmdline (`ui/cmdline.rs`)

- Строка рендерится тремя span'ами: до курсора / grapheme под курсором
  (`Style::reversed`) / после курсора; курсор в конце — reversed-блок на
  пробеле. Хвостовая `▌` убирается.
- Горизонтальный скролл: если `cursor_grapheme` выходит из окна шириной
  inner-area — окно сдвигается, чтобы курсор оставался видим (vim-cmdline
  семантика).
- Insert-режим: тот же рендер (`{insert_cmd}> {cmdline}`); префикс фиксирован,
  скроллится только буфер.

### C5: мелочи complete() (`commands.rs`)

- `fmt_item(owner: &str, id: u32)` — хелпер `format!("{}#{}")`; дедуп в
  completion-ветках form/question и add_prefill (`TODO.md:3327`).
- `--ffs` guard: completion флага только в грамматической позиции флага — по
  индексу текущего токена, не по `contains` (`TODO.md:3330`).
- `complete_path`: симлинк-на-директорию раскрывается как директория (metadata
  по symlink-target), записи-директории получают `/`-суффикс (`TODO.md:3332`).

### C6: контекстный hint-бар при активном вводе (`ui/mod.rs::render_hint`)

Сегодня Command/Insert показывают минимум («Enter execute · Esc cancel ·
Backspace»). С новым фичесетом клавиш стало больше — hint-строка при активном
вводе раскрывает набор, следуя уже сложившемуся паттерну контекстных подсказок
Normal-режима (вариация по view/focus):

- `Command`, меню закрыто:
  `COMMAND: TAB compl · ↑↓ hist · Ctrl+←→ word · Ctrl+W/U/K del · Home/End · Enter run · Esc cancel`
- `Command`, меню открыто:
  `COMMAND[menu]: ↑↓/jk select · TAB/→/Enter accept · BackTab back · Esc close`
- `Insert`: те же две строки с префиксом `INSERT` (буфер и клавиши общие).

UTF-8 стрелки допустимы (hint-бар уже соседствует с Nerd Font и
box-drawing псевдографикой). Длина строк сопоставима с существующими
Normal-подсказками.

### input.rs

`AppEvent` += `Left, Right, Home, End, Delete, WordLeft, WordRight`. Маппинг:
`KeyCode::Left/Right` с CONTROL → WordLeft/WordRight; Home/End — клавишами;
Ctrl+A/E/W/U/K уже приходят как `Ctrl(c)`.

### handle_command (`main.rs`) — итоговая клавишная карта

| Клавиша | Действие |
|---|---|
| Left / Right | ±1 grapheme; Right при открытом меню — принять кандидата |
| Ctrl+Left / Ctrl+Right | ±1 слово |
| Home / Ctrl+A, End / Ctrl+E | курсор в начало / конец строки |
| Backspace / Delete | удалить grapheme перед / под курсором |
| Ctrl+W / Ctrl+U / Ctrl+K | kill_word / kill_to_start / kill_to_end |
| Up / Down | меню открыто → навигация меню; закрыто → history prev/next(prefix) |
| j / k | меню открыто → навигация меню (down/up); закрыто → ввод символа |
| TAB | !open → доводка common + открыть меню; open → принять selected |
| BackTab | цикл по кандидатам назад |
| Enter | open → принять выбранного кандидата (как TAB/→); иначе — исполнить набранное; после исполнения history.submit |
| Esc | open → закрыть меню; иначе — выход из Command-режима |
| прочие мутации буфера | op + сброс recall/saved + live-фильтр меню |

Prefill (`enter_command_mode`/`enter_insert_mode`) — `set_str` (курсор в конец).

## Тесты

- Unit `line.rs`: каждая операция; grapheme-кейсы (эмодзи-комбинации, кириллица,
  CJK); kill-опы на границах строки; word-прыжки по путям с `/`.
- Unit `history.rs`: дедуп подряд, cap 500 (обрезка старых), prefix-фильтр,
  next за концом возвращает saved, пустой submit игнорируется.
- Unit MenuState: навигация с wrap, скролл-окно, live-фильтр сбрасывает
  selected, close-on-empty.
- `commands.rs`: миграция существующих completion-тестов на `Completion`; новые —
  `--ffs` по позиции, симлинк-директория (tempdir), `fmt_item`.
- Render (TestBackend): курсор-спан в середине строки; hscroll длинной строки;
  popup меню (высота ≤8, подсветка selected, счётчик `n/N`); hint-строка для
  четырёх состояний ввода (Command/Insert × меню открыто/закрыто).
- Integration `tui_integration`: существующие кейсы не ломаются (миграция
  вызовов complete).
- TUI-гейт владельца на живом образе в конце цикла (паттерн Revival A).

## Risks

- Смена сигнатуры `complete()` — точечная миграция (вызовы: `main.rs::
  handle_command`, тесты commands.rs); внешних потребителей нет.
- `unicode-segmentation` — новая зависимость uefi-tui (лёгкая, без
  transitive-мусора).
- State-dir может не существовать при первом запуске — `create_dir_all` в
  `save()`; отсутствие файла истории — норма (пустая история).

## Вердикт (живой гейт владельца, 2026-09-18)

Цикл принят с двумя правками по итогам живого гейта:
1. «Enter всегда исполняет набранное» → Enter при открытом меню работает с
   кандидатом; навигация меню расширена до Up/Down/**j/k**. Причина: `:h<TAB>`
   подсвечивает `hii`, но `<Enter>` исполнял сырой текст `h` (алиас help).
2. Уточнение п.1 после прогона: Enter при открытом меню **принимает**
   выбранного кандидата в буфер (= TAB/→), но НЕ исполняет строку —
   иначе ломается комплит аргументов в середине команды
   (`:hii question add <TAB>` …). Исполнение — вторым Enter при
   закрытом меню.
История после рестарта подтверждена владельцем. Non-goals цикла
(fmt_u32_ids, ассерт пустого forms-списка) остались в TODO.
3. Принятый из меню (TAB/→/Enter) и единственный TAB-кандидат получают
   хвостовой пробел — bash-паритет («:h<TAB><Enter>» → `hii `, палец
   сразу жмёт следующее слово); директории (кандидат с `/`) — без
   пробела; `History::submit` тримит строку.
4. Path-completion на всех позициях грамматики с путём: `:open/:o`,
   `:save/:s`, `:upload` (слот после глагола) и значение `--file`;
   `~` / `~/` в `complete_path` раскрывается в `$HOME`. Относительные
   пути комплитятся от cwd процесса TUI (как в bash).
