# UEFIPatcher — План (цикл 1: UEFI Engine)

## Краткое описание

Серверный движок для модификации UEFI BIOS: парсинг образа в дерево, вставка/удаление/замена/перестройка FFS-файлов и секций, экстракция/импорт/экспорт артефактов, управление видимостью пунктов Setup-меню, именованные сессии, хранилище с GC-политикой (`--purge-artifacts`), gRPC-сервер над unix-сокетом, engine binary с clap CLI.

## Цели

- Использовать специализированные крейты: `uguid` (GUID), `r-efi` (UEFI/PI типы, IFR), `binrw` (binary-парсинг), `object` (PE32)
- Реализовать парсинг UEFI-образа в дерево `FfsNode` и обратную сборку (round-trip)
- Поддержать модификации: insert/remove/replace/rebuild, с поддержкой `artifact_id`
- Экстракция/импорт/экспорт артефактов (секции целиком или тело)
- Скрытие/показ пунктов Setup-меню (IFR через `r_efi::hii`)
- Именованные сессии (name = CWD без symlink resolution через `env::var("PWD")`)
- Хранилище: SQLite + файлы, TTL 10 дней, GC с `--purge-artifacts` (default false — артефакты НЕ удаляются)
- gRPC-сервер `EngineService` над unix-сокетом
- Engine binary (`src/bin/engine.rs`) с clap CLI
- Крейт `uefi-common` (state.rs, error.rs — скелет, наполняется в цикле 2)
- Контейнеризация: `<component>.containerfile`, `registry.fedoraproject.org/fedora:44`, `rust-builder.containerfile`

## Архитектура

Cargo workspace (edition 2024) с четырьмя крейтами:

- `uefi-proto` — protobuf-схема `EngineService` и tonic-генерируемые типы.
- `uefi-common` — state.rs (State, read_state/write_state) и error.rs (AppError, ExitCode). Скелет в цикле 1, полная реализация в цикле 2.
- `uefi-engine` — ядро: `types`, `ffs`, `parser`, `decompress`, `builder`, `ops`, `setup`, `session`, `storage`, `rpc`, `bin/engine.rs`.
- `uefi-cli` — минимальный CLI для smoke-теста RPC (зависит от uefi-common).

## API/Контракты

Протокол: tonic gRPC over unix-сокет. Авторизация: `Authorization: Bearer <token>` в metadata.

### Методы EngineService

| Метод | Параметры | Ответ |
|---|---|---|
| `CreateSession` | `{name?}` | `{session_id, token}` |
| `DestroySession` | `{session_id}` | `{}` |
| `ListSessions` | `{}` | `{sessions[]: {session_id, name, created_at, last_activity}}` |
| `OpenImage` | `{session_id, image_path, mode}` | `{image_id, root_guid}` |
| `DumpTree` | `{image_id, format}` | `{text}` |
| `ListItems` | `{image_id, filter?}` | `{items[]}` |
| `FindItem` | `{image_id, target}` | `{item_id}` |
| `Insert` | `{image_id, target, ffs_path?, artifact_id?, mode}` | `{item_id}` |
| `Remove` | `{image_id, target}` | `{}` |
| `Replace` | `{image_id, target, ffs_path?, artifact_id?, body_only}` | `{item_id}` |
| `Rebuild` | `{image_id, target}` | `{}` |
| `ExtractArtifact` | `{image_id, target, body_only}` | `{artifact_id}` |
| `ExportArtifact` | `{artifact_id, output_path}` | `{}` |
| `ImportArtifact` | `{session_id, file_path}` | `{artifact_id}` |
| `ListArtifacts` | `{session_id}` | `{artifacts[]}` |
| `SetSetupItemVisibility` | `{image_id, item_id, visible}` | `{}` |
| `SaveImage` | `{image_id, output_path}` | `{}` |

## Зависимости

| Крейт | Версия | Назначение |
|-------|--------|-----------|
| `uguid` | 2.2.1 (serde) | Guid с Display/FromStr/serde, UPPERCASE wrapper |
| `r-efi` | 7.0 | UEFI типы: `base::Guid`, `hii::*` (IFR-структуры, opcode-константы) |
| `binrw` | 0.15 (std) | Declarative binary parsing/writing |
| `object` | 0.39 (read_core, pe) | PE32 parsing |
| `lzma-rs` | 0.3 | LZMA декомпрессия |
| `tonic` | 0.12 | gRPC over unix-сокет |
| `rusqlite` | 0.31 (bundled) | SQLite |
| `clap` | 4 (derive, env) | CLI |
| `tokio` | 1 (full) | Async runtime |
| `uuid` | 1 (v4) | ID generation |

## Переменные окружения

| Переменная | По умолчанию |
|---|---|
| `UEFIPATCHER_DATA` | `~/.local/share/uefipatcher` |
| `UEFIPATCHER_SOCK` | `${XDG_RUNTIME_DIR}/uefipatcher.sock` |
| `UEFIPATCHER_SESSION_TTL_SECS` | `864000` (10 дней) |
| `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` | `3600` (1 час) |
| `UEFIPATCHER_PURGE_ARTIFACTS` | `false` |

## Этапы реализации (19 задач)

Подробное описание: `docs/superpowers/plans/2026-07-22-uefi-engine.md`

## Риски

- Round-trip различия с UEFITool — фиксировать тестовые образы
- binrw variable-length структуры — изучить документацию
- Накопление артефактов при `--purge-artifacts=false` — документация по ручной очистке
