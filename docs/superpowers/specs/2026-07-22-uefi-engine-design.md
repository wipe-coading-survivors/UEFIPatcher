# UEFIPatcher — Дизайн цикла 1 (UEFI Engine)

## Краткое описание

UEFIPatcher — многокомпонентное приложение для модификации готовых (бинарных) UEFI BIOS с целью расширения функционала (бифуркация PCI-E, перенаправление вывода загрузки на серийный порт и т.д.) с настройкой через пользовательскую оболочку UEFI.

Цикл 1 покрывает серверный движок (п.4.1 из `user-input.md`), общий крейт `uefi-common` (state/error для всех клиентов) и минимальный CLI для smoke-тестирования RPC. Остальные компоненты (полный CLI, TUI, WebUI, расширенный Setup) вынесены в `roadmap.md`.

## Цели цикла 1

- Реализовать ядро бизнес-логики: парсинг UEFI-образа в дерево, модификацию дерева (вставка/удаление/замена/перестройка), сборку образа обратно.
- Использовать специализированные крейты вместо ручного парсинга байтов: `uguid` (GUID), `r-efi` (UEFI/PI типы, IFR-структуры), `binrw` (декларативный binary-парсинг), `object` (PE32).
- Реализовать работу с секциями DXE/PEI: полная замена FFS-файла и замена только тела секции.
- Реализовать экстракцию/импорт/экспорт артефактов (секции целиком или тело, подсекции) с хранением в БД.
- Реализовать ограниченную работу с Setup-секцией: только скрытие/показ существующих пунктов меню (без добавления новых — это цикл 6).
- Реализовать хранение сессий и артефактов (SQLite + файловая система) с именованными сессиями и TTL.
- Реализовать gRPC-сервер над unix-сокетом с авторизацией по токену.
- Создать крейт `uefi-common` (state.rs, error.rs) для переиспользования CLI/TUI/WebUI.
- Предоставить полноценный engine binary (`src/bin/engine.rs`) с clap CLI-флагами.
- Предоставить минимальный CLI (`uefi-cli`) для smoke-тестирования RPC.
- Подготовить containerfile и docker-compose пример для движка.

## Технологический стек

| Крейт | Версия | Назначение |
|-------|--------|-----------|
| `uguid` | 2.2.1 (serde) | `Guid` с Display (`to_ascii_hex_lower` → обёртка UPPERCASE), `try_parse` (FromStr), serde. UEFI mixed-endian. |
| `r-efi` | 7.0 | UEFI спецификация: `base::Guid` (raw struct), `hii::*` (IFR-структуры, opcode-константы, package types), EFI-типы. no_std. |
| `binrw` | 0.15 (std) | Declarative binary parsing/writing через `#[brw]`-макросы для FV/FFS/section заголовков. |
| `object` | 0.39 (read_core, pe) | PE32 parsing для PEI/DXE модулей. |
| `lzma-rs` | 0.3 | LZMA декомпрессия compressed-секций. |
| `tonic` | 0.12 | gRPC server over unix-сокет. |
| `rusqlite` | 0.31 (bundled) | SQLite хранилище. |
| `clap` | 4 (derive, env) | CLI для engine binary и uefi-cli. |
| `tokio` | 1 (full) | Async runtime. |
| `prost` | 0.13 | Protobuf кодогенерация. |
| `uuid` | 1 (v4) | Генерация session_id, artifact_id. |
| `anyhow` | 1 | Error aggregation. |
| `thiserror` | 1 | Typed errors. |

**Принцип:** не писать самописный парсинг байтов и констант UEFI. FFS/section/FV заголовки описываются через `binrw`-макросы. IFR-структуры и opcode-константы берутся из `r_efi::hii`. GUID-константы (TIANO, LZMA и др.) определяются через `uguid::Guid::try_parse`. Уход от ручной борьбы с переполнением и byte offsets.

## Архитектура

### Компоненты цикла 1

Cargo workspace `uefipatcher/`:

```
uefipatcher/
├── Cargo.toml                      # workspace manifest
├── crates/
│   ├── uefi-proto/                 # protobuf + tonic-генерируемые типы
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   └── proto/engine.proto
│   ├── uefi-common/                # общие state.rs + error.rs для CLI/TUI/WebUI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── state.rs            # State, read_state, write_state, resolve_sock
│   │       └── error.rs            # AppError, ExitCode, print_error
│   ├── uefi-engine/                # ядро: парсер, builder, setup, session, storage, rpc
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── types.rs            # FfsNode, Image, Target, Guid wrapper
│   │   │   ├── ffs.rs              # FFS constants, checksums (на базе r-efi/binrw)
│   │   │   ├── parser/             # volume, file, section, image
│   │   │   ├── decompress.rs       # Tiano/LZMA
│   │   │   ├── builder.rs          # сборка дерева → байты
│   │   │   ├── ops.rs              # insert/remove/replace/rebuild
│   │   │   ├── storage/            # SQLite schema, sessions, artifacts
│   │   │   ├── session.rs          # session manager + GC
│   │   │   ├── setup.rs            # IFR parser, SetSetupItemVisibility
│   │   │   ├── rpc/                # server, auth, EngineService impl
│   │   │   └── bin/engine.rs       # engine binary (clap CLI)
│   │   └── tests/
│   └── uefi-cli/                   # CLI-минимум (smoke-тест RPC; полный CLI — цикл 2)
│       ├── Cargo.toml
│       └── src/main.rs
├── docker/
│   ├── rust-builder.containerfile  # базовый Rust builder (fedora:44)
│   ├── engine.containerfile        # движок на базе rust-builder
│   └── docker-compose.yml
└── roadmap.md
```

### Модули `uefi-engine`

- `types` — `FfsNode`, `Image`, `Target`, Guid-wrapper (uguid + UPPERCASE Display).
- `ffs` — FFS-константы (через `r_efi::hii` где возможно), checksum-хелперы (wrapping arithmetic), well-known GUIDs (через `uguid::Guid::try_parse`).
- `parser` — чтение UEFI-образа в дерево `FfsNode` через `binrw`-декларативные структуры. Референс — UEFITool 0.28.8.
- `builder` — сборка дерева в байты, выравнивание, перестройка.
- `ops` — insert/remove/replace/rebuild модификации дерева.
- `setup` — IFR-парсинг (через `r_efi::hii` структуры), SetSetupItemVisibility.
- `session` — менеджер сессий: создание (с именем), TTL, GC.
- `storage` — SQLite (sessions, artifacts), экстракция/импорт артефактов.
- `rpc` — gRPC-сервер, EngineService impl.

### Крейт `uefi-common`

Создаётся в цикле 1 (не откладывается на цикл 3). Содержит:
- `state.rs` — `State` struct (session_id, token, active_image_id, sock_path), `read_state()`, `write_state()`, `resolve_sock()`, `default_sock()`. Файл `.uefipatcher` (TOML) в CWD.
- `error.rs` — `AppError`, `ErrKind`, `ExitCode`, `print_error()`. Унифицированные коды ошибок для CLI/TUI/WebUI.

Цикл 2 (CLI) наполнит `uefi-common` полным содержимым; цикл 3+ просто используют его.

## gRPC API

Протокол: tonic gRPC over unix-сокет (`${UEFIPATCHER_SOCK:-/run/uefipatcher.sock}`).

Авторизация: заголовок `Authorization: Bearer <token>` в gRPC metadata. Токен генерируется при старте сервера. Защита от дурака — не криптографическая.

Сессия идентифицируется `session_id` (UUID) в каждом запросе, кроме `CreateSession`.

### EngineService

| Метод | Запрос | Ответ |
|---|---|---|
| `CreateSession` | `{name?}` | `{session_id, token}` |
| `DestroySession` | `{session_id}` | `{}` |
| `ListSessions` | `{}` | `{sessions[]: {session_id, name, created_at, last_activity}}` |
| `OpenImage` | `{session_id, image_path, mode=READ\|WRITE}` | `{image_id, root_guid}` |
| `DumpTree` | `{image_id, format=TEXT\|TSV}` | `{text}` |
| `ListItems` | `{image_id, filter?}` | `{items[]: {path, type, subtype, guid, offset, size, name}}` |
| `FindItem` | `{image_id, target}` | `{item_id}` |
| `Insert` | `{image_id, target, ffs_path?, artifact_id?, mode=INTO\|BEFORE\|AFTER}` | `{item_id}` |
| `Remove` | `{image_id, target}` | `{}` |
| `Replace` | `{image_id, target, ffs_path?, artifact_id?, body_only=false}` | `{item_id}` |
| `Rebuild` | `{image_id, target}` | `{}` |
| `ExtractArtifact` | `{image_id, target, body_only=false}` | `{artifact_id}` |
| `ExportArtifact` | `{artifact_id, output_path}` | `{}` |
| `ImportArtifact` | `{session_id, file_path}` | `{artifact_id}` |
| `ListArtifacts` | `{session_id}` | `{artifacts[]: {artifact_id, kind, size, created_at, source}}` |
| `SetSetupItemVisibility` | `{image_id, item_id, visible=true}` | `{}` |
| `SaveImage` | `{image_id, output_path}` | `{}` |

### Семантика

- `CreateSession`: если `name` не указан, клиент должен передать полный путь к CWD без разрешения символических ссылок (на Linux — `env::var("PWD")`, НЕ `std::env::current_dir()` который canonicalize).
- `Insert`/`Replace`: источник данных — либо `ffs_path` (файл в ФС), либо `artifact_id` (ранее экстрагированный/импортированный артефакт). Один из двух должен быть указан.
- `ExtractArtifact`: экстрагирует секцию/подсекцию (целиком или только тело) по `target`-адресации, сохраняет в хранилище артефактов (файл + строка в БД), возвращает `artifact_id`.
- `ExportArtifact`: записывает ранее сохранённый артефакт в файл в ФС по указанному пути.
- `ImportArtifact`: читает файл из ФС, сохраняет как артефакт в хранилище, возвращает `artifact_id`.
- `ListArtifacts`: список артефактов сессии с метаданными (kind: "extracted"|"imported", size, source).
- Несколько edit-методов можно вызывать до `SaveImage` (накапливаемые изменения в дереве).
- `SaveImage` собирает дерево (`builder`) и записывает результат по `output_path`.

`target` — строка, разбирается как в UEFIEdit:
- `GUID` — `5C60F367-A505-419A-859E-2A4FF6CA6FE5` — поиск по GUID (заголовок файла, FvName, GUIDed-секция)
- `PATH` — `0/2/207/1/0` — спуск по дереву по индексам дочерних элементов
- `GUID:T` — `899407D7-...:0x10` — поиск файла по GUID, затем первая секция типа T (hex)
- `GUID:T:N` — то же, но N-я (0-based) секция типа T

## Модели данных

### In-memory (`uefi-engine::types`)

`FfsNode` — узел дерева UEFI:
- `guid: Option<Guid>` — GUID узла (uguid::Guid)
- `type: u8` — тип элемента (FFS file type / section type)
- `subtype: u8` — подтип (для section type)
- `offset: u64` — смещение в исходном образе
- `size: u64` — полный размер (заголовок + тело)
- `body_size: u64` — размер тела без заголовка
- `header: Vec<u8>` — байты заголовка as-is
- `body: Vec<u8>` — байты тела as-is
- `children: Vec<FfsNode>` — дочерние узлы
- `modified: bool` — флаг изменения
- `marked_for_rebuild: bool` — помечен для пересборки
- `marked_for_removal: bool` — помечен для удаления

`Image`:
- `image_id: String`
- `session_id: String`
- `root: FfsNode`
- `mode: ImageMode` (READ/WRITE)

`Target` (enum):
- `Guid(Guid)`
- `Path(Vec<usize>)`
- `GuidType(Guid, u8, Option<usize>)`

### SQLite

`${UEFIPATCHER_DATA}/uefipatcher.db`:

```sql
CREATE TABLE IF NOT EXISTS sessions (
    id            TEXT PRIMARY KEY,
    token         TEXT NOT NULL,
    name          TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    path       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    source     TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity);
```

Файлы артефактов: `${UEFIPATCHER_DATA}/sessions/<session_id>/artifacts/<artifact_id>.bin`.

Артефакты также сохраняются при `OpenImage(mode=WRITE)`: копия образа пишется в `${UEFIPATCHER_DATA}/sessions/<session_id>/images/<image_id>.bin`.

### Политика очистки (GC)

Фоновая задача каждые `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` секунд удаляет сессии с `last_activity < now - TTL`.

**Критически важно (issue #1, п.3):** По умолчанию (`--purge-artifacts` не передан) GC удаляет только **session metadata** из БД, но **СОХРАНЯЕТ файлы артефактов** на диске. Безопасность данных приоритетнее экономии места: избыточный рост хранилища предпочтительнее, чем потенциальная потеря результатов нескольких дней работы.

При `--purge-artifacts=true` (явный флаг) GC удаляет сессию целиком: и metadata, и файлы артефактов.

### Engine binary CLI (`src/bin/engine.rs`)

```
uefi-engine [OPTIONS]

Опции:
  --data-dir <PATH>          Корень данных [env: UEFIPATCHER_DATA]
  --sock <PATH>              Путь unix-сокета [env: UEFIPATCHER_SOCK]
  --ttl <SECS>               TTL сессии [env: UEFIPATCHER_SESSION_TTL_SECS] (default: 864000)
  --gc-interval <SECS>       Интервал GC [env: UEFIPATCHER_SESSION_GC_INTERVAL_SECS] (default: 3600)
  --purge-artifacts          Удалять файлы артефактов при GC [env: UEFIPATCHER_PURGE_ARTIFACTS] (default: false)
  --help, --version
```

### Конфигурация (переменные окружения)

| Переменная | Назначение | По умолчанию |
|---|---|---|
| `UEFIPATCHER_DATA` | корень данных | `${XDG_DATA_HOME}/uefipatcher` или `~/.local/share/uefipatcher` |
| `UEFIPATCHER_SOCK` | путь unix-сокета | `/run/uefipatcher.sock` или `${XDG_RUNTIME_DIR}/uefipatcher.sock` |
| `UEFIPATCHER_SESSION_TTL_SECS` | TTL сессии | `864000` (10 дней) |
| `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` | интервал сборщика | `3600` (1 час) |
| `UEFIPATCHER_PURGE_ARTIFACTS` | удалять артефакты при GC | `false` |

## Контейнеризация

Стандартизация Docker/Podman (issue #1, п.5):

- **Именование файлов:** `<component>.containerfile` (не `Dockerfile.X`)
- **Базовый образ:** `registry.fedoraproject.org/fedora:44` для всех контейнеров
- **Rust builder:** отдельный `docker/rust-builder.containerfile` (fedora:44 + rust toolchain + protobuf-compiler), реиспользуется всеми последующими сборочными инструкциями
- **Engine containerfile:** `FROM rust-builder` → сборка → `FROM fedora:44` (runtime)

## Этапы реализации

1. **Скелет workspace**: Cargo workspace, крейты `uefi-proto`, `uefi-common` (state.rs + error.rs скелет), `uefi-engine` (lib.rs + bin/engine.rs), `uefi-cli` (main.rs stub); `engine.proto` с полным API; workspace deps (uguid, r-efi, binrw, object).
2. **Типы и Guid**: `types.rs` — FfsNode, Image, Target. Guid через `uguid::Guid` + wrapper для UPPERCASE Display.
3. **FFS-структуры**: `ffs.rs` — константы, checksum-хелперы (wrapping arithmetic), well-known GUIDs. Референс: `refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs`.
4. **Парсер**: binrw-структуры для FV/FFS/section заголовков, чтение образа → дерево. Покрытие: FirmwareVolume, FFS-файлы, секции (PE32 через `object`, GUIDed, compressed, raw).
5. **Target-поиск**: парсинг target-строки → find_item.
6. **Builder**: сборка дерева в байты, round-trip.
7. **Модификации**: insert/remove/replace/rebuild. Insert/Replace с поддержкой artifact_id.
8. **Декомпрессия**: Tiano/LZMA для compressed-секций.
9. **Хранилище и сессии**: SQLite (схема с session_name), artifacts (extract/import/export), именованные сессии, TTL, фоновый GC с `--purge-artifacts`.
10. **Setup-visibility**: IFR-парсинг через `r_efi::hii` структуры, SetSetupItemVisibility.
11. **gRPC-сервер**: EngineService impl, авторизация, все RPC методы включая artifact operations.
12. **Engine binary**: clap CLI (`src/bin/engine.rs`), --purge-artifacts, env fallbacks.
13. **CLI-минимум**: smoke-тест RPC (uefi-cli зависит от uefi-common).
14. **Контейнеризация**: rust-builder.containerfile + engine.containerfile + docker-compose.

## Тонкости имплементации

- **CWD без symlink resolution**: при создании сессии по умолчанию `name = env::var("PWD")`, НЕ `std::env::current_dir()`. `current_dir()` вызывает `getcwd()` который canonicalize'ит путь (разрешает символические ссылки). `PWD` — это переменная окружения shell, которая сохраняет исходный путь без разрешения. Это важно, потому что пользователь может работать через symlinked-директории и ожидает видеть именно тот путь, который он набрал в shell.

- **GUID в UPPERCASE**: `uguid::Guid` выводит hex в lowercase (`to_ascii_hex_lower`). В индустрии UEFI (AMI, OEM, EDK2) принято отображать GUID в UPPERCASE. На уровне байт разницы нет. Необходим wrapper-функция для приведения выводимого GUID к верхнему регистру.

- **Порядок объявления модулей**: при создании файла модуля (например `ffs.rs`), шаг добавления `pub mod ffs;` в `lib.rs`/`mod.rs` выполняется ДО запуска `cargo test`. Binary-крейты получают `main.rs` одновременно с `Cargo.toml`. Это исключает false-positive тестов (0 из 0) из-за неподключённых модулей.

- **Checksum wrapping**: использовать `wrapping_add`/`wrapping_sub` для всех checksum-вычислений, не полагаться на default overflow behavior. Референс: `refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs`.

- **binrw для структур**: вместо ручного чтения байтов по offset, использовать `#[brw]`-макросы binrw для декларативного описания структур FFS/section/FV заголовков. Это автоматически генерирует код чтения/записи.

- **--purge-artifacts по умолчанию false**: артефакты и сессии НЕ удаляются автоматически. Безопасность данных > экономия места. Флаг включает полное удаление.

## Риски и ограничения

- **Несоответствие парсера UEFITool** — binary-различия в round-trip. Мера: фиксировать тестовые образы; сравнивать с UEFITool 0.28.8; сверяться с edk2.
- **Объём портируемой логики UEFI** — много типов секций/сжатий. Мера: поддерживать только реально встречающиеся (LZMA/Tiano); неизвестные секции сохранять as-is.
- **binrw-макросы и сложные структуры** — некоторые UEFI структуры имеют variable-length поля или conditional-чтение. Мера: binrw поддерживает `args`, `count`, `if` — изучить документацию.
- **TTL и хранение артефактов** — при `--purge-artifacts=false` (default) артефакты накапливаются. Мера: документация по ручной очистке `UEFIPATCHER_DATA`; CLI-команда для аудита.
- **Setup-visibility без NVRAM** — пользователь может ожидать полного редактирования Setup. Мера: явно зафиксировать в roadmap, что NVRAM и новые пункты — цикл 6.

## Решения (фиксация)

- Стек: Rust, tonic gRPC over unix-сокет, SQLite.
- Парсер UEFI: binrw + r-efi (декларативный парсинг, не ручной byte-offset).
- GUID: uguid (Display/FromStr/serde) + UPPERCASE wrapper для индустриального формата.
- Крейт `uefi-common` создаётся в цикле 1 (state.rs + error.rs), не откладывается.
- Менеджер сессий: встроен в движок. Именованные сессии (name = CWD без symlink resolution).
- Хранилище: SQLite + файлы; TTL по умолчанию 10 дней. Артефакты по умолчанию НЕ удаляются GC.
- Экстракция/импорт/экспорт артефактов: через gRPC API, хранение в БД + файлы.
- Engine binary: clap CLI с флагами (--purge-artifacts и др.), env fallbacks.
- Контейнеризация: `<component>.containerfile`, `registry.fedoraproject.org/fedora:44`, отдельный `rust-builder.containerfile`.
- Структура: Cargo workspace (`uefi-proto`, `uefi-common`, `uefi-engine`, `uefi-cli`).
