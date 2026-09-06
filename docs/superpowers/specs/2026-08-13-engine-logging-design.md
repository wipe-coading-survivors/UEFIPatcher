# Логирование в engine — Design

**Date:** 2026-08-13
**Scope:** `uefi-engine` (`bin/engine.rs`, `rpc/server.rs`, `ops.rs`, `parser/image.rs`, `builder/`, `setup.rs`)
**Related:** `2026-07-22-uefi-engine-design.md`, `TODO.md` (поведенческие дефекты вида «удаление не сработало на 1/13»).
**Out of scope:** `uefi-gateway` (HTTP-слой), CLI, TUI, WebUI — отдельным циклом.

## Motivation

В движке есть отдельные вкрапления `tracing` (`session.rs:85,87` — GC; `parser/image.rs:52,123`,
`parser/section.rs:44,119,139` — warn при парсинге/декомпрессии), но **пользователь ни разу
не видел сообщений**, а при сбоях (напр. «удаление не сработало даже на 1/13») невозможно
понять, что произошло.

### Root cause (две причины)

1. **Подписчик молчит.** `bin/engine.rs:25-27` инициализирует
   `EnvFilter::from_default_env()`. Если `RUST_LOG` не задан — фильтр пустой, и tracing
   отбрасывает **всё**, включая существующие `info!`/`debug!`. Существующие логи реально
   генерируются, но никогда не доходят до вывода.
2. **Логов мало и не там.** gRPC-хендлеры (`rpc/server.rs`, ~20 методов) и публичные ops
   движка (`ops.rs`, `parse_image`, `build_image`, `setup::set_item_visibility`) не
   содержат **ни одного** лога entering/exiting/«что сделано».

## Architecture

Два инструментируемых слоя (по результатам обсуждения):

| Слой | Файлы | Уровень | Что логируется |
|------|-------|---------|----------------|
| **gRPC-хендлеры** | `rpc/server.rs` (~20 методов `session_*`, `image_*`, `image_node_*`, `artifact_*`, `setup_*`) | `INFO` | entering/exiting через `#[instrument]` + структурное событие-результат «что сделано» |
| **Ключевые ops движка** | `ops.rs` (`insert/remove/replace/rebuild`), `parser/image.rs` (`parse_image`), `builder/` (`build_image`), `setup.rs` (`set_item_visibility`) | `DEBUG` (ops, setup) / `INFO` (parse_image, build_image) | entering/exiting через `#[instrument]` + milestones |

**Принципы:**

- Engine-функции под instrument добавляют **только** `#[instrument]` и опциональные
  milestones. Внутренний парсер/секции/таргеты **не трогаем** (выбранный scope «только
  ключевые ops»).
- Существующие `warn!` в `parser/image.rs`/`parser/section.rs` (сигналы о битых данных) и
  `info!` GC в `session.rs` **остаются как есть**.
- Структурные поля (имя/размер/тип/guid) берутся там, где они уже есть — преимущественно в
  RPC-хендлере и в `FfsNode` (`types.rs:71-86`).
- `RUST_LOG` всегда перекрывает CLI-флаги (явное преимущество для тонкой настройки и тестов).

### Зависимость `#[instrument]`

Workspace-dep `tracing = "0.1"` сейчас **без фичи `attributes`** — `#[instrument]`
недоступен. Меняем в `Cargo.toml`:

```toml
tracing = { version = "0.1", features = ["attributes"] }
```

Это распространяется на все крейты через `.workspace = true`.

## Дефолт verbose + CLI-флаги

`bin/engine.rs` — новый `Args` и инициализация подписчика:

```rust
#[derive(Parser)]
struct Args {
    // ... существующие поля ...
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn verbosity_to_filter(verbose: u8, quiet: bool) -> tracing_subscriber::EnvFilter {
    let default = match (quiet, verbose) {
        (true, _)       => "error",
        (false, 0)      => "uefi_engine=info,warn",
        (false, 1)      => "uefi_engine=debug,warn",
        (false, _)      => "uefi_engine=trace,warn",
    };
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default))
}
```

`main()`:

```rust
let filter = verbosity_to_filter(args.verbose, args.quiet);
let span_events = if args.verbose >= 1 {
    tracing_subscriber::fmt::format::FmtSpan::ENTER | FmtSpan::CLOSE
} else {
    tracing_subscriber::fmt::format::FmtSpan::NONE
};
tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_span_events(span_events)
    .init();
```

**Семантика:**

- **база** (`RUST_LOG` не задан, флагов нет): `uefi_engine=info,warn` — видны INFO gRPC +
  parse/build milestones, шум зависимостей подавлен. «Из коробки» видно, что движок делает.
- `-v`: `uefi_engine=debug,warn` — добавляются entering/exiting ops + setup milestones +
  явные span-границы (ENTER/CLOSE) для разбора «что вызвало что».
- `-vv`: `uefi_engine=trace,warn`.
- `-q`: `error` (минимум).
- `RUST_LOG=...` всегда побеждает (через `try_from_default_env().unwrap_or_else(...)`).

**Уровни не конфликтуют:** `trace < debug < info < warn < error`; установка уровня включает
его **и всё более серьёзное** (`info` ⊇ `warn`). Голый `warn` — catch-all fallback для
крейтов без явной директивы (tonic, h2, hyper, rusqlite, tokio). `uefi_proto` в дефолте
**отсутствует** (крейт не эмитит логов — это просто `tonic::include_proto!`).

## Структура spans и полей

### gRPC-хендлеры (INFO)

`#[instrument]` создаёт span по имени метода + ключевым идентификаторам запроса. `err`
автоматически залогирует `tonic::Status` при ошибке. В `fields(...)` — только
идентификаторы (session_id/image_id/target), без больших тел запроса.

Пример:

```rust
#[tracing::instrument(skip(self), fields(image=%r.image_id), err)]
async fn image_node_remove(&self, req: Request<ImageNodeRemoveRequest>) -> RpcResult<Empty> {
    let r = req.into_inner();
    // ... существующая логика ...
    crate::ops::remove(&mut img_slot.root, &t) ...?;
    tracing::info!(target = %r.target, "node removed");
    Ok(Response::new(Empty {}))
}
```

**События-результаты «что сделано» (INFO):**

| Метод | Событие | Поля |
|-------|---------|------|
| `session_create` | session created | `name, session_id` |
| `session_destroy` | session destroyed | `session_id, purged` |
| `image_open` | image opened | `name, size, image_id, mode, root_guid` |
| `image_node_insert` | artifact inserted | `image_id, target, mode, source(artifact_id/ffs_path), size` |
| `image_node_remove` | node removed | `image_id, target` |
| `image_node_replace` | node replaced | `image_id, target, body_only, source(artifact_id/ffs_path), size` |
| `image_node_extract` | artifact extracted | `image_id, target, body_only, type, subtype, guid, size, artifact_id` |
| `artifact_import` | artifact imported | `name(source), size, kind, artifact_id` |
| `artifact_export` | artifact exported | `artifact_id, output_path, size` |
| `setup_set_form_visibility` | form visibility set | `image_id, item_id, visible` |
| `image_close`, `images_list`, `image_status`, `image_nodes_list`, `image_nodes_search`, `image_node_rebuild`, `artifacts_list`, `image_save`, `setup_list_forms`, `setup_list_strings`, `setup_form_set_add` | span enter/exit без доп. события | — |

### Ключевые ops движка

`parse_image` и `build_image` — на **INFO** (тяжёлые операции с полезными агрегатами, без
собственного RPC-результата; INFO заполняет пробел):

```rust
#[tracing::instrument(level = "info", skip(bytes), fields(size=bytes.len()))]
pub fn parse_image(...) -> Result<Image, ParserError> {
    // ...
    tracing::info!(volumes, files, "image parsed");
    Ok(img)
}

#[tracing::instrument(level = "info", skip(root))]
pub fn build_image(root: &FfsNode) -> Result<Vec<u8>, ...> {
    // ...
    tracing::info!(size = bytes.len(), "image built");
    Ok(bytes)
}
```

`ops::{insert,remove,replace,rebuild}` и `setup::set_item_visibility` — на **DEBUG**
(детальный milestone по target; RPC-слой уже логирует результат на INFO):

```rust
#[tracing::instrument(level = "debug", skip(root), fields(target=?target), err)]
pub fn remove(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> { ... }

#[tracing::instrument(level = "debug", skip(root, data), fields(target=?target, size=data.len()), err)]
pub fn replace(root: &mut FfsNode, target: &Target, data: &[u8], body_only: bool) -> Result<(), OpsError> { ... }
```

### Про «имя» в extract

У `FfsNode` (`types.rs:71-86`) **нет поля `name`** — только `guid`/`node_type`/`subtype`/
`header`+`body`+`tail` (размеры). Поэтому в логе `image_node_extract` вместо имени берём
`guid` (реальный идентификатор ноды) + `type`/`subtype`/`size`. «Имя» есть только у
импортируемых артефактов (source-filename) — там оно и попадает в лог `artifact_import`.

## Формат и вывод

- Формат: `fmt()` по умолчанию (человекочитаемый, с timestamp). JSON/structured — **не
  сейчас** (YAGNI; добавим слой при появлении потребности в сборе в систему).
- Вывод: `stderr` (дефолт tracing-subscriber). Файл/ротация — out of scope.
- Span-границы (`FmtSpan::ENTER | FmtSpan::CLOSE`) включаются только на `-v` и выше, чтобы
  не спамить в базовом режиме.

## Testing

- **Юнит-тест `verbosity_to_filter`:** проверяет, что `(false,0)→"uefi_engine=info,warn"`,
  `(false,1)→"uefi_engine=debug,warn"`, `(false,2+)→"uefi_engine=trace,warn"`,
  `(true,_)→"error"`. Чистая функция, тривиально тестируется.
- **Smoke на ключевых RPC:** для 2-3 методов (open/remove/extract) — тест, проверяющий
  эмит события с ожидаемым полем (напр. `target`), через in-memory subscriber
  (`tracing-test` или ручной `InMemorySink` через `tracing-subscriber` в dev-deps).
  Остальное покрывается визуально (`RUST_LOG=uefi_engine=debug cargo run --bin engine`).
- **Регресс:** `cargo test -p uefi-engine`, `cargo clippy -p uefi-engine -- -D warnings`,
  `cargo fmt --all -- --check`.

## Decisions log

- **`#[instrument]` макрос** (вкл. `attributes` feature) вместо ручных spans — лаконично,
  идиоматично, авто-enter/exit + err-поля. (Альтернативы: ручные `info_span!` — шаблонный
  код в ~30 методах; только явные события — теряем timing/иерархию spans.)
- **Дефолт `info` + CLI-флаги** вместо чистого `from_default_env()` — движок «из коробки»
  показывает активность, `-v` включает DEBUG-трассировку ops. `RUST_LOG` остаётся мастером.
- **Scope DEBUG — только ключевые ops** (ops + parse_image + build_image + setup), не все
  публичные функции — чистый сигнал, минимум шума.
- **parse_image/build_image на INFO** (не DEBUG) — тяжёлые операции с агрегатами без
  собственного RPC-результата; INFO заполняет пробел.
- **`uefi_proto` убран из дефолта** — крейт не эмитит логов.
- **JSON-формат/файл-вывод — out of scope** (YAGNI).
