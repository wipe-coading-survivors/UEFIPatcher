# UEFIPatcher — Дизайн цикла 2 (Полный CLI)

## Краткое описание

Полный CLI `uefi-cli` для скриптования модификаций UEFI-образов через движок (цикл 1). Свой rust-idiomatic синтаксис с группами subcommands, клиентская сессия в `.uefipatcher` (TOML) в CWD, JSON-вывод по умолчанию для машиночитаемых команд.

## Цели цикла 2

- Реализовать полный набор команд для всех операций движка (сессии, образы, модификации, setup-видимость)
- Реализовать клиентскую сессию в `.uefipatcher` (TOML) в текущем каталоге: хранение `session_id`, `token`, `active_image_id`, `sock_path`
- Реализовать приоритет источника sock-пути: `--sock` > `UEFIPATCHER_SOCK` env > `sock_path` из state > по умолчанию (`${XDG_STATE_HOME}/uefipatcher/uefipatcher.sock` или `~/.local/state/uefipatcher/uefipatcher.sock`)
- Поддержать форматы вывода: JSON (по умолчанию для машиночитаемых), text, tsv — через `--format`
- Обеспечить unix-way скриптуемость (одна команда = один вызов; скриптование через shell)
- Обновить `roadmap.md` (отметить цикл 1 завершённым, цикл 2 — текущим)

## Архитектура

### Крейт `uefi-cli` (расширение из цикла 1)

```
crates/uefi-cli/
├── Cargo.toml
├── src/
│   ├── main.rs          # clap entry, dispatch, глобальные флаги, exit codes
│   ├── state.rs         # .uefipatcher TOML read/write, приоритет sock
│   ├── client.rs        # gRPC-клиент (обёртка над uefi-proto, токен в metadata)
│   ├── output.rs        # JSON/text/tsv форматирование
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── session.rs   # init/list/destroy
│   │   ├── image.rs     # open/switch/close/dump/list/find/save
│   │   ├── edit.rs      # insert/remove/replace/rebuild
│   │   └── setup.rs     # set-visibility/list-items
│   └── error.rs         # Exit codes, форматы ошибок
└── tests/
    ├── cli_integration.rs
    └── e2e.rs
```

### State-файл `.uefipatcher` (TOML)

```toml
session_id = "uuid"
token = "uuid"
active_image_id = "uuid"
sock_path = "/home/user/.local/state/uefipatcher/uefipatcher.sock"
```

### Зависимости

Использует `uefi-common` (создан в цикле 1):
- `uefi-common` (path) — state.rs (State, read_state, write_state, resolve_sock) и error.rs (AppError, ExitCode)
- `toml` (0.8) — через uefi-common
- `serde` + `serde_json` (1) — JSON-вывод
- `clap` (4, derive, env) — CLI-фреймворк
- `tonic`, `uefi-proto`, `tokio`, `anyhow` — уже из workspace

## Фронтенды

- [x] CLI `uefi-cli` — единственный фронтенд цикла 2
- [ ] TUI — цикл 3 (roadmap)
- [ ] WebUI — цикл 5 (roadmap)

## Команды

### Группа `session`

| Команда | Параметры | Описание |
|---|---|---|
| `session init [--sock PATH] [--force]` | `--sock` (override), `--force` (перезаписать существующий state) | CreateSession(name=CWD) на движке, запись `.uefipatcher`. **name = `env::var("PWD")`** (без symlink resolution) |
| `session list` | нет | ListSessions (требует токен из state или `--sock`) |
| `session destroy` | нет | DestroySession + удалить `.uefipatcher` |

### Группа `image`

| Команда | Параметры | Описание |
|---|---|---|
| `image open <PATH> [--mode read\|write]` | `PATH` (обязательный), `--mode` (по умолч. `read`) | OpenImage, записать `active_image_id` |
| `image switch <IMAGE_ID>` | `IMAGE_ID` (обязательный) | Сменить `active_image_id` |
| `image close` | нет | Очистить `active_image_id` (оставить сессию) |
| `image dump [--format text\|tsv]` | `--format` (по умолч. `text`) | DumpTree активного образа |
| `image list [--filter STR]` | `--filter` (необязательный) | ListItems активного образа |
| `image find <TARGET>` | `TARGET` (обязательный, GUID/PATH/GUID:T/GUID:T:N) | FindItem, вывести item_id |
| `image save <OUTPUT_PATH>` | `OUTPUT_PATH` (обязательный) | SaveImage активного образа |
| `image extract <TARGET> [--body-only]` | `TARGET`, `--body-only` (экстрагировать только тело, без заголовка) | ExtractArtifact → artifact_id |
| `image export <ARTIFACT_ID> [OUTPUT_PATH]` | `ARTIFACT_ID` (обязательный), `OUTPUT_PATH` (по умолч. текущий каталог) | ExportArtifact в файл |
| `image import <FILE_PATH>` | `FILE_PATH` (обязательный) | ImportArtifact из файла → artifact_id |
| `image artifacts` | нет | ListArtifacts — список артефактов сессии |

### Группа `edit`

| Команда | Параметры | Описание |
|---|---|---|
| `edit insert <TARGET> <FFS_PATH> [--mode into\|before\|after]` или `--from-artifact <ID>` | `TARGET`, `FFS_PATH` ИЛИ `--from-artifact`, `--mode` (по умолч. `into`) | Insert (из файла или артефакта) |
| `edit remove <TARGET>` | `TARGET` | Remove |
| `edit replace <TARGET> <DATA_PATH> [--body-only]` или `--from-artifact <ID>` | `TARGET`, `DATA_PATH` ИЛИ `--from-artifact`, `--body-only` | Replace (из файла или артефакта) |
| `edit rebuild <TARGET>` | `TARGET` | Rebuild |

### Группа `setup`

| Команда | Параметры | Описание |
|---|---|---|
| `setup set-visibility <ITEM_ID> [--visible\|--hidden]` | `ITEM_ID`, `--visible`/`--hidden` (взаимоисключающие, по умолч. `--visible`) | SetSetupItemVisibility |
| `setup list-items` | нет | ListItems с фильтром по Setup-формам (если движок поддерживает; иначе полный list) |

### Глобальные флаги

| Флаг | Env | По умолчанию | Описание |
|---|---|---|---|
| `--sock <PATH>` | `UEFIPATCHER_SOCK` | `${XDG_STATE_HOME}/uefipatcher/uefipatcher.sock` или `~/.local/state/uefipatcher/uefipatcher.sock` | Путь unix-сокета |
| `--format json\|text\|tsv` | нет | `json` (машиночитаемые), `text` (human) | Формат вывода |
| `--help` / `--version` | нет | нет | Стандартные |

### Приоритет sock-пути

От высокого к низкому:
1. Глобальный флаг `--sock <PATH>`
2. Переменная окружения `UEFIPATCHER_SOCK`
3. Значение `sock_path` из `.uefipatcher` (state-файл в CWD)
4. По умолчанию: `${XDG_STATE_HOME}/uefipatcher/uefipatcher.sock` или `~/.local/state/uefipatcher/uefipatcher.sock`

При `session init`: вычисленный sock-путь записывается в `sock_path`. Последующие команды используют его (если не переопределено `--sock`/env).

## State-файл lifecycle

- `session init` — создаёт/обновляет файл: `session_id`, `token`, `sock_path`. Если файл уже есть — предупреждение + `--force` для перезаписи. При недоступности движка — exit 2.
- `session destroy` — удаляет файл.
- `image open` — дописывает/обновляет `active_image_id`.
- `image switch` — обновляет `active_image_id`.
- `image close` — очищает `active_image_id`.
- Все команды кроме `session init` — читают state для `session_id`/`token`/`sock_path`/`active_image_id`. Если файл отсутствует — ошибка «запустите `uefi-cli session init`», exit 3.
- Команды, требующие образ (dump/list/find/save/edit/setup) — требуют `active_image_id`. Если нет — ошибка «нет активного образа, запустите `uefi-cli image open`», exit 3.

## Error handling

### Exit codes

| Код | Значение |
|---|---|
| 0 | Успех |
| 1 | Ошибка CLI/аргументов (clap) |
| 2 | Ошибка RPC/движка (UNAUTHENTICATED, NOT_FOUND, INTERNAL и т.д.) |
| 3 | Ошибка state-файла (отсутствует, битый TOML, нет активного образа) |

### Формат ошибок

- `--format json`: `{ "error": "сообщение", "code": "ENUM" }` в stderr
- `--format text`/`tsv`: `error: сообщение (ENUM)` в stderr

Коды ошибок (ENUM): `STATE_MISSING`, `STATE_CORRUPT`, `NO_ACTIVE_IMAGE`, `RPC_UNAUTHENTICATED`, `RPC_NOT_FOUND`, `RPC_INVALID_ARGUMENT`, `RPC_INTERNAL`, `IO_ERROR`.

## Тестирование

### Unit-тесты
- `state.rs`: чтение/запись `.uefipatcher`, приоритет sock (`--sock` > env > state > default), обработка битого TOML, отсутствие файла.
- `output.rs`: JSON/text/tsv форматирование, edge cases (пустой список, длинные пути, unicode).

### Integration-тесты (`cli_integration.rs`)
Запуск движка на tempdir + unix-сокет, полный flow:
- `session init` → создаёт `.uefipatcher`, state корректен
- `session list` → показывает сессию
- `image open` (write) → `active_image_id` в state
- `image dump` → непустой вывод
- `image list` → ненулевой список
- `image find <guid>` → item_id
- `edit insert` → элемент в dump
- `edit remove` → элемента нет в dump
- `image save` → файл создан
- `image close` → `active_image_id` пуст
- `session destroy` → `.uefipatcher` удалён

### E2E-тесты (`e2e.rs`)
- Round-trip: `image open` (write) → `image save` → бинарно идентичен исходнику
- No-op: `image open` → `edit insert` → `edit remove` → `image save` → dump идентичен исходному
- Multi-command: init → open → insert → set-visibility → save → destroy

### Цели покрытия
- `state.rs`, `output.rs`: ≥90%
- Команды (через integration): ≥70%

## Этапы реализации

1. **state.rs**: TOML-схема, чтение/запись, приоритет sock, обработка отсутствующего/битого файла. Unit-тесты.
2. **client.rs**: расширение gRPC-обёртки — все методы EngineService, передача токена в metadata.
3. **output.rs**: JSON/text/tsv форматтеры. Unit-тесты.
4. **commands/session.rs**: `init`/`list`/`destroy` + запись state. Integration-тесты.
5. **commands/image.rs**: `open`/`switch`/`close`/`dump`/`list`/`find`/`save` + обновление state. Integration-тесты.
6. **commands/edit.rs**: `insert`/`remove`/`replace`/`rebuild`. Integration-тесты.
7. **commands/setup.rs**: `set-visibility`/`list-items`. Integration-тесты.
8. **main.rs**: clap-структура, dispatch, глобальные флаги, exit codes.
9. **E2E-тесты**: round-trip, no-op, multi-command flow.

## Риски и ограничения

- **Цикл 1 не реализован ещё** — CLI зависит от gRPC-контракта `uefi-proto` (уже определён в плане цикла 1). Команды можно писать против контракта; integration-тесты потребуют запуска движка (цикл 1 должен быть реализован или замокан).
- **TOML-парсинг битых файлов** — некорректный TOML должен давать `STATE_CORRUPT`, не panic. Мера: `toml::from_str` с `Result`, graceful error.
- **Sock-приоритет в тестах** — тесты с разными источниками (env/state/flag) должны быть изолированы. Мера: `temp_env` или явная передача в конструктор.
- **JSON-вывод больших деревьев** — `image dump` на реальном BIOS может быть мегабайты. Мера: потоковый вывод (по строкам), не буферизация в памяти.
- **`setup list-items` без поддержки движка** — в цикле 1 движок не различает Setup-формы. Мера: `list-items` делает полный ListItems + клиентская фильтрация по type/subtype (Section с IFR-признаком), либо просто алиас к `image list` с пометкой в документации.

## Решения (фиксация)

- Синтаксис: свой rust-idiomatic, subcommands по группам (session/image/edit/setup).
- State-файл: `.uefipatcher` (TOML) в CWD с `session_id`, `token`, `active_image_id`, `sock_path`.
- Sock-приоритет: `--sock` > `UEFIPATCHER_SOCK` > state `sock_path` > `${XDG_STATE_HOME}/uefipatcher/uefipatcher.sock`.
- Активный образ: `image open` делает его активным; edit/setup/image-команды используют `active_image_id` из state.
- Вывод: JSON по умолчанию (машиночитаемые), text/tsv по `--format`.
- Скриптование: одна команда = один вызов, unix-way.
- Exit codes: 0 успех, 1 CLI, 2 RPC, 3 state.