# UEFIPatcher — Дизайн цикла 1 (UEFI Engine)

## Краткое описание

UEFIPatcher — многокомпонентное приложение для модификации готовых (бинарных) UEFI BIOS с целью расширения функционала (бифуркация PCI-E, перенаправление вывода загрузки на серийный порт и т.д.) с настройкой через пользовательскую оболочку UEFI.

Цикл 1 покрывает только серверный движок (п.4.1 из `user-input.md`) с минимальным CLI для smoke-тестирования RPC. Остальные компоненты (полный CLI, TUI, WebUI, менеджер сессий как отдельный процесс, расширенная работа с Setup) вынесены в `roadmap.md` и реализуются последующими циклами.

## Цели цикла 1

- Реализовать ядро бизнес-логики: парсинг UEFI-образа в дерево, модификацию дерева (вставка/удаление/замена/перестройка), сборку образа обратно.
- Реализовать работу с секциями DXE/PEI на двух уровнях: полная замена FFS-файла и замена только тела секции (с сохранением оригинальных заголовков).
- Реализовать ограниченную работу с Setup-секцией: только скрытие/показ существующих пунктов меню (без добавления новых пунктов и без управления NVRAM — это циклы 4+).
- Реализовать хранение сессий и артефактов (SQLite + файловая система) с TTL 10 дней и фоновым сборщиком мусора.
- Реализовать gRPC-сервер над unix-сокетом с авторизацией по токену (защита от дурака).
- Предоставить минимальный CLI (`uefi-cli`) для smoke-тестирования RPC; полный CLI — цикл 2.
- Подготовить Dockerfile и docker-compose пример для движка.

## Архитектура

### Компоненты цикла 1

Cargo workspace `uefipatcher/`:

```
uefipatcher/
├── Cargo.toml                  # workspace manifest
├── crates/
│   ├── uefi-proto/             # protobuf + tonic-генерируемые типы, общий для всех клиентов
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   └── proto/engine.proto
│   ├── uefi-engine/            # ядро: парсер, builder, setup, session, storage, rpc
│   │   ├── Cargo.toml
│   │   └── src/
│   └── uefi-cli/               # CLI-минимум (smoke-тест RPC; полный CLI — цикл 2)
│       ├── Cargo.toml
│       └── src/
├── roadmap.md
├── PLANNING.md
├── IMPLEMENTATION.md
└── TESTING.md
```

### Модули `uefi-engine`

- `parser` — чтение UEFI-образа в дерево `FfsNode`. Референс — UEFITool 0.28.8 (`FfsParser`).
- `builder` — сборка дерева обратно в байты, выравнивание (padding/gap), перестройка дерева. Референс — UEFITool `FfsBuilder`.
- `setup` — работа с Setup-секцией. В цикле 1: только видимость пунктов (парсинг IFR, референс ifrExtractor).
- `session` — менеджер сессий: создание, TTL, удаление по таймауту. Встроен в движок (не отдельный процесс).
- `storage` — SQLite (таблицы `sessions`, `artifacts`), артефакты на диске в `${UEFIPATCHER_DATA}`.
- `rpc` — gRPC-сервер над unix-сокетом (`EngineService`), токены авторизации.

### Компоненты вне цикла 1 (в `roadmap.md`)

Полный CLI (цикл 2), TUI (цикл 3), менеджер сессий как отдельный процесс (опционально, цикл 4), WebUI (цикл 5), расширенный setup: новые пункты и NVRAM (цикл 6), grpc-шлюз для внешних клиентов (опционально, цикл 7).

## Фронтенды

- [x] gRPC API: `EngineService` над unix-сокетом (детально в разделе «gRPC API»)
- [~] CLI-минимум (`uefi-cli`): smoke-тест RPC, подмножество команд UEFIEdit. Полный CLI — цикл 2.
- [ ] TUI — цикл 3 (roadmap)
- [ ] WebUI — цикл 5 (roadmap)

## gRPC API

Протокол: tonic gRPC over unix-сокет (`${UEFIPATCHER_SOCK:-/run/uefipatcher.sock}`).

Авторизация: заголовок `Authorization: Bearer <token>` в gRPC metadata. Токен генерируется при старте сервера, пишется в `${UEFIPATCHER_DATA}/token` с правами `0600`. Защита от дурака — не криптографическая.

Сессия идентифицируется `session_id` (UUID) в каждом запросе, кроме `CreateSession`.

### EngineService

| Метод | Запрос | Ответ |
|---|---|---|
| `CreateSession` | `{}` | `{session_id, token}` |
| `DestroySession` | `{session_id}` | `{}` |
| `ListSessions` | `{}` | `{sessions[]: {session_id, created_at, last_activity}}` |
| `OpenImage` | `{session_id, image_path, mode=READ\|WRITE}` | `{image_id, root_guid}` |
| `DumpTree` | `{image_id, format=TEXT\|TSV}` | `{text}` |
| `ListItems` | `{image_id, filter?}` | `{items[]: {path, type, subtype, guid, offset, size, name}}` |
| `FindItem` | `{image_id, target}` | `{item_id}` |
| `Insert` | `{image_id, target, ffs_path, mode=INTO\|BEFORE\|AFTER}` | `{item_id}` |
| `Remove` | `{image_id, target}` | `{}` |
| `Replace` | `{image_id, target, ffs_path, body_only=false}` | `{item_id}` |
| `Rebuild` | `{image_id, target}` | `{}` |
| `SetSetupItemVisibility` | `{image_id, item_id, visible=true}` | `{}` |
| `SaveImage` | `{image_id, output_path}` | `{}` |

`target` — строка, разбирается как в UEFIEdit:
- `GUID` — `5C60F367-A505-419A-859E-2A4FF6CA6FE5` — поиск по GUID (заголовок файла, FvName, GUIDed-секция)
- `PATH` — `0/2/207/1/0` — спуск по дереву по индексам дочерних элементов
- `GUID:T` — `899407D7-...:0x10` — поиск файла по GUID, затем первая секция типа T (hex)
- `GUID:T:N` — то же, но N-я (0-based) секция типа T

### Семантика

- Несколько edit-методов можно вызывать до `SaveImage` (накапливаемые изменения в дереве).
- При `OpenImage` артефакт (копия образа) пишется в `${UEFIPATCHER_DATA}/sessions/<session_id>/images/<image_id>.bin`.
- `SaveImage` собирает дерево (`builder`) и записывает результат по `output_path`.
- `mode=READ` открывает образ без копирования; `mode=WRITE` копирует и позволяет модификации.

## Модели данных

### In-memory (`uefi-engine::parser`)

`FfsNode` — узел дерева UEFI:
- `guid: Option<Guid>` — GUID узла (для FFS-файлов, FvName, GUIDed-секций)
- `type: u8` — тип элемента (FFS file type / section type)
- `subtype: u8` — подтип (для section type)
- `offset: u64` — смещение в исходном образе
- `size: u64` — полный размер (заголовок + тело)
- `body_size: u64` — размер тела без заголовка
- `header: Vec<u8>` — байты заголовка as-is
- `body: Vec<u8>` — байты тела as-is
- `children: Vec<FfsNode>` — дочерние узлы (для контейнеров: FirmwareVolume, FFS-файлы с секциями, compressed-секции)
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
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,
    token         TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);

CREATE TABLE artifacts (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    path       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_artifacts_session ON artifacts(session_id);
CREATE INDEX idx_sessions_last_activity ON sessions(last_activity);
```

Файлы артефактов: `${UEFIPATCHER_DATA}/sessions/<session_id>/images/<image_id>.bin`.

Менеджер сессий: фоновая задача каждые `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` секунд удаляет сессии с `last_activity < now - TTL` (TTL = `UEFIPATCHER_SESSION_TTL_SECS`). Удаляет файлы из `${UEFIPATCHER_DATA}/sessions/<session_id>/` и строки из БД.

### Конфигурация (переменные окружения)

| Переменная | Назначение | По умолчанию |
|---|---|---|
| `UEFIPATCHER_DATA` | корень данных | `${XDG_DATA_HOME}/uefipatcher` или `~/.local/share/uefipatcher` |
| `UEFIPATCHER_SOCK` | путь unix-сокета | `/run/uefipatcher.sock` или `${XDG_RUNTIME_DIR}/uefipatcher.sock` |
| `UEFIPATCHER_SESSION_TTL_SECS` | TTL сессии | `864000` (10 дней) |
| `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` | интервал сборщика | `3600` (1 час) |

## Этапы реализации

1. **Скелет workspace**: Cargo workspace, крейты `uefi-proto`, `uefi-engine`, `uefi-cli`; `engine.proto` с заглушками методов; tonic-генерация; unix-сокет сервер поднимается, `CreateSession`/`DestroySession` работают.
2. **Парсер**: чтение UEFI-образа → дерево `FfsNode`. Референс — UEFITool `FfsParser`. Покрытие: FirmwareVolume, FFS-файлы, секции (включая PE32, GUIDed, compressed). `DumpTree`/`ListItems` работают на реальных образах.
3. **Target-поиск**: парсинг строки `target` (GUID/PATH/GUID:T/GUID:T:N) → `FindItem`.
4. **Builder**: сборка дерева в байты, выравнивание (padding/gap), перестройка дерева. Референс — UEFITool `FfsBuilder`. `SaveImage` работает; round-trip (parse→build) бинарно идентичен исходнику.
5. **Модификации**: `Insert` (into/before/after), `Remove`, `Replace` (full/body), `Rebuild`. Тесты на вставку/удаление/замену.
6. **Setup-visibility**: парсинг IFR Setup-форм (референс ifrExtractor), `SetSetupItemVisibility`. Только скрытие/показ готовых пунктов.
7. **Хранилище и сессии**: SQLite (схема выше), артефакты на диске, TTL 10 дней, фоновый GC, токены авторизации.
8. **CLI-минимум**: `uefi-cli <image> dump|list|save|insert|remove|replace|rebuild|set-visibility` — smoke-тест RPC, как у UEFIEdit. Полный CLI — цикл 2.
9. **Контейнеризация**: Dockerfile для `uefi-engine`, docker-compose пример (движок общается через unix-сокет в volume).

## Риски и ограничения

- **Несоответствие парсера UEFITool** — binary-различия в round-trip. Мера: фиксировать тестовые образы; сравнивать с UEFITool 0.28.8; в случае багов сверяться с edk2.
- **Объём портируемой логики UEFI** — много типов секций/сжатий. Мера: поддерживать только реально встречающиеся (LZMA/EFI fattest); неизвестные секции сохранять as-is.
- **Родная реализация на Rust длиннее FFI** — мер по срокам нет. Мера: этапы 2-5 делать поэтажно, после каждого — бинарный тест round-trip; сверяться с UEFITool и edk2.
- **gRPC над unix-сокетом** — меньше примеров, чем TCP. Мера: tonic поддерживает `Endpoint::from_unix`; smoke-тест на раннем этапе 1.
- **TTL 10 дней** — артефакты накапливаются. Мера: GC раз в час; ручной `DestroySession`; документация по `UEFIPATCHER_DATA`.
- **Setup-visibility без NVRAM** — пользователь может ожидать полного редактирования Setup. Мера: явно зафиксировать в roadmap, что NVRAM и новые пункты — цикл 6.

## Решения (фиксация)

- Стек: Rust, tonic gRPC over unix-сокет, SQLite.
- Парсер UEFI: родная реализация на Rust (UEFITool 0.28.8 как референс).
- Менеджер сессий: встроен в движок (не отдельный процесс).
- Хранилище: SQLite + файлы в `${UEFIPATCHER_DATA}`; TTL по умолчанию 10 дней, GC раз в час.
- gRPC API: `EngineService` с методами-командами.
- Цикл 1 = движок + setup-visibility (только видимость) + CLI-минимум. TUI/WebUI/NVRAM/новые пункты — roadmap.
- Структура: Cargo workspace (`uefi-proto`, `uefi-engine`, `uefi-cli`).