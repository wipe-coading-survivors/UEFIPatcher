# AGENTS.md — инструкции для AI-агента при реализации UEFIPatcher

## Контекст проекта

UEFIPatcher — многокомпонентное приложение для модификации UEFI BIOS. Rust workspace + SvelteKit WebUI.

## Структура планирования

- `roadmap.md` — декомпозиция на циклы, зависимости, статусы
- `docs/superpowers/specs/` — дизайн-спеки каждого цикла
- `docs/superpowers/plans/` — детальные планы реализации (TDD, пошаговые задачи)
- `PLANNING.md`, `IMPLEMENTATION.md`, `TESTING.md` — документы цикла 1

## Порядок реализации (по приоритетам)

1. **Цикл 1** (UEFI Engine) — `docs/superpowers/plans/2026-07-22-uefi-engine.md` (18 задач)
2. **Цикл 2** (CLI) — `docs/superpowers/plans/2026-07-22-uefi-cli.md` (12 задач)
3. **Цикл 3** (TUI) — `docs/superpowers/plans/2026-07-22-uefi-tui.md` (9 задач)
4. **Цикл 6** (Расширенный Setup) — `docs/superpowers/plans/2026-07-22-uefi-setup-advanced.md` (8 задач)
5. **Цикл 5+7** (WebUI + шлюз) — `docs/superpowers/plans/2026-07-22-uefi-webui.md` (12 задач)

## Как реализовывать (для AI-агента)

### Правила работы с планом

1. **Читай план цикла перед стартом.** Открой соответствующий файл из `docs/superpowers/plans/`.
2. **Иди строго по задачам (Task 1, Task 2, ...).** Не перескакивай.
3. **Внутри задачи — по шагам (Step 1, Step 2, ...).** Каждый шаг = одно действие.
4. **TDD:** сначала failing test → запусти (упал) → реализация → запусти (прошёл) → коммит.
5. **Один коммит на шаг** где указано `git commit`. Сообщения коммитов — из плана.
6. **Не добавляй комментарии в код** (кроме ссылок на референс `file:line`).
7. **После каждой задачи** запускай `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`.
8. **Если тест падает** — исправляй, не двигайся дальше пока не пройдёт.

### Референсы (читать при необходимости)

- `../refs/UEFITool-ai-fork/common/ffsparser.{h,cpp}` — парсер UEFI
- `../refs/UEFITool-ai-fork/common/ffsbuilder.{h,cpp}` — билдер
- `../refs/UEFITool-ai-fork/common/ffs.h` — FFS-структуры
- `../refs/UEFITool-ai-fork/UEFIEdit/uefiedit.{h,cpp}` — Target-парсинг, команды
- `../refs/IFRExtractor-RS/src/uefi_parser.rs` — IFR-парсер
- `../refs/UEFI-Editor/src/components/scripts/scripts.ts` — AMI-патчинг
- `../refs/edk2/MdePkg/Include/Uefi/UefiInternalFormRepresentation.h` — IFR-структуры

### Команды проверки

```bash
cargo test --all                    # все тесты
cargo clippy --all -- -D warnings   # lint
cargo fmt --all -- --check          # форматирование
cd webui && npm run check           # svelte-check (цикл 5+7)
```

### Стек

- Rust: tonic, prost, rusqlite, tokio, clap, ratatui, axum, serde, uuid, anyhow, thiserror, tracing
- TypeScript: SvelteKit, vite, svelte-check
- gRPC over unix-сокет (engine), REST /api/v1/ (gateway)
- SQLite (сессии, артефакты), TTL 10 дней
- Docker/podman (контейнеры)

### Переменные окружения

- `UEFIPATCHER_DATA` — корень данных (по умолч. `~/.local/share/uefipatcher`)
- `UEFIPATCHER_SOCK` — unix-сокет движка (по умолч. `~/.local/state/uefipatcher/uefipatcher.sock`)
- `UEFIPATCHER_SESSION_TTL_SECS` — TTL сессии (864000 = 10 дней)
- `UEFIPATCHER_GATEWAY_LISTEN` — адрес шлюза (0.0.0.0:8080)

## Специфика для моделей с ограниченным контекстом (Qwen3-Coder-30B)

- **Не пытайся охватить весь план сразу.** Работай по одной задаче за раз.
- **Если задача большая** (например Task 6 цикла 1 — парсер секций) — разбей на подшаги mentally, реализуй по частям.
- **Если не хватает контекста** на весь файл — читай только нужные части через `read` с `offset`/`limit`.
- **Референсы читай точечно** — только нужные функции/структуры, не весь файл.
- **Если тест не проходит** — не пытайся угадать, прочитай ошибку, прочитай код, исправь осознанно.
- **После каждого коммита** — короткая проверка: `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`.
- **Не пиши весь крейт за один присест** — задача за задачей, коммит за коммитом.

## Старт

Начни с Цикла 1, Task 1: открой `docs/superpowers/plans/2026-07-22-uefi-engine.md`, найди `### Task 1`, выполни Step 1 → Step 2 → ... → Step 7 (коммит). Затем Task 2, и так далее.