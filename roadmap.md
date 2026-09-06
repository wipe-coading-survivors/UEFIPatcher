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

## Технологический стек (общий)

| Крейт | Версия | Назначение |
|-------|--------|-----------|
| `uguid` | 2.2.1 (serde) | `Guid` с Display/FromStr/serde. UEFI mixed-endian. UPPERCASE wrapper для индустриального формата. |
| `r-efi` | 7.0 | UEFI спецификация: `base::Guid`, `hii::*` (IFR-структуры, opcode-константы, package types). no_std. |
| `binrw` | 0.15 (std) | Declarative binary parsing/writing через `#[brw]`-макросы. |
| `object` | 0.39 (read_core, pe) | PE32 parsing для PEI/DXE модулей. |
| `lzma-rs` | 0.3 | LZMA декомпрессия. |
| `tonic` | 0.12 | gRPC over unix-сокет. |
| `rusqlite` | 0.31 (bundled) | SQLite хранилище. |
| `clap` | 4 (derive, env) | CLI. |
| Rust edition | 2024 | |

## Циклы

### Цикл 1 — UEFI Engine (план готов, к реализации)

- **Scope**: серверный движок (п.4.1) + setup-visibility (часть п.1.2, только видимость) + крейт `uefi-common` (state/error) + engine binary с clap CLI + CLI-минимум для smoke-теста RPC.
- **Зависимости**: нет.
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-engine-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-engine.md`
- **Статус**: план реализации готов (19 задач TDD), к исполнению.
- **Стек**: Rust edition 2024, uguid + r-efi + binrw + object, tonic gRPC over unix-сокет, SQLite.
- **Что включено**: крейт `uefi-common` (state.rs, error.rs — скелет, наполняется в цикле 2); парсер UEFI-образа → дерево `FfsNode` (через binrw); builder (сборка обратно); модификации (insert/remove/replace/rebuild, с поддержкой artifact_id); экстракция/импорт/экспорт артефактов; SetSetupItemVisibility (IFR через `r_efi::hii`); именованные сессии (name = CWD без symlink resolution); хранилище сессий+артефактов (TTL 10 дней, GC с `--purge-artifacts` по умолчанию false); gRPC-сервер `EngineService`; engine binary (`src/bin/engine.rs`, clap CLI); токены авторизации.
- **Что НЕ включено**: полный CLI, TUI, WebUI, новые пункты Setup, NVRAM, grpc-шлюз.

### Цикл 2 — Полный CLI (план готов, к реализации)

- **Scope**: полный CLI (п.2.1) для скриптования, аналог UEFIEdit, но свой rust-idiomatic синтаксис.
- **Зависимости**: цикл 1 (gRPC-контракт `uefi-proto`, `uefi-common`).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-cli-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-cli.md`
- **Статус**: план реализации готов (12 задач TDD), к исполнению.
- **Что включено**: наполнение `uefi-common` (state.rs, error.rs — полная реализация); команды session (init/list/destroy, name=CWD), image (open/switch/close/dump/list/find/save, extract/export/import/artifacts), edit (insert/remove/replace/rebuild, --from-artifact), setup (set-visibility/list-items); клиентская сессия в `.uefipatcher` (TOML) в CWD; JSON/text/tsv вывод.
- **Вопросы для brainstorm**: разрешены — синтаксис, state, вывод, lifecycle согласованы.

### Цикл 3 — TUI (план готов, к реализации)

- **Scope**: TUI (п.2.2) с ANSI + UTF-8 (иконки, псевдографика), аналог yazi/nvim.
- **Зависимости**: цикл 1 (gRPC-контракт, `uefi-common`), цикл 2 (переиспользование commands).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-tui-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-tui.md`
- **Статус**: план реализации готов (8 задач TDD), к исполнению.
- **Что включено**: крейт `uefi-tui` (ratatui + crossterm); `uefi-common` уже существует из цикла 1 (не нужен шаг извлечения); vim-like режимы; `:`-command-line для всех операций движка (включая :extract/:export/:import/:artifacts); `:help` popup.

### Цикл 4 — Менеджер сессий (ОТМЕНЁН)

- **Scope**: менеджер сессий (п.4.3) как отдельный процесс.
- **Зависимости**: цикл 1.
- **Статус**: отменён. В цикле 1 менеджер сессий встроен в движок (TTL 10 дней, фоновый GC с --purge-artifacts, SQLite). Встроенного достаточно.

### Цикл 5+7 — WebUI + gRPC-шлюз (план готов, к реализации)

- **Scope**: WebUI (п.2.3, SvelteKit/TypeScript) + gRPC-шлюз (п.4.2, Rust/axum REST+WS прокси). Объединены — шлюз нужен для WebUI.
- **Зависимости**: цикл 1 (gRPC-контракт), цикл 6 (add-formset, опционально).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-webui-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-webui.md`
- **Статус**: план реализации готов (12 задач TDD), к исполнению.
- **Что включено**: крейт `uefi-gateway` (axum REST+WS, cookie→gRPC metadata, upload/download, artifact endpoints); `webui/` (SvelteKit SPA, tree-view, details, операции, setup add-formset); контейнеризация (`<component>.containerfile`, `registry.fedoraproject.org/fedora:44`, `rust-builder.containerfile`); тесты.

### Цикл 6 — Расширенный Setup (план готов, к реализации)

- **Scope**: доработка п.1.2 — добавление новых пунктов и разделов меню, управление NVRAM-переменными.
- **Зависимости**: цикл 1 (парсер IFR, SetSetupItemVisibility, ops::insert).
- **Spec**: `docs/superpowers/specs/2026-07-22-uefi-setup-advanced-design.md`
- **Plan**: `docs/superpowers/plans/2026-07-22-uefi-setup-advanced.md`
- **Статус**: план реализации готов (8 задач TDD), к исполнению.
- **Что включено**: JSON-схема; генерация IFR (FormSet/Form/VarStore/OneOf/CheckBox/Numeric/Ref/Text/Default) через `r_efi::hii` структуры; авто-добавление строк в HII String-пакет; сборка отдельного FFS; AMI-патчинг; gRPC-метод AddSetupFormSet. Использует `uguid::Guid` и `binrw`.

### Цикл 7 — gRPC-шлюз (объединён с циклом 5)

- **Статус**: объединён с циклом 5. Шлюз `uefi-gateway` реализован как часть цикла 5+7.

## Дуга Serial Console (S0–S5) — основная задача проекта

Полный AMI-функционал serial-консоли на HNX99TF (boot-вывод UEFI на
COM + redirect Setup-экрана, настройка из Setup) — флагманская цель из
`user-input.md`. Подход — «лестница доказательств»: каждая ступень
заканчивается железным гейтом (для S0/S1 — детерминированным
артефактом).

- **Spec (живой документ)**: `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md`
- **S0** — закрыта 2026-09-05: отчёт
  `docs/reports/2026-09-05-serial-s0-recon.md` (решения §8; гипотезы
  молчания (а)–(д) закрыты; целевой FV = FV1 DXE @0x890000; конфиг
  edk2-пары и развилка glue — вход S1). Аддендум спеки — §7.
- **S1** — закрыта 2026-09-06: отчёт
  `docs/reports/2026-09-06-serial-s1-build.md` (стенд
  `edk2-builder.containerfile` + `UefiPatcherSerialPkg`; SerialDxe +
  TerminalDxe + SerialConsoleGlue воспроизводимы (identical) и
  закоммичены как тест-данные; glue — развилка (а) с маркерами SC-S1,
  вход S2). Аддендум спеки — §7.
- **S2** — в ожидании E30 (кандидат собран 2026-09-06): отчёт
  `docs/reports/2026-09-06-serial-s2-e30-pack.md` (E30-candidate.bin
  sha256 `09f5e897…`, вставка тройки SerialDxe/TerminalDxe/Glue
  @0xB63B18 в хвост FV1, span 123 136 Б; инварианты — движковый
  гейт-тест + fv_audit, state-адаптация `e88691a`; протокол приёмки —
  отчёт §4–5). Вердикт E30 — за владельцем. **Текущая ступень.**
- **S3** — настройки serial в Setup (hijack/append + varstore; эталон
  геометрии $SPF — MNX99MR9A).
- **S4** — AMI-стек TermSrc+SerialIo с починенной привязкой (основной
  путь — своя сборка (б); патч донора (а) — резерв).
- **S5** — redirect Setup-экрана (политика AMITSE; легаси-трое не
  нужны, риск — CsmDxe отбор UART при CSM-буте).

Зависимости: построена на циклах 1–6 (движок, CLI, hijack-механика).
Ступени строго последовательны; каждая может уточняться аддендумом к
спеке (прецедент: hijack-v2 → v2.1).

## Связи между циклами

```
Цикл 1 (движок + uefi-common)
├── Цикл 2 (CLI) ──────── Цикл 3 (TUI)
├── Цикл 4 (session mgr, опц.)
├── Цикл 5 (WebUI) ────── Цикл 7 (grpc-шлюз, опц.)
└── Цикл 6 (расш. Setup)
```

Циклы 2 и 3 можно делать параллельно после цикла 1. Цикл 6 независим от 2/3/5.
