# TUI migration + bugfix — Design

**Date:** 2026-08-12
**Scope:** `uefi-tui` (также небольшое расширение mock-сервера в `tests/mock_server.rs`)
**Related:** `2026-08-10-cli-topology-and-image-storage-design.md` (План A — механическая миграция TUI завершена в commit `344eb5f`), `2026-08-09-display-and-search-design.md` (`uefi-common::format`).
**TODO-блок:** `TODO.md` строки 143-155 («TUI: migration + bugfix (после Плана A)»).

## Motivation

План A (commit `344eb5f`, Task 6) выполнил **механический** rename RPC-вызовов в TUI и
заменил удалённый `dump_tree` (text) на `image_nodes_list` (структурированные `Node`).
При этом сознанно сохранён **display-bug** и накоплен ряд регрессий/дефектов:

1. **Плоский список вместо дерева.** `parse_nodes_flat` (`commands.rs:194`) ставит
   `depth: 0` и `has_children: false` для **каждого** узла → нет отступов, нет
   expand/collapse-глифов. Старый `parse_tree_dump` хотя бы считал `depth` из path
   (через `path.matches('/').count()`), но работал по text-дампу и терял subtype/GUID.
2. **Нет скроллинга.** `ui/tree.rs:42` использует `f.render_widget(list, …)` (non-stateful)
   → при числе узлов больше экрана (реальный образ = сотни узлов) курсор уходит за
   viewport, выбрать/увидеть его нельзя. Нужно `render_stateful_widget` + `ListState`.
3. **`active_image_id` не выставляется после `:open`** (`commands.rs:91`) → `:save` и
   `:extract` **всегда** падают с `"no active image"`. CLI выставляет его (`image.rs:17`),
   TUI — нет.
4. **Hint врёт:** обещает «Space expand», но в `handle_normal` (`main.rs:61`) нет ни
   expand/collapse, ни их клавиш. `expanded`/`has_children`-поля ни на что не влияют.
5. **`Mode::Insert` недостижим:** ни один путь не выполняет `app.mode = Mode::Insert`
   — мёртвый код. Нужное применение — интерактивный ввод для вставки секций.
6. **Details показывает сырые числа:** `node_type=66`, `subtype=04` без человеко-читаемых
   имён, хотя в `uefi-common::names` уже есть `node_type_name`/`file_type_name_or_raw`/
   `section_type_name_or_raw` (используются CLI в `output.rs`).
7. **Нет команд insert/replace/remove/rebuild** (есть в CLI `node …`, нет в TUI).
8. **Нет видимости артефактов и загруженных образов** — а они нужны, чтобы
   конструктивно использовать `--artifact-id` и переключаться между образами.

## Goals

1. **Локальный tree-рендер** из структурированных `Node`: `depth` и `has_children`
   вычисляются из `path`-иерархии; rich ratatui-виджет (иконки/цвета/курсор) сохранён.
2. **Корректный скроллинг** через stateful `List` + `ListState`.
3. **Expand/collapse** с vi-style клавишами `h`/`l`; курсор ходит по видимым строкам,
   свёрнутые поддеревья пропускаются; при сворачивании родителя курсор, стоявший на
   потомке, переходит на родителя. **По умолчанию развернуты только Image и его прямые
   дети (FV) — depth ≤ 1**; глубокие секции свёрнуты (см. «Default collapse»).
4. **Фикс `active_image_id`** после `:open` — `:save`/`:extract`/`:insert`/… работают.
5. **Новые ex-команды** (паритет с CLI `node insert/replace/remove/rebuild`,
   `image switch/close`): `--file PATH | --artifact-id ID` ровно один.
6. **Registry-панель** (третья панель): показывает загруженные образы (`images_list`) и
   артефакты (`artifacts_list`); авто-refresh на мутациях; **интерактивна** — `j/k`
   выбор, `Enter` на артефакте вставляет его id в cmdline, `Enter` на образе переключает
   активный.
7. **Навигация между панелями** (`Ctrl-h/j/k/l`, циклически) + focus-подсветка.
8. **Mode::Insert достижим**: `i` на узле предзаполняет cmdline шаблоном вставки.
9. **Details — человеко-читаемые имена** типов/подтипов (общая логика с CLI).

## Non-goals (перенесены в следующие циклы)

- **`ImageUpload` RPC** для docker-use-case (см. `TODO.md` «ImageUpload RPC»).
- **`:search`/`/`-filter** в TUI (CLI `node search`) — отдельный цикл; `image_nodes_list`
  пока вызывается с пустым фильтром.
- **Редактирование setup-полей** (Цикл 6).
- **Многошаговые wizard-промпты** в Mode::Insert — в этом цикле предзаполнение cmdline
  одной строкой; тонкая multi-field навигация — позже.
- **Сериализация/кэш дерева** — re-fetch `image_nodes_list` на каждом `:open` как сегодня.

## Архитектура

### Структура данных дерева (новый модуль `crates/uefi-tui/src/tree.rs`)

Чистые функции, детерминированно выводимые из `&[Node]` и `expanded`-флагов:

```rust
pub fn segments(path: &str) -> Vec<&str>;
//   ""     -> []
//   "0"    -> ["0"]
//   "0/3"  -> ["0", "3"]
//   "0/3/1"-> ["0", "3", "1"]

pub fn build_tree(nodes: &[Node]) -> Vec<TreeNode>;
//   depth        = segments(path).len()        // "" -> 0, "0" -> 1, "0/0" -> 2
//   has_children = exists node, чьи segments — строгий префикс
//   expanded     = depth <= 1   (см. «Default collapse» ниже)
//   action       = ACTION_NO (50)

pub fn visible_rows(tree: &[TreeNode]) -> Vec<usize>;
//   Обход индексов; пропус поддерева, чей родитель не expanded.
//   Возвращает индексы в `tree` (не сами узлы) — для cursor-mapping.
```

**Обоснование правила depth.** Engine строит путь так (`parser/image.rs:127-133`):
корень (Image) имеет `path: ""`, его дети — `"0"`, `"1"` (НЕ `"/0"`), внуки — `"0/0"`.
Старый `parse_tree_dump` текстового дампа **не включал** корень, поэтому его
`depth = path.matches('/').count()` лечил только top-level. Со включённым корнем
корректное правило — по сегментам: `depth = segments(path).len()`. Это даёт root=0,
дети=1, внуки=2 — совпадает с CLI `format_tree` (где root показывается как `Image`).

**`has_children` без O(n²).** Nodes приходят из engine уже в tree-order (обход в
глубину, `list_recursive`). Поэтому `has_children(i)` = «следующий узел (i+1) имеет
`segments.len() > segments(i).len()` И его сегменты — продолжение». Достаточно одного
линейного прохода с peek-next. Запасной `O(n²)`-fallback не нужен, но допустим
(n ≈ сотни).

### App state (`app.rs`)

```rust
pub enum Focus { Tree, Details, Registry }     // новая

pub struct RegistryData {
    pub images: Vec<ImageInfo>,
    pub artifacts: Vec<ArtifactInfo>,
    pub cursor: usize,        // индекс в «плоском» selectable-списке (см. ниже)
}

pub struct App {
    pub mode: Mode,
    pub tree: Vec<TreeNode>,            // полный плоский список с depth/has_children/expanded
    pub cursor: usize,                  // индекс в visible_rows() (Tree-focus)
    pub focus: Focus,                   // умолчание Tree
    pub registry: RegistryData,         // новое
    pub selected: Option<String>,
    pub cmdline: String,
    pub status_msg: String,
    pub image_loaded: bool,
    pub quit: bool,
    pub show_help: bool,
    pub active_image_id: Option<String>, // дублирует client.state для рендера
}
```

- `selected_path() -> Option<&str>` = путь узла под курсором (`tree[visible_rows[cursor]]`).
  Tree-курсор **независим** от registry-курсора: когда пользователь навигирует в registry
  (Ctrl-*), tree-курсор стоит на последнем выбранном узле — именно его путь подставляется
  в `:insert`/artifact-prefill через `selected_path()`.
- `cursor_down/up` ходят по `visible_rows()` с clamp; при `has_children=false`/нет
  видимого движения — clamp на границах.
- `toggle_expand_path()` флип `expanded`; после collapse если курсор был на потомке —
  переставить курсор так, чтобы он указывал на родителя (через индекс в `visible_rows`).

### Рендер дерева (`ui/tree.rs`)

```rust
let visible = visible_rows(&app.tree);
let state = {
    let mut s = ListState::default();
    s.select(Some(app.cursor.min(visible.len().saturating_sub(1))));
    s
};
let items: Vec<ListItem> = visible.iter().map(|&idx| { /* из tree[idx] */ }).collect();
f.render_stateful_widget(List::new(items).block(...), area, &mut state);
```

Stateful-рендер даёт автоматический скроллинг viewport-а к выбранной строке.

### Default collapse

`expanded = depth <= 1` при первичной загрузке (`build_tree`):
- depth 0 (Image-корень) и depth 1 (топовые firmware volumes — ME/DXE/PEI, пути `"0"`,`"1"`,`"2"`)
  **развёрнуты** → пользователь сразу видит список FFS-файлов внутри каждого FV.
- depth ≥ 2 (секции внутри файлов и глубже) **свёрнуты** → типовой UX: открыл BIOS,
  сразу видишь три больших раздела и их файлы, затем точечно раскрываешь нужный файл.

Ручной expand/collapse (`l`/`h`) свободно меняет состояние после загрузки.

### Details (`ui/details.rs`)

Человеко-читаемые имена из `uefi_common::names`:
```
Path:     0/3
Type:     File (66 / 0x42)        <- node_type_name() + код
Subtype:  DXE driver (0x07)       <- file_type_name_or_raw() / section_type_name_or_raw()
GUID:     8AC6...
Name:     Setup
Action:   none (50)
Children: 3                       <- число прямых детей (из иерархии)
```
Border подсвечивается когда `focus == Details` (детали read-only, `j/k` в нём no-op).

## Команды (commands.rs)

Новые ex-команды (синтаксис паритетен CLI `node …`):

| Команда | TARGET | обязательные | RPC |
|---|---|---|---|
| `:insert [TARGET] (--file PATH \| --artifact-id ID) [--mode into\|before\|after]` | selected_path | ровно один из `--file`/`--artifact-id` | `image_node_insert` |
| `:replace [TARGET] (--file PATH \| --artifact-id ID) [--body-only]` | selected_path | ровно один из `--file`/`--artifact-id` | `image_node_replace` |
| `:remove [TARGET]` | selected_path | — | `image_node_remove` |
| `:rebuild [TARGET]` | selected_path | — | `image_node_rebuild` |
| `:image switch ID` | — | ID | (локально) `app.active_image_id = client.state.active_image_id = Some(ID)` + re-fetch `image_nodes_list` → `build_tree` (другой образ = другие nodes) + registry refresh + `cursor=0` |
| `:image close [ID]` | active | — | `image_close` + registry refresh; если закрыт активный → `active_image_id=None`, `app.tree.clear()` |
| `:refresh` | — | — | рефетч `images_list` + `artifacts_list` |

Поведение `--file`/`--artifact-id` повторяет CLI (`node.rs:70-79`): при `--file` engine
сам загружает секцию и создаёт артефакт; при `--artifact-id` берётся из таблицы. После
каждой мутации — авто-refresh registry (новый артефакт).

**`:open` фикс** (главный регресс-тест бага #3):
```rust
app.active_image_id = Some(r.image_id.clone());
client.state.active_image_id = Some(r.image_id.clone());   // parity с CLI image.rs:17
```
+ refresh registry + (если нужно) re-fetch `image_nodes_list` уже для нового active image.

### Парсинг флагов

Утилита `parse_flags(parts: &[&str]) -> (Option<target>, Option<file>, Option<artifact_id>, Option<mode>, bool body_only)`
в `commands.rs` (или переиспользуется из `uefi-common`, если вынести туда позже).
`insert`/`replace` проверяют ровно-один-из и возвращают usage-error иначе.

## Mode::Insert

Нажатие `i`/`r`/`d` в Normal mode (Tree focus) предзаполняет cmdline и переходит в Insert:
```
i -> app.cmdline = format!("insert {} --file ", path);
r -> app.cmdline = format!("replace {} --file ", path);
d -> app.cmdline = format!("remove {}", path);
app.mode = Mode::Insert;
```
(`path` = `selected_path().unwrap_or_default()`.) `ui/cmdline.rs` рендерит Insert как
`{cmd}> {cmdline}` (подсказка операции вместо `:`). Существующая машина (`handle_command`:
Key/Enter/Esc/Backspace) переиспользуётся — Enter выполняет `app.cmdline` как ex-команду,
Esc отменяет. Для `d` (remove) prefill уже полный — достаточно Enter; для `i`/`r`
пользователь дописывает `--file PATH` / `--artifact-id ID` (или берёт артефакт из registry
через `Enter` на нём). Multi-field wizard — out-of-scope.

## Registry-панель (новый `ui/registry.rs`)

Layout B — правая колонка вертикально сплитится: details сверху, registry снизу:

```
┌─ tree ──────┬─ details ─────────────┐
│             │                       │
│  (full      │                       │
│   height)   ├─ registry ────────────┤
│             │ Images                │
│             │  ► abc123 HNX99TF 16M │
│             │ Artifacts             │
│             │    g1k2   section 4K  │
├─────────────┴───────────────────────┤
│ cmdline                             │
└─ hint ──────────────────────────────┘
```

**Selectable list:** секционные заголовки (`Images`, `Artifacts`) — non-selectable rows
(пропускаются курсором). Selectable rows = image-rows ++ artifact-rows. `RegistryData.cursor`
индексирует только selectable.

**Действия (Registry focus):**
- `j`/`k` — перемещение курсора (clamp, skip заголовков).
- `Enter` на **image** → выполнить `:image switch <id>` (немедленно), refresh registry,
  focus → Tree (возврат к работе с деревом).
- `Enter` на **artifact** → переход в Command mode с
  `app.cmdline = format!("insert {} --artifact-id {} ", selected_path, id)`,
  курсор-в-конце; пользователь правит/дополняет и Enter.
- Border подсвечивается при `focus == Registry`.

**Refresh:** метод `refresh_registry(client) -> Result` дёргает `images_list` +
`artifacts_list` (session_id из `client.state`). Вызывается после: `:open`, `:extract`,
`:import`, `:insert`, `:replace`, `:image switch`, `:image close`, и вручную через
`:refresh`. Insert/replace via `--file` создают артефакт на сервере → refresh его покажет.

## Навигация и keybindings

### Focus ring (циклическая)

Панели образуют кольцо `[Tree, Details, Registry]` (layout B). Из любой панели:
- `Ctrl-l` / `Ctrl-j` → next (Tree→Details→Registry→Tree)
- `Ctrl-h` / `Ctrl-k` → prev (Tree→Registry→Details→Tree)

«По циклу»: с верхней панели `Ctrl-k` оборачивается на нижнюю и наоборот. Все 4 клавиши
работают (h/l и j/k — алиасы направлений кольца; для 3 панелей этого достаточно и
соответствует запросу «по цикку»).

> Альтернатива — честная 2D-навигация (h/l = горизонталь Tree↔right-col, j/k =
> вертикальь Details↔Registry) — отложена: на 3 панелях кольцо проще и предсказуемее,
> а все 4 клавиши активны. Если на ревизии захочется 2D — уточним в плане.

### Клавиши по режимам

**Normal (mode = Normal):**
| Key | Tree focus | Details focus | Registry focus |
|---|---|---|---|
| `j`/`k` | tree cursor по visible | no-op | registry cursor |
| `h` | collapse selected | no-op | no-op |
| `l` | expand selected | no-op | no-op |
| `Enter` | no-op | no-op | image→switch / artifact→cmdline prefill |
| `i` | Mode::Insert (prefill `insert … --file `) | — | — |
| `r` | Mode::Insert (prefill `replace … --file `) | — | — |
| `d` | Mode::Insert (prefill `remove <selected>`) | — | — |
| `Ctrl-h/j/k/l` | focus ring | focus ring | focus ring |
| `:` | Command mode | Command mode | Command mode |
| `?` | help overlay (toggle) | help overlay | help overlay |
| `q` | quit | quit | quit |

`i`/`r`/`d` — быстрые переходы в Insert для частых операций (insert/replace/remove).
`:rebuild` остаётся только ex-командой (редкая операция; ключ `b` зарезервирован на
будущее, если потребуется).

**Command / Insert:** существующая логика (`handle_command`) без изменений — Key/Enter/
Esc/Backspace作用于 `app.cmdline`. Разница только в prompt-префиксе рендера (`:` vs `insert>`).

`Mode::Insert` теперь достижим (через `i`) — баг #5 закрыт.

## Тесты (TDD, module-first)

- **`tree.rs`** (новый, объявить `pub mod tree;` в `lib.rs` **до** первого `cargo test`):
  - `segments_empty`, `segments_root`, `segments_nested`.
  - `build_tree_depth_and_children` на фикстуре `[Image"", Volume"0", File"0/0", Section"0/0/0"]`
    (проверить depth = 0/1/2/3, has_children true/false).
  - `visible_rows_hides_collapsed` (после ручного `tree[0].expanded=false` проверка).
  - `visible_rows_all_expanded_shows_everything`.
- **`app.rs`**: `selected_path_indexes_visible`; `toggle_expand_moves_cursor_to_parent_when_on_descendant`;
  `cursor_down_clamps_visible`; focus ring transitions `Ctrl-*`.
- **`commands.rs`**: обновить `parse_nodes_flat_basic` → тесты `build_tree`;
  `parse_insert_flags_exactly_one_of`; `open_sets_active_image_id` (мок `image_open` +
  проверка `active_image_id == Some(resp.image_id)`); `save_uses_active_image_id`.
- **`tests/mock_server.rs`**: расширить `image_nodes_list` до многоуровневой фикстуры
  (Image + Volume + File + Section), добавить `images_list`/`artifacts_list` непустые
  данные; smoke-тест цикла open→(tree rendered)→insert→extract.

## File map

| Файл | Изменение |
|---|---|
| `crates/uefi-tui/src/tree.rs` | **новый** — `segments`, `build_tree`, `visible_rows` |
| `crates/uefi-tui/src/lib.rs` | `pub mod tree;` |
| `crates/uefi-tui/src/app.rs` | `Focus`, `RegistryData`, `active_image_id`, `selected_path`, `toggle_expand_path`, focus-ring, registry cursor |
| `crates/uefi-tui/src/commands.rs` | удалить `parse_nodes_flat`; добавить `:insert/:replace/:remove/:rebuild/:image switch/:image close/:refresh`; фикс `:open` (active_image_id); `refresh_registry` |
| `crates/uefi-tui/src/main.rs` | `handle_normal`: `h`/`l`/`i`/`Ctrl-*`; фокус-маршрутизация `j`/`k`/`Enter` по focus |
| `crates/uefi-tui/src/ui/mod.rs` | layout B (правая колонка сплит: details/registry) |
| `crates/uefi-tui/src/ui/tree.rs` | `render_stateful_widget` + `ListState` (скроллинг) |
| `crates/uefi-tui/src/ui/details.rs` | имена из `uefi_common::names`; focus-бордер |
| `crates/uefi-tui/src/ui/registry.rs` | **новый** — образы + артефакты, selectable, focus-бордер |
| `crates/uefi-tui/src/ui/help.rs` | **переписать**: полноэкранный scrollable-оверлей, актуальный список команд/клавиш |
| `crates/uefi-tui/src/ui/cmdline.rs` | `Mode::Insert` → prompt `<cmd>> ` |
| `crates/uefi-tui/src/ui/hint.rs` (или `mod.rs::render_hint`) | обновить hint: `h/l collapse/expand · Ctrl-hjkl focus · i insert · : cmd` |
| `crates/uefi-tui/tests/mock_server.rs` | многоуровневая фикстура + непустые images/artifacts lists |

## Help overlay (`ui/help.rs`)

Текущий help **устарел** (перечисляет несуществующие `:dump`/`:find`/`:set-visibility`/
`:session`) и краток. Заменяется на **полноэкранный scrollable-оверлей** (`?` — toggle),
содержит:
- **Modes:** Normal / Command / Insert — что можно делать.
- **Keybindings по mode и focus** (таблица из секции «Навигация»).
- **Ex-команды** — полный список с аргументами и default-значениями (из таблицы
  «Команды»): `:open`, `:save`, `:extract`, `:export`, `:import`, `:insert`, `:replace`,
  `:remove`, `:rebuild`, `:image switch/close`, `:refresh`, `:artifacts`, `:help`, `:quit`.
- **Registry interaction:** `Enter` на артефакте/образе.

Реализация: `Paragraph` в full-area блоке + `ListState`-скроллинг (j/k внутри help-оверлея),
Esc/`?` — закрытие. Высота содержимого > экрана → скроллинг как у дерева.

## Decisions log

- **D1:** Rich ratatui-виджет сохранён; `format_tree`/`format_legend` НЕ используются
  как рендерер (они дают plain text, конфликт со styled TUI). Общая логика с CLI —
  только через `uefi-common::names` (имена типов/подтипов). Иконки/цвета — TUI-side
  (`theme.rs`).
- **D2:** `depth = segments(path).len()` (не `path.matches('/').count()`) — корректно
  для root с пустым path (баг старого `parse_tree_dump`).
- **D3:** Focus — кольцевая навигация (все 4 `Ctrl-*` активны), не 2D-сетка.
- **D4:** Registry интерактивна (выбор + Enter), не read-only — пользователь явно просил
  не печатать artifact_id руками.
- **D5:** `h`=collapse, `l`=expand (vim-convention) — подтверждено пользователем.
- **D6:** `:image switch`/`:image close` локально манипулируют `active_image_id`
  (отдельного RPC «switch» нет — это клиент-side концепция активного образа). Switch
  также перезагружает дерево через `image_nodes_list`; close — вызывает `image_close`.
- **D7:** Default `expanded = depth <= 1` (Image + FV развёрнуты, секции свёрнуты) —
  типовой UX, чтобы сразу видеть ME/DXE/PEI и их файлы без раскрытия.
- **D8:** `i`/`r`/`d` — Normal-mode быстрые клавиши insert/replace/remove (prefill
  cmdline); `:rebuild` пока только ex-command (редкая операция).
