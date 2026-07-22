# UEFIPatcher — План (цикл 1: UEFI Engine)

## Краткое описание

Серверный движок для модификации UEFI BIOS: парсинг образа в дерево, вставка/удаление/замена/перестройка FFS-файлов и секций (DXE/PEI), управление видимостью пунктов Setup-меню, хранение сессий и артефактов, gRPC-сервер над unix-сокетом.

## Цели

- Реализовать парсинг UEFI-образа в дерево `FfsNode` и обратную сборку (round-trip бинарно идентичен исходнику)
- Поддержать модификации: вставка (into/before/after), удаление, замена (полная и только тело), пересборка
- Поддержать скрытие/показ существующих пунктов Setup-меню (без добавления новых и без NVRAM — это цикл 6)
- Реализовать хранилище сессий и артефактов: SQLite + файлы, TTL 10 дней, фоновый GC
- Реализовать gRPC-сервер `EngineService` над unix-сокетом с авторизацией по токену
- Предоставить минимальный CLI для smoke-тестирования RPC
- Подготовить Dockerfile и docker-compose пример

## Архитектура

Cargo workspace с тремя крейтами:

- `uefi-proto` — protobuf-схема `EngineService` и tonic-генерируемые типы. Общий для движка и всех клиентов (CLI/TUI/WebUI в последующих циклах).
- `uefi-engine` — ядро: модули `parser`, `builder`, `setup`, `session`, `storage`, `rpc`. Движок слушает unix-сокет, обслуживает gRPC-запросы клиентов.
- `uefi-cli` — минимальный CLI для smoke-теста RPC (полный CLI — цикл 2).

Связи: клиенты (CLI в цикле 1; TUI/WebUI в следующих циклах) подключаются к unix-сокету движка, авторизуются токеном, создают сессию, открывают образ, выполняют операции, сохраняют результат. Менеджер сессий встроен в движок (фоновой GC-task).

## Фронтенды

- [x] gRPC API: `EngineService` над unix-сокетом — см. раздел «API/Контракты»
- [~] CLI-минимум (`uefi-cli`): dump/list/save/insert/remove/replace/rebuild/set-visibility — подмножество для smoke-теста. Полный CLI — цикл 2 (roadmap)
- [ ] TUI — цикл 3 (roadmap)
- [ ] WebUI — цикл 5 (roadmap)

## Модули/Компоненты

### uefi-proto
- Описание: protobuf-схема `EngineService`, tonic-генерация типов. Общий контракт для всех клиентов.
- Зависимости: нет (только tonic/prost build-dependencies).
- API: публикует сгенерированные `engine.proto` типы и trait `engine_service_server::EngineService`.

### parser
- Описание: чтение UEFI-образа в дерево `FfsNode`. Референс — UEFITool 0.28.8 (`FfsParser`).
- Зависимости: `uefi-proto` (типы).
- API: `parse_image(bytes) -> Image`, `dump_tree(image, format) -> String`, `list_items(image, filter) -> Vec<Item>`.

### builder
- Описание: сборка дерева `FfsNode` обратно в байты, выравнивание (padding/gap), перестройка дерева. Референс — UEFITool `FfsBuilder`.
- Зависимости: `parser` (тип `FfsNode`).
- API: `build_image(image) -> bytes`.

### setup
- Описание: работа с Setup-секцией. Парсинг IFR (референс ifrExtractor). В цикле 1 — только видимость пунктов.
- Зависимости: `parser`.
- API: `set_item_visibility(image, item_id, visible) -> ()`.

### session
- Описание: менеджер сессий. Создание, обновление `last_activity`, удаление по TTL, фоновый GC.
- Зависимости: `storage`.
- API: `create_session() -> (session_id, token)`, `destroy_session(id)`, `touch_session(id)`, `list_sessions()`, `gc_loop()`.

### storage
- Описание: SQLite-хранилище (таблицы `sessions`, `artifacts`), артефакты на диске в `${UEFIPATCHER_DATA}`.
- Зависимости: нет (rusqlite).
- API: `open_db(path)`, CRUD для `sessions` и `artifacts`, `store_artifact(session_id, bytes) -> artifact_path`.

### rpc
- Описание: gRPC-сервер над unix-сокетом. Реализует `EngineService`. Авторизация по токену в metadata.
- Зависимости: `uefi-proto`, `parser`, `builder`, `setup`, `session`, `storage`.
- API: `serve(socket_path, engine) -> ()`.

### uefi-cli
- Описание: минимальный CLI для smoke-теста RPC. Подмножество команд UEFIEdit.
- Зависимости: `uefi-proto` (клиент).
- API: `uefi-cli <image> <command> [args]`.

## API/Контракты

Протокол: tonic gRPC over unix-сокет (`${UEFIPATCHER_SOCK}`).
Авторизация: `Authorization: Bearer <token>` в metadata. Токен генерируется при старте, пишется в `${UEFIPATCHER_DATA}/token` (0600).

### CreateSession
- Метод: gRPC `CreateSession`
- Параметры: нет
- Ответ: `{session_id: string, token: string}`
- Пример: `grpc_cli call ${SOCK} EngineService/CreateSession`

### DestroySession
- Метод: gRPC `DestroySession`
- Параметры: `session_id: string` (обязательный)
- Ответ: `{}`
- Ошибки: `NOT_FOUND` — сессия не найдена

### ListSessions
- Метод: gRPC `ListSessions`
- Параметры: нет (требует токен)
- Ответ: `{sessions: [{session_id, created_at, last_activity}]}`

### OpenImage
- Метод: gRPC `OpenImage`
- Параметры: `session_id: string` (обязательный), `image_path: string` (обязательный), `mode: enum READ|WRITE` (обязательный)
- Ответ: `{image_id: string, root_guid: string}`
- Ошибки: `NOT_FOUND` (сессия), `INVALID_ARGUMENT` (путь), `INTERNAL` (невалидный образ)
- При `mode=WRITE` образ копируется в `${UEFIPATCHER_DATA}/sessions/<id>/images/<image_id>.bin`.

### DumpTree
- Метод: gRPC `DumpTree`
- Параметры: `image_id: string` (обязательный), `format: enum TEXT|TSV` (обязательный)
- Ответ: `{text: string}`
- Ошибки: `NOT_FOUND` (image_id)

### ListItems
- Метод: gRPC `ListItems`
- Параметры: `image_id: string` (обязательный), `filter: string` (необязательный)
- Ответ: `{items: [{path, type, subtype, guid, offset, size, name}]}`

### FindItem
- Метод: gRPC `FindItem`
- Параметры: `image_id: string` (обязательный), `target: string` (обязательный; GUID | PATH | GUID:T | GUID:T:N)
- Ответ: `{item_id: string}`
- Ошибки: `NOT_FOUND` — элемент не найден

### Insert
- Метод: gRPC `Insert`
- Параметры: `image_id`, `target`, `ffs_path: string` (обязательный), `mode: enum INTO|BEFORE|AFTER` (обязательный)
- Ответ: `{item_id: string}`
- Ошибки: `NOT_FOUND`, `INVALID_ARGUMENT`

### Remove
- Метод: gRPC `Remove`
- Параметры: `image_id`, `target`
- Ответ: `{}`

### Replace
- Метод: gRPC `Replace`
- Параметры: `image_id`, `target`, `ffs_path`, `body_only: bool` (по умолчанию false)
- Ответ: `{item_id: string}`

### Rebuild
- Метод: gRPC `Rebuild`
- Параметры: `image_id`, `target`
- Ответ: `{}`

### SetSetupItemVisibility
- Метод: gRPC `SetSetupItemVisibility`
- Параметры: `image_id`, `item_id: string`, `visible: bool` (по умолчанию true)
- Ответ: `{}`
- Ошибки: `NOT_FOUND`, `INVALID_ARGUMENT` (элемент не является пунктом Setup)

### SaveImage
- Метод: gRPC `SaveImage`
- Параметры: `image_id`, `output_path: string` (обязательный)
- Ответ: `{}`
- Ошибки: `INTERNAL` — сборка не удалась

## Зависимости

### Внешние библиотеки (Rust)

- `tonic` (latest): gRPC-сервер/клиент над unix-сокетом
- `prost` (latest): protobuf-генерация
- `rusqlite` (latest): SQLite-хранилище
- `uuid` (latest): генерация session_id/image_id
- `clap` (latest): разбор аргументов CLI (крейт `uefi-cli`)
- `anyhow` / `thiserror`: обработка ошибок
- `tracing` / `tracing-subscriber`: логирование
- `tokio` (latest): async-runtime для tonic и GC-task

### Референсные исходники (не зависимости, только для чтения)

- `../refs/UEFITool` (0.28.8) — `FfsParser`, `FfsBuilder` — парсинг/сборка UEFI
- `../refs/IFRExtractor-RS` — парсинг IFR Setup-форм
- `../refs/edk2` — точка истины при багах/непонятностях

## Этапы реализации

1. **Скелет workspace**: Cargo workspace, крейты `uefi-proto`, `uefi-engine`, `uefi-cli`; `engine.proto` с заглушками; tonic-генерация; unix-сокет сервер поднимается; `CreateSession`/`DestroySession` работают.
2. **Парсер**: чтение UEFI-образа → `FfsNode`. Покрытие: FirmwareVolume, FFS-файлы, секции (PE32, GUIDed, compressed). `DumpTree`/`ListItems` работают на реальных образах.
3. **Target-поиск**: парсинг `target` (GUID/PATH/GUID:T/GUID:T:N) → `FindItem`.
4. **Builder**: сборка дерева в байты, выравнивание. `SaveImage` работает; round-trip бинарно идентичен исходнику.
5. **Модификации**: `Insert`, `Remove`, `Replace` (full/body), `Rebuild`. Тесты.
6. **Setup-visibility**: парсинг IFR, `SetSetupItemVisibility`. Только скрытие/показ.
7. **Хранилище и сессии**: SQLite, артефакты на диске, TTL 10 дней, GC, токены.
8. **CLI-минимум**: `uefi-cli` команды dump/list/save/insert/remove/replace/rebuild/set-visibility.
9. **Контейнеризация**: Dockerfile для `uefi-engine`, docker-compose пример.

## Риски и ограничения

- **Несоответствие парсера UEFITool** — binary-различия в round-trip. Мера: фиксировать тестовые образы; сравнивать с UEFITool 0.28.8; сверяться с edk2.
- **Объём портируемой логики UEFI** — много типов секций/сжатий. Мера: поддерживать только реально встречающиеся (LZMA/EFI fattest); неизвестные секции сохранять as-is.
- **Родная реализация на Rust длиннее FFI** — этапы 2-5 делать поэтажно, после каждого — бинарный тест round-trip.
- **gRPC над unix-сокетом** — меньше примеров. Мера: smoke-тест на этапе 1 (tonic `Endpoint::from_unix`).
- **TTL 10 дней** — накопление артефактов. Мера: GC раз в час; ручной `DestroySession`; документация `UEFIPATCHER_DATA`.
- **Setup-visibility без NVRAM** — ожидание полного Setup. Мера: явно зафиксировать в roadmap, что NVRAM и новые пункты — цикл 6.