# AGENTS.md — инструкции для AI-агента при реализации UEFIPatcher

## Контекст проекта

UEFIPatcher — многокомпонентное приложение для модификации UEFI BIOS. Rust workspace (edition 2024) + SvelteKit WebUI.

## Структура планирования

- `roadmap.md` — декомпозиция на циклы, зависимости, статусы
- `docs/superpowers/specs/` — дизайн-спеки каждого цикла
- `docs/superpowers/plans/` — детальные планы реализации (TDD, пошаговые задачи)
- `TODO.md` — отложенные и несрочные правки (deprecated-код, известные дефекты, пробелы в API). Сюда же фиксируются находки ревизии CLI/кода, пока по ним не принят отдельный план.

## Порядок реализации (по приоритетам)

1. **Цикл 1** (UEFI Engine) — `docs/superpowers/plans/2026-07-22-uefi-engine.md` (19 задач)
2. **Цикл 2** (CLI) — `docs/superpowers/plans/2026-07-22-uefi-cli.md` (12 задач)
3. **Цикл 3** (TUI) — `docs/superpowers/plans/2026-07-22-uefi-tui.md` (8 задач)
4. **Цикл 6** (Расширенный Setup) — `docs/superpowers/plans/2026-07-22-uefi-setup-advanced.md` (8 задач)
5. **Цикл 5+7** (WebUI + шлюз) — `docs/superpowers/plans/2026-07-22-uefi-webui.md` (12 задач)

## Как реализовывать (для AI-агента)

### Правила работы с планом

1. **Читай план цикла перед стартом.** Открой соответствующий файл из `docs/superpowers/plans/`.
2. **Иди строго по задачам (Task 1, Task 2, ...).** Не перескакивай.
3. **Внутри задачи — по шагам (Step 1, Step 2, ...).** Каждый шаг = одно действие.
4. **Module-first rule (критично!):** сразу после создания файла модуля (например `ffs.rs`), добавь `pub mod ffs;` в `lib.rs`/`main.rs`/`mod.rs` **В ТОМ ЖЕ ШАГЕ**, ДО запуска `cargo test`. Это исключает false-positive (0 из 0 тестов).
5. **TDD порядок:** (1) создать файл с тестами + объявить `mod` → (2) `cargo test` (падает) → (3) реализация → (4) `cargo test` (проходит) → (5) commit.
6. **Binary crates:** `main.rs` / `src/bin/*.rs` создаётся одновременно с `Cargo.toml` (минимальный `fn main() {}`).
7. **Один коммит на шаг** где указано `git commit`. Сообщения коммитов — из плана.
8. **Не добавляй комментарии в код** (кроме: ссылок на референс `file:line`; коротких rustdoc-контрактов `///` на pub/pub(crate)-функциях — что функция делает/чего НЕ делает + ссылка на спеку/отчёт-первоисточник). Narration-комментарии в теле кода («здесь мы делаем X») — по-прежнему нет.
9. **После каждой задачи** запускай `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`.
10. **Если тест падает** — исправляй, не двигайся дальше пока не пройдёт.
11. **Дефекты плана — отдельным коммитом ДО реализации.** Если в шаге обнаружено расхождение с реальностью (несуществующий API, неверный тип/сигнатура, код не компилируется, тест не может пройти на данных из плана, унаследованные баги из прошлых циклов, и т.п.) — не правь молча код под неправильный план и не вставляй fixup в implementation-коммит. Сначала отдельный коммит `docs: fix Task N in cycle X plan (<конкретные дефекты через запятую>)` с правкой `docs/superpowers/plans/*.md`, затем — реализация. История коммитов должна показывать, что именно выяснилось и где. Примеры: `0d2c7e1`, `4a8ef43`, `014fc58`.
12. **Container runtime** - если присутствует файл `/run/.containerenv`, значит разработка ведется в контейнере, проверить наличие настроенного `podman-remote`, альтернатива docker/podman-compose - `podman-remote kube play`

### Крейты (НЕ писать самописный парсинг!)

| Крейт | Назначение |
|-------|-----------|
| `uguid` | `Guid` — Display (`to_ascii_hex_lower`), FromStr (`try_parse`), serde. UPPERCASE wrapper: `g.to_string().to_ascii_uppercase()`. Не использовать struct literal с data1/data2/data3/data4 — их нет в uguid. |
| `r-efi` | UEFI runtime + HII типы: `r_efi::hii::*` (IFR-структуры: IfrFormSet, IfrForm, IfrCheckbox, IfrNumeric, IfrOneOf, IfrDefault, IfrVarstoreEfi, IfrEnd, etc.), opcode-константы (IFR_FORM_SET_OP, etc.). НЕ определять свои IFR-структуры. ВАЖНО: r-efi 7.0 НЕ содержит PI-констант (EFI_SECTION_*, EFI_FV_FILETYPE_*, EFI_FVH_SIGNATURE, EFI_FVB2_*) — они в `uefi-common::pi`. |
| `binrw` | `#[brw]`-макросы для декларативного описания binary-структур (FFS/section/FV заголовки). НЕ читать байты вручную по offset. |
| `object` | PE32 parsing (features: read_core, pe) для PEI/DXE модулей. |
| `lzma-rs` | LZMA декомпрессия compressed-секций. |

**Принцип:** не писать самописный byte-offset парсинг. Использовать binrw-макросы и типы из r-efi. Checksum — через `wrapping_add`/`wrapping_sub`.

### Референсы (читать при необходимости)

- `../refs/UEFITool-ai-fork/common/ffsparser.{h,cpp}` — парсер UEFI
- `../refs/UEFITool-ai-fork/common/ffsbuilder.{h,cpp}` — билдер
- `../refs/UEFITool-ai-fork/common/ffs.h` — FFS-структуры
- `../refs/UEFITool-ai-fork/UEFIEdit/uefiedit.{h,cpp}` — Target-парсинг, команды
- `../refs/IFRExtractor-RS/src/uefi_parser.rs` — IFR-парсер (логика)
- `../refs/UEFI-Editor/src/components/scripts/scripts.ts` — AMI-патчинг
- `../refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs` — эталонный ffs.rs (checksums, wrapping arithmetic)
- IFR-структуры: `r_efi::hii::*` (крейт r-efi, не читать заголовки C)

### Тестовые данные

- `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` — реальный 16MB BIOS-образ для валидации парсера/билдера на живых данных. Используется в `crates/uefi-engine/tests/real_image.rs` (тесты `#[ignore]` по умолчанию, запускаются явно при наличии файла). Путь из крейта: `../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin`.

### Команды проверки

```bash
cargo test --all                    # все тесты
cargo clippy --all -- -D warnings   # lint
cargo fmt --all -- --check          # форматирование
cd webui && npm run check           # svelte-check (цикл 5+7)
```

### Переменные окружения

- `UEFIPATCHER_DATA` — корень данных (по умолч. `~/.local/share/uefipatcher`)
- `UEFIPATCHER_SOCK` — unix-сокет движка (по умолч. `~/.local/state/uefipatcher/uefipatcher.sock`)
- `UEFIPATCHER_SESSION_TTL_SECS` — TTL сессии (864000 = 10 дней)
- `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` — интервал сборщика (3600 = 1 час)
- `UEFIPATCHER_PURGE_ARTIFACTS` — удалять артефакты при GC (false по умолчанию — безопасность данных приоритетнее)
- `UEFIPATCHER_GATEWAY_LISTEN` — адрес шлюза (0.0.0.0:8080)

### Фоновые процессы и сокет движка

- Перед запуском `uefi-engine` удаляй осевший сокет прошлой сессии: `rm -f "$UEFIPATCHER_SOCK"` (клиент на мёртвом сокете падает с `transport error (RPC_INTERNAL)` — это окружение, не код).
- При `RPC_INTERNAL` у `uefi-cli`: проверь жив ли движок (`pgrep`), удали сокет, перезапусти движок, повтори. Не диагностируй RPC-код.
- Не гаси фоновые процессы через `pkill -f <подстрока>`: паттерн совпадает с командной строкой собственного shell и убивает её (вместе с недовыполненной командой). Используй `pkill -x <имя>` или `pkill -f 'target/debug/<bin>'`.

### Docker/Podman конвенции

- Именование файлов: `<component>.containerfile` (НЕ `Dockerfile.X`)
- Базовый образ: `registry.fedoraproject.org/fedora:44`
- Rust builder: отдельный `docker/rust-builder.containerfile` (fedora:44 + rust toolchain)
- Все Rust-сборки наследуются от `rust-builder`

## Специфика для моделей с ограниченным контекстом

- **Не пытайся охватить весь план сразу.** Работай по одной задаче за раз.
- **Если задача большая** — разбей на подшаги, реализуй по частям.
- **Если не хватает контекста** — читай только нужные части через `read` с `offset`/`limit`.
- **Референсы читай точечно** — только нужные функции/структуры.
- **Если тест не проходит** — прочитай ошибку, прочитай код, исправь осознанно.
- **После каждого коммита** — `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`.
- **Не пиши весь крейт за один присест** — задача за задачей, коммит за коммитом.

## Старт

Начни с Цикла 1, Task 1: открой `docs/superpowers/plans/2026-07-22-uefi-engine.md`, найди `### Task 1`, выполняй по шагам до коммита. Затем Task 2, и так далее.
