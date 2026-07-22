# UEFIPatcher — Реализация (цикл 1: UEFI Engine)

## Структура проекта

```
uefipatcher/
├── Cargo.toml                       # workspace manifest
├── crates/
│   ├── uefi-proto/                  # protobuf + tonic-генерация
│   │   ├── Cargo.toml
│   │   ├── build.rs                 # tonic_build::compile_protos
│   │   ├── proto/
│   │   │   └── engine.proto         # EngineService, сообщения, enums
│   │   └── src/
│   │       └── lib.rs               # re-export сгенерированных типов
│   ├── uefi-engine/                 # ядро
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # re-export модулей
│   │       ├── parser/              # парсинг UEFI → FfsNode
│   │       │   ├── mod.rs
│   │       │   ├── ffs.rs           # FFS-файлы
│   │       │   ├── volume.rs       # FirmwareVolume
│   │       │   ├── section.rs       # секции (PE32, GUIDed, compressed)
│   │       │   └── target.rs       # парсинг Target-строки
│   │       ├── builder/             # сборка дерева → байты
│   │       │   ├── mod.rs
│   │       │   ├── align.rs         # выравнивание/padding
│   │       │   └── rebuild.rs      # пересборка помеченных узлов
│   │       ├── setup/               # работа с Setup-секцией
│   │       │   ├── mod.rs
│   │       │   └── ifr.rs           # парсинг IFR (референс ifrExtractor)
│   │       ├── session/             # менеджер сессий + GC
│   │       │   └── mod.rs
│   │       ├── storage/             # SQLite + файлы артефактов
│   │       │   ├── mod.rs
│   │       │   ├── schema.rs        # миграции/CREATE TABLE
│   │       │   └── artifact.rs     # чтение/запись файлов артефактов
│   │       └── rpc/                 # gRPC-сервер над unix-сокетом
│   │           ├── mod.rs           # EngineService impl
│   │           ├── auth.rs         # проверка токена из metadata
│   │           └── server.rs       # запуск/слушание unix-сокета
│   └── uefi-cli/                    # CLI-минимум
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs             # разбор аргументов (clap)
│           └── client.rs           # gRPC-клиент к движку
├── docker/
│   ├── Dockerfile.engine            # multi-stage build uefi-engine
│   └── docker-compose.yml          # пример развертывания
├── roadmap.md
├── PLANNING.md
├── IMPLEMENTATION.md
└── TESTING.md
```

## Модули и их ответственность

### uefi-proto
- Описание: protobuf-схема `EngineService` и tonic-генерация типов. Единый контракт для движка и всех клиентов.
- Зависимости: нет (build-dependencies: `tonic`, `prost`).
- Публичный интерфейс: сгенерированные сообщения (`OpenImageRequest`, `DumpTreeResponse` и т.д.), enums (`ImageMode`, `InsertMode`, `DumpFormat`), trait `engine_service_server::EngineService`.

### parser
- Описание: чтение UEFI-образа в дерево `FfsNode`. Поддержка FirmwareVolume, FFS-файлов, секций (PE32, GUIDed, compressed — LZMA/EFI fattest). Парсинг строки `Target` (GUID/PATH/GUID:T/GUID:T:N).
- Зависимости: `uefi-proto`.
- Публичный интерфейс:
  - `parse_image(bytes: &[u8], mode: ImageMode) -> Result<Image>`
  - `dump_tree(image: &Image, format: DumpFormat) -> String`
  - `list_items(image: &Image, filter: Option<&str>) -> Vec<Item>`
  - `find_item(image: &Image, target: &str) -> Result<ItemId>`
  - типы: `FfsNode`, `Image`, `Target`

### builder
- Описание: сборка дерева `FfsNode` в байты. Выравнивание (padding/gap), перестройка помеченных узлов, удаление помеченных на удаление.
- Зависимости: `parser`.
- Публичный интерфейс:
  - `build_image(image: &Image) -> Result<Vec<u8>>`

### setup
- Описание: парсинг IFR Setup-форм (референс ifrExtractor). В цикле 1 — только `SetSetupItemVisibility`.
- Зависимости: `parser`.
- Публичный интерфейс:
  - `set_item_visibility(image: &mut Image, item_id: &str, visible: bool) -> Result<()>`

### session
- Описание: менеджер сессий. Создание (генерация UUID + токена), обновление `last_activity`, удаление по TTL, фоновый GC-task.
- Зависимости: `storage`.
- Публичный интерфейс:
  - `create_session() -> Result<(SessionId, Token)>`
  - `destroy_session(id: &SessionId) -> Result<()>`
  - `touch_session(id: &SessionId) -> Result<()>`
  - `list_sessions() -> Result<Vec<SessionInfo>>`
  - `gc_loop(interval: Duration, ttl: Duration) -> !` (spawn как tokio task)

### storage
- Описание: SQLite-хранилище (таблицы `sessions`, `artifacts`), файлы артефактов в `${UEFIPATCHER_DATA}/sessions/<id>/images/`.
- Зависимости: `rusqlite`.
- Публичный интерфейс:
  - `open_db(path: &Path) -> Result<Db>`
  - `Db::insert_session`, `Db::get_session`, `Db::delete_session`, `Db::list_expired_sessions(ttl)`
  - `Db::insert_artifact`, `Db::get_artifact_path`
  - `store_artifact_file(session_id, image_id, bytes) -> Result<PathBuf>`

### rpc
- Описание: реализация `EngineService`, gRPC-сервер над unix-сокетом. Проверка токена из `Authorization: Bearer <token>` в metadata.
- Зависимости: `uefi-proto`, `parser`, `builder`, `setup`, `session`, `storage`.
- Публичный интерфейс:
  - `serve(socket_path: &Path, db: Db, data_dir: &Path) -> Result<()>`
  - impl `EngineService for EngineServer` (все методы)

### uefi-cli
- Описание: минимальный CLI для smoke-теста RPC. Подмножество UEFIEdit.
- Зависимости: `uefi-proto` (клиент).
- Публичный интерфейс: `uefi-cli <image> <command> [args]` (см. ниже)

## API/Контракты (детально)

### CreateSession
- Команда/gRPC: `EngineService/CreateSession`
- Параметры: нет
- Ответ:
  - Успех: `{session_id: string, token: string}`
  - Ошибка: `INTERNAL`
- Примеры:
  - CLI: `uefi-cli session create`
  - grpcurl: `grpcurl -plaintext -unix ${SOCK} EngineService/CreateSession`

### DestroySession
- Команда/gRPC: `EngineService/DestroySession`
- Параметры:
  - `session_id`: string, обязательный
- Ответ:
  - Успех: `{}`
  - Ошибка: `NOT_FOUND` — сессия не найдена
- Примеры:
  - CLI: `uefi-cli session destroy <session_id>`

### OpenImage
- Команда/gRPC: `EngineService/OpenImage`
- Параметры:
  - `session_id`: string, обязательный
  - `image_path`: string, обязательный — путь к UEFI-образу
  - `mode`: enum `READ|WRITE`, обязательный
- Ответ:
  - Успех: `{image_id: string, root_guid: string}`
  - Ошибка: `NOT_FOUND` (сессия), `INVALID_ARGUMENT` (путь), `INTERNAL` (невалидный образ)
- Примеры:
  - CLI: `uefi-cli <session_id> open <image_path> --mode WRITE`

### DumpTree
- Команда/gRPC: `EngineService/DumpTree`
- Параметры:
  - `image_id`: string, обязательный
  - `format`: enum `TEXT|TSV`, обязательный
- Ответ:
  - Успех: `{text: string}`
  - Ошибка: `NOT_FOUND` (image_id)
- Примеры:
  - CLI: `uefi-cli <image_id> dump --format text`

### ListItems
- Команда/gRPC: `EngineService/ListItems`
- Параметры:
  - `image_id`: string, обязательный
  - `filter`: string, необязательный
- Ответ:
  - Успех: `{items: [{path, type, subtype, guid, offset, size, name}]}`
- Примеры:
  - CLI: `uefi-cli <image_id> list`

### FindItem
- Команда/gRPC: `EngineService/FindItem`
- Параметры:
  - `image_id`: string, обязательный
  - `target`: string, обязательный (GUID | PATH | GUID:T | GUID:T:N)
- Ответ:
  - Успех: `{item_id: string}`
  - Ошибка: `NOT_FOUND` — элемент не найден
- Примеры:
  - CLI: `uefi-cli <image_id> find 5C60F367-A505-419A-859E-2A4FF6CA6FE5`

### Insert
- Команда/gRPC: `EngineService/Insert`
- Параметры:
  - `image_id`: string, обязательный
  - `target`: string, обязательный
  - `ffs_path`: string, обязательный — путь к FFS-файлу для вставки
  - `mode`: enum `INTO|BEFORE|AFTER`, обязательный
- Ответ:
  - Успех: `{item_id: string}`
  - Ошибка: `NOT_FOUND`, `INVALID_ARGUMENT`
- Примеры:
  - CLI: `uefi-cli <image_id> insert <target> <ffs_path> --mode before`

### Remove
- Команда/gRPC: `EngineService/Remove`
- Параметры:
  - `image_id`: string, обязательный
  - `target`: string, обязательный
- Ответ:
  - Успех: `{}`
- Примеры:
  - CLI: `uefi-cli <image_id> remove <target>`

### Replace
- Команда/gRPC: `EngineService/Replace`
- Параметры:
  - `image_id`: string, обязательный
  - `target`: string, обязательный
  - `ffs_path`: string, обязательный
  - `body_only`: bool, необязательный (по умолчанию false)
- Ответ:
  - Успех: `{item_id: string}`
- Примеры:
  - CLI: `uefi-cli <image_id> replace <target> <ffs_path> --body-only`

### Rebuild
- Команда/gRPC: `EngineService/Rebuild`
- Параметры:
  - `image_id`: string, обязательный
  - `target`: string, обязательный
- Ответ:
  - Успех: `{}`
- Примеры:
  - CLI: `uefi-cli <image_id> rebuild <target>`

### SetSetupItemVisibility
- Команда/gRPC: `EngineService/SetSetupItemVisibility`
- Параметры:
  - `image_id`: string, обязательный
  - `item_id`: string, обязательный
  - `visible`: bool, необязательный (по умолчанию true)
- Ответ:
  - Успех: `{}`
  - Ошибка: `NOT_FOUND`, `INVALID_ARGUMENT` (элемент не является пунктом Setup)
- Примеры:
  - CLI: `uefi-cli <image_id> set-visibility <item_id> --visible false`

### SaveImage
- Команда/gRPC: `EngineService/SaveImage`
- Параметры:
  - `image_id`: string, обязательный
  - `output_path`: string, обязательный
- Ответ:
  - Успех: `{}`
  - Ошибка: `INTERNAL` — сборка не удалась
- Примеры:
  - CLI: `uefi-cli <image_id> save <output_path>`

## Модели данных

### FfsNode
- Поля:
  - `guid: Option<Guid>` — GUID узла (FFS-файл, FvName, GUIDed-секция)
  - `type: u8` — тип (FFS file type / section type)
  - `subtype: u8` — подтип (для section)
  - `offset: u64` — смещение в исходном образе
  - `size: u64` — полный размер (заголовок + тело)
  - `body_size: u64` — размер тела без заголовка
  - `header: Vec<u8>` — байты заголовка as-is
  - `body: Vec<u8>` — байты тела as-is
  - `children: Vec<FfsNode>` — дочерние узлы
  - `modified: bool` — флаг изменения
  - `marked_for_rebuild: bool` — помечен для пересборки
  - `marked_for_removal: bool` — помечен для удаления
- Валидация: `size == header.len() + body.len() + sum(children.size)` (с учётом выравнивания)
- Связи: `Image.root: FfsNode` (корень дерева)

### Image
- Поля:
  - `image_id: String`
  - `session_id: String`
  - `root: FfsNode`
  - `mode: ImageMode` (READ/WRITE)
- Связи: принадлежит `Session`

### Session (в БД)
- Поля:
  - `id: TEXT PK`
  - `token: TEXT NOT NULL`
  - `created_at: INTEGER NOT NULL` (unix timestamp)
  - `last_activity: INTEGER NOT NULL`
- Связи: 1:N с `Artifact`

### Artifact (в БД)
- Поля:
  - `id: TEXT PK`
  - `session_id: TEXT FK -> sessions(id) ON DELETE CASCADE`
  - `kind: TEXT NOT NULL` (например `image`)
  - `path: TEXT NOT NULL` — путь к файлу на диске
  - `size: INTEGER NOT NULL`
  - `created_at: INTEGER NOT NULL`
- Связи: N:1 с `Session`

### Target (enum, in-memory)
- Варианты:
  - `Guid(Guid)` — `5C60F367-A505-419A-859E-2A4FF6CA6FE5`
  - `Path(Vec<usize>)` — `0/2/207/1/0`
  - `GuidType(Guid, u8, Option<usize>)` — `899407D7-...:0x10` или `899407D7-...:0x10:2`

## Паттерны и конвенции

### Архитектура
- Clean Architecture: `parser`/`builder`/`setup` — домен (чистые функции над `FfsNode`), `storage`/`rpc` — инфраструктура.
- Repository Pattern: `storage::Db` инкапсулирует SQLite-доступ.
- Command Pattern: каждый gRPC-метод — атомарная команда над сессией/образом.

### Стиль кода
- Именование: snake_case для функций/переменных, PascalCase для типов/трейтов, UPPER_SNAKE для констант.
- Комментарии: только для нетривиальной логики парсинга UEFI (ссылки на UEFITool `file:line` и edk2).
- Ошибки: `thiserror` для доменных ошибок (`ParserError`, `BuilderError`), `anyhow` для верхнеуровневого glue. Sentinel errors для ожидаемых (`NotFound`, `InvalidTarget`).
- Логирование: `tracing` с уровнями `INFO` (сессии, RPC-вызовы), `DEBUG` (детали парсинга), `WARN` (неизвестные типы секций — сохранять as-is), `ERROR` (ошибки сборки/БД).

## Зависимости и инструменты

### Библиотеки (Rust)

- `tonic` (latest): gRPC-сервер/клиент над unix-сокетом
- `prost` (latest): protobuf-генерация
- `rusqlite` (latest, bundled feature): SQLite-хранилище
- `uuid` (latest, v4 feature): генерация session_id/image_id
- `clap` (latest, derive feature): разбор аргументов CLI
- `anyhow` (latest): контекстные ошибки
- `thiserror` (latest): доменные ошибки
- `tracing` (latest) + `tracing-subscriber` (latest, env-filter): логирование
- `tokio` (latest, full features): async-runtime

### Референсные исходники (не зависимости, только для чтения)

- `../refs/UEFITool` (0.28.8) — `FfsParser`, `FfsBuilder`
- `../refs/IFRExtractor-RS` — парсинг IFR
- `../refs/edk2` — точка истины при багах

### Инструменты разработки

- `cargo`: сборка, тесты
- `rustfmt`: форматирование
- `clippy`: linting
- `tonic-build`: protobuf-генерация (через `build.rs`)
- `protoc` или `protoc-gen-tonic`: генерация (build-dependency)
- `podman` / `docker`: контейнерные сборки/тесты

### Конфигурация (переменные окружения)

| Переменная | Назначение | По умолчанию |
|---|---|---|
| `UEFIPATCHER_DATA` | корень данных | `${XDG_DATA_HOME}/uefipatcher` или `~/.local/share/uefipatcher` |
| `UEFIPATCHER_SOCK` | путь unix-сокета | `${XDG_RUNTIME_DIR}/uefipatcher.sock` |
| `UEFIPATCHER_SESSION_TTL_SECS` | TTL сессии | `864000` (10 дней) |
| `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` | интервал GC | `3600` (1 час) |