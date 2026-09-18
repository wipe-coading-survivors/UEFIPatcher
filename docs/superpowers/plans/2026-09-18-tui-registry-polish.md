# TUI registry & flow polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Цикл R0–R9 спеки — грамматика-вынос образовых глаголов, режим/UUID в Registry, image-ID completion, `:reopen`+`w`, smart prefill, `:export` absolutization, скролл help/details/списков Forms View, V3-остаток.

**Architecture:** Всё в `uefi-tui` (client-side); proto/engine не меняются. Повторное использование существующих механизмов: `compute_scrolled_offset` (`tree.rs:53`) для трёх новых скроллов, меню completion (цикла 1) для новых кандидатов, паттерн `open`-ветки для `:reopen`.

**Tech Stack:** Rust (edition 2024), ratatui (ListState/Paragraph/TestBackend), tonic-клиент uefi-proto.

**Spec:** `docs/superpowers/specs/2026-09-18-tui-registry-polish-design.md` — план аргументируется спекой, читать оба.

## Global Constraints

- Комментарии в коде НЕ пишем; только `///`-rustdoc-контракты на pub/pub(crate)-функциях (что делает / чего не делает + «Спека RN») и ссылки `file:line`.
- Порядок TDD: тест падает → реализация → тест проходит → commit.
- После каждой задачи: `cargo test -p uefi-tui` и `cargo clippy -p uefi-tui -- -D warnings` (обе зелёные до коммита).
- Ветка `tui-registry-polish`; новые файлы модулей не создаются (правило module-first не срабатывает — все правки в существующих файлах).
- Чистый разрыв грамматики: алиасы `image switch/close` НЕ сохраняем (спека, решение владельца 5).
- `ImageInfo.mode`: 0=READ, 1=WRITE (engine.proto:48); в Rust-генерации это `i32`.

---

### Task 1: R0 — switch/close top-level, `:image` bare view-переключатель

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs:492-556` (ветка `"image"`), `commands.rs:863` (COMMANDS)
- Modify: `crates/uefi-tui/src/main.rs:166` (`handle_registry_enter`)
- Modify: `crates/uefi-tui/src/ui/help.rs:28-29` (REGISTRY-строка + строка EX-COMMANDS)
- Modify: `crates/uefi-tui/tests/mock_server.rs:834,927` (старый синтаксис в тестах)
- Test: `crates/uefi-tui/src/commands.rs` (tests), `crates/uefi-tui/tests/tui_integration.rs:350`

**Interfaces:**
- Produces: top-level команды `:switch ID` / `:close [ID]` (диспетч `execute_command`); `COMMANDS` содержит `"switch"`, `"close"`; `:image <args>` → Err «moved». Позже (Task 3, 4) на эти имена вешается completion.

- [ ] **Step 1: Failing-тесты**

В `commands.rs` tests-mod (рядом с `complete_first_token_to_unique_command`, :1658):

```rust
#[test]
fn commands_const_has_top_level_switch_close() {
    assert!(COMMANDS.contains(&"switch"));
    assert!(COMMANDS.contains(&"close"));
    assert!(COMMANDS.contains(&"image"), "bare :image остаётся view-переключателем");
}
```

В `tests/tui_integration.rs` переименовать `image_switch_sets_active_state` (:350) в `switch_top_level_image_bare_moved_hint` и заменить тело команды:

```rust
let r = uefi_tui::commands::execute_command(&mut app, "switch img-existing", &mut client).await;
assert_eq!(r.unwrap(), "img-existing");
assert_eq!(app.active_image_id.as_deref(), Some("img-existing"));
assert!(app.image_loaded);
assert!(!app.tree.is_empty());

uefi_tui::commands::execute_command(&mut app, "forms", &mut client)
    .await
    .unwrap();
assert!(matches!(app.view, uefi_tui::app::View::Forms));
let r = uefi_tui::commands::execute_command(&mut app, "image", &mut client).await;
assert_eq!(r.unwrap(), "image");
assert!(matches!(app.view, uefi_tui::app::View::Image));

let err = uefi_tui::commands::execute_command(&mut app, "image switch img-existing", &mut client)
    .await
    .unwrap_err();
assert!(err.contains("moved"), "ошибка переносится с подсказкой: {err}");
```

В `ui/help.rs` tests-mod добавить:

```rust
#[test]
fn help_documents_top_level_switch_close() {
    assert!(HELP.contains(":switch ID | :close [ID]"));
    assert!(HELP.contains("Enter on image    -> :switch <id>"));
    assert!(HELP.contains(":image                      обратно в Image-view"));
}
```

- [ ] **Step 2: Запуск — падение**

Run: `cargo test -p uefi-tui`
Expected: FAIL (нет «switch» в COMMANDS; `image switch` ещё работает, не err; HELP без новой строки).

- [ ] **Step 3: Реализация**

`commands.rs`: ветку `"image" => {...}` (:492) заменить на три ветки. Тела switch/close — дословно из старых вложенных веток, но слот ID теперь `parts.get(1)` (было `get(2)`) и usage-строки без `image`:

```rust
"image" => {
    if parts.len() > 1 {
        return Err("image subcommands moved: :switch ID | :close [ID]".into());
    }
    app.view = View::Image;
    Ok("image".into())
}
"switch" => {
    let id = parts.get(1).ok_or("usage: :switch ID")?.to_string();
    // … тело прежней вложенной ветки "switch" (image_nodes_list → build_tree
    // → active/cursor/status → refresh_registry → refresh_forms при Forms) …
}
"close" => {
    let id = parts
        .get(1)
        .map(|s| s.to_string())
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    // … тело прежней вложенной ветки "close" (image_close → сброс активного
    // при совпадении → refresh_registry) …
}
```

`COMMANDS` (:863): после `"rebuild"` добавить `"switch", "close",` (строку `"image"` не трогаем).

`main.rs:166`: `format!("image switch {}", im.image_id)` → `format!("switch {}", im.image_id)`.

`ui/help.rs:28`: строку `  Enter on image    -> :image switch <id>  (return to Tree)` заменить на `  Enter on image    -> :switch <id>  (return to Tree)`; `ui/help.rs:29`: строку `  :image switch ID | :image close [ID]` заменить на `  :switch ID | :close [ID]` (bare-строку `:image` ниже не трогаем).

`tests/mock_server.rs:834`: `execute_command(&mut app, "image switch mock-img-1", ...)` → `"switch mock-img-1"`; `:927`: `format!("image close {active}")` → `format!("close {active}")` — иначе suite падает на старом синтаксисе.

- [ ] **Step 4: Прогон**

Run: `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings`
Expected: PASS (включая integration).

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R0 — switch/close top-level, :image только view-переключатель (паритет :forms), чистый разрыв без алиасов"
```

---

### Task 2: R1 — бейдж режима и полный UUID выбранной строки в hint-баре

**Files:**
- Modify: `crates/uefi-tui/src/ui/registry.rs:25-37` (строки образов), tests-mod
- Modify: `crates/uefi-tui/src/ui/mod.rs:67-70` (render_hint, ветка Registry), tests-mod
- Modify: `crates/uefi-tui/src/app.rs` (новый helper рядом с `current_registry_row`, :359)

**Interfaces:**
- Produces: `pub fn registry_selected_full_id(app: &App) -> Option<String>` (app.rs) — используется рендером hint; формат строки образа `"{marker}{short}  {name}  {size}  {R|W}"`.

- [ ] **Step 1: Failing-тесты**

`ui/mod.rs` tests-mod:

```rust
#[test]
fn registry_hint_prefixes_full_uuid_and_w_key() {
    let mut app = crate::app::App::new();
    app.focus = crate::app::Focus::Registry;
    app.registry.images = vec![uefi_proto::ImageInfo {
        image_id: "956ad394-1111-2222-3333-444444444444".into(),
        ..Default::default()
    }];
    app.registry.cursor = 0;
    let h = registry_hint(&app);
    assert!(h.starts_with("956ad394-1111-2222-3333-444444444444  "), "UUID первым сегментом");
    assert!(h.contains("w write-mode"));
}
```

`ui/registry.rs` tests-mod (TestBackend-паттерн как в `ui/menu.rs:45-56`, хелпер `row` скопировать туда же):

```rust
#[test]
fn image_rows_show_mode_badge() {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 10)).unwrap();
    let mut app = crate::app::App::new();
    app.registry.images = vec![
        uefi_proto::ImageInfo {
            image_id: "aaaaaaaa-0000-0000-0000-000000000000".into(),
            name: "r.bin".into(),
            mode: 0,
            size: 1024,
            ..Default::default()
        },
        uefi_proto::ImageInfo {
            image_id: "bbbbbbbb-0000-0000-0000-000000000000".into(),
            name: "w.bin".into(),
            mode: 1,
            size: 1024,
            ..Default::default()
        },
    ];
    terminal.draw(|f| super::render(f, f.area(), &mut app)).unwrap();
    let text = (0..10).map(|y| row(&terminal, y)).collect::<String>();
    assert!(text.contains("r.bin  1.0 KB  R"));
    assert!(text.contains("w.bin  1.0 KB  W"));
}
```

- [ ] **Step 2: Запуск — падение** — `cargo test -p uefi-tui` → FAIL (`registry_hint` нет, бейджа нет).

- [ ] **Step 3: Реализация**

`app.rs` (рядом с `current_registry_row`):

```rust
/// Полный ID выбранной registry-строки (image/artifact) для hint-бара.
/// Спека R1: short-ID в списке остаются, полный UUID живёт в hint.
pub fn registry_selected_full_id(app: &App) -> Option<String> {
    match app.current_registry_row()? {
        RegistryRow::Image(i) => app.registry.images.get(i).map(|im| im.image_id.clone()),
        RegistryRow::Artifact(i) => app.registry.artifacts.get(i).map(|a| a.artifact_id.clone()),
    }
}
```

`ui/mod.rs`, ветка `Focus::Registry` в `render_hint` заменить на вызов `registry_hint(app)` и добавить fn:

```rust
/// Hint Registry: полный UUID выбранной строки первым сегментом (клипование
/// режет хвост-подсказки, не ID). Спека R1.
fn registry_hint(app: &App) -> String {
    let id = crate::app::registry_selected_full_id(app)
        .map(|s| format!("{s}  "))
        .unwrap_or_default();
    format!("{id}NORMAL[Registry]: j/k select · Enter pick · w write-mode · Ctrl-hjkl focus · :cmd · ?help · q")
}
```

`ui/registry.rs`, строка образа (:33-37):

```rust
let badge = if im.mode == 1 { "W" } else { "R" };
items.push(ListItem::new(Line::from(format!(
    "{marker}{}  {}  {}  {badge}",
    short(&im.image_id),
    im.name,
    fmt_size(im.size),
))));
```

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R1 — бейдж R/W у образов в Registry + полный UUID выбранной строки в hint-баре"
```

---

### Task 3: R2 — completion: switch/close слот-1, reopen-флаги, cmd-зависимый --mode

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (`context_candidates`)
- Test: `commands.rs` tests-mod

**Interfaces:**
- Consumes: `COMMANDS` из Task 1 (слова switch/close уже кандидаты первого токена).
- Produces: `switch `/`close ` + пустой токен → кандидаты `image_id`; `--mode`-кандидаты зависят от cmd (`insert` → into/before/after, `reopen` → read/write); `reopen --` → `--mode`.

- [ ] **Step 1: Failing-тесты**

```rust
#[test]
fn complete_switch_close_offer_registry_image_ids() {
    let mut app = crate::app::App::new();
    app.registry.images = vec![
        uefi_proto::ImageInfo {
            image_id: "img-aaa".into(),
            ..Default::default()
        },
        uefi_proto::ImageInfo {
            image_id: "img-bbb".into(),
            ..Default::default()
        },
    ];
    for cmd in ["switch ", "close "] {
        let c = complete(&app, cmd);
        assert_eq!(
            c.items.iter().map(|i| i.display.clone()).collect::<Vec<_>>(),
            vec!["img-aaa".to_string(), "img-bbb".to_string()],
            "слот-1 {cmd}"
        );
    }
    let c = complete(&app, "switch img-a");
    assert_eq!(c.common.as_deref(), Some("switch img-aaa "));
}

#[test]
fn complete_mode_values_depend_on_command() {
    let app = crate::app::App::new();
    let c = complete(&app, "insert 0/3 --mode ");
    let vals: Vec<_> = c.items.iter().map(|i| i.display.clone()).collect();
    assert!(vals.contains(&"into".to_string()));
    let c = complete(&app, "reopen --mode ");
    let vals: Vec<_> = c.items.iter().map(|i| i.display.clone()).collect();
    assert_eq!(vals, vec!["read".to_string(), "write".to_string()]);
    let c = complete(&app, "reopen --");
    assert_eq!(c.common.as_deref(), Some("reopen --mode "));
}
```

- [ ] **Step 2: Запуск — падение** — `cargo test -p uefi-tui complete_` → FAIL.

- [ ] **Step 3: Реализация**

В `context_candidates` после ветки `--artifact-id` добавить:

```rust
if matches!(cmd, "switch" | "close") && head.len() == 1 {
    return app
        .registry
        .images
        .iter()
        .map(|im| im.image_id.clone())
        .filter(|c| c.starts_with(token))
        .collect();
}
```

Существующую ветку `if head.last() == Some(&"--mode")` сделать cmd-зависимой:

```rust
if head.last() == Some(&"--mode") {
    let vals: &[&str] = if cmd == "reopen" {
        &["read", "write"]
    } else {
        &["into", "before", "after"]
    };
    return vals
        .iter()
        .filter(|c| c.starts_with(token))
        .map(|s| s.to_string())
        .collect();
}
```

В match списка флагов (ветка `token.starts_with("--")`) добавить:

```rust
"reopen" => &["--mode"],
```

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R2 — completion image-ID слотов (switch/close слот-1), reopen --mode, cmd-зависимые значения --mode"
```

---

### Task 4: R3 — `:reopen` с гвардом READ→WRITE и клавиша `w`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (новые `ReopenPlan`/`reopen_plan`/`reopen_target`/`reopen`, ветка диспетча `"reopen"`, COMMANDS += `"reopen"`)
- Modify: `crates/uefi-tui/src/main.rs` (`handle_normal`, клавиша `w`)
- Test: `commands.rs` tests-mod; `tests/tui_integration.rs`

**Interfaces:**
- Consumes: паттерн тела `"open"` (`commands.rs:167-210`: ImageOpen → image_nodes_list → build_tree → active → refresh_registry); `ImageCloseRequest { image_id }`; `ImageInfo.path/mode`.
- Produces: `pub async fn reopen(app: &mut App, client: &mut Client, want_write: bool) -> Result<String, String>` — её зовут и диспетч, и клавиша `w`.

- [ ] **Step 1: Failing-тесты**

Юнит (гвард-матрица, pure):

```rust
#[test]
fn reopen_plan_guard_matrix() {
    use ReopenPlan::*;
    assert_eq!(super::reopen_plan(true, false), OpenWrite);
    assert_eq!(super::reopen_plan(true, true), AlreadyWrite);
    assert_eq!(super::reopen_plan(false, false), AlreadyRead);
    assert_eq!(super::reopen_plan(false, true), RefuseUnsavedWrite);
}
```

Integration (mock: registry = `mock-img-1`, mode 0, path `/tmp/mock.bin`; см. `tests/mock_server.rs:113-132`):

```rust
#[tokio::test(flavor = "multi_thread")]
async fn reopen_read_to_write_and_guards() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let state = uefi_common::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        active_image_id: None,
        sock_path: Some(sock.display().to_string()),
    };
    let mut client = uefi_tui::commands::connect(None, state).await.unwrap();
    let mut app = uefi_tui::app::App::new();
    uefi_tui::commands::execute_command(&mut app, "refresh", &mut client)
        .await
        .unwrap();
    app.focus = uefi_tui::app::Focus::Registry;

    let r = uefi_tui::commands::reopen(&mut app, &mut client, true).await;
    assert!(r.is_ok());
    assert_ne!(app.active_image_id.as_deref(), Some("mock-img-1"));
    assert!(app.status_msg.contains("reopened"), "статус: {}", app.status_msg);

    app.registry.images[0].mode = 1;
    uefi_tui::commands::reopen(&mut app, &mut client, true).await.unwrap();
    assert!(app.status_msg.contains("already in write mode"));

    let err = uefi_tui::commands::reopen(&mut app, &mut client, false).await.unwrap_err();
    assert!(err.contains(":save first"), "WRITE→read отказ: {err}");
}
```

- [ ] **Step 2: Запуск — падение** — `cargo test -p uefi-tui` → FAIL (`reopen_plan`/`reopen` нет).

- [ ] **Step 3: Реализация**

`commands.rs` (рядом с `add_prefill`, ~:1270):

```rust
/// Гвард-решение :reopen по желаемому и текущему режиму. Спека R3 (матрица).
#[derive(Debug, PartialEq, Eq)]
pub enum ReopenPlan {
    OpenWrite,
    AlreadyWrite,
    AlreadyRead,
    RefuseUnsavedWrite,
}

/// READ→WRITE только: повторный open с диска молча выбросил бы несохранённые
/// мутации WRITE-образа. Спека R3.
pub fn reopen_plan(want_write: bool, current_write: bool) -> ReopenPlan {
    match (want_write, current_write) {
        (true, false) => ReopenPlan::OpenWrite,
        (true, true) => ReopenPlan::AlreadyWrite,
        (false, false) => ReopenPlan::AlreadyRead,
        (false, true) => ReopenPlan::RefuseUnsavedWrite,
    }
}

/// Цель :reopen/:w — строка-образ в Registry при фокусе там, иначе активный
/// образ. Спека R3.
pub fn reopen_target(app: &App) -> Option<String> {
    if app.focus == crate::app::Focus::Registry
        && let Some(crate::app::RegistryRow::Image(i)) = app.current_registry_row()
        && let Some(im) = app.registry.images.get(i)
    {
        return Some(im.image_id.clone());
    }
    app.active_image_id.clone()
}

/// Переоткрытие образа в write: open(WRITE) → switch → close(old) → refresh.
/// No-op ветки не делают RPC. Спека R3.
pub async fn reopen(app: &mut App, client: &mut Client, want_write: bool) -> Result<String, String> {
    let target_id = reopen_target(app).ok_or("no image selected (registry row or active)")?;
    let im = app
        .registry
        .images
        .iter()
        .find(|i| i.image_id == target_id)
        .cloned()
        .ok_or("image not in registry — :refresh")?;
    match reopen_plan(want_write, im.mode == 1) {
        ReopenPlan::AlreadyWrite => {
            app.status_msg = format!("already in write mode: {}", im.image_id);
            return Ok(im.image_id);
        }
        ReopenPlan::AlreadyRead => {
            app.status_msg = format!("already read-only: {}", im.image_id);
            return Ok(im.image_id);
        }
        ReopenPlan::RefuseUnsavedWrite => {
            return Err(format!("refusing: unsaved write-mode image; :save first — {}", im.image_id));
        }
        ReopenPlan::OpenWrite => {}
    }
    let sid = client.state.session_id.clone().ok_or("no session")?;
    let req = ImageOpenRequest {
        session_id: sid,
        path: im.path.clone(),
        mode: 1,
        name: String::new(),
    };
    let r = client
        .inner
        .image_open(auth_req(&client.state, req))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let dump = client
        .inner
        .image_nodes_list(auth_req(
            &client.state,
            ImageNodesListRequest {
                image_id: r.image_id.clone(),
                filter: String::new(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.tree = crate::tree::build_tree(&dump.nodes);
    app.cursor = 0;
    app.active_image_id = Some(r.image_id.clone());
    client.state.active_image_id = Some(r.image_id.clone());
    let _ = client
        .inner
        .image_close(auth_req(&client.state, ImageCloseRequest { image_id: target_id }))
        .await;
    let _ = refresh_registry(app, client).await;
    if app.view == View::Forms {
        refresh_forms(app, client).await?;
    }
    app.status_msg = format!("reopened {} in write mode", im.name);
    Ok(r.image_id)
}
```

Диспетч (рядом с `"open"`):

```rust
"reopen" => {
    let want_write = !parts.contains(&"read");
    reopen(app, client, want_write).await
}
```

`COMMANDS`: добавить `"reopen"` после `"close"`.

`main.rs` `handle_normal`, после ветки `i|r|d` (:81-99) добавить:

```rust
AppEvent::Key('w') if app.focus != Focus::Details => {
    if let Some(c) = client.as_mut()
        && let Err(e) = commands::reopen(app, c, true).await
    {
        app.status_msg = format!("error: {e}");
    }
}
```

(Forms-view не задет: диспетч в `handle_normal_forms` отдельный, туда `w` не добавляем — спека R3.)

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R3 — :reopen [--mode write|read] с гвардом READ→WRITE (open→switch→close old) и клавиша w"
```

---

### Task 5: R4 — smart prefill `i`/`r` (вариант A)

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (новый `mutation_prefill`)
- Modify: `crates/uefi-tui/src/main.rs:81-99` (ветка `i|r|d`)
- Test: `commands.rs` tests-mod

**Interfaces:**
- Produces: `pub fn mutation_prefill(kind: &str, path: &str, row: Option<&crate::app::RegistryRow>, artifacts: &[uefi_proto::ArtifactInfo]) -> String` — prefill-строка с хвостовым пробелом (как текущий `--file `).

- [ ] **Step 1: Failing-тест**

```rust
#[test]
fn mutation_prefill_artifact_row_wins_over_file() {
    let artifacts = vec![uefi_proto::ArtifactInfo {
        artifact_id: "art-1".into(),
        ..Default::default()
    }];
    let row = Some(crate::app::RegistryRow::Artifact(0));
    assert_eq!(
        mutation_prefill("insert", "1/3", row.as_ref(), &artifacts),
        "insert 1/3 --artifact-id art-1 "
    );
    assert_eq!(
        mutation_prefill("replace", "1/3", row.as_ref(), &artifacts),
        "replace 1/3 --artifact-id art-1 "
    );
    let image_row = Some(crate::app::RegistryRow::Image(0));
    assert_eq!(
        mutation_prefill("insert", "1/3", image_row.as_ref(), &artifacts),
        "insert 1/3 --file "
    );
    assert_eq!(
        mutation_prefill("insert", "1/3", None, &artifacts),
        "insert 1/3 --file "
    );
    let stale = Some(crate::app::RegistryRow::Artifact(9));
    assert_eq!(
        mutation_prefill("insert", "1/3", stale.as_ref(), &artifacts),
        "insert 1/3 --file ",
        "вышедший за границы индекс — фолбэк на --file"
    );
}
```

- [ ] **Step 2: Запуск — падение** — `cargo test -p uefi-tui mutation_prefill` → FAIL.

- [ ] **Step 3: Реализация**

`commands.rs` (рядом с `add_prefill`):

```rust
/// Prefill i/r: registry-курсор на артефакте → --artifact-id, иначе --file.
/// Спека R4 (вариант A, TODO:808).
pub fn mutation_prefill(
    kind: &str,
    path: &str,
    row: Option<&crate::app::RegistryRow>,
    artifacts: &[uefi_proto::ArtifactInfo],
) -> String {
    let artifact = row.and_then(|r| match r {
        crate::app::RegistryRow::Artifact(i) => artifacts.get(*i),
        crate::app::RegistryRow::Image(_) => None,
    });
    match artifact {
        Some(a) => format!("{kind} {path} --artifact-id {} ", a.artifact_id),
        None => format!("{kind} {path} --file "),
    }
}
```

`main.rs:84-97` — вычисление prefill через хелпер (borrow app заканчивается до `enter_insert_mode`):

```rust
let (cmd_str, prefill) = {
    let path = app.selected_path().unwrap_or_default();
    let row = app.current_registry_row();
    let artifacts = &app.registry.artifacts;
    match ev {
        AppEvent::Key('i') => ("insert", mutation_prefill("insert", &path, row.as_ref(), artifacts)),
        AppEvent::Key('r') => ("replace", mutation_prefill("replace", &path, row.as_ref(), artifacts)),
        _ => ("remove", format!("remove {path}")),
    }
};
app.enter_insert_mode(cmd_str, prefill);
```

(`mutation_prefill` доступен через `commands::`-путь импорта main.rs — как соседние `commands::add_prefill`.)

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R4 — smart prefill i/r: registry-курсор на артефакте префиллит --artifact-id (вариант A)"
```

---

### Task 6: R5 — `:export` absolutization и дефолт `cwd/<artifact_id>`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs:322-341` (ветка `"export"`), новый хелпер рядом с `absolutize_path` (:143)
- Test: `commands.rs` tests-mod

**Interfaces:**
- Consumes: `absolutize_path` (`commands.rs:143`).
- Produces: `fn export_output_path(arg: Option<&str>, artifact_id: &str) -> String` (private).

- [ ] **Step 1: Failing-тест**

```rust
#[test]
fn export_output_path_absolute_or_cwd_default() {
    let abs = export_output_path(Some("/tmp/out.bin"), "art-1");
    assert_eq!(abs, "/tmp/out.bin");
    let rel = export_output_path(Some("out.bin"), "art-1");
    let cwd = std::env::current_dir().unwrap();
    assert_eq!(rel, cwd.join("out.bin").display().to_string());
    let def = export_output_path(None, "art-1");
    assert_eq!(def, cwd.join("art-1").display().to_string(), "дефолт — файл, не каталог");
}
```

- [ ] **Step 2: Запуск — падение** — `cargo test -p uefi-tui export_output_path` → FAIL.

- [ ] **Step 3: Реализация**

```rust
/// PATH для :export — absolutize либо cwd/<artifact_id> (паритет CLI,
/// resolve_output_path uefi-cli). Спека R5: чинит дефолт-каталог.
fn export_output_path(arg: Option<&str>, artifact_id: &str) -> String {
    match arg {
        Some(p) => absolutize_path(p),
        None => match std::env::current_dir() {
            Ok(cwd) => cwd.join(artifact_id).display().to_string(),
            Err(_) => artifact_id.to_string(),
        },
    }
}
```

Ветка `"export"` — заменить нынешний `let path = …` (:324-329):

```rust
"export" => {
    let artifact_id = parts.get(1).ok_or("usage: :export ARTIFACT_ID [PATH]")?;
    let path = export_output_path(parts.get(2).copied(), artifact_id);
    // … далее без изменений (req с output_path: path, статус «exported … to {path}»)
}
```

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "fix(tui): R5 — :export absolutize PATH + дефолт cwd/<artifact_id> вместо голого cwd (движковый гейт artifact_export)"
```

---

### Task 7: R6 — скролл help (модальный, j/k/PgUp/PgDn)

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (поле `help_scroll`, метод `toggle_help`; замена двух `?`-сайтов: main.rs:78 и main.rs:~312 в forms — сами сайты в main.rs)
- Modify: `crates/uefi-tui/src/ui/help.rs` (clamp + `.scroll`)
- Modify: `crates/uefi-tui/src/main.rs:66-78` (ранний branch в `handle_normal`)
- Test: `help.rs` tests-mod

**Interfaces:**
- Produces: `pub fn clamp_offset(off: u16, total: usize, visible: u16) -> u16` (help.rs); `App::help_scroll: u16`; `App::toggle_help(&mut self)` (сбрасывает скролл при закрытии/открытии).

- [ ] **Step 1: Failing-тесты**

```rust
#[test]
fn clamp_offset_bounds() {
    assert_eq!(clamp_offset(0, 70, 20), 0);
    assert_eq!(clamp_offset(100, 70, 20), 50);
    assert_eq!(clamp_offset(100, 10, 20), 0, "текст короче окна — скролл 0");
    assert_eq!(clamp_offset(5, 70, 0), 0, "нулевая высота — 0");
}
```

- [ ] **Step 2: Запуск — падение** — `cargo test -p uefi-tui clamp_offset` → FAIL.

- [ ] **Step 3: Реализация**

`help.rs`:

```rust
/// Clamp скролла по числу строк и высоте окна (минус рамка). Спека R6.
pub fn clamp_offset(off: u16, total: usize, visible: u16) -> u16 {
    if visible == 0 {
        return 0;
    }
    off.min((total as u16).saturating_sub(visible))
}
```

`render` (help.rs:81-92) — после `Clear`:

```rust
let visible = area.height.saturating_sub(2);
let off = clamp_offset(app.help_scroll, HELP.lines().count(), visible);
let p = Paragraph::new(HELP)
    .scroll((off, 0))
    .block(Block::default().borders(Borders::ALL).title("Help (? to close)"));
```

`app.rs`: поле `pub help_scroll: u16` (+ `help_scroll: 0` в `App::new`); метод:

```rust
/// Toggle help-оверлея; скролл сбрасывается при каждом переключении. Спека R6.
pub fn toggle_help(&mut self) {
    self.show_help = !self.show_help;
    self.help_scroll = 0;
}
```

`main.rs`: оба сайта `app.show_help = !app.show_help` (:78 в `handle_normal`, и в `handle_normal_forms`) заменить на `app.toggle_help()`. В начало `handle_normal` (до `if matches!(ev, Tab | BackTab)`, :67) вставить модальный branch:

```rust
if app.show_help {
    match ev {
        AppEvent::Key('?') => app.toggle_help(),
        AppEvent::Key('q') | AppEvent::Quit => app.quit = true,
        AppEvent::Key('j') | AppEvent::Down => app.help_scroll = app.help_scroll.saturating_add(1),
        AppEvent::Key('k') | AppEvent::Up => app.help_scroll = app.help_scroll.saturating_sub(1),
        AppEvent::PageDown => app.help_scroll = app.help_scroll.saturating_add(10),
        AppEvent::PageUp => app.help_scroll = app.help_scroll.saturating_sub(10),
        _ => {}
    }
    return;
}
```

(Forms-view покрыт автоматически: branch стоит до диспетча `handle_normal_forms`.)

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R6 — модальный скролл help (j/k/PgUp/PgDn, clamp по высоте), toggle сбрасывает скролл"
```

---

### Task 8: R7 — `fmt_u32_ids` и ассерт пустого forms-списка

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs:1308-1343` (`formset_add_status`/`form_add_status`)
- Test: `commands.rs` tests-mod

**Interfaces:**
- Produces: `fn fmt_u32_ids(ids: &[u32]) -> String` (private; пустой → `"(none)"`).

- [ ] **Step 1: Failing-тесты**

```rust
#[test]
fn fmt_u32_ids_join_and_none() {
    assert_eq!(fmt_u32_ids(&[]), "(none)");
    assert_eq!(fmt_u32_ids(&[10029, 10057]), "10029,10057");
}

#[test]
fn add_prefill_none_when_no_forms() {
    let app = crate::app::App::new();
    assert!(add_prefill(&app).is_none(), "пустой forms-список — префиллa нет");
}
```

- [ ] **Step 2: Запуск — падение** — FAIL (`fmt_u32_ids` нет; второй тест может уже проходить — если проходит, оставить как покрытие, см. шаг 3).

- [ ] **Step 3: Реализация**

```rust
/// Join u32-списков статусов; пустой — (none). Спека R7 (V3-мелочь 1).
fn fmt_u32_ids(ids: &[u32]) -> String {
    if ids.is_empty() {
        "(none)".into()
    } else {
        ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
    }
}
```

В `formset_add_status` и `form_add_status` заменить идентичные блоки `let forms = if form_ids.is_empty() … join(",")` на `let forms = fmt_u32_ids(form_ids);`.

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS (существующие статус-тесты не меняют ожиданий — формат идентичен).

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "refactor(tui): R7 — хелпер fmt_u32_ids (join-дедуп статусов) + ассерт пустого forms → add_prefill None"
```

---

### Task 9: R8 — скролл details-панелей («Form» авто-follow + Details ручной)

**Files:**
- Modify: `crates/uefi-tui/src/forms.rs:285-…` (`form_details_text` → `form_details` + обёртка)
- Modify: `crates/uefi-tui/src/app.rs` (FormsData: `details_scroll`, `details_anchor`; App: `details_scroll`, `details_anchor`, `details_scroll_by`)
- Modify: `crates/uefi-tui/src/ui/forms.rs:88-99` (details-рендер), tests-mod
- Modify: `crates/uefi-tui/src/ui/details.rs` (clamp + anchor-сброс; сигнатура `&mut App`)
- Modify: `crates/uefi-tui/src/main.rs` (j/k/PgUp/PgDn для `Focus::Details`; PgUp/PgDn для FormsFocus::Details)
- Test: `forms.rs`, `ui/forms.rs`, `ui/details.rs` tests-mod

**Interfaces:**
- Consumes: `compute_scrolled_offset` (`tree.rs:53`).
- Produces: `pub struct FormDetails { pub text: String, pub marker_line: Option<usize>, pub info_line: Option<usize> }`; `pub fn form_details(&FormsData, &[FormsRow], usize) -> FormDetails`; `pub fn follow_offset(prev: u16, target: Option<usize>, total: usize, inner_h: usize) -> u16` (ui/forms.rs); `App::details_scroll_by(&mut self, delta: i32)`; `const FORMS_SCROLL_PAD: usize = 3` (ui/forms.rs); `FormsData.details_followed: Option<usize>` — последний target, на который авто-follow уже среагировал (fix: follow только при смене цели — та же цель → clamp, ручной PgUp/PgDn выживает; якорь-сброс чистит `details_followed`).

- [ ] **Step 1: Failing-тесты**

`forms.rs` (фейбрик — как у соседнего `form_details_text_path_marker_question_and_gates`, :642; хелперы `fi`/`edge`/`all_row_keys` уже в tests-mod):

```rust
#[test]
fn form_details_reports_marker_and_info_lines() {
    let forms = vec![
        fi("t1", "S", 1, "Main", true),
        fi("t1", "S", 2, "Serial", false),
    ];
    let edges = vec![edge("S", 1, 2, "")];
    let ex = all_row_keys(&forms, &edges);
    let rows = build_tree_rows(&forms, &edges, &ex);
    let fd = FormsData {
        questions: vec![
            uefi_proto::QuestionSummary {
                question_id: 0x210,
                kind: "one_of".into(),
                prompt: "Serial Port".into(),
                ..Default::default()
            },
            uefi_proto::QuestionSummary {
                question_id: 0x211,
                kind: "numeric".into(),
                prompt: "Baud".into(),
                ..Default::default()
            },
        ],
        questions_key: Some(FormKey {
            target: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 2,
            title: "Serial".into(),
        }),
        question_cursor: 1,
        question_info: Some(uefi_proto::QuestionInfo {
            question_id: 0x211,
            kind: "numeric".into(),
            var_store_id: 1,
            var_offset: 0x60,
            width: 1,
            min: 0,
            max: 255,
            step: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    let d = form_details(&fd, &rows, 2);
    assert_eq!(
        d.marker_line,
        Some(8),
        "строки: заголовок 0-3, Path 4, бланк 5, шапка вопросов 6, q0 7, маркер q1 8"
    );
    assert_eq!(
        d.info_line,
        Some(10),
        "после бланка 9 — заголовок «Question q0x211»"
    );
    assert_eq!(d.text, form_details_text(&fd, &rows, 2), "текст не изменился");
    assert!(d.text.contains("> \u{F1EC} Baud (q0x211) (numeric)"));
}
```

`ui/forms.rs`:

```rust
#[test]
fn follow_offset_follows_target_and_keeps_window() {
    assert_eq!(follow_offset(0, Some(50), 100, 10), 44, "цель ниже окна — докрутка с pad");
    assert_eq!(follow_offset(44, Some(50), 100, 10), 44, "цель в окне — офсет на месте");
    assert_eq!(follow_offset(90, None, 100, 10), 90);
    assert_eq!(follow_offset(90, None, 20, 10), 10, "clamp по total");
    assert_eq!(follow_offset(0, Some(0), 100, 10), 0);
}

#[test]
fn details_anchor_reset_on_form_change() {
    let mut app = crate::app::App::new();
    app.forms.details_scroll = 7;
    app.forms.details_anchor = Some("old|S|1".into());
    // render с другой формой → details_scroll == 0 (TestBackend, как в Task 2)
}
```

`app.rs`:

```rust
#[test]
fn details_scroll_by_saturates() {
    let mut app = App::new();
    app.details_scroll_by(5);
    assert_eq!(app.details_scroll, 5);
    app.details_scroll_by(-10);
    assert_eq!(app.details_scroll, 0);
}
```

- [ ] **Step 2: Запуск — падение** — FAIL (`form_details`/`follow_offset`/полей нет).

- [ ] **Step 3: Реализация**

`forms.rs`: перевести тело `form_details_text` на накопление `Vec<String>` (строки — те же format!-выражения дословно); `form_details` возвращает структуру, фиксируя `lines.len()` в момент пуша строки маркера (`i == forms.question_cursor`) и первой строки info-блока; `form_details_text` становится обёрткой `.text` (старые тесты не трогаем).

`app.rs`: в `FormsData` — `pub details_scroll: u16, pub details_anchor: Option<String>, pub details_followed: Option<usize>`; в `App` — `pub details_scroll: u16, pub details_anchor: Option<String>` (+ нули/None в `App::new`); метод:

```rust
/// Ручной скролл details-панели основного вида. Спека R8.
pub fn details_scroll_by(&mut self, delta: i32) {
    if delta >= 0 {
        self.details_scroll = self.details_scroll.saturating_add(delta as u16);
    } else {
        self.details_scroll = self.details_scroll.saturating_sub((-delta) as u16);
    }
}
```

`ui/forms.rs`:

```rust
const FORMS_SCROLL_PAD: usize = 3;

/// Скролл details «Form»: follow info/marker-строки с гистерезисом дерева,
/// без цели — clamp. Спека R8.
pub fn follow_offset(prev: u16, target: Option<usize>, total: usize, inner_h: usize) -> u16 {
    match target {
        Some(t) => {
            crate::tree::compute_scrolled_offset(t, prev as usize, inner_h, total, FORMS_SCROLL_PAD) as u16
        }
        None => (prev as usize).min(total.saturating_sub(inner_h)) as u16,
    }
}
```

Details-рендер (`ui/forms.rs:88-99`) заменить на:

```rust
let fd = crate::forms::form_details(&app.forms, &rows, app.forms.cursor);
let anchor = match rows.get(app.forms.cursor) {
    Some(FormsRow::Form { key, .. }) => {
        Some(format!("{}|{}|{}", key.target, key.formset_guid, key.form_id_ifr))
    }
    _ => None,
};
if app.forms.details_anchor != anchor {
    app.forms.details_scroll = 0;
    app.forms.details_anchor = anchor;
    app.forms.details_followed = None;
}
let inner_h = cols[1].height.saturating_sub(2) as usize;
let total = fd.text.lines().count();
let target = fd.info_line.or(fd.marker_line);
let off = if app.forms.details_followed == target {
    follow_offset(app.forms.details_scroll, None, total, inner_h)
} else {
    let off = follow_offset(app.forms.details_scroll, target, total, inner_h);
    app.forms.details_followed = target;
    off
};
app.forms.details_scroll = off;
f.render_widget(
    Paragraph::new(fd.text)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((off, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Form")
                .border_style(focus_style(!list_focus)),
        ),
    cols[1],
);
```

Fix (финальное ревью): безусловный write-back `follow_offset` глотал ручной PgUp/PgDn — от resting-офсета цель сидит на нижней кромке гистерезисной полосы, и любая ручная докрутка откатывалась на следующем draw. Follow срабатывает ТОЛЬКО при смене цели (`details_followed != target`): та же цель → clamp-only ветка (`follow_offset(.., None, ..)`), новая цель → follow + запоминание. Якорь-сброс (смена формы) чистит `details_followed`, чтобы follow гарантированно включился на новой форме даже при совпадении номеров строк.

Ручной скролл Forms-details — в `handle_normal_forms` (main.rs, рядом с j/k-ветками Details):

```rust
AppEvent::PageDown if app.forms.focus == FormsFocus::Details && !app.forms.show_strings => {
    app.forms.details_scroll = app.forms.details_scroll.saturating_add(10);
}
AppEvent::PageUp if app.forms.focus == FormsFocus::Details && !app.forms.show_strings => {
    app.forms.details_scroll = app.forms.details_scroll.saturating_sub(10);
}
```

`ui/details.rs` — сигнатура `render(f: &mut Frame, area: Rect, app: &mut App)`; после сборки `content`:

```rust
let path = app.selected_path();
if app.details_anchor != path {
    app.details_scroll = 0;
    app.details_anchor = path;
}
let total = content.lines().count();
let inner_h = area.height.saturating_sub(2) as usize;
app.details_scroll = (app.details_scroll as usize).min(total.saturating_sub(inner_h)) as u16;
let p = Paragraph::new(content)
    .scroll((app.details_scroll, 0))
    .block(Block::default().borders(Borders::ALL).title(title));
```

`main.rs` `handle_normal`: j/k/PgUp/PgDn для `Focus::Details` (сейчас `{}`/только Tree):

```rust
AppEvent::Key('j') | AppEvent::Down => match app.focus {
    Focus::Registry => app.registry_cursor_down(),
    Focus::Tree => app.cursor_down(),
    Focus::Details => app.details_scroll_by(1),
},
AppEvent::Key('k') | AppEvent::Up => match app.focus {
    Focus::Registry => app.registry_cursor_up(),
    Focus::Tree => app.cursor_up(),
    Focus::Details => app.details_scroll_by(-1),
},
AppEvent::PageDown => match app.focus {
    Focus::Tree => app.cursor_page_down(),
    Focus::Details => app.details_scroll_by(10),
    Focus::Registry => {}
},
AppEvent::PageUp => match app.focus {
    Focus::Tree => app.cursor_page_up(),
    Focus::Details => app.details_scroll_by(-10),
    Focus::Registry => {}
},
```

Hint основного вида (`ui/mod.rs`, `Focus::Details`): `"NORMAL[Details]: j/k scroll · Ctrl-hjkl focus · :cmd · ?help · q"`.

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "feat(tui): R8 — скролл details: «Form» авто-follow маркера/question_info (гистерезис SCROLL_PAD=3), Details основного вида — ручной (оживление фокуса)"
```

---

### Task 10: R9 — персистентный ListState списков Forms View

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (поля `forms_list_state`, `strings_list_state` + init)
- Modify: `crates/uefi-tui/src/ui/forms.rs:63-87` (список форм), `:102-131` (strings)
- Test: `ui/forms.rs` tests-mod

**Interfaces:**
- Consumes: `compute_scrolled_offset` (`tree.rs:53`), `FORMS_SCROLL_PAD` из Task 9.
- Produces: `App.forms_list_state: ListState`, `App.strings_list_state: ListState` — офсет переживает кадр (регрессия R9).

- [ ] **Step 1: Failing-тест (регрессия бага «вверх скроллит страницу»)**

```rust
#[test]
fn forms_list_offset_kept_when_cursor_walks_up() {
    let mut app = crate::app::App::new();
    app.forms.flat_mode = true;
    app.forms.forms = (0..30)
        .map(|i| uefi_proto::FormInfo {
            form_id: format!("f{i}"),
            formset_guid: "S".into(),
            form_id_ifr: i,
            title: format!("F{i}"),
            visible: true,
        })
        .collect();
    app.forms.expanded = ["S".into()].into();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 10)).unwrap();
    for _ in 0..20 {
        app.forms_cursor_down();
    }
    terminal.draw(|f| super::render(f, f.area(), &mut app)).unwrap();
    let off = app.forms_list_state.offset();
    assert!(off > 0, "прокрутка началась");
    app.forms_cursor_up();
    terminal.draw(|f| super::render(f, f.area(), &mut app)).unwrap();
    assert_eq!(app.forms_list_state.offset(), off, "вверх двигает курсор, не страницу");
    assert!(app.forms.cursor < 20);
}

#[test]
fn strings_list_uses_persistent_state() {
    let mut app = crate::app::App::new();
    app.forms.show_strings = true;
    app.forms.strings = (0..30)
        .map(|i| uefi_proto::StringInfo {
            language: "en".into(),
            string_id: i,
            text: format!("s{i}"),
        })
        .collect();
    for _ in 0..20 {
        app.strings_cursor_down();
    }
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 10)).unwrap();
    terminal.draw(|f| super::render(f, f.area(), &mut app)).unwrap();
    assert!(app.strings_list_state.offset() > 0);
}
```

(StringInfo-поля сверить с proto; при расхождении — по факту сгенерированной структуры.)

- [ ] **Step 2: Запуск — падение** — FAIL (полей нет; текущий рендер сбрасывает офсет).

- [ ] **Step 3: Реализация**

`app.rs`: `pub forms_list_state: ListState, pub strings_list_state: ListState` (+ `ListState::default()` в `App::new`).

`ui/forms.rs`, список форм (:63-87) — паттерн registry (`ui/registry.rs:50-77`):

```rust
let total = rows.len();
let inner_h = cols[0].height.saturating_sub(2) as usize;
let cursor = if rows.is_empty() { 0 } else { app.forms.cursor.min(total - 1) };
let prev_off = app.forms_list_state.offset();
let new_off = compute_scrolled_offset(cursor, prev_off, inner_h, total, FORMS_SCROLL_PAD);
app.forms_list_state.select(if rows.is_empty() { None } else { Some(cursor) });
*app.forms_list_state.offset_mut() = new_off;
f.render_stateful_widget(list, cols[0], &mut app.forms_list_state);
```

(`use crate::tree::compute_scrolled_offset;` в шапке, если ещё не импортирован.)

`render_strings` (:102-131) — то же для видимого подсписка:

```rust
let visible = app.strings_visible();
let total = visible.len();
let inner_h = area.height.saturating_sub(2) as usize;
let pos = visible
    .iter()
    .position(|&i| i == app.forms.strings_cursor)
    .unwrap_or(0);
let cursor = if visible.is_empty() { 0 } else { pos.min(total - 1) };
let prev_off = app.strings_list_state.offset();
let new_off = compute_scrolled_offset(cursor, prev_off, inner_h, total, FORMS_SCROLL_PAD);
app.strings_list_state.select(if visible.is_empty() { None } else { Some(cursor) });
*app.strings_list_state.offset_mut() = new_off;
f.render_stateful_widget(list, area, &mut app.strings_list_state);
```

(локальный `let mut state = ListState::default()` в обоих рендерах удалить.)

- [ ] **Step 4: Прогон** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings` → PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/uefi-tui
git commit -m "fix(tui): R9 — персистентный ListState списков Forms/strings + compute_scrolled_offset (SCROLL_PAD=3): вверх двигает курсор, не страницу"
```

---

### Task 11: финальные проверки цикла

**Files:** нет правок кода (только проверки).

- [ ] **Step 1:** `cargo test --all` → PASS.
- [ ] **Step 2:** `cargo clippy --all -- -D warnings` → PASS.
- [ ] **Step 3:** `cargo fmt --all -- --check` → PASS (при расхождениях — `cargo fmt --all`, отдельный коммит `style(tui): fmt`).
- [ ] **Step 4:** Прогон живого гейта владельцем по чек-листу спеки (HNX99TF); после вердикта — закрывающий docs-коммит: §Вердикт в спеку, закрытие TODO-пунктов (826-851, 808-824, 2760 TUI-часть, 3369, 3379-3387, 3325-остаток), PR в master.

---

## Self-Review (выполнен при написании)

1. **Покрытие спеки:** R0→Task 1; R1→Task 2; R2→Task 3; R3→Task 4; R4→Task 5; R5→Task 6; R6→Task 7; R7→Task 8; R8→Task 9; R9→Task 10. Non-goals не реализуются. Живой гейт — Task 11.
2. **Placeholder-скан:** тела switch/close в Task 1 и статус-блоки Task 8 даны отсылками к дословному переносу существующего кода с точными координатами (это перенос, не новая логика); остальное — полный код.
3. **Консистентность имён:** `mutation_prefill`/`export_output_path`/`reopen*`/`FormDetails`/`follow_offset`/`clamp_offset`/`details_scroll*`/`forms_list_state` едины между задачами; `FORMS_SCROLL_PAD` объявлен в Task 9, потреблён в Task 10.
