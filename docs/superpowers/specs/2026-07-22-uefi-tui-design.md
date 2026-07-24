# UEFIPatcher — Дизайн цикла 3 (TUI)

## Краткое описание

TUI-фронтенд `uefi-tui` для навигации и редактирования UEFI-образов через движок. Vim-like режимы (Normal/Command/Insert), 3 панели (дерево|детали|команды), Nerd Font иконки + цветовое кодирование, `:`-command-line для операций. Переиспользует `uefi-proto` (gRPC) и `uefi-common` (state/error, уже создан в цикле 1).

## Цели цикла 3

- Реализовать TUI-фронтенд `uefi-tui` как отдельный бинарник, подключающийся к движку через gRPC over unix-сокет
- `uefi-common` уже создан в цикле 1, переиспользуется напрямую (state `.uefipatcher`, error/exit-codes) — не нужен шаг извлечения
- Реализовать vim-like режимы: Normal (навигация), Command (`:`-команды), Insert (ввод target/path)
- Реализовать 3-панельный layout: дерево UEFI-образа слева, детали узла центр, command-line/хелп внизу, статус-бар сверху
- Реализовать Nerd Font иконки для типов узлов + Unicode псевдографику (├──, └──, │) + цветовое кодирование по Action
- Реализовать `:`-command-line для всех операций движка (open/dump/find/insert/remove/replace/rebuild/set-visibility/save/session)
- Обеспечить всегда видимый hint-бар (режим + ключевые клавиши) + `:help` popup

## Архитектура

### Новые крейты

```
crates/
├── uefi-common/          # ИЗ ЦИКЛА 1: state.rs + error.rs (переиспользуется напрямую, без извлечения)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── state.rs      # .uefipatcher TOML, sock-приоритет
│       └── error.rs      # ExitCode, ErrKind, AppError
├── uefi-tui/             # НОВЫЙ
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs       # entry, ratatui/crossterm setup, event loop
│       ├── app.rs        # App state machine, Mode enum, tree state
│       ├── input.rs      # crossterm events → AppEvent
│       ├── commands.rs   # :команды → парсинг → gRPC-вызовы
│       ├── theme.rs      # Nerd Font иконки, цвета по FfsType/Action
│       └── ui/
│           ├── mod.rs    # layout (3 панели + статус-бар + hint-бар)
│           ├── tree.rs   # рендер дерева UEFI-образа
│           ├── details.rs# детали выбранного узла
│           ├── cmdline.rs# command-line (:vim-стиль)
│           ├── status.rs # статус-бар
│           └── help.rs   # :help popup
```

### Зависимости

- `ratatui` (latest) — TUI-рендеринг
- `crossterm` (latest) — terminal backend, events, ANSI/UTF-8
- `uefi-proto` (path) — gRPC-контракт
- `uefi-common` (path, из цикла 1) — state, error
- `tonic`, `tokio` — gRPC-клиент
- `clap` (derive) — `--sock`, `--format` глобальные флаги
- `anyhow` — ошибки

### Связь с движком

TUI — gRPC-клиент к `EngineService` (как CLI в цикле 2). Подключается через unix-сокет, авторизуется токеном из `.uefipatcher` (state в CWD). Переиспользует sock-приоритет: `--sock` > env > state > default.

## Фронтенды

- [x] TUI `uefi-tui` — единственный фронтенд цикла 3
- [ ] WebUI — цикл 5 (roadmap)

## Режимы (vim-like)

| Mode | Описание | Переход |
|---|---|---|
| `Normal` | Навигация по дереву, expand/collapse, выбор узла | `:` → Command, `i` → Insert, `q` → quit |
| `Command` | Ввод `:команды` (Enter — выполнить, Esc — отмена) | Enter/Esc → Normal |
| `Insert` | Ввод текста (target/path) в prompt | Esc → Normal |

### Клавиши Normal mode

| Клавиша | Действие |
|---|---|
| `j`/`↓` | курсор вниз |
| `k`/`↑` | курсор вверх |
| `gg` | в начало списка |
| `G` | в конец списка |
| `Enter` | выбрать узел (details) |
| `Space` | expand/collapse узла |
| `:` | перейти в Command mode |
| `i` | перейти в Insert mode (поиск) |
| `q` | quit |
| `?` | help popup |

### Команды (`:`-command-line)

| Команда | Описание |
|---|---|
| `:open PATH [--mode read\|write]` | OpenImage, загрузить дерево |
| `:dump [--format text\|tsv]` | DumpTree (в pager/файл) |
| `:find TARGET` | FindItem, переместить курсор |
| `:insert TARGET FFS [--mode into\|before\|after]` | Insert |
| `:remove TARGET` | Remove |
| `:replace TARGET DATA [--body-only]` | Replace |
| `:rebuild TARGET` | Rebuild |
| `:set-visibility ITEM [--visible\|--hidden]` | SetSetupItemVisibility |
| `:save OUTPUT` | SaveImage |
| `:extract TARGET [--body-only]` | ExtractArtifact |
| `:export ARTIFACT_ID [PATH]` | ExportArtifact to current directory |
| `:import FILE` | ImportArtifact from filesystem |
| `:artifacts` | ListArtifacts |
| `:session init\|destroy\|list` | Управление сессией |
| `:help` / `:h` | Help popup |
| `:quit` / `:q` | Выход |

> **Note:** `:session init` передаёт CWD (`env::var("PWD")`) как имя сессии.

## Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ [NORMAL] session:s1 image:img1 | Ready                          │ ← статус-бар
├──────────────────────┬──────────────────────────────────────────┤
│                      │                                          │
│  📁 Image            │  Type:    Volume                         │
│  ├── 📦 Volume       │  Subtype: Ffs2                           │
│  │   ├── 🗎 File     │  GUID:    5C60F367-...                   │
│  │   │   ├── ⬡ PE32  │  Offset:  0x1000                        │
│  │   │   └── ⬡ UI    │  Size:    0x2000                        │
│  │   └── 🗎 File +   │  Action:  Insert (+)                    │
│  └── 📦 Volume       │                                          │
│                      │                                          │
├──────────────────────┴──────────────────────────────────────────┤
│ :open /tmp/bios.bin --mode write                                │ ← command-line
├─────────────────────────────────────────────────────────────────┤
│ NORMAL: j/k move · Space expand · :commands · ?help · q quit    │ ← hint-бар
└─────────────────────────────────────────────────────────────────┘
```

## State machine

```rust
pub enum Mode { Normal, Command, Insert }

pub struct App {
    pub mode: Mode,
    pub tree: Vec<TreeNode>,
    pub cursor: usize,
    pub selected: Option<String>,
    pub cmdline: String,
    pub cmdline_history: Vec<String>,
    pub status_msg: String,
    pub image_loaded: bool,
    pub client: Client,
    pub state: State,
    pub quit: bool,
}

pub struct TreeNode {
    pub path: String,
    pub depth: usize,
    pub node_type: FfsType,
    pub subtype: u8,
    pub guid: Option<String>,
    pub name: String,
    pub action: Action,
    pub expanded: bool,
    pub has_children: bool,
}
```

### Поток данных

1. `:open PATH` → gRPC `OpenImage` → `DumpTree` (text) → парс плоского дерева → `app.tree`
2. `j/k` → `cursor ± 1` → рендер tree panel (скролл к курсору)
3. `Enter` на узле → `selected = path` → `ListItems`/парс для details → details panel
4. `Space` → `expanded = !expanded` → re-flatten tree
5. `:insert/remove/...` → gRPC-вызов → `DumpTree` re-fetch → обновление `app.tree`
6. `:save OUTPUT` → gRPC `SaveImage` → status_msg "saved"

## Тема/Иконки

### Nerd Font иконки по FfsType

| FfsType | Иконка | Цвет |
|---|---|---|
| Image | `` | cyan |
| Volume | `` | blue |
| File | `` | green |
| Section (PE32) | `` | yellow |
| Section (GUIDed) | `` | magenta |
| Section (compressed) | `` | red |
| Section (UI) | `` | dark_gray |
| Section (raw) | `` | gray |
| Padding | `` | dark_gray |
| FreeSpace | `` | dark_gray |

### Action маркеры

| Action | Маркер | Цвет |
|---|---|---|
| NoAction | ` ` | default |
| Insert | `+` | green |
| Remove | `-` | red |
| Rebuild | `~` | yellow |
| Replace | `*` | blue |

### Псевдографика

- `├──`, `└──`, `│` — Unicode box-drawing для дерева
- `` (expanded) / `` (collapsed) — Nerd Folder иконки для узлов с детьми

## Ошибки

- gRPC-ошибки (UNAUTHENTICATED/NOT_FOUND/INTERNAL) → статус-бар красным: `error: ... (CODE)`, app остаётся в Normal mode
- State-ошибки (STATE_MISSING/NO_ACTIVE_IMAGE) → статус-бар: `run :session init first` / `:open an image first`
- Неверная команда → статус-бар: `unknown command: :foo, try :help`
- TUI не падает на ошибках — всегда показывает сообщение и возвращается в Normal mode

## Тестирование

### Unit-тесты
- `app.rs`: state machine transitions (Normal→Command→Insert), cursor movement, expand/collapse, парс плоского дерева из text-dump.
- `commands.rs`: парсинг `:команды` → аргументы, маппинг на gRPC-вызовы (mock-клиент).
- `theme.rs`: маппинг FfsType→icon/color.

### Интеграционные тесты
- Запуск TUI с mock-движком (как в цикле 2), проверка потока `:open`→`:insert`→`:save` через assert на app state (test backend ratatui, без реального терминала).

### E2E-тесты
- Реальный BIOS-образ, `:open`→ навигация → `:save` → бинарно идентичен (round-trip).

## Этапы реализации

0. **uefi-common**: уже создан в цикле 1 — переиспользуется напрямую, шаг извлечения не нужен.
1. **uefi-tui скелет**: Cargo.toml, main.rs (ratatui/crossterm setup), app.rs (Mode enum, App struct, базовый event loop).
2. **input.rs**: crossterm events → AppEvent (key/mode/quit).
3. **theme.rs**: Nerd Font иконки/цвета по FfsType/Action.
4. **ui/tree.rs**: рендер дерева + cursor + expand/collapse.
5. **ui/details.rs + status.rs + cmdline.rs + help.rs**: остальные панели.
6. **commands.rs**: парсинг `:`-команд, gRPC-вызовы, обновление app state.
7. **app.rs**: полный state machine (Normal/Command/Insert transitions).
8. **Integration-тесты**: mock-движок, полный flow через app state.
9. **E2E**: round-trip через TUI.

## Риски и ограничения

- **Nerd Font не установлен** — иконки покажутся как `□`/``. Мера: fallback на Unicode-символы при обнаружении (опция `--no-icons` или авто-детект через `TERM`); документировать требование Nerd Font.
- **ratatui test backend** — для integration-тестов без реального терминала. Мера: `ratatui::backend::TestBackend` — встроенный, подходит.
- **Большие деревья BIOS** — реальный образ может иметь сотни узлов. Мера: виртуальный скролл (рендер только видимых строк), lazy expand.
- **uefi-common уже существует из цикла 1** — не нужен шаг рефакторинга/извлечения. Мера: TUI зависит от uefi-common напрямую.
- **Async в TUI** — ratatui синхронный, gRPC async. Мера: tokio runtime в main, gRPC-вызовы через `tokio::runtime::Handle::block_on` или канал между input-loop и async-задачей.

## Решения (фиксация)

- Библиотека: ratatui + crossterm.
- Навигация: vim-like режимы (Normal/Command/Insert) + hint-бар + `:help` popup.
- Связь с движком: gRPC-клиент over unix-сокет (как CLI).
- Layout: 3 панели (дерево|детали|команды) + статус-бар + hint-бар.
- Операции: просмотр + `:`-command-line для всех операций движка.
- Иконки: Nerd Font по FfsType + Unicode псевдографика + цвета по Action.
- Общий код: крейт `uefi-common` (state + error) — уже существует из цикла 1, не нужен шаг рефакторинга/извлечения.
- Архитектура: отдельный крейт `uefi-tui`, переиспользует uefi-proto/uefi-common.

> **Тонкости имплементации:** uefi-common уже существует из цикла 1 — не нужен шаг рефакторинга/извлечения.