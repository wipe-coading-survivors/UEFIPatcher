# CLI topology, proto rename, and image storage — Design

**Date:** 2026-08-10
**Scope:** `uefi-proto`, `uefi-engine`, `uefi-common`, `uefi-cli`, `uefi-tui`, `uefi-gateway`, WebUI
**Related:** `2026-08-09-display-and-search-design.md` (форматирование дерева через `uefi-common::format`)
**Baseline commit:** `0470553bcc579af0eb72075533bc2c73f77d543f`

## Motivation

Ревизия CLI (см. `TODO.md` «Ревизия CLI (2026-08-10)») выявила системные проблемы:

1. **`image list` конфликтует семантически** — это `ListItems` RPC (FFS-узлы внутри образа), а не список открытых образов. Имя сбивает с толку.
2. **Нет `ListImages` RPC** — нельзя узнать, какие образы открыты на сервере; `image switch <id>` бесполезен без знания валидных id.
3. **Конфликт имён «items»** — `image list-items` (FFS-узлы) и `setup list-items` (планировались как IFR-пункты) — одно имя для разных концепций. Текущий `setup list-items` (`commands/setup.rs:22`) регресснул: это просто `list_items` с фильтром `type == 67`, а не IFR-парсинг форм или string-package'ей.
4. **Несоответствие RPC-имён их scope:** `ListItems`/`SearchItems`/`FindItem` работают в image-scope, но не отражают это в имени; параллельно `ListArtifacts`/`ListSessions` работают в session-scope.
5. **Образы не персистятся:** `OpenImage` читает файл с серверной FS, парсит в `HashMap<String, Image>` в памяти. После рестарта engine всё теряется; нельзя реализовать `ImagesList`; модификации (insert/remove/replace/rebuild) живут только in-memory.
6. **`DumpTree` RPC — deprecated:** дублирует `ListItems` + клиентское форматирование. TUI и Gateway должны мигрировать (см. `TODO.md`).
7. **`image find` бесполезен** (TODO endorsed removal): только валидирует существование узла и эхо вернёт target.
8. **UX-пробелы:** `--format`/`--mode` — String вместо ValueEnum; `session init` неявно берёт имя из `PWD`; нет `about:` у подкоманд.

## Goals

1. **CLI topology:** 5 ортогональных top-level ресурсов (session/image/node/artifact/setup) с явным разделением scope-ов.
2. **Proto rename (noun-first):** все RPC приведены к шаблону `<Resource><Action>` / `<Resource><Collection><Action>` для группировки по ресурсу в codegen.
3. **Image storage:** образы персистятся в SQLite (`images` table) + bytes в `data_dir/sessions/<sid>/images/<image_id>.bin`. Write-through на каждую мутацию. Lazy re-load после рестарта engine.
4. **Удаление deprecated-кода:** `DumpTree` RPC + `DumpFormat` enum + `parser::image::dump_tree` + `FindItem` + мёртвые stub'ы в mock-тестах.
5. **UX-чистка:** ValueEnum для `--format`/`--mode`; `--name` у `session init` и `image open`; `about:` для всех подкоманд; `ArgGroup` для `--file|--artifact` у `node insert/replace`.

## Non-goals (перенесены в План B)

- **Реализация IFR form/string extraction** (`SetupListForms` / `SetupListStrings` движения). В Плане A заводим stub'ы RPC, возвращающие `UNIMPLEMENTED`. Полная реализация (включая слияние с `setup_advanced/`) — отдельный spec.
- **Сериализация parsed `FfsNode`** — не делаем; персистим только bytes, re-parse по необходимости.
- **Пагинация `ImageNodesList`** — YAGNI; возврат всех nodes как сегодня.
- **Batch-мутации** в одной транзакции — за рамками; одна мутация = один write-through.

## CLI topology

```
uefi-cli
├── session
│     init [--name <n>] [--force]
│     list
│     destroy
├── image
│     open [--name <n>] [--mode read|write] <path>
│     switch <image_id>
│     close [<image_id>]                # default: active; destroy на сервере (row + bytes + in-memory) +
│                                       # чистит .uefipatcher если image_id == active
│     save <output_path>                # export: copy из data_dir в пользовательский путь
│     list                              # ImagesList: образы текущей сессии
│     status                            # active image_id + метаданные с сервера (валидация)
├── node                                # все операции — над active image (из .uefipatcher)
│     list [--filter <expr>] [--tree]   # --tree: формат legend+дерево (через uefi-common::format)
│     search <query> [--mode ...] [--limit N]
│     insert <target> (--file <path> | --artifact <id>) [--mode into|before|after]
│     remove <target>
│     replace <target> (--file <path> | --artifact <id>) [--body-only]
│     rebuild <target>
│     extract <target> [--body-only]    # возвращает artifact_id
├── artifact
│     list                              # ArtifactsList текущей сессии
│     import <path>                     # file → artifact
│     export <artifact_id> [output_path]
└── setup                               # все операции — над active image
      form
        list                            # SetupListForms (stub в Плане A)
        set-visibility <form_id> [--visible|--hidden]
      string
        list                            # SetupListStrings (stub в Плане A)
```

**Solution notes:**

- `node list --tree` заменяет бывший `image dump` (tree+legend); без флага — плоский вывод (бывший `image list` items).
- `node insert`/`node replace`: `ArgGroup("source")` с `--file <path> | --artifact <id>` (взаимоисключающие).
- `node extract` остаётся в `node` (он работает с node, возвращает artifact_id); session-scoped ops (`list`/`import`/`export`) — в `artifact`.
- `image close` без arg = active; с arg = указанный (если он active, попутно чистится `.uefipatcher`).
- `image save <output_path>` — обязательный arg; **всегда делает `build_image` из in-memory** (не copy из data_dir). Причина: если прошлый `flush_image` упал, in-memory обгоняет disk; copy из disk потерял бы изменения.
- Глобальный `--format` и `--mode` (где есть) — `clap::ValueEnum`, валидация в clap, не в runtime.

## Proto contract (noun-first, full rename)

```protobuf
service EngineService {
  // Session (session-scoped)
  rpc SessionCreate(SessionCreateRequest)   returns (SessionCreateResponse);
  rpc SessionDestroy(SessionDestroyRequest) returns (Empty);
  rpc SessionsList(SessionsListRequest)     returns (SessionsListResponse);

  // Image (session-scoped resource)
  rpc ImageOpen(ImageOpenRequest)   returns (ImageOpenResponse);
  rpc ImageClose(ImageCloseRequest) returns (Empty);
  rpc ImagesList(ImagesListRequest) returns (ImagesListResponse);
  rpc ImageSave(ImageSaveRequest)   returns (Empty);
  rpc ImageStatus(ImageStatusRequest) returns (ImageStatusResponse);

  // ImageNode (image-scoped; mutations write-through to data_dir)
  rpc ImageNodesList(ImageNodesListRequest)      returns (ImageNodesResponse);
  rpc ImageNodesSearch(ImageNodesSearchRequest)  returns (ImageNodesResponse);
  rpc ImageNodeInsert(ImageNodeInsertRequest)    returns (ImageNodeResponse);
  rpc ImageNodeRemove(ImageNodeRemoveRequest)    returns (Empty);
  rpc ImageNodeReplace(ImageNodeReplaceRequest)  returns (ImageNodeResponse);
  rpc ImageNodeRebuild(ImageNodeRebuildRequest)  returns (Empty);
  rpc ImageNodeExtract(ImageNodeExtractRequest)  returns (ImageNodeExtractResponse);

  // Artifact (session-scoped resource)
  rpc ArtifactsList(ArtifactsListRequest)    returns (ArtifactsListResponse);
  rpc ArtifactImport(ArtifactImportRequest) returns (ArtifactImportResponse);
  rpc ArtifactExport(ArtifactExportRequest) returns (Empty);

  // Setup (image-scoped; IFR)
  rpc SetupListForms(SetupListFormsRequest)                 returns (SetupListFormsResponse);
  rpc SetupSetFormVisibility(SetupSetFormVisibilityRequest) returns (Empty);
  rpc SetupListStrings(SetupListStringsRequest)             returns (SetupListStringsResponse);
  rpc SetupFormSetAdd(SetupFormSetAddRequest)               returns (SetupFormSetAddResponse);
}
```

### Rename map (old → new)

| Old RPC | New RPC | Поля |
|---|---|---|
| `CreateSession` | `SessionCreate` | без изм. |
| `DestroySession` | `SessionDestroy` | без изм. |
| `ListSessions` | `SessionsList` | без изм. |
| `OpenImage` | `ImageOpen` | `image_path` → `path`; **+ `name`** в request, **+ `name`** в response |
| — (new) | `ImageClose` | `{ image_id }` |
| — (new) | `ImagesList` | `{ session_id } → { images: [ImageInfo] }` |
| `SaveImage` | `ImageSave` | без изм. |
| — (new) | `ImageStatus` | `{ image_id } → { info: ImageInfo }` |
| `ListItems` | `ImageNodesList` | без изм. (message rename only) |
| `SearchItems` | `ImageNodesSearch` | без изм. |
| `Insert` | `ImageNodeInsert` | без изм. |
| `Remove` | `ImageNodeRemove` | без изм. |
| `Replace` | `ImageNodeReplace` | без изм. |
| `Rebuild` | `ImageNodeRebuild` | без изм. |
| `ExtractArtifact` | `ImageNodeExtract` | без изм. |
| `ListArtifacts` | `ArtifactsList` | без изм. |
| `ImportArtifact` | `ArtifactImport` | `file_path` → `path` |
| `ExportArtifact` | `ArtifactExport` | без изм. |
| — (new) | `SetupListForms` | `{ image_id } → { forms: [FormInfo] }` |
| `SetSetupItemVisibility` | `SetupSetFormVisibility` | без изм. (rename only) |
| — (new) | `SetupListStrings` | `{ image_id } → { strings: [StringInfo] }` |
| `AddSetupFormSet` | `SetupFormSetAdd` | без изм. |
| ~~`DumpTree`~~ | — | **удаляется** |
| ~~`FindItem`~~ | — | **удаляется** |

### New / changed messages

```protobuf
message ImageOpenRequest {
  string session_id = 1;
  string path = 2;        // rename с image_path
  ImageMode mode = 3;
  string name = 4;        // NEW: "" → fallback на file_name(path)
}
message ImageOpenResponse {
  string image_id = 1;
  string root_guid = 2;
  string name = 3;        // NEW: подтверждённое имя
}

message ImageInfo {                   // NEW
  string image_id = 1;
  string name = 2;
  string path = 3;         // исходный путь (бывший source_path)
  ImageMode mode = 4;
  uint64 size = 5;
  int64 created_at = 6;
  int64 last_activity = 7;
}
message ImagesListRequest   { string session_id = 1; }
message ImagesListResponse  { repeated ImageInfo images = 1; }
message ImageCloseRequest   { string image_id = 1; }
message ImageStatusRequest  { string image_id = 1; }
message ImageStatusResponse { ImageInfo info = 1; }

message SetupListFormsRequest  { string image_id = 1; }
message FormInfo {                                     // NEW
  string form_id = 1;       // path-target секции (для setup set-visibility / target.rs)
  string formset_guid = 2;  // GUID FormSet
  uint32 form_id_ifr = 3;   // IFR FormId
  string title = 4;         // из string-package по title-string-id
  bool visible = 5;
}
message SetupListFormsResponse { repeated FormInfo forms = 1; }

message SetupListStringsRequest { string image_id = 1; }
message StringInfo {                                   // NEW
  string language = 1;      // IFR language identifier
  uint32 string_id = 2;
  string text = 3;
}
message SetupListStringsResponse { repeated StringInfo strings = 1; }

message ArtifactImportRequest {
  string session_id = 1;
  string path = 2;         // rename с file_path
}
```

**SetupListForms / SetupListStrings в Плане A — stub'ы `UNIMPLEMENTED`.** Реализация IFR form/string extraction — План B.

## Image storage architecture

### SQLite schema (расширение `storage/schema.rs`)

```sql
CREATE TABLE IF NOT EXISTS images (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    mode          INTEGER NOT NULL,    -- 0=Read, 1=Write
    size          INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_images_session ON images(session_id);
```

### Storage layout

```
<data_dir>/
└── sessions/
    └── <session_id>/
        ├── artifacts/
        │   └── <artifact_id>          (файлы артефактов — без изм.)
        └── images/
            └── <image_id>.bin         (write-through bytes)
```

### Db API (расширение `storage/mod.rs`)

```rust
impl Db {
    pub fn insert_image(&self, id: &str, session_id: &str, name: &str,
                        path: &str, mode: i64, size: i64) -> Result<()>;
    pub fn get_image(&self, id: &str) -> Result<Option<ImageRow>>;
    pub fn list_images(&self, session_id: &str) -> Result<Vec<ImageRow>>;
    pub fn touch_image(&self, id: &str) -> Result<()>;
    pub fn delete_image(&self, id: &str) -> Result<()>;
    pub fn delete_images_for_session(&self, session_id: &str) -> Result<()>;
}

pub struct ImageRow {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub path: String,
    pub mode: i64,
    pub size: i64,
    pub created_at: i64,
    pub last_activity: i64,
}
```

### ImageOpen handler (новая логика)

1. `fs::read(&req.path)` → bytes
2. `image_id = Uuid::new_v4().to_string()`
3. `name = if req.name.is_empty() { Path::new(&req.path).file_name().to_string() } else { req.name.clone() }`
4. `parse_image(&bytes, mode, &image_id, &session_id)` → in-memory `Image`
5. `atomic_write(data_dir/sessions/<sid>/images/<image_id>.bin, &bytes)` — через tmp + rename
6. `db.insert_image(image_id, session_id, name, req.path, mode_i64, bytes.len())`
7. `images.lock().await.insert(image_id.clone(), img)`
8. `sm.touch(session_id)`
9. Вернуть `ImageOpenResponse { image_id, root_guid, name }`

### Write-through (helper в `rpc/server.rs`)

```rust
async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
    let (bytes, session_id) = {
        let images = self.images.lock().await;
        let img = images.get(image_id).ok_or_else(||
            Status::not_found("image not found"))?;
        (crate::builder::build_image(img)?, img.session_id.clone())
    };
    let path = self.data_dir.join("sessions").join(&session_id)
               .join("images").join(format!("{image_id}.bin"));
    atomic_write(&path, &bytes)?;
    self.sm.db.lock().unwrap().touch_image(image_id)?;
    Ok(())
}
```

Каждый mutation-handler после успешной операции вызывает `self.flush_image(&image_id).await?`:
- `image_node_insert`, `image_node_remove`, `image_node_replace`, `image_node_rebuild`
- `setup_set_form_visibility`, `setup_form_set_add`

`image_save` (бывший `save_image`) — **всегда делает `build_image` из in-memory** и пишет в `req.output_path` (как сегодня). НЕ copy из data_dir: если прошлый `flush_image` упал, in-memory обгоняет disk, и copy потеряло бы изменения. data_dir — durability-кэш для lazy re-load после рестарта, не source of truth для `save`.

**Компромисс write-through:** если мутация в памяти успешна, но `flush_image` падает (диск full / I/O), возвращаем ошибку клиенту, но in-memory состояние уже изменено. Следующая успешная мутация (через свой `flush_image`) запишет cumulative-состояние. `image_save` всегда выведет актуальное in-memory состояние. После рестарта engine lazy re-load вернёт состояние с последнего **успешного** flush (последняя отвалившаяся мутация потеряна). Это сознательный trade-off; альтернатива (rollback мутации в дереве) требует deep clone перед каждой операцией.

### Lazy re-load (helper в `rpc/server.rs`)

```rust
async fn get_or_load_image(&self, image_id: &str) -> Result<Image, Status> {
    {
        let images = self.images.lock().await;
        if let Some(img) = images.get(image_id) {
            return Ok(img.clone());
        }
    }
    // Cache miss — load from disk
    let row = self.sm.db.lock().unwrap().get_image(image_id)?
        .ok_or_else(|| Status::not_found("image not found"))?;
    let path = self.data_dir.join("sessions").join(&row.session_id)
               .join("images").join(format!("{image_id}.bin"));
    let bytes = fs::read(&path)?;
    let mode = if row.mode == 1 { ImageMode::Write } else { ImageMode::Read };
    let img = parse_image(&bytes, mode, image_id, &row.session_id)?;
    self.images.lock().await.insert(image_id.into(), img.clone());
    Ok(img)
}
```

Все handler'ы, которым нужен только чтение образа (`image_nodes_list`, `image_nodes_search`, `image_status`, `setup_list_forms/strings`), используют `get_or_load_image` вместо прямого `images.lock().await.get(...)`.

Mutation handler'ы берут `Arc::clone` через `get_or_load_image`, затем делают `images.lock().await.get_mut(image_id)` для применения операции, затем `flush_image`.

### ImageClose handler

1. `db.delete_image(image_id)` (FK CASCADE чистит row)
2. `fs::remove_file(data_dir/.../<image_id>.bin)` (best-effort)
3. `images.lock().await.remove(image_id)`
4. Return `Empty`

### Session lifecycle

- `SessionManager::destroy_session(id, purge_files)`:
  - `db.delete_session(id)` уже делает cascade (FK ON DELETE CASCADE на `images` и `artifacts`).
  - Если `purge_files` — рекурсивно удалить `data_dir/sessions/<id>/` (включая `images/` и `artifacts/`). Расширить существующий cleanup (сегодня чистит только `artifacts/` через cascade таблицы).
- GC loop (existing) чистит истекшие сессии тем же путём.

### Touch semantics

- `touch_image` обновляет `last_activity` при каждом обращении через `get_or_load_image` и при `flush_image`.
- `SessionManager::touch(session_id)` вызывается после каждой image-op (как сегодня).

## Client migration impact

### uefi-proto

- Полный rewrite `proto/engine.proto` (rename всех RPC + новые messages + удаление `DumpTree`/`FindItem`/`DumpFormat`).
- codegen (`build.rs`) автоматически обновит типы в Rust.

### uefi-engine

- `rpc/server.rs`: rename всех handler'ов; новые handler'ы (`image_close`, `images_list`, `image_status`, `setup_list_forms`, `setup_list_strings` — последние два stub'ы); write-through `flush_image`; lazy-load `get_or_load_image`; рефакторинг существующих handler'ов через helper'ы.
- `parser/image.rs`: удалить `dump_tree` и `DumpFormat` enum (если ещё есть).
- `storage/mod.rs` + `schema.rs`: добавить `images` table + CRUD (`ImageRow`, insert/get/list/touch/delete/delete_for_session).
- `session.rs`: расширить `destroy_session` чтобы чистить `images/` директорию (если `purge_files=true`).
- `types.rs`: без изменений. Метаданные (`name`/`path`/`size`/`created_at`/`last_activity`) живут **только в DB** (`images` table); in-memory `Image` остаётся минимальным (`image_id`, `session_id`, `root`, `mode`) — как сегодня. Handler'ы `images_list`/`image_status` джойнят метаданные из DB при формировании ответа.
- `setup/mod.rs`: добавить stub-функции `list_forms`, `list_strings` (возвращают `unimplemented!()` или empty + warning), либо на уровне RPC handler'ов возвращать `UNIMPLEMENTED`.

### uefi-common

- `state.rs`: без изменений (struct `State` остаётся; `active_image_id` хранит image_id).

### uefi-cli

- `main.rs`: полный rewrite `Cmd` enum (5 top-level), все `Subcommand` enum'ы, все dispatch arms.
- `commands/`: новый `node.rs`; новый `artifact.rs`; удаление `edit.rs`; рефакторинг `image.rs`, `session.rs`, `setup.rs`.
- `client.rs`: rename всех методов под новые RPC + добавление `images_list`, `image_close`, `image_status`, `setup_list_forms`, `setup_list_strings`.
- `output.rs`: добавить `print_image_info`, `print_images_list`, `print_forms`, `print_strings`, `print_image_status`; обновить `print_find` удалить.

### uefi-tui

- `commands.rs`: выкинуть `dump_tree`-вызов и `parse_tree_dump`, заменить на `image_nodes_list` + локальный рендер через `uefi-common::format` (см. spec `2026-08-09-display-and-search`).
- Все client-методы → rename.
- Stub'ы в mock-тестах обновить.

### uefi-gateway

- `client.rs`: rename всех методов под новые RPC.
- `routes/image.rs`: rename маршрутов: `/api/v1/image/:id/dump` → `/api/v1/image/:id/nodes`; удалить `/api/v1/image/:id/dump` и `/api/v1/image/:id/dump/ws`.
- `routes/mod.rs`: update таблицы маршрутов; добавить `/api/v1/images` (ImagesList), `/api/v1/image/:id/forms`, `/api/v1/image/:id/strings`.
- `routes/setup.rs`: добавить `forms`, `strings` handlers.
- `routes/upload.rs`: без изменений.
- `routes/ws.rs`: удалить (только dump_ws там был).

### WebUI

- Обновить все fetch-вызовы под новые маршруты (`image Nodes list`, `images list`, `forms list`, `strings list`).

## UX cleanups (включены в План A)

- `clap::ValueEnum` для `--format` (`Json|Text|Tsv`), `--mode` у `image open` (`Read|Write`), `--mode` у `node insert` (`Into|Before|After`).
- `session init --name <name>` с fallback на `PWD` (если не задан).
- `image open --name <name>` с fallback на `file_name(path)`.
- `node insert`/`node replace`: `ArgGroup("source")` с `--file <path>` и `--artifact <id>` (взаимоисключающие, required).
- `about:` для всех subcommands; `help:` для всех args.
- `node list --tree` flag (бывший `image dump` формат).
- Имена подкоманд в исходнике — snake_case (`ListItems`, `SetVisibility`); clap автоматически конвертирует в kebab-case для CLI (`list-items`, `set-visibility`). Единый стиль во всех subcommand enum'ах.

## Migration strategy

Breaking change, all-in-one coordinated update (всё в одном repo):
1. Пройти по плану A последовательно (TDD per task), правя proto → uefi-engine → uefi-common → uefi-cli → uefi-tui → uefi-gateway → WebUI.
2. После каждого шага — `cargo test --all`, `cargo clippy --all -- -D warnings`, `cargo fmt --all -- --check`.
3. Один финальный squash-or-multi-commit по плану; критично что все клиенты компилируются вместе (нет stale-клиентов с разнонаправленными proto-именами).

## Risks & open questions

| Риск | Mitigation |
|---|---|
| Write-through дорог на больших образах (16MB) | `build_image` уже не медленный; TUI можно добавить `--deferred` в будущем. |
| Crash между mutation и flush | In-memory mutated, disk not — клиент видит ошибку. `image_save` всегда строит из in-memory, так что пользователь может сохранить актуальное состояние. После рестарта вернётся последний успешный flush. |
| Lazy re-load race condition (между проверкой map и вставкой) | Двойная блокировка; в тесте проверить идемпотентность. |
| Migration ломает все клиенты одновременно | Всё в одном repo; координированный коммит по плану A. |
| `setup form/string list` — stub'ы возвращают UNIMPLEMENTED | Завести TODO-ссылку на План B; CLI-команды присутствуют, при вызове возвращают ошибку от сервера. |

## Decomposition

- **План A (этот spec → план):** CLI topology + proto rename + image storage (write-through) + UX cleanups + миграция всех клиентов на новые RPC + stub'ы setup forms/strings.
- **План B (отдельный spec, future):** IFR form/string extraction — реализация `SetupListForms`/`SetupListStrings` движком + слияние с `setup_advanced/`. Завести запись в `TODO.md` со ссылкой на baseline-коммит для onboarding.
