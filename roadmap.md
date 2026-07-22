# UEFIPatcher — Roadmap

Декомпозиция проекта на циклы разработки по приоритетам из `user-input.md`.
Каждый цикл — отдельный brainstorm → spec → plan → implementation.
Зависимости указывают, какой цикл должен быть завершён перед стартом данного.

## Приоритеты (из user-input.md)

1. Основной компонент бизнес-логики, п.4.1 — движок
2. CLI, п.2.1
3. TUI, п.2.2
4. Менеджер сессий, п.4.3
5. WebUI, п.2.3

## Циклы

### Цикл 1 — UEFI Engine (план готов, к реализации)

- **Scope**: серверный движок (п.4.1) + setup-visibility (часть п.1.2, только видимость) + CLI-минимум для smoke-теста RPC.
- **Зависимости**: нет.
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-engine-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-engine.md`
- **Статус**: план реализации готов (18 задач TDD), к исполнению.
- **Стек**: Rust, tonic gRPC over unix-сокет, SQLite, родная реализация парсера UEFI (референс UEFITool 0.28.8).
- **Что включено**: парсер UEFI-образа → дерево `FfsNode`, builder (сборка обратно), модификации (insert/remove/replace/rebuild), SetSetupItemVisibility, хранилище сессий+артефактов (TTL 10 дней, GC), gRPC-сервер `EngineService`, токены авторизации.
- **Что НЕ включено**: полный CLI, TUI, WebUI, новые пункты Setup, NVRAM, grpc-шлюз, менеджер сессий как отдельный процесс.

### Цикл 2 — Полный CLI (план готов, к реализации)

- **Scope**: полный CLI (п.2.1) для скриптования, аналог UEFIEdit, но свой rust-idiomatic синтаксис.
- **Зависимости**: цикл 1 (gRPC-контракт `uefi-proto`).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-cli-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-cli.md`
- **Статус**: план реализации готов (12 задач TDD), к исполнению.
- **Что включено**: команды session (init/list/destroy), image (open/switch/close/dump/list/find/save), edit (insert/remove/replace/rebuild), setup (set-visibility/list-items); клиентская сессия в `.uefipatcher` (TOML) в CWD; приоритет sock `--sock` > env > state > default (`${XDG_STATE_HOME}/uefipatcher/uefipatcher.sock`); JSON/text/tsv вывод.
- **Вопросы для brainstorm**: разрешены — синтаксис, state, вывод, lifecycle согласованы.

### Цикл 3 — TUI (план готов, к реализации)

- **Scope**: TUI (п.2.2) с ANSI + UTF-8 (иконки, псевдографика), аналог yazi/nvim.
- **Зависимости**: цикл 1 (gRPC-контракт), цикл 2 (переиспользование state/commands).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-tui-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-tui.md`
- **Статус**: план реализации готов (9 задач TDD), к исполнению.
- **Что включено**: крейт `uefi-tui` (ratatui + crossterm); крейт `uefi-common` (state/error из uefi-cli); vim-like режимы (Normal/Command/Insert); 3 панели (дерево|детали|команды) + статус-бар + hint-бар; Nerd Font иконки по FfsType + Unicode псевдографика + цвета по Action; `:`-command-line для всех операций движка; `:help` popup; тесты (unit + integration через TestBackend + E2E round-trip).

### Цикл 4 — Менеджер сессий (ОТМЕНЁН)

- **Scope**: менеджер сессий (п.4.3) как отдельный процесс.
- **Зависимости**: цикл 1.
- **Статус**: отменён. В цикле 1 менеджер сессий встроен в движок (TTL 10 дней, фоновый GC, SQLite). Встроенного достаточно для текущих требований. Вынесение в отдельный процесс не требуется.

### Цикл 5+7 — WebUI + gRPC-шлюз (план готов, к реализации)

- **Scope**: WebUI (п.2.3, SvelteKit/TypeScript) + gRPC-шлюз (п.4.2, Rust/axum REST+WS прокси). Объединены — шлюз нужен для WebUI.
- **Зависимости**: цикл 1 (gRPC-контракт), цикл 6 (add-formset, опционально).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-webui-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-webui.md`
- **Статус**: план реализации готов (12 задач TDD), к исполнению.
- **Что включено**: крейт `uefi-gateway` (axum REST+WS, cookie→gRPC metadata, upload/download); `webui/` (SvelteKit SPA, tree-view, details, операции, setup add-formset); Docker (engine+gateway+webui); тесты (gateway integration + Playwright E2E).

### Цикл 6 — Расширенный Setup (план готов, к реализации)

- **Scope**: доработка п.1.2 — добавление новых пунктов и разделов меню, управление NVRAM-переменными (существующими и новыми).
- **Зависимости**: цикл 1 (парсер IFR, SetSetupItemVisibility, ops::insert).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-setup-advanced-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-setup-advanced.md`
- **Статус**: план реализации готов (8 задач TDD), к исполнению.
- **Что включено**: JSON-схема для описания FormSet/форм/пунктов; генерация IFR (FormSet/Form/VarStore/OneOf/CheckBox/Numeric/Ref/Text/Default); авто-добавление строк в HII String-пакет; сборка отдельного FFS с новым FormSet (аналог IntelRCSetup); обязательный AMI-патчинг setupdataBin (accessLevel/failsafe/optimal) + amitseSct (регистрация FormId); дефолты через EFI_IFR_DEFAULT (0=Optimized, 1=Failsafe); gRPC-метод AddSetupFormSet.
- **Вопросы для brainstorm**: разрешены — JSON-схема, отдельный FFS, обязательный AMI, авто-strings.

### Цикл 7 — gRPC-шлюз (объединён с циклом 5)

- **Статус**: объединён с циклом 5. Шлюз `uefi-gateway` реализован как часть цикла 5+7.

## Связи между циклами

```
Цикл 1 (движок)
├── Цикл 2 (CLI) ──────── Цикл 3 (TUI)
├── Цикл 4 (session mgr, опц.)
├── Цикл 5 (WebUI) ────── Цикл 7 (grpc-шлюз, опц.)
└── Цикл 6 (расш. Setup)
```

Циклы 2 и 3 можно делать параллельно после цикла 1. Цикл 5 может потребовать цикл 7, если WebUI не работает с unix-сокетом. Цикл 6 независим от 2/3/5.