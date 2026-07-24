# UEFIPatcher — Реализация (цикл 1: UEFI Engine)

## Структура проекта

```
uefipatcher/
├── Cargo.toml                              # workspace manifest (edition 2024)
├── crates/
│   ├── uefi-proto/                         # protobuf + tonic-генерация
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   ├── proto/engine.proto              # EngineService (17 RPC), сообщения, enums
│   │   └── src/lib.rs
│   ├── uefi-common/                        # общий крейт (state + error)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── state.rs                    # State, read_state, write_state, resolve_sock
│   │       └── error.rs                    # AppError, ExitCode, print_error
│   ├── uefi-engine/                        # ядро
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                      # re-export модулей
│   │       ├── types.rs                    # FfsNode, Image, Target, Guid wrapper (uguid)
│   │       ├── ffs.rs                      # FFS-константы, checksums (wrapping arithmetic)
│   │       ├── parser/                     # парсинг UEFI → FfsNode (binrw)
│   │       │   ├── mod.rs                  # ParserError
│   │       │   ├── volume.rs              # FirmwareVolume
│   │       │   ├── file.rs                # FFS-файлы
│   │       │   ├── section.rs             # секции (PE32, GUIDed, compressed, raw)
│   │       │   ├── image.rs              # parse_image, dump_tree, list_items
│   │       │   └── target.rs             # парсинг Target-строки
│   │       ├── decompress.rs              # Tiano/LZMA декомпрессия
│   │       ├── builder.rs                 # сборка дерева → байты
│   │       ├── ops.rs                     # insert/remove/replace/rebuild
│   │       ├── setup.rs                   # IFR-парсинг (r_efi::hii), SetSetupItemVisibility
│   │       ├── session.rs                 # менеджер сессий + GC (--purge-artifacts)
│   │       ├── storage/                   # SQLite + файлы артефактов
│   │       │   ├── mod.rs                 # Db, SessionRow, ArtifactRow
│   │       │   ├── schema.rs             # CREATE TABLE (sessions с name, artifacts с source)
│   │       │   └── artifact.rs           # extract/import/export файловых операций
│   │       ├── rpc/                       # gRPC-сервер
│   │       │   ├── mod.rs
│   │       │   ├── auth.rs               # проверка токена
│   │       │   └── server.rs             # EngineService impl, serve()
│   │       └── bin/
│   │           └── engine.rs              # engine binary (clap CLI, --purge-artifacts)
│   └── uefi-cli/                          # CLI-минимум (depends on uefi-common)
│       ├── Cargo.toml
│       └── src/main.rs
├── docker/
│   ├── rust-builder.containerfile          # базовый Rust builder (fedora:44)
│   ├── engine.containerfile               # движок на базе rust-builder
│   └── docker-compose.yml
├── roadmap.md
├── PLANNING.md
├── IMPLEMENTATION.md
└── TESTING.md
```

## Модули и их ответственность

### uefi-proto
- protobuf-схема `EngineService` (17 RPC) и tonic-генерация типов.
- Зависимости: `tonic`, `prost`.

### uefi-common
- `state.rs` — `State` (session_id, token, active_image_id, sock_path), `read_state()`, `write_state()`, `resolve_sock()`. Файл `.uefipatcher` (TOML) в CWD.
- `error.rs` — `AppError`, `ErrKind`, `ExitCode`, `print_error()`.
- Зависимости: `serde`, `toml`, `directories`, `thiserror`.

### types
- `FfsNode`, `Image`, `Target`, `ImageMode`.
- Guid через `uguid::Guid` (re-export). UPPERCASE wrapper: `fn guid_to_upper_string(g: &Guid) -> String`.
- Зависимости: `uguid`, `num_enum`.

### ffs
- FFS-константы (section types, FVH signature), well-known GUIDs (`tiano_guid()`, `lzma_guid()`, etc. через `Guid::try_parse`).
- Checksum-хелперы: `calculate_checksum8`, `calculate_checksum16` (wrapping arithmetic).
- `is_large_section`, `is_large_ffs`, `section_size`, `ffs_file_size`.
- Референс: `refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs`.

### parser
- Чтение UEFI-образа в дерево `FfsNode` через binrw-декларативные структуры.
- `parse_volume`, `parse_file`, `parse_section`, `parse_sections`, `parse_image`.
- `dump_tree`, `list_items`.
- `parse_target`, `find_item`, `find_item_mut`.
- Зависимости: `types`, `ffs`, `binrw`.

### decompress
- Tiano/LZMA декомпрессия для compressed-секций.
- Зависимости: `lzma-rs`.

### builder
- Сборка дерева в байты, выравнивание (padding/gap), перестройка.
- `build_image(image: &Image) -> Result<Vec<u8>>`.

### ops
- `insert(root, target, ffs_bytes, mode)` — вставка (into/before/after).
- `remove(root, target)`, `replace(root, target, data, body_only)`, `rebuild(root, target)`.
- `mark_rebuild_to_root_by_path` — каскадная пометка.

### setup
- IFR-парсинг через `r_efi::hii` структуры.
- `set_item_visibility(image, item_id, visible)` — скрытие/показ через SuppressIf.

### session
- `SessionManager` — именованные сессии, TTL, GC.
- `create_session(name)`, `destroy_session(id, purge_files)`, `validate_token`.
- `spawn_gc()` — фоновый GC, учитывает `purge_artifacts` flag.

### storage
- SQLite: `sessions` (id, token, name, created_at, last_activity), `artifacts` (id, session_id, kind, path, size, source, created_at).
- `Db::insert_session(id, token, name)`, `insert_artifact(...)`, `list_artifacts(session_id)`, `get_artifact(id)`.
- Файловые операции: `store_artifact_file`, `read_artifact_file`, `write_artifact_to_output`.

### rpc
- `EngineServer` — реализация `EngineService` (все 17 RPC).
- `serve(socket_path, db, data_dir, ttl, gc_interval, purge_artifacts)`.

### bin/engine
- Clap CLI: `--data-dir`, `--sock`, `--ttl`, `--gc-interval`, `--purge-artifacts`.
- Env fallbacks для всех параметров.

## Модели данных

### FfsNode
- `guid: Option<Guid>` (uguid::Guid), `node_type: FfsType`, `subtype: u8`, `offset: u32`.
- `header: Vec<u8>`, `body: Vec<u8>`, `tail: Vec<u8>`, `children: Vec<FfsNode>`.
- `action: Action`, `parsing_data: ParsingData`, `fixed: bool`, `compressed: bool`.

### Session (БД)
- `id TEXT PK`, `token TEXT`, `name TEXT` (CWD без symlink resolution), `created_at INTEGER`, `last_activity INTEGER`.

### Artifact (БД)
- `id TEXT PK`, `session_id TEXT FK`, `kind TEXT` ("extracted"/"imported"/"body"/"whole"), `path TEXT`, `size INTEGER`, `source TEXT`, `created_at INTEGER`.

## Переменные окружения

| Переменная | Назначение | По умолчанию |
|---|---|---|
| `UEFIPATCHER_DATA` | корень данных | `~/.local/share/uefipatcher` |
| `UEFIPATCHER_SOCK` | путь unix-сокета | `${XDG_RUNTIME_DIR}/uefipatcher.sock` |
| `UEFIPATCHER_SESSION_TTL_SECS` | TTL сессии | `864000` (10 дней) |
| `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` | интервал GC | `3600` (1 час) |
| `UEFIPATCHER_PURGE_ARTIFACTS` | удалять артефакты при GC | `false` |

## Контейнеризация

- Файлы: `<component>.containerfile` (не `Dockerfile.X`)
- Базовый образ: `registry.fedoraproject.org/fedora:44`
- Rust builder: `docker/rust-builder.containerfile` (fedora:44 + rust toolchain + protobuf-compiler)
- Engine: `FROM rust-builder` → сборка → `FROM fedora:44` (runtime)
