# Анализ блокеров для планирования Плана B

> Дата: 2026-08-14
> Предмет: `TODO.md:96-121` «План B: IFR forms/strings extraction + слияние с setup_advanced»
> Цель: определить, какие задачи из `TODO.md` (engine/CLI и смежные) мешают
> приступить к **планированию** Плана B, а какие лишь кажутся блокерами.

## TL;DR

Жёстких блокеров для планирования Плана B **нет**. План B — это read-only
extraction (чтение IFR/строк), а не мутация. Датамодель, декомпрессия, proto,
CLI-скаффолдинг и форматтеры вывода уже готовы. Парсер прозрачно расжимает
LZMA/GUIDED, так что IFR- и string-секции внутри сжатия достижимы как обычные
`FfsNode.children`. Большая часть работы лежит *внутри* самого Плана B.

Единственная реальная связанная задача — **модель targeting'а для `form_id`**
(`TODO.md:188-195` + `TODO.md:623-628`): это design-вход, а не хард-блокер.

## Вердикт по готовности

| Слой | Статус | file:line |
|---|---|---|
| Proto messages | ГОТОВО | `crates/uefi-proto/proto/engine.proto:144-166` |
| Engine RPC handlers | **STUB (UNIMPLEMENTED)** | `crates/uefi-engine/src/rpc/server.rs:648-666` |
| Engine `setup` module fns | **ОТСУТСТВУЮТ** (только `set_item_visibility`) | `crates/uefi-engine/src/setup/mod.rs:17` |
| Engine `setup_advanced/` | существует, отдельным деревом (будет влит) | `crates/uefi-engine/src/setup_advanced/{string_pack,ifr_builder,...}.rs` |
| CLI command tree | ГОТОВО | `crates/uefi-cli/src/main.rs:175-202`, диспетч `:314-338` |
| CLI handlers | ГОТОВО (зовут RPC) | `crates/uefi-cli/src/commands/setup.rs:6-38` |
| CLI client wrappers | ГОТОВО | `crates/uefi-cli/src/client.rs:276-301` |
| CLI output formatters | ГОТОВО | `crates/uefi-cli/src/output.rs:112-153` |
| TUI ex-commands | **ОТСУТСТВУЮТ** (нет setup-ветки) | `crates/uefi-tui/src/commands.rs:112-441` |
| Gateway routes | **ОТСУТСТВУЮТ** (только legacy `/setup-items` type==67 hack) | `crates/uefi-gateway/src/routes/mod.rs:57-64`, `routes/setup.rs:30-43` |
| Gateway client wrappers | **ОТСУТСТВУЮТ** (только `setup_set_form_visibility`) | `crates/uefi-gateway/src/client.rs:193-210` |
| WebUI fetch | **ОТСУТСТВУЮТ** (legacy `listItems`) | `webui/src/routes/setup/+page.svelte:11-15`, `lib/api.ts` |
| Mock-серверы (cli/tui/gateway) | ГОТОВО (возвращают пустые vec) | `*/tests/mock_server.rs` |

## Что уже есть (можно опираться при планировании)

- **IFR-структуры из `r-efi`**: `IfrFormSet<N>` (`r-efi-7.0.0/src/hii.rs:584-591`)
  даёт FormSet GUID + title-string-id напрямую; `IfrForm` (`hii.rs:552-556`)
  даёт FormId + title-string-id. Поля маппятся на proto-контракт `FormInfo`.
- **`find_suppress_if_scopes`** — `crates/uefi-engine/src/setup/ifr.rs:7`;
  используется в `setup/mod.rs:35`. Механизм вычисления `visible` для Плана B.
- **`is_string_package` / `scan_sibt`** — `crates/uefi-engine/src/setup_advanced/string_pack.rs:73,92`.
  `scan_sibt` уже понимает SIBT_STRING_SCSU/UCS2/SKIP/DUPLICATE и обходит поток,
  но трекает только *next free ID*, **сбрасывая текст**. План B нужен параллельный
  walker, захватывающий `(string_id, text)` пары.
- **`IfrBuilder`** — `setup_advanced/ifr_builder.rs` — это **писатель**, не
  ридер. Не инвертируется для чтения. План B нужен новый IFR-ридер (opcode walker).
- **Декомпрессия прозрачна в дерево**: `parse_section`/`parse_sections`
  рекурсивно расжимают GUIDED/LZMA/COMPRESSION, дети появляются как обычные
  `FfsNode`. Тесты: `parse_guided_lzma_section_decompresses_children`
  (`section.rs:183`), `real_image_decompresses_lzma_sections`
  (`tests/real_image.rs:214-273`).
- **Baseline-коммит для onboarding**: `0470553bcc579af0eb72075533bc2c73f77d543f`.

## Единственная реальная связанная задача (design-вход)

### Unified path addressing + Positional `Target::Path` нестабилен на full-flash

- `TODO.md:188-195` (unified `/`-addressing)
- `TODO.md:623-628` (gap-capture сдвигает индексы Volume'ов)

**Почему связано:** Plan A определяет `FormInfo.form_id` как **path-target
секции** (`engine.proto:145`, `setup/mod.rs:17`), чтобы `setup set-visibility`
мог по нему работать. Но gap-aware round-trip (фикс issue V) сдвигает индексы
Volume'ов → позиционный path, который вернёт `SetupListForms`, может **не
разрешиться** на полном flash-образе.

**Что нужно решить до написания спеки:** позиционный path vs GUID-target
(`<guid>:<type>`, `<guid>/<index>`). Escape — стабильный GUID-targeting уже
существует. Это **design-вход, а не хард-блокер**: План B можно планировать,
зафиксировав выбор targeting-модели в спеке.

## Что выглядит блокером, но НЕ мешает (отсеять)

| Задача | Почему НЕ блокер |
|---|---|
| **Compression barrier / issue IV** (`TODO.md:419-539`) | Живёт **только в билдере** (`builder/mod.rs:109` `is_compressed_or_guided`), блокирует **мутации**. План B читает уже распакованное дерево — барьера для чтения нет. |
| **`node_name` не поднимает имя через GUIDED/LZMA** (`TODO.md:207-216`) | Display-баг. Title формы План B берёт из string-package по title-string-id, а не из `node_name`. |
| **issue II / II-bis / II-ter** (`TODO.md:266,315,352`) | Про мутации и pending-операции (prune, `ops::rebuild` перетирает Remove). К чтению IFR/строк отношения не имеет. |
| **ME/IFD opaque** (`TODO.md:618-621`) | IFR живёт в FFS-файлах внутри FV, не в ME/IFD. |
| **issue V (full-flash round-trip)** (`TODO.md:541-600`) | Уже закрыт (gap-aware). Оставшиеся ограничения — про Volume-level Remove, Padding action — не релевантны чтению. |

## Мягкие соображения (ordering, не блокеры)

- **TUI migration** (`TODO.md:143-154`) — План B добавляет `:setup` ex-команды в
  TUI с нуля. Чище делать после устаканивания миграции, но задачи ортогональны.
- **Gateway + WebUI rework** (`TODO.md:156-170`) — обвязка Gateway/WebUI из
  Плана B (item 4) **пересекается** с этим rework'ом (маршруты `/forms`,
  `/strings` уже намечены в `TODO.md:164-167`). План B его **скармливает**, а не
  ждёт.
- **CLI-тесты проверяют только exit-code** (`TODO.md:89`) — когда План B оживит
  `setup form list`, стоит сразу добавить content-assertions. Качество, не блокер.

## Известные пробелы внутри самого Плана B (не внешние блокеры)

- **Нет IFR-ридера**: `setup/ifr.rs` (48 строк) обрабатывает только suppress-if
  scopes. Нужен opcode-walker для извлечения FormSet GUID / FormId /
  title-string-id.
- **Нет детектора «эта секция содержит IFR»**: IFR живёт в RAW-секции
  (`EFI_SECTION_RAW = 0x19`) как HII package list либо в `.rsrc` PE32. Требуется
  content-inspection (проба на `IFR_FORM_SET_OP = 0x0E`), а не проверка subtype.
- **Нет чтения строк по ID**: `string_pack::scan_sibt` считает next free ID, но
  сбрасывает текст. Нужен ридер-вариант.
- **`StringInfo.language`**: language живёт в HII package-list header, который
  текущий код не моделирует. Потребует парсинга обёртки package-list.
- **Мердж `setup_advanced/` ↔ `setup/`** (`TODO.md:117-120`): механический
  рефакторинг, но затрагивает рабочий `SetupFormSetAdd` RPC (`server.rs:679-699`),
  который использует `setup_advanced/`. Целевая иерархия:
  `crates/uefi-engine/src/setup/{mod,ifr,forms,strings,schema,ami_patcher,
  ffs_assembler,ifr_builder,string_pack}.rs` + обновление `lib.rs:9-10`.

## Итог

Приступить к планированию можно **сейчас**. Единственное, что стоит *решить до
написания спеки* — модель targeting'а для `form_id` (задачи `TODO.md:188-195` +
`TODO.md:623-628`), чтобы контракт `SetupListForms` не пришлось переделывать.
Всё остальное — либо внутри Плана B, либо не влияет на чтение.
