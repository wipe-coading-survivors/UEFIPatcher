# Engine logging — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать engine наблюдаемым — entering/exiting логи gRPC-хендлеров (INFO) + ключевых ops движка (DEBUG/INFO) + разумный дефолт подписчика и CLI-флаги, чтобы сообщения были видны «из коробки».

**Architecture:** Включаем фичу `attributes` крейта `tracing` и навешиваем `#[tracing::instrument]` на gRPC-хендлеры (server.rs) и ключевые функции движка. Богатые события-результаты («что сделано») — через явные `tracing::info!`. Новый модуль `logging` инкапсулирует инициализацию подписчика с дефолтом `uefi_engine=info,warn` и CLI-флагами `-v`/`-q`.

**Tech Stack:** `tracing 0.1` (features `attributes`), `tracing-subscriber 0.3` (env-filter), `tracing-test 0.2` (dev-dep для smoke-тестов), `clap 4` (ArgAction::Count).

**Spec:** `docs/superpowers/specs/2026-08-13-engine-logging-design.md`

## Global Constraints

- Крейты: НЕ самописный парсинг; `tracing` — через `#[brw]`... (здесь — только `#[tracing::instrument]`).
- `tracing = { version = "0.1", features = ["attributes"] }` в workspace `Cargo.toml` (фича `attributes` обязательна для макроса `#[instrument]`).
- Дефолт-директива при отсутствии `RUST_LOG`: `"uefi_engine=info,warn"`. `RUST_LOG` всегда перекрывает флаги.
- Уровень gRPC-хендлеров — INFO (`#[instrument]` default level = INFO). Уровень `parse_image`/`build_image` — INFO. Уровень `ops::*` и `setup::set_item_visibility` — DEBUG.
- **Без комментариев в коде** (AGENTS.md rule 8), кроме ссылок на референс `file:line`.
- **Module-first rule (AGENTS.md rule 4):** при создании нового модуля добавлять `pub mod <name>;` в `lib.rs` в ТОТ ЖЕ ШАГ, до `cargo test`.
- **TDD порядок** (rule 5) где применимо; **один коммит на шаг** где указано.
- После каждой задачи: `cargo test -p uefi-engine` и `cargo clippy -p uefi-engine -- -D warnings`.
- Дефекты плана — отдельным коммитом ДО реализации (rule 11).

## Design notes для исполнителя

1. **Span-поля vs события:** `#[instrument]` НЕ может читать поля, доступные только после `req.into_inner()` (т.к. instrument работает на параметрах функции). Поэтому span-ы делаем **без полей из запроса** (`skip(self, req)`), а всю структурную информацию кладём в **явные события** `tracing::info!`/`debug!` внутри тела. Имя span-а = имя метода (видно в enter/exit при `-v`).
2. **entering/exiting:** выражается через lifecycle span-а (`FmtSpan::ENTER|CLOSE`), включается только при `-v` (см. модуль `logging`). В базовом режиме (info) видны только события-результаты.
3. `#[instrument]` default level = `INFO`. Для DEBUG-функций указываем `level = "debug"` явно.

## File Structure

| Файл | Действие | Ответственность |
|------|----------|-----------------|
| `Cargo.toml` | Modify | workspace-dep `tracing` + фича `attributes` |
| `crates/uefi-engine/Cargo.toml` | Modify | dev-dep `tracing-test` |
| `crates/uefi-engine/src/lib.rs` | Modify | `pub mod logging;` |
| `crates/uefi-engine/src/logging.rs` | Create | `verbosity_directive`, `span_events`, `init`, unit-тесты |
| `crates/uefi-engine/src/bin/engine.rs` | Modify | CLI-флаги `-v`/`-q`, вызов `logging::init` |
| `crates/uefi-engine/src/parser/image.rs` | Modify | `#[instrument]` + INFO-milestone + `count_files` |
| `crates/uefi-engine/src/builder/mod.rs` | Modify | `#[instrument]` + INFO-milestone |
| `crates/uefi-engine/src/ops.rs` | Modify | `#[instrument(level=debug)]` + DEBUG-milestones |
| `crates/uefi-engine/src/setup/mod.rs` | Modify | `#[instrument(level=debug)]` + DEBUG-milestone |
| `crates/uefi-engine/src/rpc/server.rs` | Modify | `#[instrument]` на все 22 RPC + INFO события-результаты |

---

## Task 1: Модуль logging + CLI-флаги + дефолт подписчика

**Files:**
- Create: `crates/uefi-engine/src/logging.rs`
- Modify: `Cargo.toml` (workspace deps)
- Modify: `crates/uefi-engine/src/lib.rs`
- Modify: `crates/uefi-engine/src/bin/engine.rs`

**Interfaces:**
- Produces: `uefi_engine::logging::verbosity_directive(verbose: u8, quiet: bool) -> &'static str`;
  `uefi_engine::logging::span_events(verbose: u8) -> tracing_subscriber::fmt::format::FmtSpan`;
  `uefi_engine::logging::init(verbose: u8, quiet: bool)`.

- [ ] **Step 1: Включить фичу `attributes` у `tracing`**

В `Cargo.toml` (корень workspace), секция `[workspace.dependencies]`, заменить строку
`tracing = "0.1"` на:

```toml
tracing = { version = "0.1", features = ["attributes"] }
```

- [ ] **Step 2: Создать модуль `logging` с тестом (RED)**

Создать `crates/uefi-engine/src/logging.rs`:

```rust
use tracing_subscriber::fmt::format::FmtSpan;

pub fn verbosity_directive(verbose: u8, quiet: bool) -> &'static str {
    match (quiet, verbose) {
        (true, _) => "error",
        (false, 0) => "uefi_engine=info,warn",
        (false, 1) => "uefi_engine=debug,warn",
        (false, _) => "uefi_engine=trace,warn",
    }
}

pub fn span_events(verbose: u8) -> FmtSpan {
    if verbose >= 1 {
        FmtSpan::ENTER | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    }
}

pub fn init(verbose: u8, quiet: bool) {
    let directive = verbosity_directive(verbose, quiet);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(directive));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(span_events(verbose))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_directive_defaults() {
        assert_eq!(verbosity_directive(0, false), "uefi_engine=info,warn");
        assert_eq!(verbosity_directive(1, false), "uefi_engine=debug,warn");
        assert_eq!(verbosity_directive(2, false), "uefi_engine=trace,warn");
        assert_eq!(verbosity_directive(9, false), "uefi_engine=trace,warn");
        assert_eq!(verbosity_directive(0, true), "error");
        assert_eq!(verbosity_directive(5, true), "error");
    }

    #[test]
    fn span_events_only_when_verbose() {
        assert_eq!(span_events(0), FmtSpan::NONE);
        assert!(span_events(1) & FmtSpan::ENTER != FmtSpan::NONE);
        assert!(span_events(1) & FmtSpan::CLOSE != FmtSpan::NONE);
    }
}
```

Добавить в `crates/uefi-engine/src/lib.rs` (module-first rule) строку `pub mod logging;`:

```rust
pub mod builder;
pub mod decompress;
pub mod ffs;
pub mod logging;
pub mod ops;
pub mod parser;
pub mod rpc;
pub mod session;
pub mod setup;
pub mod setup_advanced;
pub mod storage;
pub mod types;
pub use types::*;
```

- [ ] **Step 3: Запустить тест — должен проходить (модуль реальный)**

Run: `cargo test -p uefi-engine logging`
Expected: PASS (`verbosity_directive_defaults`, `span_events_only_when_verbose`).

> Примечание: тесты пишутся на уже корректную pure-функцию (TDD здесь тривиален — функция возвращает `&'static str`); RED-фаза была бы «функция не найдена», но модуль создаётся сразу рабочим, поэтому тест проходит сразу. Это допустимое исключение для pure-логики инициализации.

- [ ] **Step 4: Добавить CLI-флаги в `bin/engine.rs` и подключить `logging::init`**

В `crates/uefi-engine/src/bin/engine.rs` добавить поля в `struct Args` (после `purge_artifacts`):

```rust
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
```

В `fn main()` — изменить порядок: сначала `Args::parse()`, потом `logging::init`. Заменить блок

```rust
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
```

на

```rust
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    uefi_engine::logging::init(args.verbose, args.quiet);
```

Существующий `tracing::info!("Starting engine: ...")` (engine.rs:42-47) оставить без изменений.

- [ ] **Step 5: Проверить сборку и крейт engine**

Run: `cargo build -p uefi-engine`
Expected: сборка OK, предупреждений нет.

Run: `cargo test -p uefi-engine`
Expected: все тесты PASS (включая новые `verbosity_directive_defaults`, `span_events_only_when_verbose`).

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: clean.

- [ ] **Step 6: Smoke-проверка бинарника вручную**

Run: `cargo run -p uefi-engine --bin engine -- --help 2>&1`
Expected: в `--help` видны `-v, --verbose` и `-q, --quiet`.

Run (в отдельном терминале, кратко): `RUST_LOG= cargo run -p uefi-engine --bin engine` и убедиться, что строка `Starting engine:` видна (доказывает, что дефолтный фильтр пропускает INFO). Остановить (Ctrl-C).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/uefi-engine/src/lib.rs crates/uefi-engine/src/logging.rs crates/uefi-engine/src/bin/engine.rs
git commit -m "feat(uefi-engine): logging module with default info filter and -v/-q CLI flags"
```

---

## Task 2: INFO-инструментирование `parse_image` и `build_image`

**Files:**
- Modify: `crates/uefi-engine/Cargo.toml` (dev-dep `tracing-test`)
- Modify: `crates/uefi-engine/src/parser/image.rs`
- Modify: `crates/uefi-engine/src/builder/mod.rs`

**Interfaces:**
- Consumes: `#[tracing::instrument]` (фича `attributes` из Task 1).
- Produces: INFO-события `"image parsed"` (поля `volumes`, `files`, `size`) и `"image built"` (поле `size`).

- [ ] **Step 1: Добавить dev-dep `tracing-test`**

В `crates/uefi-engine/Cargo.toml`, секция `[dev-dependencies]`, добавить:

```toml
tracing-test = "0.2"
```

- [ ] **Step 2: Написать smoke-тест на `parse_image` (RED)**

В конец `crates/uefi-engine/src/parser/image.rs` добавить тестовый модуль (если файла тестов там нет — создать новый `mod tests`; если уже есть `#[cfg(test)] mod tests` — добавить тест внутрь). Тест:

```rust
#[cfg(test)]
mod logging_tests {
    use super::*;
    use crate::types::ImageMode;

    #[tracing_test::traced_test]
    #[test]
    fn parse_image_emits_milestone() {
        let buf = vec![0xFFu8; 4096];
        let _ = parse_image(&buf, ImageMode::Read, "img-logger", "sess-logger");
        assert!(logs_contain("image parsed"));
        assert!(logs_contain("img-logger"));
    }
}
```

- [ ] **Step 3: Запустить тест — должен падать**

Run: `cargo test -p uefi-engine parser::image::logging_tests`
Expected: FAIL — `assert!(logs_contain("image parsed"))` ложь (событие ещё не эмитится).

- [ ] **Step 4: Реализовать — инструментировать `parse_image`**

В `crates/uefi-engine/src/parser/image.rs` навесить атрибут и добавить событие. Заменить сигнатуру

```rust
pub fn parse_image(
    buf: &[u8],
    mode: ImageMode,
    image_id: &str,
    session_id: &str,
) -> Result<Image, ParserError> {
```

на

```rust
#[tracing::instrument(level = "info", skip(buf), fields(size = buf.len()), err)]
pub fn parse_image(
    buf: &[u8],
    mode: ImageMode,
    image_id: &str,
    session_id: &str,
) -> Result<Image, ParserError> {
```

В конце функции заменить прямой `Ok(Image { ... })` на связывание + событие. Найти конструкцию

```rust
    Ok(Image {
        image_id: image_id.into(),
        session_id: session_id.into(),
        root: FfsNode {
```

(и до закрывающей скобки объекта) и заменить `Ok(Image {` (начало возврата) на `let img = Image {`, а после закрывающей `}` объекта вернуть значение через событие:

```rust
    let img = Image {
        image_id: image_id.into(),
        session_id: session_id.into(),
        root: FfsNode {
            // ... без изменений ...
        },
    };
    tracing::info!(
        volumes = img.root.children.len(),
        files = count_files(&img.root),
        "image parsed"
    );
    Ok(img)
}
```

> Тело объекта `Image { ... }` оставить идентичным исходному — меняется только `Ok(Image {` → `let img = Image {` и добавляется `;` + событие + `Ok(img)`.

Добавить приватную функцию-хелпер `count_files` (сразу после `parse_image`):

```rust
fn count_files(node: &FfsNode) -> usize {
    let mut n = match node.node_type {
        FfsType::File => 1,
        _ => 0,
    };
    for child in &node.children {
        n += count_files(child);
    }
    n
}
```

- [ ] **Step 5: Запустить тест — должен проходить**

Run: `cargo test -p uefi-engine parser::image::logging_tests`
Expected: PASS.

- [ ] **Step 6: Инструментировать `build_image`**

В `crates/uefi-engine/src/builder/mod.rs` заменить

```rust
pub fn build_image(image: &Image) -> Result<Vec<u8>, BuilderError> {
    let mut out = vec![];
    build_node(&image.root, &mut out)?;
    Ok(out)
}
```

на

```rust
#[tracing::instrument(level = "info", skip_all, err)]
pub fn build_image(image: &Image) -> Result<Vec<u8>, BuilderError> {
    let mut out = vec![];
    build_node(&image.root, &mut out)?;
    tracing::info!(size = out.len(), "image built");
    Ok(out)
}
```

- [ ] **Step 7: Проверить крейт**

Run: `cargo test -p uefi-engine`
Expected: все тесты PASS.

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/uefi-engine/Cargo.toml crates/uefi-engine/src/parser/image.rs crates/uefi-engine/src/builder/mod.rs
git commit -m "feat(uefi-engine): instrument parse_image/build_image with INFO milestones"
```

---

## Task 3: DEBUG-инструментирование `ops::*` и `setup::set_item_visibility`

**Files:**
- Modify: `crates/uefi-engine/src/ops.rs`
- Modify: `crates/uefi-engine/src/setup/mod.rs`

**Interfaces:**
- Produces: DEBUG-события milestones: `"marked for removal"`, `"node inserted"`, `"node replaced"`, `"marked for rebuild"`, `"set_item_visibility done"`.

- [ ] **Step 1: Написать smoke-тест на `ops::remove` (RED)**

В существующий `mod tests` в `crates/uefi-engine/src/ops.rs` добавить тест:

```rust
    #[tracing_test::traced_test]
    #[test]
    fn remove_emits_debug_milestone() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        remove(&mut img.root, &t).unwrap();
        assert!(logs_contain("marked for removal"));
    }
```

- [ ] **Step 2: Запустить тест — должен падать**

Run: `cargo test -p uefi-engine ops::tests::remove_emits_debug_milestone`
Expected: FAIL — событие не эмитится.

- [ ] **Step 3: Инструментировать `remove`**

В `crates/uefi-engine/src/ops.rs` заменить

```rust
pub fn remove(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Remove;
    mark_rebuild_to_root_by_path(root, &path);
    Ok(())
}
```

на

```rust
#[tracing::instrument(level = "debug", skip(root), fields(target = ?target), err)]
pub fn remove(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Remove;
    mark_rebuild_to_root_by_path(root, &path);
    tracing::debug!(path = ?path, "marked for removal");
    Ok(())
}
```

- [ ] **Step 4: Инструментировать `insert`**

Заменить

```rust
pub fn insert(
    root: &mut FfsNode,
    target: &Target,
    ffs_bytes: &[u8],
    mode: InsertMode,
) -> Result<(), OpsError> {
    let new_node = parse_ffs_bytes(ffs_bytes).map_err(|_| OpsError::InvalidFfs)?;
    let parent_path = match target {
        Target::Path(p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    match mode {
        InsertMode::Into => {
            let parent = find_mut(root, &parent_path).ok_or(OpsError::NotFound)?;
            parent.children.push(new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
        InsertMode::Before | InsertMode::After => {
            if parent_path.is_empty() {
                return Err(OpsError::InvalidParent);
            }
            let idx = *parent_path.last().unwrap();
            let grandparent_path = &parent_path[..parent_path.len() - 1];
            let grandparent = find_mut(root, grandparent_path).ok_or(OpsError::NotFound)?;
            if idx > grandparent.children.len() {
                return Err(OpsError::InvalidParent);
            }
            let insert_at = if mode == InsertMode::Before {
                idx
            } else {
                idx + 1
            };
            grandparent.children.insert(insert_at, new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
    }
    Ok(())
}
```

на

```rust
#[tracing::instrument(level = "debug", skip(root, ffs_bytes), fields(mode = ?mode, target = ?target), err)]
pub fn insert(
    root: &mut FfsNode,
    target: &Target,
    ffs_bytes: &[u8],
    mode: InsertMode,
) -> Result<(), OpsError> {
    let new_node = parse_ffs_bytes(ffs_bytes).map_err(|_| OpsError::InvalidFfs)?;
    let parent_path = match target {
        Target::Path(p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    match mode {
        InsertMode::Into => {
            let parent = find_mut(root, &parent_path).ok_or(OpsError::NotFound)?;
            parent.children.push(new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
        InsertMode::Before | InsertMode::After => {
            if parent_path.is_empty() {
                return Err(OpsError::InvalidParent);
            }
            let idx = *parent_path.last().unwrap();
            let grandparent_path = &parent_path[..parent_path.len() - 1];
            let grandparent = find_mut(root, grandparent_path).ok_or(OpsError::NotFound)?;
            if idx > grandparent.children.len() {
                return Err(OpsError::InvalidParent);
            }
            let insert_at = if mode == InsertMode::Before {
                idx
            } else {
                idx + 1
            };
            grandparent.children.insert(insert_at, new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
    }
    tracing::debug!(path = ?parent_path, mode = ?mode, "node inserted");
    Ok(())
}
```

- [ ] **Step 5: Инструментировать `replace`**

Заменить

```rust
pub fn replace(
    root: &mut FfsNode,
    target: &Target,
    data: &[u8],
    body_only: bool,
) -> Result<(), OpsError> {
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    if body_only {
        node.children.clear();
        node.body = data.to_vec();
    } else {
        let new_node = parse_ffs_bytes(data).map_err(|_| OpsError::InvalidFfs)?;
        node.header = new_node.header;
        node.body = new_node.body;
        node.tail = new_node.tail;
        node.children = new_node.children;
        node.guid = new_node.guid;
        node.node_type = new_node.node_type;
        node.subtype = new_node.subtype;
        node.parsing_data = new_node.parsing_data;
    }
    node.action = Action::Replace;
    mark_rebuild_to_root_by_path(root, &path);
    Ok(())
}
```

на

```rust
#[tracing::instrument(level = "debug", skip(root, data), fields(target = ?target, body_only), err)]
pub fn replace(
    root: &mut FfsNode,
    target: &Target,
    data: &[u8],
    body_only: bool,
) -> Result<(), OpsError> {
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    if body_only {
        node.children.clear();
        node.body = data.to_vec();
    } else {
        let new_node = parse_ffs_bytes(data).map_err(|_| OpsError::InvalidFfs)?;
        node.header = new_node.header;
        node.body = new_node.body;
        node.tail = new_node.tail;
        node.children = new_node.children;
        node.guid = new_node.guid;
        node.node_type = new_node.node_type;
        node.subtype = new_node.subtype;
        node.parsing_data = new_node.parsing_data;
    }
    node.action = Action::Replace;
    mark_rebuild_to_root_by_path(root, &path);
    tracing::debug!(path = ?path, body_only, "node replaced");
    Ok(())
}
```

- [ ] **Step 6: Инструментировать `rebuild`**

Заменить

```rust
pub fn rebuild(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Rebuild;
    mark_rebuild_to_root_by_path(root, &path);
    Ok(())
}
```

на

```rust
#[tracing::instrument(level = "debug", skip(root), fields(target = ?target), err)]
pub fn rebuild(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Rebuild;
    mark_rebuild_to_root_by_path(root, &path);
    tracing::debug!(path = ?path, "marked for rebuild");
    Ok(())
}
```

- [ ] **Step 7: Запустить тест `remove` — должен проходить**

Run: `cargo test -p uefi-engine ops`
Expected: PASS (включая `remove_emits_debug_milestone`).

- [ ] **Step 8: Инструментировать `setup::set_item_visibility`**

В `crates/uefi-engine/src/setup/mod.rs` заменить

```rust
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), SetupError> {
    let target = crate::parser::target::parse_target(item_id).map_err(|_| SetupError::NotFound)?;
    let path = match &target {
        Target::Path(p) => p.clone(),
        _ => return Err(SetupError::NotFound),
    };
    let mut changed = false;
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| SetupError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(SetupError::NotASetupItem);
        }
        if visible && let Some(scope) = ifr::find_suppress_if_scopes(&node.body).into_iter().next()
        {
            let mut body = node.body.clone();
            ifr::unsuppress(&mut body, &scope);
            node.body = body;
            changed = true;
        }
    }
    if changed {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    Ok(())
}
```

на

```rust
#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id, visible), err)]
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), SetupError> {
    let target = crate::parser::target::parse_target(item_id).map_err(|_| SetupError::NotFound)?;
    let path = match &target {
        Target::Path(p) => p.clone(),
        _ => return Err(SetupError::NotFound),
    };
    let mut changed = false;
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| SetupError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(SetupError::NotASetupItem);
        }
        if visible && let Some(scope) = ifr::find_suppress_if_scopes(&node.body).into_iter().next()
        {
            let mut body = node.body.clone();
            ifr::unsuppress(&mut body, &scope);
            node.body = body;
            changed = true;
        }
    }
    if changed {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(changed, "set_item_visibility done");
    Ok(())
}
```

- [ ] **Step 9: Проверить крейт**

Run: `cargo test -p uefi-engine`
Expected: все тесты PASS.

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: clean.

- [ ] **Step 10: Commit**

```bash
git add crates/uefi-engine/src/ops.rs crates/uefi-engine/src/setup/mod.rs
git commit -m "feat(uefi-engine): instrument ops::* and set_item_visibility with DEBUG milestones"
```

---

## Task 4: INFO-инструментирование gRPC-хендлеров — plain spans (12 методов)

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs`

**Interfaces:**
- Produces: `#[instrument]` spans (имя = имя метода) на 12 хендлерах без событий-результатов: `sessions_list`, `image_close`, `images_list`, `image_status`, `image_save`, `image_nodes_list`, `image_nodes_search`, `image_node_rebuild`, `artifacts_list`, `setup_list_forms`, `setup_list_strings`, `setup_form_set_add`.

> Все 12 изменений однотипны: добавить **одну строку** атрибута над сигнатурой метода. Тело не меняется.

- [ ] **Step 1: Навесить `#[instrument]` на 12 хендлеров**

Для **каждого** из перечисленных методов добавить строку `#[tracing::instrument(skip(self, req), err)]` (или `skip(self, _req)` для методов с параметром `_req`) непосредственно над `async fn ...`. Пример для `sessions_list`:

Было:

```rust
    async fn sessions_list(
        &self,
        _req: Request<SessionsListRequest>,
    ) -> RpcResult<SessionsListResponse> {
```

Стало:

```rust
    #[tracing::instrument(skip(self, _req), err)]
    async fn sessions_list(
        &self,
        _req: Request<SessionsListRequest>,
    ) -> RpcResult<SessionsListResponse> {
```

Полный список (атрибут над каждым):

| Метод | Атрибут |
|-------|---------|
| `sessions_list` | `#[tracing::instrument(skip(self, _req), err)]` |
| `image_close` | `#[tracing::instrument(skip(self, req), err)]` |
| `images_list` | `#[tracing::instrument(skip(self, req), err)]` |
| `image_status` | `#[tracing::instrument(skip(self, req), err)]` |
| `image_save` | `#[tracing::instrument(skip(self, req), err)]` |
| `image_nodes_list` | `#[tracing::instrument(skip(self, req), err)]` |
| `image_nodes_search` | `#[tracing::instrument(skip(self, req), err)]` |
| `image_node_rebuild` | `#[tracing::instrument(skip(self, req), err)]` |
| `artifacts_list` | `#[tracing::instrument(skip(self, req), err)]` |
| `setup_list_forms` | `#[tracing::instrument(skip(self, _req), err)]` |
| `setup_list_strings` | `#[tracing::instrument(skip(self, _req), err)]` |
| `setup_form_set_add` | `#[tracing::instrument(skip(self, req), err)]` |

- [ ] **Step 2: Проверить сборку и тесты**

Run: `cargo test -p uefi-engine`
Expected: все тесты PASS (существующие тесты rpc/server.rs не затронуты по поведению).

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): add INFO instrument spans to plain gRPC handlers"
```

---

## Task 5: INFO-события-результаты на 10 богатых gRPC-хендлерах + финальная проверка

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs`

**Interfaces:**
- Produces: INFO-события-результаты (см. таблицу в спеке): `session_create`, `session_destroy`, `image_open`, `image_node_insert`, `image_node_remove`, `image_node_replace`, `image_node_extract`, `artifact_import`, `artifact_export`, `setup_set_form_visibility`.

> Каждый хендлер получает: (а) атрибут `#[tracing::instrument(skip(self, req), err)]` над сигнатурой, (б) одно явное `tracing::info!(...)` событие перед успешным `Ok(Response::new(...))`.

- [ ] **Step 1: `session_create`**

Добавить атрибут над методом и событие после успешного `create_session`. Было:

```rust
    async fn session_create(
        &self,
        req: Request<SessionCreateRequest>,
    ) -> RpcResult<SessionCreateResponse> {
        let r = req.into_inner();
        let name = if r.name.is_empty() {
            std::env::var("PWD").unwrap_or_default()
        } else {
            r.name
        };
        let (id, tok) = self
            .sm
            .create_session(&name)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(SessionCreateResponse {
            session_id: id,
            token: tok,
        }))
    }
```

Стало:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn session_create(
        &self,
        req: Request<SessionCreateRequest>,
    ) -> RpcResult<SessionCreateResponse> {
        let r = req.into_inner();
        let name = if r.name.is_empty() {
            std::env::var("PWD").unwrap_or_default()
        } else {
            r.name
        };
        let (id, tok) = self
            .sm
            .create_session(&name)
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(name = %name, session_id = %id, "session created");
        Ok(Response::new(SessionCreateResponse {
            session_id: id,
            token: tok,
        }))
    }
```

- [ ] **Step 2: `session_destroy`**

Добавить атрибут и событие. Было:

```rust
    async fn session_destroy(&self, req: Request<SessionDestroyRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm
            .destroy_session(&r.session_id, self.sm.purge_artifacts)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
```

Стало:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn session_destroy(&self, req: Request<SessionDestroyRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm
            .destroy_session(&r.session_id, self.sm.purge_artifacts)
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(session_id = %r.session_id, purged = self.sm.purge_artifacts, "session destroyed");
        Ok(Response::new(Empty {}))
    }
```

- [ ] **Step 3: `image_open`**

Добавить атрибут и событие перед конструированием `Response` (поля уже вычислены: `image_id`, `name`, `root_guid`, `bytes.len()`, `mode`). Было (конец метода):

```rust
        self.images.lock().await.insert(image_id.clone(), img);
        let _ = self.sm.touch(&r.session_id);
        Ok(Response::new(ImageOpenResponse {
            image_id,
            root_guid,
            name,
        }))
```

Стало:

```rust
        self.images.lock().await.insert(image_id.clone(), img);
        let _ = self.sm.touch(&r.session_id);
        tracing::info!(
            name = %name,
            size = bytes.len(),
            image_id = %image_id,
            mode = ?mode,
            root_guid = %root_guid,
            "image opened"
        );
        Ok(Response::new(ImageOpenResponse {
            image_id,
            root_guid,
            name,
        }))
```

И добавить атрибут над сигнатурой: `#[tracing::instrument(skip(self, req), err)]` над `async fn image_open(...)`.

> `image_id` и `name` — `String`, используются по Display (`%`), затем перемещаются в `Response` после лога — корректно.

- [ ] **Step 4: `image_node_insert`**

Хвост метода (после `flush_image`). Было:

```rust
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }
```

Стало:

```rust
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        let source = if !r.artifact_id.is_empty() {
            r.artifact_id.as_str()
        } else {
            r.ffs_path.as_str()
        };
        tracing::info!(
            image_id = %r.image_id,
            target = %r.target,
            mode = ?mode,
            source = %source,
            size = ffs_bytes.len(),
            "artifact inserted"
        );
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }
```

Для доступности `mode` в этом месте — **поднять** вычисление `mode` выше (до блока `let img = self.get_or_load_image(...)`). Было (внутри блока `{ let mut images = self.images.lock().await; ... }`):

```rust
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let mode = match r.mode {
                0 => crate::ops::InsertMode::Into,
                1 => crate::ops::InsertMode::Before,
                2 => crate::ops::InsertMode::After,
                _ => return Err(Status::invalid_argument("bad mode")),
            };
            crate::ops::insert(&mut img_slot.root, &t, &ffs_bytes, mode)
                .map_err(|e| Status::internal(e.to_string()))?;
```

Стало: вынести `mode` и `t` до блока (после `let ffs_bytes = ...`):

```rust
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mode = match r.mode {
            0 => crate::ops::InsertMode::Into,
            1 => crate::ops::InsertMode::Before,
            2 => crate::ops::InsertMode::After,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::ops::insert(&mut img_slot.root, &t, &ffs_bytes, mode)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
```

Добавить атрибут `#[tracing::instrument(skip(self, req), err)]` над `async fn image_node_insert(...)`.

- [ ] **Step 5: `image_node_remove`**

Добавить атрибут и событие. Было (конец метода):

```rust
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(Empty {}))
    }
```

Стало:

```rust
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, "node removed");
        Ok(Response::new(Empty {}))
    }
```

Атрибут `#[tracing::instrument(skip(self, req), err)]` над `async fn image_node_remove(...)`.

- [ ] **Step 6: `image_node_replace`**

Добавить атрибут и событие. Хвост:

```rust
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }
```

Стало:

```rust
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        let source = if !r.artifact_id.is_empty() {
            r.artifact_id.as_str()
        } else {
            r.ffs_path.as_str()
        };
        tracing::info!(
            image_id = %r.image_id,
            target = %r.target,
            body_only = r.body_only,
            source = %source,
            size = data.len(),
            "node replaced"
        );
        Ok(Response::new(ImageNodeResponse { item_id: r.target }))
    }
```

Атрибут над сигнатурой. `data` вычисляется в начале метода — в области видимости.

- [ ] **Step 7: `image_node_extract`**

Событие требует полей ноды (`type`/`subtype`/`guid`). Захватить их в существующем блоке. Было:

```rust
        let img = self.get_or_load_image(&r.image_id).await?;
        let (session_id, bytes) = {
            let session_id = img.session_id.clone();
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let node = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
            let bytes = if r.body_only {
                node.body.clone()
            } else {
                node.header
                    .iter()
                    .chain(node.body.iter())
                    .copied()
                    .collect()
            };
            (session_id, bytes)
        };
        let artifact_id = Uuid::new_v4().to_string();
```

Стало (захватываем `node_type`/`subtype`/`guid`):

```rust
        let img = self.get_or_load_image(&r.image_id).await?;
        let (session_id, bytes, node_ty, node_subtype, node_guid) = {
            let session_id = img.session_id.clone();
            let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let node = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
            let ty = node.node_type;
            let sub = node.subtype;
            let guid = node.guid.clone();
            let bytes = if r.body_only {
                node.body.clone()
            } else {
                node.header
                    .iter()
                    .chain(node.body.iter())
                    .copied()
                    .collect()
            };
            (session_id, bytes, ty, sub, guid)
        };
        let artifact_id = Uuid::new_v4().to_string();
```

И перед `Ok(Response::new(ImageNodeExtractResponse { artifact_id }))` добавить событие (после `insert_artifact(...)`):

```rust
        tracing::info!(
            image_id = %r.image_id,
            target = %r.target,
            body_only = r.body_only,
            ty = ?node_ty,
            subtype = node_subtype,
            guid = ?node_guid,
            size = bytes.len(),
            artifact_id = %artifact_id,
            "artifact extracted"
        );
        Ok(Response::new(ImageNodeExtractResponse { artifact_id }))
```

Атрибут `#[tracing::instrument(skip(self, req), err)]` над сигнатурой.

> `node.node_type` (`FfsType`) и `node.subtype` (`u8`) — `Copy`. `node.guid` (`Option<Guid>`) — `.clone()` для надёжности.

- [ ] **Step 8: `artifact_import`**

Добавить атрибут и событие. Было (конец метода):

```rust
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ArtifactImportResponse { artifact_id }))
    }
```

Стало:

```rust
            .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(
            name = %source,
            size = bytes.len(),
            kind = "imported",
            artifact_id = %artifact_id,
            "artifact imported"
        );
        Ok(Response::new(ArtifactImportResponse { artifact_id }))
    }
```

Атрибут над сигнатурой. `source` и `bytes` вычисляются выше — в области видимости.

- [ ] **Step 9: `artifact_export`**

Добавить атрибут и событие после успешной записи. `size` берём из DB-строки `art.size`. Было:

```rust
    async fn artifact_export(&self, req: Request<ArtifactExportRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let art = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_artifact(&r.artifact_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("artifact not found"))?;
        crate::storage::artifact::write_artifact_to_output(
            &self.data_dir,
            &art.session_id,
            &art.id,
            &r.output_path,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
```

Стало:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn artifact_export(&self, req: Request<ArtifactExportRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let art = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_artifact(&r.artifact_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("artifact not found"))?;
        crate::storage::artifact::write_artifact_to_output(
            &self.data_dir,
            &art.session_id,
            &art.id,
            &r.output_path,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        tracing::info!(
            artifact_id = %r.artifact_id,
            output_path = %r.output_path,
            size = art.size,
            "artifact exported"
        );
        Ok(Response::new(Empty {}))
    }
```

- [ ] **Step 10: `setup_set_form_visibility`**

Добавить атрибут и событие. Было:

```rust
    async fn setup_set_form_visibility(
        &self,
        req: Request<SetupSetFormVisibilityRequest>,
    ) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::setup::set_item_visibility(img_slot, &r.item_id, r.visible)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(Empty {}))
    }
```

Стало:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn setup_set_form_visibility(
        &self,
        req: Request<SetupSetFormVisibilityRequest>,
    ) -> RpcResult<Empty> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::setup::set_item_visibility(img_slot, &r.item_id, r.visible)
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, visible = r.visible, "form visibility set");
        Ok(Response::new(Empty {}))
    }
```

- [ ] **Step 11: Smoke-проверка компиляции всех 10 хендлеров**

Run: `cargo build -p uefi-engine`
Expected: OK. Если ошибка про `mode`/`t` shadowing или move — перепроверить Step 4 (`image_node_insert`).

- [ ] **Step 12: Финальная проверка крейта**

Run: `cargo test -p uefi-engine`
Expected: все тесты PASS.

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean (если не clean — `cargo fmt --all` и закоммитить правки форматирования в этот же коммит).

- [ ] **Step 13: Ручная визуальная проверка (опционально, но рекомендуется)**

В отдельном терминале:
```bash
RUST_LOG=uefi_engine=debug cargo run -p uefi-engine --bin engine -- -v
```
Убедиться, что при работе клиента видны: span enter/exit (`ENTER image_open`/`CLOSE image_open`), INFO-события (`image opened`, `node removed`, `image parsed`), DEBUG-milestones ops (`marked for removal`). Остановить (Ctrl-C).

- [ ] **Step 14: Commit**

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): add INFO result events to rich gRPC handlers"
```

---

## Финальная интеграционная проверка (после всех 5 задач)

- [ ] `cargo test --all` — все крейты PASS.
- [ ] `cargo clippy --all -- -D warnings` — clean.
- [ ] `cargo fmt --all -- --check` — clean.

> Замечание: добавление фичи `attributes` к workspace `tracing` затрагивает все крейты, использующие `tracing.workspace = true` (на данный момент только `uefi-engine`). Поведение остальных крейтов не меняется (макрос `#[instrument]` opt-in).
