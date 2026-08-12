# TUI migration + bugfix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Заменить плоский `parse_nodes_flat` на локальный tree-рендер с корректной иерархией, скроллингом, expand/collapse; починить `active_image_id` после `:open`; добавить команды insert/replace/remove/rebuild + image switch/close; добавить интерактивную Registry-панель (образы + артефакты) и focus-навигацию (`Ctrl-h/j/k/l`); сделать Mode::Insert достижимым (`i`/`r`/`d`); переписать help.

**Architecture:** Новый чистый модуль `tree.rs` (`segments`/`build_tree`/`visible_rows`) вычисляет иерархию из `path` (depth = число сегментов; has_children по следующему узлу в DFS-порядке). `App` хранит полный `tree: Vec<TreeNode>` + `cursor` по **видимым** строкам + `Focus`/`RegistryData`. Рендер дерева — stateful `List`+`ListState` (автоскролл). Registry — третья панель (layout B: правая колонка сплит details/registry), `Enter` на артефакте prefill-ит cmdline. Переиспользуются `uefi_common::names` (имена типов) — самописного нейминга нет; иконки/цвета остаются в `theme.rs`.

**Tech Stack:** Rust (edition 2024), ratatui 0.28 (`ListState`, `render_stateful_widget`), crossterm 0.28 (Ctrl-modifier capture), tonic (engine RPC), `uefi-proto` (`Node`, `ImageInfo`, `ArtifactInfo`), `uefi-common` (`names`, `state`).

**Spec:** `docs/superpowers/specs/2026-08-12-tui-migration-bugfix-design.md`
**Baseline commit:** `ed0e993` (зелёный: `cargo test -p uefi-tui` = 13 unit + 3 integration).

## Global Constraints

- **Module-first (критично из AGENTS.md):** при создании нового файла-модуля — `pub mod X;` в `lib.rs` **в том же шаге**, до `cargo test`.
- **Без комментариев** в коде (кроме ссылок на референс `file:line`).
- **TDD-порядок:** (1) тесты + объявить mod → (2) `cargo test -p uefi-tui` (RED) → (3) реализация → (4) `cargo test -p uefi-tui` (GREEN) → (5) `cargo clippy -p uefi-tui -- -D warnings` → (6) commit.
- **Имена RPC** noun-first: `image_open`, `image_nodes_list`, `image_node_insert`, `images_list`, `artifacts_list`, `image_close` и т.д. (см. `crates/uefi-proto/proto/engine.proto`).
- **Node-типы:** `Node { path: String, r#type: u32, subtype: u32, guid: String, offset: u64, size: u64, name: String }`. `subtype` кастуется `as u8` (тема/имена работают с u8).
- **Path-формат engine:** корень `""`, дети `"0"`/`"1"`, внуки `"0/0"` (см. `crates/uefi-engine/src/parser/image.rs:107,115,127-133`).
- **Commit-сообщения** conventional: `feat(uefi-tui): …` / `fix(uefi-tui): …` / `refactor(uefi-tui): …`.

---

## File map

| Файл | Действие | Зона ответственности |
|---|---|---|
| `crates/uefi-tui/src/tree.rs` | **create** | `segments`, `build_tree`, `visible_rows` (чистые фн) |
| `crates/uefi-tui/src/lib.rs` | modify | `pub mod tree;` (+ `pub mod ui;` уже есть) |
| `crates/uefi-tui/src/app.rs` | modify | `Focus`, `RegistryData`, `active_image_id`, `selected_*`, `toggle_expand_selected`, `sanitize_cursor`, focus-ring, registry cursor, `details_text` |
| `crates/uefi-tui/src/commands.rs` | modify | удалить `parse_nodes_flat`; `:open` фикс; `refresh_registry`; `parse_node_cmd_args`; `:insert/:replace/:remove/:rebuild`; `:image switch/close`; `:refresh` |
| `crates/uefi-tui/src/input.rs` | modify | `AppEvent::Ctrl(char)` для focus-ring |
| `crates/uefi-tui/src/main.rs` | modify | `handle_normal` routing по focus; `h/l/i/r/d`; `Ctrl-*`; `Mode::Insert` через `handle_command` |
| `crates/uefi-tui/src/ui/mod.rs` | modify | layout B (сплит правой колонки details/registry) |
| `crates/uefi-tui/src/ui/tree.rs` | modify | `render_stateful_widget` + `ListState` |
| `crates/uefi-tui/src/ui/details.rs` | modify | имена из `uefi_common::names`; focus-бордер; `details_text` |
| `crates/uefi-tui/src/ui/registry.rs` | **create** | образы + артефакты; selectable; focus-бордер |
| `crates/uefi-tui/src/ui/cmdline.rs` | modify | `Mode::Insert` → prompt `<cmd>> ` |
| `crates/uefi-tui/src/ui/help.rs` | rewrite | полноэкранный scrollable overlay, актуальные команды/клавиши |
| `crates/uefi-tui/tests/mock_server.rs` | modify | многоуровневая фикстура `image_nodes_list`; непустые `images_list`/`artifacts_list` |

---

## Task 1: `tree.rs` — чистые функции иерархии

**Files:**
- Create: `crates/uefi-tui/src/tree.rs`
- Modify: `crates/uefi-tui/src/lib.rs` (добавить `pub mod tree;`)
- Test: `crates/uefi-tui/src/tree.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn segments(path: &str) -> Vec<&str>`; `pub fn build_tree(nodes: &[uefi_proto::Node]) -> Vec<crate::app::TreeNode>`; `pub fn visible_rows(tree: &[crate::app::TreeNode]) -> Vec<usize>`.
- Consumes: `crate::app::TreeNode` (поля: `path:String, depth:usize, node_type:u8, subtype:u8, guid:Option<String>, name:String, action:u8, expanded:bool, has_children:bool`), `crate::theme::ACTION_NO`, `uefi_proto::Node`.

- [ ] **Step 1: Создать `tree.rs` с тестами + объявить модуль**

Создай `crates/uefi-tui/src/tree.rs`:

```rust
use crate::app::TreeNode;
use crate::theme::ACTION_NO;
use uefi_proto::Node;

pub fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

pub fn build_tree(nodes: &[Node]) -> Vec<TreeNode> {
    let n = nodes.len();
    nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| {
            let segs = segments(&nd.path);
            let depth = segs.len();
            let has_children = i + 1 < n && {
                let next_segs = segments(&nodes[i + 1].path);
                next_segs.len() > depth && next_segs[..depth] == segs[..]
            };
            TreeNode {
                path: nd.path.clone(),
                depth,
                node_type: nd.r#type as u8,
                subtype: nd.subtype as u8,
                guid: if nd.guid.is_empty() {
                    None
                } else {
                    Some(nd.guid.clone())
                },
                name: nd.name.clone(),
                action: ACTION_NO,
                expanded: depth <= 1,
                has_children,
            }
        })
        .collect()
}

pub fn visible_rows(tree: &[TreeNode]) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut ancestor_expanded: Vec<bool> = Vec::new();
    for (i, node) in tree.iter().enumerate() {
        ancestor_expanded.truncate(node.depth);
        if ancestor_expanded.iter().all(|&e| e) {
            visible.push(i);
        }
        ancestor_expanded.push(node.expanded);
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nd(path: &str, type_: u32, subtype: u32) -> Node {
        Node {
            path: path.into(),
            r#type: type_,
            subtype,
            guid: String::new(),
            offset: 0,
            size: 0,
            name: String::new(),
        }
    }

    #[test]
    fn segments_root_and_children() {
        assert_eq!(segments(""), Vec::<&str>::new());
        assert_eq!(segments("0"), vec!["0"]);
        assert_eq!(segments("0/3"), vec!["0", "3"]);
        assert_eq!(segments("0/3/1"), vec!["0", "3", "1"]);
    }

    #[test]
    fn build_tree_depth_and_children() {
        let nodes = vec![
            nd("", 62, 0),
            nd("0", 65, 0),
            nd("0/0", 66, 0x07),
            nd("0/0/0", 67, 0x15),
        ];
        let tree = build_tree(&nodes);
        assert_eq!(tree[0].depth, 0);
        assert_eq!(tree[1].depth, 1);
        assert_eq!(tree[2].depth, 2);
        assert_eq!(tree[3].depth, 3);
        assert!(tree[0].has_children);
        assert!(tree[1].has_children);
        assert!(tree[2].has_children);
        assert!(!tree[3].has_children);
    }

    #[test]
    fn build_tree_default_expanded_only_top_two_levels() {
        let nodes = vec![nd("", 62, 0), nd("0", 65, 0), nd("0/0", 66, 0), nd("0/0/0", 67, 0)];
        let tree = build_tree(&nodes);
        assert!(tree[0].expanded);
        assert!(tree[1].expanded);
        assert!(!tree[2].expanded);
        assert!(!tree[3].expanded);
    }

    #[test]
    fn visible_rows_all_expanded_shows_everything() {
        let mut nodes = vec![nd("", 62, 0), nd("0", 65, 0), nd("0/0", 66, 0)];
        nodes[0].r#type = 62;
        let mut tree = build_tree(&nodes);
        for n in &mut tree {
            n.expanded = true;
        }
        let v = visible_rows(&tree);
        assert_eq!(v, vec![0, 1, 2]);
    }

    #[test]
    fn visible_rows_hides_collapsed_subtree() {
        let nodes = vec![
            nd("", 62, 0),
            nd("0", 65, 0),
            nd("0/0", 66, 0),
            nd("0/0/0", 67, 0),
            nd("1", 65, 0),
        ];
        let mut tree = build_tree(&nodes);
        tree[1].expanded = false;
        let v = visible_rows(&tree);
        assert_eq!(v, vec![0, 1, 4]);
    }
}
```

Добавь в `crates/uefi-tui/src/lib.rs` строку `pub mod tree;` (сразу после `pub mod app;`).

- [ ] **Step 2: RED — `cargo test -p uefi-tui tree` (падает: модуля/функций нет до создания — но мы создали файл в Step 1; тесты должны сразу PASS).**

Если следуешь строго TDD: сначала создай файл БЕЗ тел `build_tree`/`visible_rows` (только сигнатуры `todo!()`), запусти — упадёт; потом вставь тела. Для экономии шагов допускается создать с телами сразу и убедиться, что PASS. 

Run: `cargo test -p uefi-tui tree::tests`
Expected: `5 passed`.

- [ ] **Step 3: Полная проверка + clippy**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```
Expected: все тесты PASS (13 + 5 новых = 18 unit), warnings нет.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/tree.rs crates/uefi-tui/src/lib.rs
git commit -m "feat(uefi-tui): tree.rs hierarchy helpers (segments/build_tree/visible_rows)"
```

---

## Task 2: `app.rs` — состояние Focus, Registry, cursor по visible

**Files:**
- Modify: `crates/uefi-tui/src/app.rs`
- Test: inline `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `crate::tree::visible_rows`, `uefi_proto::{ImageInfo, ArtifactInfo}`.
- Produces: `enum Focus { Tree, Details, Registry }`; `struct RegistryData { images: Vec<ImageInfo>, artifacts: Vec<ArtifactInfo>, cursor: usize }`; поля `App.focus: Focus`, `App.registry: RegistryData`, `App.active_image_id: Option<String>`; методы `selected_tree_idx`, `selected_path`, `toggle_expand_selected`, `sanitize_cursor`, `focus_next`, `focus_prev`, `registry_cursor_down/up`, `current_registry_row`; фн `details_text(&TreeNode) -> String`.

- [ ] **Step 1: Расширить `app.rs` — написать тесты первой (RED)**

Замени весь `crates/uefi-tui/src/app.rs` содержимым ниже (включает новые поля, методы, фокус-кольцо, registry-курсор и тесты):

```rust
use uefi_proto::{ArtifactInfo, ImageInfo};

use crate::theme::ACTION_NO;
use crate::tree::visible_rows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Details,
    Registry,
}

impl Focus {
    pub fn next(self) -> Focus {
        match self {
            Focus::Tree => Focus::Details,
            Focus::Details => Focus::Registry,
            Focus::Registry => Focus::Tree,
        }
    }
    pub fn prev(self) -> Focus {
        match self {
            Focus::Tree => Focus::Registry,
            Focus::Details => Focus::Tree,
            Focus::Registry => Focus::Details,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub path: String,
    pub depth: usize,
    pub node_type: u8,
    pub subtype: u8,
    pub guid: Option<String>,
    pub name: String,
    pub action: u8,
    pub expanded: bool,
    pub has_children: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RegistryData {
    pub images: Vec<ImageInfo>,
    pub artifacts: Vec<ArtifactInfo>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub enum RegistryRow {
    Image(usize),
    Artifact(usize),
}

pub struct App {
    pub mode: Mode,
    pub tree: Vec<TreeNode>,
    pub cursor: usize,
    pub focus: Focus,
    pub registry: RegistryData,
    pub active_image_id: Option<String>,
    pub selected: Option<String>,
    pub cmdline: String,
    pub insert_cmd: &'static str,
    pub status_msg: String,
    pub image_loaded: bool,
    pub quit: bool,
    pub show_help: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            tree: vec![],
            cursor: 0,
            focus: Focus::Tree,
            registry: RegistryData::default(),
            active_image_id: None,
            selected: None,
            cmdline: String::new(),
            insert_cmd: "",
            status_msg: "Welcome. Press : for commands, ? for help".into(),
            image_loaded: false,
            quit: false,
            show_help: false,
        }
    }

    pub fn visible(&self) -> Vec<usize> {
        visible_rows(&self.tree)
    }

    pub fn selected_tree_idx(&self) -> Option<usize> {
        self.visible().get(self.cursor).copied()
    }

    pub fn selected_path(&self) -> Option<String> {
        self.selected_tree_idx()
            .and_then(|i| self.tree.get(i))
            .map(|n| n.path.clone())
    }

    pub fn cursor_down(&mut self) {
        let n = self.visible().len();
        if n > 0 && self.cursor + 1 < n {
            self.cursor += 1;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn toggle_expand_selected(&mut self) {
        if let Some(idx) = self.selected_tree_idx()
            && let Some(node) = self.tree.get_mut(idx)
            && node.has_children
        {
            node.expanded = !node.expanded;
        }
    }

    pub fn set_expand_selected(&mut self, expand: bool) {
        if let Some(idx) = self.selected_tree_idx()
            && let Some(node) = self.tree.get_mut(idx)
            && node.has_children
        {
            node.expanded = expand;
        }
    }
            }
        }
    }

    pub fn sanitize_cursor(&mut self) {
        let n = self.visible().len();
        if n == 0 {
            self.cursor = 0;
        } else if self.cursor >= n {
            self.cursor = n - 1;
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    pub fn registry_selectable(&self) -> Vec<RegistryRow> {
        let mut rows = Vec::new();
        for i in 0..self.registry.images.len() {
            rows.push(RegistryRow::Image(i));
        }
        for i in 0..self.registry.artifacts.len() {
            rows.push(RegistryRow::Artifact(i));
        }
        rows
    }

    pub fn current_registry_row(&self) -> Option<RegistryRow> {
        self.registry_selectable().get(self.registry.cursor).cloned()
    }

    pub fn registry_cursor_down(&mut self) {
        let n = self.registry_selectable().len();
        if n > 0 && self.registry.cursor + 1 < n {
            self.registry.cursor += 1;
        }
    }

    pub fn registry_cursor_up(&mut self) {
        if self.registry.cursor > 0 {
            self.registry.cursor -= 1;
        }
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.cmdline.clear();
        self.insert_cmd = "";
    }

    pub fn enter_insert_mode(&mut self, cmd: &'static str, prefill: String) {
        self.mode = Mode::Insert;
        self.insert_cmd = cmd;
        self.cmdline = prefill;
    }

    pub fn exit_to_normal(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
        self.insert_cmd = "";
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

pub fn details_text(node: &TreeNode) -> String {
    let type_name = uefi_common::names::node_type_name(node.node_type as u32);
    let sub_name = match node.node_type {
        66 => uefi_common::names::file_type_name_or_raw(node.subtype),
        67 => uefi_common::names::section_type_name_or_raw(node.subtype),
        _ => String::new(),
    };
    let sub_part = if sub_name.is_empty() {
        format!("0x{:02X}", node.subtype)
    } else {
        format!("{} (0x{:02X})", sub_name, node.subtype)
    };
    format!(
        "Path:     {}\nType:     {} ({} / 0x{:02X})\nSubtype:  {}\nGUID:     {}\nName:     {}\nAction:   {}\nChildren: {}",
        node.path, type_name, node.node_type, node.node_type, sub_part,
        node.guid.as_deref().unwrap_or("(none)"), node.name, node.action, node.has_children,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(path: &str, depth: usize) -> TreeNode {
        TreeNode {
            path: path.into(),
            depth,
            node_type: 62,
            subtype: 0,
            guid: None,
            name: String::new(),
            action: ACTION_NO,
            expanded: true,
            has_children: depth == 0,
        }
    }

    #[test]
    fn app_new_starts_normal_tree_focus() {
        let app = App::new();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Tree);
        assert!(app.active_image_id.is_none());
    }

    #[test]
    fn selected_path_indexes_visible() {
        let mut app = App::new();
        app.tree = vec![node("0", 0), node("0/0", 1)];
        assert_eq!(app.selected_path(), Some("0".into()));
        app.cursor_down();
        assert_eq!(app.selected_path(), Some("0/0".into()));
    }

    #[test]
    fn toggle_expand_selected_flips_only_parents() {
        let mut app = App::new();
        app.tree = vec![node("", 0), node("0", 1)];
        app.cursor = 0;
        app.toggle_expand_selected();
        assert!(!app.tree[0].expanded);
        app.toggle_expand_selected();
        assert!(app.tree[0].expanded);
    }

    #[test]
    fn sanitize_cursor_clamps_to_visible() {
        let mut app = App::new();
        app.tree = vec![node("", 0), node("0", 1)];
        app.cursor = 5;
        app.sanitize_cursor();
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn focus_ring_cycles() {
        let mut app = App::new();
        assert_eq!(app.focus, Focus::Tree);
        app.focus_next();
        assert_eq!(app.focus, Focus::Details);
        app.focus_next();
        assert_eq!(app.focus, Focus::Registry);
        app.focus_next();
        assert_eq!(app.focus, Focus::Tree);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Registry);
    }

    #[test]
    fn registry_selectable_and_cursor() {
        let mut app = App::new();
        app.registry.images = vec![ImageInfo::default()];
        app.registry.artifacts = vec![ArtifactInfo::default(), ArtifactInfo::default()];
        let rows = app.registry_selectable();
        assert_eq!(rows.len(), 3);
        app.registry_cursor_down();
        assert_eq!(app.registry.cursor, 1);
        assert!(matches!(app.current_registry_row(), Some(RegistryRow::Artifact(0))));
    }
}
```

> Примечание: `details_text` использует `uefi_common::names`. `format_icon_fallback` — заглушка для не-`ACTION_NO` (в этом цикле все узлы `ACTION_NO`; функция оставлена, чтобы `match` был exhaustive без `unreachable!`). Если clippy ругается на неиспользуемое — упрости: убери `format_icon_fallback`, замени ветку `x => …` на `x => format!("action {}", x),` и вставь строку прямо. См. Step 2.

- [ ] **Step 2: RED→GREEN**

Run: `cargo test -p uefi-tui app::tests`
Expected: PASS (полное тело из Step 1 компилируется и проходит 7 тестов: `app_new_starts_normal_tree_focus`, `selected_path_indexes_visible`, `toggle_expand_selected_flips_only_parents`, `sanitize_cursor_clamps_to_visible`, `focus_ring_cycles`, `registry_selectable_and_cursor`). Старые тесты `cursor_down_increments`/`cursor_down_clamps`/`mode_transitions` удалены — покрыты новыми.

- [ ] **Step 3: Полная проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```
Expected: все тесты PASS, warnings нет.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/app.rs
git commit -m "feat(uefi-tui): App Focus/RegistryData/active_image_id + visible-based cursor"
```

---

## Task 3: `commands.rs` — фикс `:open`, `build_tree`, `refresh_registry`; многоуровневый mock

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs`
- Modify: `crates/uefi-tui/tests/mock_server.rs`
- Test: inline `commands::tests` + `tests/mock_server.rs`

**Interfaces:**
- Consumes: `crate::tree::build_tree`, `App.registry`, `App.active_image_id`, RPC `images_list`/`artifacts_list`.
- Produces: `pub async fn refresh_registry(app: &mut App, client: &mut Client) -> Result<(), String>`; `:open` выставляет `active_image_id`; `app.tree = build_tree(&dump.nodes)`.

- [ ] **Step 1: `commands.rs` — заменить `parse_nodes_flat` на `build_tree`, фикс `:open`, добавить `refresh_registry`**

В `crates/uefi-tui/src/commands.rs`:
1. Удали функцию `parse_nodes_flat` (и её тест `parse_nodes_flat_basic`).
2. В ветке `"open" | "o"` замени `app.tree = parse_nodes_flat(&dump.nodes);` на:
```rust
            app.tree = crate::tree::build_tree(&dump.nodes);
            app.cursor = 0;
            app.active_image_id = Some(r.image_id.clone());
            client.state.active_image_id = Some(r.image_id.clone());
            let _ = refresh_registry(app, client).await;
```
(вставь это после `app.status_msg = format!("opened image {}", r.image_id);`, заменив существующие строки `app.tree = parse_nodes_flat(...)` и `app.cursor = 0;`).

3. Добавь `refresh_registry` (вне `execute_command`):
```rust
pub async fn refresh_registry(app: &mut App, client: &mut Client) -> Result<(), String> {
    let sid = client.state.session_id.clone().ok_or("no session")?;
    let imgs = client
        .inner
        .images_list(auth_req(&client.state, ImagesListRequest { session_id: sid.clone() }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let arts = client
        .inner
        .artifacts_list(auth_req(&client.state, ArtifactsListRequest { session_id: sid }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.registry.images = imgs.images;
    app.registry.artifacts = arts.artifacts;
    if app.registry.cursor >= app.registry_selectable().len() {
        app.registry.cursor = 0;
    }
    Ok(())
}
```

4. В `use uefi_proto::*;` уже импортирует `ImagesListRequest`/`ArtifactsListRequest`/`ImagesListResponse`/`ArtifactsListResponse` (проверь что `*` их тянет — да, `engine.proto` defines them).

- [ ] **Step 2: Обновить mock — многоуровневая фикстура + непустые lists**

В `crates/uefi-tui/tests/mock_server.rs` замени тело `image_nodes_list`:
```rust
    async fn image_nodes_list(
        &self,
        _req: Request<ImageNodesListRequest>,
    ) -> Result<Response<ImageNodesResponse>, Status> {
        Ok(Response::new(ImageNodesResponse {
            nodes: vec![
                Node { path: "".into(),       r#type: 62, subtype: 0,    guid: String::new(), offset: 0,    size: 16777216, name: "Image".into() },
                Node { path: "0".into(),      r#type: 65, subtype: 0,    guid: String::new(), offset: 0,    size: 8388608,  name: "ME".into() },
                Node { path: "1".into(),      r#type: 65, subtype: 0,    guid: String::new(), offset: 8388608, size: 4194304, name: "DXE".into() },
                Node { path: "1/0".into(),    r#type: 66, subtype: 0x07, guid: "ABC".into(),   offset: 8388608, size: 4096,   name: "Setup".into() },
                Node { path: "1/0/0".into(),  r#type: 67, subtype: 0x15, guid: String::new(), offset: 8388608, size: 24,     name: String::new() },
            ],
        }))
    }
```
И замени `images_list` / `artifacts_list` (возвращают непустые):
```rust
    async fn images_list(
        &self,
        _req: Request<ImagesListRequest>,
    ) -> Result<Response<ImagesListResponse>, Status> {
        Ok(Response::new(ImagesListResponse {
            images: vec![ImageInfo {
                image_id: "mock-img-1".into(),
                name: "mock.bin".into(),
                path: "/tmp/mock.bin".into(),
                mode: 0,
                size: 16777216,
                created_at: 0,
                last_activity: 0,
            }],
        }))
    }
```
```rust
    async fn artifacts_list(
        &self,
        _req: Request<ArtifactsListRequest>,
    ) -> Result<Response<ArtifactsListResponse>, Status> {
        Ok(Response::new(ArtifactsListResponse {
            artifacts: vec![ArtifactInfo {
                artifact_id: "mock-art-1".into(),
                kind: "section".into(),
                size: 4096,
                created_at: 0,
                source: "extracted".into(),
            }],
        }))
    }
```

- [ ] **Step 3: Тест — `:open` выставляет active_image_id и строит дерево (integration через mock)**

Добавь в `crates/uefi-tui/tests/mock_server.rs` внутри `mod tests` (после `mock_roundtrip`):
```rust
    #[tokio::test]
    async fn open_sets_active_image_and_builds_tree() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state).await.unwrap();
        let mut app = App::new();
        let res = commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client).await;
        assert!(res.is_ok(), "open failed: {:?}", res.err());
        assert_eq!(app.active_image_id, client.state.active_image_id);
        assert!(app.active_image_id.is_some());
        assert!(app.tree.len() >= 5);
        assert!(app.tree[0].has_children);
        assert!(app.registry.images.len() == 1);
        assert!(app.registry.artifacts.len() == 1);
    }
```
В начало `mod tests` добавь `use uefi_tui::commands; use uefi_tui::app::App;` (если ещё нет).

- [ ] **Step 4: RED→GREEN**

```bash
cargo test -p uefi-tui --test mock_server
```
Expected: PASS (`open_sets_active_image_and_builds_tree` + прежние).

- [ ] **Step 5: Полная проверка + clippy**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-tui/src/commands.rs crates/uefi-tui/tests/mock_server.rs
git commit -m "fix(uefi-tui): :open sets active_image_id + build_tree + refresh_registry (multi-level mock)"
```

---

## Task 4: `ui/tree.rs` — stateful List + автоскролл

**Files:**
- Modify: `crates/uefi-tui/src/ui/tree.rs`

**Interfaces:**
- Consumes: `App.visible()` (`Vec<usize>`), `App.cursor`, `App.focus`, `crate::tree::visible_rows`.
- Produces: рендер через `render_stateful_widget` (скроллинг работает).

- [ ] **Step 1: Переписать `ui/tree.rs`**

Замени всё содержимое `crates/uefi-tui/src/ui/tree.rs`:
```rust
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::{App, Focus};
use crate::theme::*;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&idx| {
            let node = &app.tree[idx];
            let indent = "  ".repeat(node.depth);
            let icon = type_icon(node.node_type, node.subtype);
            let expand = if !node.has_children {
                " "
            } else if node.expanded {
                "\u{EAB4}"
            } else {
                "\u{EAB6}"
            };
            let marker = action_marker(node.action);
            let color = action_color(node.action);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{indent}{expand} {icon} "),
                    Style::default().fg(type_color(node.node_type)),
                ),
                Span::styled(format!("{} ", node.name), Style::default().fg(color)),
                Span::raw(format!("{} {marker}", node.guid.as_deref().unwrap_or(""))),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    let sel = if visible.is_empty() { None } else { Some(app.cursor.min(visible.len() - 1)) };
    state.select(sel);
    let title = if app.focus == Focus::Tree { "Tree *" } else { "Tree" };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list, area, &mut state);
}
```

- [ ] **Step 2: Проверить, что `ui/mod.rs` и `main.rs` не требуют правок**

Сигнатура осталась `app: &App` — `render_stateful_widget` принимает `&mut state` (локальная переменная, App не мутируется). Правки `ui/mod.rs`/`main.rs` НЕ нужны: вызовы `ui::render(f, &app)` и `tree::render(f, …, app)` работают как прежде. (Контекст для `&app`: в `main.rs` `terminal.draw(|f| { ui::render(f, &app); … })` — замыкание берёт `&app`.)

- [ ] **Step 3: Проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```
Expected: PASS (логика visible покрыта тестами Task 1+2; рендер compile-only).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/ui/tree.rs
git commit -m "feat(uefi-tui): stateful List rendering (auto-scroll to cursor)"
```

---

## Task 5: `ui/details.rs` — имена типов + focus-бордер

**Files:**
- Modify: `crates/uefi-tui/src/ui/details.rs`
- Test: `crates/uefi-tui/src/app.rs::tests::details_text_*` (чистая фн `details_text` уже в app.rs из Task 2)

**Interfaces:**
- Consumes: `crate::app::details_text`, `App.focus`, `App.cursor`, `App.tree`.

- [ ] **Step 1: Добавить тесты на `details_text` в `app.rs` (RED)**

В `crates/uefi-tui/src/app.rs` `mod tests` добавь:
```rust
    #[test]
    fn details_text_named_file_and_section() {
        let f = TreeNode {
            path: "1/0".into(), depth: 2, node_type: 66, subtype: 0x07,
            guid: Some("ABC".into()), name: "Setup".into(),
            action: ACTION_NO, expanded: false, has_children: true,
        };
        let t = details_text(&f);
        assert!(t.contains("Type:     File (66 / 0x42)"));
        assert!(t.contains("Subtype:  DXE driver (0x07)"));
        assert!(t.contains("GUID:     ABC"));
        assert!(t.contains("Children: true"));
    }

    #[test]
    fn details_text_volume_no_subtype_name() {
        let v = TreeNode {
            path: "1".into(), depth: 1, node_type: 65, subtype: 0,
            guid: None, name: "DXE".into(),
            action: ACTION_NO, expanded: true, has_children: true,
        };
        let t = details_text(&v);
        assert!(t.contains("Type:     Volume (65 / 0x41)"));
        assert!(t.contains("Subtype:  0x00"));
        assert!(t.contains("GUID:     (none)"));
    }
```
Run: `cargo test -p uefi-tui details_text` — ожидаемо PASS (details_text уже реализован в Task 2). Если falls — поправь реализацию в app.rs.

- [ ] **Step 2: Переписать `ui/details.rs`**

Замени всё содержимое `crates/uefi-tui/src/ui/details.rs`:
```rust
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{details_text, App, Focus};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(idx) = app.selected_tree_idx() {
        if let Some(node) = app.tree.get(idx) {
            details_text(node)
        } else {
            "No node selected".into()
        }
    } else {
        "No node selected".into()
    };
    let title = if app.focus == Focus::Details { "Details *" } else { "Details" };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}
```

- [ ] **Step 3: Проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/app.rs crates/uefi-tui/src/ui/details.rs
git commit -m "feat(uefi-tui): details panel human-readable type/subtype names + focus border"
```

---

## Task 6: `commands.rs` — `parse_node_cmd_args` + `:insert`/`:replace`/`:remove`/`:rebuild`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs`
- Test: inline `commands::tests`

**Interfaces:**
- Consumes: `App.selected_path()`, RPC `image_node_insert/replace/remove/rebuild`, `refresh_registry`.
- Produces: `struct NodeCmdArgs`; `fn parse_node_cmd_args(parts: &[&str]) -> NodeCmdArgs`; новые ветки match в `execute_command`.

- [ ] **Step 1: Тесты на парсер (RED)**

Добавь в `commands.rs` `mod tests`:
```rust
    #[test]
    fn parse_node_cmd_target_and_flags() {
        let a = parse_node_cmd_args(&["insert", "1/0", "--file", "/x.bin", "--mode", "into"]);
        assert_eq!(a.target.as_deref(), Some("1/0"));
        assert_eq!(a.file.as_deref(), Some("/x.bin"));
        assert_eq!(a.mode.as_deref(), Some("into"));
        assert!(a.artifact_id.is_none());
        assert!(!a.body_only);
    }

    #[test]
    fn parse_node_cmd_artifact_and_body_only() {
        let a = parse_node_cmd_args(&["replace", "--artifact-id", "art1", "--body-only"]);
        assert_eq!(a.artifact_id.as_deref(), Some("art1"));
        assert!(a.body_only);
        assert!(a.target.is_none());
        assert!(a.file.is_none());
    }

    #[test]
    fn parse_node_cmd_target_is_first_non_flag() {
        let a = parse_node_cmd_args(&["remove", "0/3"]);
        assert_eq!(a.target.as_deref(), Some("0/3"));
    }
```

- [ ] **Step 2: Реализовать парсер + команды**

В `commands.rs` добавь (выше `execute_command`):
```rust
struct NodeCmdArgs {
    target: Option<String>,
    file: Option<String>,
    artifact_id: Option<String>,
    mode: Option<String>,
    body_only: bool,
}

fn parse_node_cmd_args(parts: &[&str]) -> NodeCmdArgs {
    let mut a = NodeCmdArgs {
        target: None,
        file: None,
        artifact_id: None,
        mode: None,
        body_only: false,
    };
    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "--file" => {
                a.file = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--artifact-id" => {
                a.artifact_id = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--mode" => {
                a.mode = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--body-only" => {
                a.body_only = true;
                i += 1;
            }
            other if !other.starts_with("--") && a.target.is_none() => {
                a.target = Some(other.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    a
}

fn mode_to_i32(s: &str) -> Result<i32, String> {
    match s {
        "into" | "" => Ok(0),
        "before" => Ok(1),
        "after" => Ok(2),
        _ => Err(format!("unknown mode: {s} (into|before|after)")),
    }
}
```

В `execute_command` добавь ветки (перед `"quit" | "q"`):
```rust
        "insert" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a.target.or_else(|| app.selected_path()).ok_or("no target (select a node or pass TARGET)")?;
            let (ffs_path, artifact_id) = match (a.file, a.artifact_id) {
                (Some(p), None) => (p, String::new()),
                (None, Some(id)) => (String::new(), id),
                _ => return Err("usage: :insert [TARGET] (--file PATH | --artifact-id ID) [--mode into|before|after]".into()),
            };
            let mode = mode_to_i32(a.mode.as_deref().unwrap_or(""))?;
            let req = ImageNodeInsertRequest { image_id: iid, target, ffs_path, artifact_id, mode };
            let r = client.inner.image_node_insert(auth_req(&client.state, req)).await.map_err(|e| e.message().to_string())?.into_inner();
            app.status_msg = format!("inserted {}", r.item_id);
            let _ = refresh_registry(app, client).await;
            Ok(r.item_id)
        }
        "replace" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a.target.or_else(|| app.selected_path()).ok_or("no target")?;
            let (ffs_path, artifact_id) = match (a.file, a.artifact_id) {
                (Some(p), None) => (p, String::new()),
                (None, Some(id)) => (String::new(), id),
                _ => return Err("usage: :replace [TARGET] (--file PATH | --artifact-id ID) [--body-only]".into()),
            };
            let req = ImageNodeReplaceRequest { image_id: iid, target, ffs_path, artifact_id, body_only: a.body_only };
            let r = client.inner.image_node_replace(auth_req(&client.state, req)).await.map_err(|e| e.message().to_string())?.into_inner();
            app.status_msg = format!("replaced {}", r.item_id);
            let _ = refresh_registry(app, client).await;
            Ok(r.item_id)
        }
        "remove" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a.target.or_else(|| app.selected_path()).ok_or("no target")?;
            let req = ImageNodeRemoveRequest { image_id: iid, target: target.clone() };
            client.inner.image_node_remove(auth_req(&client.state, req)).await.map_err(|e| e.message().to_string())?;
            app.status_msg = format!("removed {target}");
            Ok(target)
        }
        "rebuild" => {
            let a = parse_node_cmd_args(&parts);
            let iid = client.state.active_image_id.clone().ok_or("no active image")?;
            let target = a.target.or_else(|| app.selected_path()).ok_or("no target")?;
            let req = ImageNodeRebuildRequest { image_id: iid, target: target.clone() };
            client.inner.image_node_rebuild(auth_req(&client.state, req)).await.map_err(|e| e.message().to_string())?;
            app.status_msg = format!("rebuilt {target}");
            Ok(target)
        }
```

> Проверь имена полей RPC-запросов в `crates/uefi-proto/proto/engine.proto` (`ImageNodeInsertRequest`, `ImageNodeReplaceRequest`, `ImageNodeRemoveRequest`, `ImageNodeRebuildRequest`, `ImageNodeResponse{item_id}`). Если proto требует `item_id`/`target`/`ffs_path`/`artifact_id`/`mode`/`body_only` — совпадает. Если имена полей в proto отличаются (например `mode: int32`) — поправь под proto.

- [ ] **Step 3: RED→GREEN**

```bash
cargo test -p uefi-tui commands::tests
```
Expected: PASS (3 теста парсера).

- [ ] **Step 4: Полная проверка + clippy**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```
> Если clippy ругается на слишком длинный `match` arm или `needless return` — поправь минимально.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-tui/src/commands.rs
git commit -m "feat(uefi-tui): :insert/:replace/:remove/:rebuild commands + flag parser"
```

---

## Task 7: `commands.rs` — `:image switch`/`:image close`/`:refresh`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs`
- Test: inline `commands::tests` + mock integration

**Interfaces:**
- Consumes: `App.active_image_id`, `App.tree`, `build_tree`, `refresh_registry`, RPC `image_nodes_list`/`image_close`.

- [ ] **Step 1: Команды `:image …` и `:refresh`**

В `execute_command` добавь (перед `"quit" | "q"`):
```rust
        "image" => {
            let sub = parts.get(1).ok_or("usage: :image switch ID | close [ID]")?;
            match *sub {
                "switch" => {
                    let id = parts.get(2).ok_or("usage: :image switch ID")?.to_string();
                    let dump = client.inner.image_nodes_list(auth_req(&client.state, ImageNodesListRequest { image_id: id.clone(), filter: String::new() })).await.map_err(|e| e.message().to_string())?.into_inner();
                    app.tree = crate::tree::build_tree(&dump.nodes);
                    app.cursor = 0;
                    app.active_image_id = Some(id.clone());
                    client.state.active_image_id = Some(id.clone());
                    app.status_msg = format!("switched to {id}");
                    let _ = refresh_registry(app, client).await;
                    Ok(id)
                }
                "close" => {
                    let id = parts.get(2).map(|s| s.to_string()).or_else(|| client.state.active_image_id.clone()).ok_or("no active image")?;
                    let req = ImageCloseRequest { image_id: id.clone() };
                    client.inner.image_close(auth_req(&client.state, req)).await.map_err(|e| e.message().to_string())?.into_inner();
                    if app.active_image_id.as_deref() == Some(id.as_str()) {
                        app.active_image_id = None;
                        client.state.active_image_id = None;
                        app.tree.clear();
                        app.cursor = 0;
                        app.image_loaded = false;
                    }
                    app.status_msg = format!("closed {id}");
                    let _ = refresh_registry(app, client).await;
                    Ok(id)
                }
                other => Err(format!("unknown image subcommand: {other}")),
            }
        }
        "refresh" => {
            refresh_registry(app, client).await?;
            app.status_msg = format!("registry: {} images, {} artifacts", app.registry.images.len(), app.registry.artifacts.len());
            Ok("refreshed".into())
        }
```

- [ ] **Step 2: Integration-тест — `:refresh` через mock**

В `tests/mock_server.rs` `mod tests` добавь:
```rust
    #[tokio::test]
    async fn refresh_populates_registry() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state).await.unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "refresh", &mut client).await.unwrap();
        assert_eq!(app.registry.images.len(), 1);
        assert_eq!(app.registry.artifacts.len(), 1);
    }
```

- [ ] **Step 3: RED→GREEN + clippy**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/commands.rs crates/uefi-tui/tests/mock_server.rs
git commit -m "feat(uefi-tui): :image switch/close + :refresh commands"
```

---

## Task 8: `ui/registry.rs` — новая панель (рендер + selectable)

**Files:**
- Create: `crates/uefi-tui/src/ui/registry.rs`
- Modify: `crates/uefi-tui/src/ui/mod.rs` (`pub mod registry;` + layout B)
- Modify: `crates/uefi-tui/src/main.rs` (ничего — пока только рендер; интерактив в Task 11)
- Test: compile + логика `registry_selectable` покрыта в app.rs (Task 2).

**Interfaces:**
- Consumes: `App.registry`, `App.focus`, `App.active_image_id`, `App.registry_selectable()`, `App.registry.cursor`.
- Produces: `pub fn render(f: &mut Frame, area: Rect, app: &App)`.

- [ ] **Step 1: Создать `ui/registry.rs` + объявить модуль**

Создай `crates/uefi-tui/src/ui/registry.rs`:
```rust
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
        let marker = if active == Some(im.image_id.as_str()) { "► " } else { "  " };
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
```

> Ключевой момент: `selectable_items_idx.push(items.len())` записывает items-индекс selectable-строки **до** её push (две header-строки `Images`/`Artifacts` в `selectable_items_idx` не попадают). Иначе ListState подсветил бы header. `RegistryRow`/`registry_selectable()` в рендере не используются — только `app.registry.cursor`.

В `crates/uefi-tui/src/ui/mod.rs` добавь `pub mod registry;` после `pub mod details;`.

- [ ] **Step 2: RED→GREEN (тесты fmt_size/short)**

```bash
cargo test -p uefi-tui registry::tests
```
Expected: PASS.

- [ ] **Step 3: Полная проверка + clippy**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/ui/registry.rs crates/uefi-tui/src/ui/mod.rs
git commit -m "feat(uefi-tui): registry panel (images + artifacts, selectable highlight)"
```

---

## Task 9: `ui/mod.rs` — layout B (сплит правой колонки)

**Files:**
- Modify: `crates/uefi-tui/src/ui/mod.rs`

**Interfaces:**
- Consumes: `registry::render`, `details::render`, `tree::render`.
- Produces: layout с правой колонкой, разбитой на details (сверху) + registry (снизу).

- [ ] **Step 1: Переписать `ui/mod.rs` layout**

Замени тело `pub fn render` в `crates/uefi-tui/src/ui/mod.rs`:
```rust
pub fn render(f: &mut Frame, app: &App) {
    let full = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(full);
    status::render(f, vertical[0], app);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(vertical[1]);
    tree::render(f, horizontal[0], app);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(horizontal[1]);
    details::render(f, right[0], app);
    registry::render(f, right[1], app);
    cmdline::render(f, vertical[2], app);
    render_hint(f, vertical[3], app);
}
```
(`render_hint` и `use …Constraint/Direction/Layout/Rect` уже есть — не трогай.)

- [ ] **Step 2: Проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-tui/src/ui/mod.rs
git commit -m "feat(uefi-tui): layout B (right column split: details + registry)"
```

---

## Task 10: `ui/cmdline.rs` + Mode::Insert prompt

**Files:**
- Modify: `crates/uefi-tui/src/ui/cmdline.rs`

**Interfaces:**
- Consumes: `App.mode`, `App.insert_cmd`, `App.cmdline`.

- [ ] **Step 1: Переписать `ui/cmdline.rs`**

Замени всё содержимое `crates/uefi-tui/src/ui/cmdline.rs`:
```rust
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let content = match app.mode {
        Mode::Command => format!(":{}", app.cmdline),
        Mode::Insert => {
            if app.insert_cmd.is_empty() {
                format!("> {}", app.cmdline)
            } else {
                format!("{}> {}", app.insert_cmd, app.cmdline)
            }
        }
        Mode::Normal => String::from("Press : for commands, i/r/d for insert/replace/remove"),
    };
    let title = match app.mode {
        Mode::Insert => "Insert",
        _ => "Command",
    };
    let p = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}
```

- [ ] **Step 2: Проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-tui/src/ui/cmdline.rs
git commit -m "feat(uefi-tui): Mode::Insert cmdline prompt (insert>/replace>/remove>)"
```

---

## Task 11: `input.rs` + `main.rs` — Ctrl-focus-ring, h/l expand, j/k/Enter routing, i/r/d prefill

**Files:**
- Modify: `crates/uefi-tui/src/input.rs`
- Modify: `crates/uefi-tui/src/main.rs`
- Test: `input.rs` Ctrl-capture test.

**Interfaces:**
- Consumes: `App.focus`, `App.toggle_expand_selected`, `App.focus_next/prev`, `App.cursor_*`, `App.registry_cursor_*`, `App.current_registry_row`, `App.enter_insert_mode`, `App.selected_path`, RPC через commands (image switch, insert prefill).
- Produces: `AppEvent::Ctrl(char)`; обновлённый `handle_normal` с focus-routing.

- [ ] **Step 1: `input.rs` — добавить `AppEvent::Ctrl(char)`**

Замени `crates/uefi-tui/src/input.rs`:
```rust
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Up,
    Down,
    Quit,
    Tick,
}

pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if !event::poll(timeout).unwrap_or_default() {
        return Some(AppEvent::Tick);
    }
    if let Event::Key(k) = event::read().ok()? {
        if k.kind != KeyEventKind::Press {
            return None;
        }
        return Some(match k.code {
            KeyCode::Char(c) if k.modifiers.contains(KeyModifiers::CONTROL) => AppEvent::Ctrl(c),
            KeyCode::Char(c) => AppEvent::Key(c),
            KeyCode::Enter => AppEvent::Enter,
            KeyCode::Esc => AppEvent::Esc,
            KeyCode::Backspace => AppEvent::Backspace,
            KeyCode::Up => AppEvent::Up,
            KeyCode::Down => AppEvent::Down,
            _ => return None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tick_on_timeout() {
        let e = poll_event(Duration::from_millis(0));
        assert_eq!(e, Some(AppEvent::Tick));
    }
}
```

> `Ctrl('q')` не нужен (есть `Quit` через `q` без модификатора). Ctrl-h/j/k/l попадают в `AppEvent::Ctrl(c)`.

- [ ] **Step 2: `main.rs` — `handle_normal` с focus-routing + `handle_command` для Insert**

Замени `handle_normal`, `handle_command`, `handle_insert` и `match app.mode` блок. Сначала — блок диспетчера в `loop`:
```rust
        match app.mode {
            Mode::Normal => handle_normal(app, &ev, &mut client).await,
            Mode::Command | Mode::Insert => handle_command(app, &ev, &mut client).await,
        }
```
Расширь импорт main.rs: `use uefi_tui::app::{App, Focus, Mode, RegistryRow};`.

Новые версии функций (замени `handle_normal`, удали `handle_insert`):
```rust
async fn handle_normal(app: &mut App, ev: &AppEvent, client: &mut Option<commands::Client>) {
    match ev {
        AppEvent::Ctrl('h') | AppEvent::Ctrl('k') => app.focus_prev(),
        AppEvent::Ctrl('l') | AppEvent::Ctrl('j') => app.focus_next(),
        AppEvent::Key('?') => app.show_help = !app.show_help,
        AppEvent::Key('q') | AppEvent::Quit => app.quit = true,
        AppEvent::Key(':') => app.enter_command_mode(),
        AppEvent::Key('i') | AppEvent::Key('r') | AppEvent::Key('d') if app.focus == Focus::Tree => {
            let (cmd_str, prefill) = match ev {
                AppEvent::Key('i') => ("insert", format!("insert {} --file ", app.selected_path().unwrap_or_default())),
                AppEvent::Key('r') => ("replace", format!("replace {} --file ", app.selected_path().unwrap_or_default())),
                _ => ("remove", "remove ".to_string()),
            };
            app.enter_insert_mode(cmd_str, prefill);
        }
        AppEvent::Key('j') | AppEvent::Down => match app.focus {
            Focus::Registry => app.registry_cursor_down(),
            Focus::Tree => app.cursor_down(),
            Focus::Details => {}
        },
        AppEvent::Key('k') | AppEvent::Up => match app.focus {
            Focus::Registry => app.registry_cursor_up(),
            Focus::Tree => app.cursor_up(),
            Focus::Details => {}
        },
        AppEvent::Key('h') => {
            if app.focus == Focus::Tree {
                app.set_expand_selected(false);
            }
        }
        AppEvent::Key('l') => {
            if app.focus == Focus::Tree {
                app.set_expand_selected(true);
            }
        }
        AppEvent::Enter => {
            if app.focus == Focus::Registry {
                handle_registry_enter(app, client).await;
            }
        }
        _ => {}
    }
}

async fn handle_registry_enter(app: &mut App, client: &mut Option<commands::Client>) {
    let Some(c) = client else { return };
    match app.current_registry_row() {
        Some(RegistryRow::Image(i)) => {
            if let Some(im) = app.registry.images.get(i).cloned() {
                let cmd = format!("image switch {}", im.image_id);
                if let Err(e) = commands::execute_command(app, &cmd, c).await {
                    app.status_msg = format!("error: {e}");
                }
                app.focus = Focus::Tree;
            }
        }
        Some(RegistryRow::Artifact(i)) => {
            if let Some(ar) = app.registry.artifacts.get(i).cloned() {
                let path = app.selected_path().unwrap_or_default();
                app.enter_insert_mode("insert", format!("insert {path} --artifact-id {} ", ar.artifact_id));
            }
        }
        None => {}
    }
}
```
`handle_command` оставь без изменений (он выполняет `app.cmdline` через `commands::execute_command` и `app.exit_to_normal()` на Enter/Esc — работает и для Command, и для Insert). На Enter после успеха/ошибки `exit_to_normal` сбрасывает `insert_cmd=""`.

- [ ] **Step 3: Проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
cargo build -p uefi-tui
```

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/input.rs crates/uefi-tui/src/main.rs
git commit -m "feat(uefi-tui): Ctrl-hjkl focus ring, h/l expand, i/r/d prefill, registry Enter"
```

---

## Task 12: `ui/help.rs` — полноэкранный scrollable overlay

**Files:**
- Rewrite: `crates/uefi-tui/src/ui/help.rs`
- Modify: `crates/uefi-tui/src/main.rs` (`?` toggles; j/k scroll внутри help — опционально, можно оставить без скролла если влезает).
- Test: compile-only (статичный текст).

**Interfaces:**
- Consumes: `App.show_help`.

- [ ] **Step 1: Переписать `ui/help.rs`**

Замени всё содержимое `crates/uefi-tui/src/ui/help.rs`:
```rust
use ratatui::Frame;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;

const HELP: &str = "\
UEFI TUI — Help
===============

Modes:  Normal (default) · Command (:) · Insert (i/r/d)

NORMAL (Tree focus)
  j / k          move cursor (по видимым строкам)
  h              collapse selected node
  l              expand selected node
  i              insert  -> prefill :insert <path> --file
  r              replace -> prefill :replace <path> --file
  d              remove  -> prefill :remove <path>
  Ctrl-h / Ctrl-k  focus prev panel (cycle: Tree->Registry->Details->Tree)
  Ctrl-l / Ctrl-j  focus next panel
  :              Command mode
  ?              toggle this help
  q              quit

REGISTRY focus
  j / k          move selection
  Enter on image    -> :image switch <id>  (return to Tree)
  Enter on artifact -> prefill :insert <path> --artifact-id <id>

COMMAND / INSERT
  Enter           execute cmdline   ·  Esc  cancel   ·  Backspace  delete

EX-COMMANDS
  :open PATH [--mode read|write]
  :save OUTPUT
  :extract TARGET [--body-only]
  :export ARTIFACT_ID [PATH]
  :import FILE
  :insert [TARGET] (--file PATH | --artifact-id ID) [--mode into|before|after]
  :replace [TARGET] (--file PATH | --artifact-id ID) [--body-only]
  :remove [TARGET]
  :rebuild [TARGET]
  :image switch ID | :image close [ID]
  :refresh
  :artifacts
  :help | :h
  :quit | :q

TARGET default = node under cursor. Exactly one of --file / --artifact-id.
";

pub fn render(f: &mut Frame, app: &App) {
    if !app.show_help {
        return;
    }
    let area = f.area();
    f.render_widget(Clear, area);
    let p = Paragraph::new(HELP).block(Block::default().borders(Borders::ALL).title("Help (? to close)"));
    f.render_widget(p, area);
}
```

> Скроллинг внутри help не реализуем (YAGNI; текст компактный, влезает в 80x24). Если на узких терминалах обрезается — добавим ListState позже.

- [ ] **Step 2: Проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
```

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-tui/src/ui/help.rs
git commit -m "feat(uefi-tui): full-screen help overlay with current commands and keybindings"
```

---

## Task 13: Финальный smoke + `render_hint` update + full-suite

**Files:**
- Modify: `crates/uefi-tui/src/ui/mod.rs` (`render_hint` текст)
- Modify: `crates/uefi-tui/tests/mock_server.rs` (smoke-тест)

**Interfaces:**
- Consumes: всё из Tasks 1-12.

- [ ] **Step 1: Обновить `render_hint` в `ui/mod.rs`**

Замени тело `render_hint`:
```rust
fn render_hint(f: &mut Frame, area: Rect, app: &App) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    let hint = match app.mode {
        crate::app::Mode::Normal => match app.focus {
            crate::app::Focus::Tree => "NORMAL[Tree]: j/k move · h/l collapse/expand · i/r/d · Ctrl-hjkl focus · :cmd · ?help · q",
            crate::app::Focus::Details => "NORMAL[Details]: Ctrl-hjkl focus · :cmd · ?help · q",
            crate::app::Focus::Registry => "NORMAL[Registry]: j/k select · Enter pick · Ctrl-hjkl focus · ?help · q",
        },
        crate::app::Mode::Command => "COMMAND: Enter execute · Esc cancel · Backspace",
        crate::app::Mode::Insert => "INSERT: Enter execute · Esc cancel · Backspace",
    };
    let p = Paragraph::new(hint).block(Block::default().borders(Borders::NONE));
    f.render_widget(p, area);
}
```

- [ ] **Step 2: Smoke-тест — open→expand→switch→insert-prefill flow (mock)**

В `tests/mock_server.rs` `mod tests` добавь:
```rust
    #[tokio::test]
    async fn smoke_open_collapse_switch_registry_pick() {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("mock.sock");
        let _handle = start_mock(&sock).await;
        let state = uefi_common::state::State {
            session_id: Some("s1".into()),
            token: Some("t1".into()),
            ..Default::default()
        };
        let mut client = commands::connect(Some(sock.to_str().unwrap()), state).await.unwrap();
        let mut app = App::new();
        commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client).await.unwrap();
        assert!(app.tree[1].expanded);
        app.toggle_expand_selected();
        assert!(!app.tree[app.selected_tree_idx().unwrap()].expanded);
        commands::execute_command(&mut app, "image switch mock-img-1", &mut client).await.unwrap();
        assert_eq!(app.active_image_id.as_deref(), Some("mock-img-1"));
        app.focus_next();
        app.focus_next();
        assert_eq!(app.focus, Focus::Registry);
        assert!(matches!(app.current_registry_row(), Some(RegistryRow::Artifact(0)) | Some(RegistryRow::Image(_))));
    }
```
Добавь `use uefi_tui::app::{App, Focus, RegistryRow};` в начало `mod tests` (если ещё не).

- [ ] **Step 3: Полная проверка**

```bash
cargo test -p uefi-tui
cargo clippy -p uefi-tui -- -D warnings
cargo fmt --all -- --check
```
Expected: все тесты PASS (включая smoke), warnings нет, формат чёткий.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/ui/mod.rs crates/uefi-tui/tests/mock_server.rs
git commit -m "test(uefi-tui): smoke open/collapse/switch/registry + context-aware hint"
```

---

## Self-Review (выполнено автором плана)

**1. Spec coverage:**
- Дерево/depth/has_children/visible → Task 1.
- Скроллинг stateful-List → Task 4.
- Default collapse depth<=1 → Task 1 (`build_tree`, тест `build_tree_default_expanded_only_top_two_levels`).
- Expand/collapse h/l + cursor по visible → Tasks 1, 2, 11.
- `active_image_id` фикс → Task 3 (тест `open_sets_active_image_and_builds_tree`).
- Команды insert/replace/remove/rebuild → Task 6.
- `:image switch/close`/`:refresh` → Task 7.
- Registry-панель (рендер + selectable) → Task 8.
- Layout B → Task 9.
- Mode::Insert prompt → Task 10.
- Focus-ring Ctrl-hjkl + registry Enter + i/r/d → Task 11.
- Details имена → Task 5.
- Help rewrite → Task 12.
- Hint context-aware → Task 13.
- Сквозной smoke → Task 13.
- Gap: `:artifacts` уже существует (не трогаем) — ок.

**2. Placeholder scan:** TBD/TODO в коде отсутствуют. `details_text` сразу чистый (без `format_icon_fallback`); `selected_path()` возвращает owned `Option<String>` (нет borrow-конфликтов). State-литералы в тестах используют `..Default::default()` (поле `sock_path` учтено). `InsertMode.mode` — prost-поле `i32` (доказано CLI `client.rs:194`), `mode_to_i32` корректен.

**3. Type consistency:** `selected_path() -> Option<String>` (владеющий; используется и в commands, и в main prefill — ok, нет borrow-conflictов т.к. клонируется через `unwrap_or_default()`). `selected_tree_idx() -> Option<usize>` едино везде. `RegistryRow::{Image(usize),Artifact(usize)}` — индексы в `app.registry.images`/`artifacts`. `Focus` enum идентичен в app.rs/ui/main. `AppEvent::Ctrl(char)` добавлен в input.rs и обрабатывается в main.rs. `insert_cmd: &'static str` — статичные литералы `"insert"/"replace"/"remove"`.
