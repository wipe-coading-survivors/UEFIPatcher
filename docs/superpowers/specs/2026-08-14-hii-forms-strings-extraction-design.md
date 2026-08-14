# План B: HII Forms/Strings Extraction + слияние setup/setup_advanced

> **Дата:** 2026-08-14
> **Тема:** Реализация `HiiListForms` / `HiiListStrings` (IFR form + HII string
> extraction), слияние модулей `setup`/`setup_advanced` → единый модуль `hii`,
> полный ребрендинг `Setup*` → `Hii*` (proto + CLI).
> **Origin:** `TODO.md:96-121` «План B».
> **Связанные документы:**
> - `docs/superpowers/2026-08-14-plan-b-blockers-analysis.md` — анализ блокеров.
> - `docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md`
>   — План A (завёл stub'ы RPC `SetupListForms`/`SetupListStrings`).
> - **Baseline-коммит для onboarding:** `0470553bcc579af0eb72075533bc2c73f77d543f`
>   (актуальный `setup_advanced/` и `setup/`).

## Контекст и мотивация

UEFIPatcher — инструмент для модификации UEFI BIOS с упором на **CLI для
скриптования** (TUI и WebUI — надстройки над тем же RPC). Конечная цель проекта:
(1) получать консольный вывод процесса setup-конфигурации до загрузки OS/EfiApp
(аналог serial-over-lan в IPMI), (2) настраивать PCI-E bifurcation.

Существующий инструментарий в этой области разнороден и преимущественно
интерактивен; скриптуемый CLI отсутствует. План A (`a02654a`) подготовил
скаффолдинг: proto-контракты `FormInfo`/`StringInfo`, CLI-команды
`setup form list` / `setup string list`, форматтеры вывода — но сами RPC
`SetupListForms`/`SetupListStrings` возвращают `UNIMPLEMENTED`. План B оживляет
их и консолидирует разрозненные setup-модули.

**Тестовый образ:** `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` содержит две
form-секции — **Setup** и **Platform** (последняя содержит IntelRCSetup —
Intel Reference Code Setup). Ожидания real-image-тестов строятся на этом.

> **Вектор развития (не входит в План B):** PCI-E bifurcation — это *question*
> (`OneOf`/`Numeric`) внутри формы, а не сама форма. План B (формы + строки) —
> фундамент; enumerate questions / varstore-offsets / значения NVRAM — следующий
> цикл после B.

## Готовность (из анализа блокеров)

Жёстких блокеров нет. План B — read-only extraction (чтение IFR/строк), а не
мутация. Compression-barrier (issue IV) блокирует только **мутации** внутри
LZMA/GUIDED; парсер прозрачно расжимает их, так что IFR- и string-секции внутри
сжатия достижимы как обычные `FfsNode.children`. Декомпрессия, датамодель,
proto, CLI-скаффолдинг и форматтеры вывода — готовы.

## Решения (зафиксированы при планировании)

| # | Решение | Выбор |
|---|---------|-------|
| 1 | Targeting-модель `form_id` | **GUID-target** (`<file_guid>:<type>:<idx>`). Стабилен на full-flash. Требует расширить `set_item_visibility`/`find_item_mut` до GUID-target. |
| 2 | Scope client-wiring'а | **engine + CLI**. TUI/Gateway/WebUI — отдельные циклы (см. `TODO.md:143-170`). |
| 3 | Сколько языков в `HiiListStrings` | **Primary-язык** (первый найденный string-package). Поле `language` заполняется truthfully из header'а пакета (обычно English). |
| 4 | Порядок merge `setup_advanced/` ↔ `setup/` | **Merge первым**, затем новые ридеры в едином модуле. |
| 5 | Rename модуля | **`setup` → `hii`** (UEFI-термин «Human Interface Infrastructure», точно покрывает IFR + string-packages). |
| 6 | Масштаб rename | **Полный**: модуль + proto RPC/messages (`Setup*`→`Hii*`) + CLI noun (`setup`→`hii`). |

## Scope

**В scope:**
- Engine: IFR-reader (opcode walker) + HII string-reader (SIBT walker + language).
- Engine: реализация `HiiListForms`/`HiiListStrings` RPC (вместо `UNIMPLEMENTED`).
- Engine: слияние `setup_advanced/` в `hii/` (плоская иерархия).
- Engine: targeting-extension — `find_item_mut` для `Guid`/`GuidSection`,
  `set_item_visibility` принимает GUID-target.
- Engine/Proto/CLI: ребрендинг `Setup*` → `Hii*` (module, proto RPC+messages,
  CLI noun, клиенты cli/tui/gateway, mock'и, webui-фечи).
- CLI: оживление `hii form list` / `hii string list` / `hii form set-visibility`
  на реальных данных.

**Вне scope (отдельные циклы):**
- TUI setup ex-команды (`:setup ...`) — `TODO.md:143-154`.
- Gateway-маршруты `/forms` `/strings` + WebUI fetch — `TODO.md:156-170`.
- Per-form visibility (form_id → конкретный SuppressIf-scope) — фича низкого
  приоритета: видимость форм почти всегда открыта; section-level достаточно для v1.
- Enumerate IFR questions (OneOf/Numeric/CheckBox) + varstore-offsets + NVRAM —
  следующий цикл (фундамент для PCI-E bifurcation).
- Unified path addressing (`TODO.md:188-195`) — обходится GUID-target'ом.

## Архитектура

Подход: **один tree-walk, inline-парс** с **pure-парсерами** `fn(&[u8])` (TDD,
тестируются без `Image`). Tree-walk-driver тонкий, собирает `FormInfo`/
`StringInfo`. Альтернативы (two-phase locate→parse, parser-integrated markers)
отвергнуты: первая избыточна структурами, вторая рискует core-парсером (issue V).

### Целевой layout модуля `hii` (после merge + rename)

```
crates/uefi-engine/src/hii/
  mod.rs           — façade: pub mod-декларации, единый HiiError, set_item_visibility,
                     collect_forms, collect_strings, реэкспорты
  ifr.rs           — suppress-if (существующий) + НОВЫЙ opcode-walker (parse_form_package)
  forms.rs         — НОВО: collect_forms(image) → Vec<FormInfo>, детекция form-package,
                     построение GUID-target, резолв title
  strings.rs       — НОВО: collect_strings(image) → Vec<StringInfo>, language + SIBT-ридер
  formset_add.rs   — НОВЫЙ дом для add_setup_formset + хелперов (из setup_advanced/mod.rs)
  schema.rs        — из setup_advanced (FormSetSchema и т.д.)
  ami_patcher.rs   — из setup_advanced
  ffs_assembler.rs — из setup_advanced
  ifr_builder.rs   — из setup_advanced
  string_pack.rs   — из setup_advanced (writer add_strings; strings.rs — reader)
```

`lib.rs:9-10`: `pub mod setup;` + `pub mod setup_advanced;` → `pub mod hii;`.

**Error enum:** `SetupAdvancedError` вливается в единый `HiiError` (бывший
`SetupError`) с вариантами: `NotFound`, `NotASetupItem`, `InvalidIfr`,
`InvalidSchema(String)`, `StringPackageNotFound`, `AmiFilesNotFound`,
`IfrBuildError(String)`, `FfsAssemblyError(String)`. RPC-обработчик
`HiiFormSetAdd` меняет только путь ошибки (`.map_err`).

### IFR-walker — `hii/ifr.rs` + `hii/forms.rs`

**Pure-парсер в `ifr.rs`:**

```rust
pub struct FormSetInfo { pub guid: Guid, pub title: StringId, pub forms: Vec<RawForm> }
pub struct RawForm { pub form_id: FormId, pub title: StringId, pub suppressed: bool }
pub fn parse_form_package(body: &[u8]) -> Option<FormSetInfo>
```

- **Детекция** (`forms.rs`): `is_form_package(body) = body.len() >= 5 && body[3]
  == PACKAGE_FORMS && body[4] == IFR_FORM_SET_OP`. (`PACKAGE_FORMS = 0x02`,
  `IFR_FORM_SET_OP = 0x0E` из `r_efi::hii`.)
- **Walker** стартует с offset 4 (после `PackageHeader` = `length[3] + type[1]`).
  Читает `IfrOpHeader { op_code, length_and_scope }` (`r_efi::hii::IfrOpHeader`):
  `length = length_and_scope & 0x7F` (полная длина opcode'а, включая 2-байтный
  header, min 2), `scope = length_and_scope & 0x80`.
- **Scope-stack** (`Vec<u8>` opcode'ов): opcode с `scope`-битом пушит `op_code`,
  `IFR_END_OP` (`0x29`) попает.
- `IFR_FORM_SET_OP` (`0x0E`): guid по offset+2 (16 байт, `r_efi::hii::IfrFormSet`),
  `form_set_title: StringId` (u16) +18, `help` +20. Число class_guid выводится из
  `length` (ручное чтение — `r_efi::IfrFormSet<N>` const-generic). Заполняет
  `FormSetInfo.guid`/`title`.
- `IFR_FORM_OP` (`0x01`): `form_id: FormId` (u16) +2, `form_title: StringId` (u16)
  +4 (`r_efi::hii::IfrForm`). Записывает `RawForm { suppressed = scope_stack
  contains IFR_SUPPRESS_IF_OP (0x0A) }`.
- Неизвестные opcode'ы — skip на `length` (robustness; `tracing::trace`).
- `parse_form_package` возвращает `None` при отсутствии `IFR_FORM_SET_OP` или
  truncation.

**`collect_forms(image: &Image) -> Vec<FormInfo>`** (`forms.rs`):
рекурсивно обходит `image.root`; для каждой `Section` через `is_form_package`
вызывает `parse_form_package`; для каждого `RawForm` строит `FormInfo`:
- `form_id` = GUID-target секции: `<file_guid>:<section_subtype_hex>:<index>`
  (производный от GUID родительского `File` + subtype + index секции; формат
  `Target::GuidSection`). Несколько форм одного FormSet разделяют `form_id`.
- `formset_guid` = `FormSetInfo.guid.to_string().to_ascii_uppercase()`.
- `form_id_ifr` = `RawForm.form_id` (u16 → uint32 proto-поля; info-поле).
- `title` = резолв `RawForm.title` (`StringId`) через map StringId→text (из
  `collect_strings`); fallback `""` если ID не найден.
- `visible` = `!RawForm.suppressed`.

### String-reader — `hii/strings.rs`

**Pure-парсер:**

```rust
pub struct ParsedStringPackage { pub language: String, pub strings: Vec<(u16, String)> }
pub fn parse_string_package(body: &[u8]) -> Option<ParsedStringPackage>
```

- **Детекция:** `string_pack::is_string_package(body)` (существующая, `body[3]
  == PACKAGE_STRINGS` = 0x04).
- **language:** null-terminated ASCII из header-области `EFI_HII_STRING_PACKAGE_HDR`
  (после `StringInfoOffset`@8). r-efi **не моделирует** этот header. Точный
  start-offset подтвержать против `refs/IFRExtractor-RS/src/uefi_parser.rs` на
  этапе реализации (common AMI/edk2: offset 12). **[detail-to-confirm]**
- **SIBT-walk** из `StringInfoOffset` (переиспользует логику `string_pack::
  scan_sibt`, но захватывает текст вместо only-next-free-ID): `SIBT_STRING_SCSU`
  → ASCII до NUL; `SIBT_STRING_UCS2` → UTF-16LE до double-NUL, decode в UTF-8;
  `SIBT_STRINGS_*` → u16-count + блоки; `SIBT_SKIP1`/`SIBT_SKIP2` advancing ID
  без текста; `SIBT_DUPLICATE` advancing ID (текст = копия референса).
  StringId начинаются с 1, последовательные.
- `collect_strings(image: &Image) -> Vec<StringInfo>`: обходит дерево, берёт
  **первый** string-package (primary-язык), парсит, возвращает `Vec<StringInfo>
  { language, string_id, text }`.

### Targeting-extension — `parser/target.rs` + `hii/mod.rs`

- `find_item_mut` (`target.rs:87-105`) сейчас поддерживает только `Target::Path`
  (Guid/GuidSection → ошибка «only path supported for mutable»). **Добавить arms
  для `Target::Guid` и `Target::GuidSection`** (mirror логики `find_item:50-85`,
  mutable-версия). Это точечное, well-bounded расширение.
- `set_item_visibility` (`hii/mod.rs`) принимает GUID-target (не только Path):
  `find_item_mut` резолвит секцию, далее существующая логика
  `find_suppress_if_scopes` + `unsuppress`. Без proto-change (`item_id` — строка).
- `form_id` round-trip: `collect_forms` отдаёт `Target::GuidSection`-строку →
  `setup set-visibility <form_id>` парсит её → `set_item_visibility` резолвит.

## Семантика видимости (v1)

`set_item_visibility` оперирует на уровне **Section-узла** (`FfsType::Section`),
чье `body` содержит IFR-блоб всего FormSet'а: `find_suppress_if_scopes(&node.body)`
сканит весь поток, `unsuppress` нейтрализует первый scope. Гранулярность —
«весь IFR-блоб секции», а не отдельные формы.

`FormInfo.visible` при этом **per-form** (opcode-walker знает lexical
scope-stack): `false` если `IfrForm` лексически внутри активного `SUPPRESS_IF`.
После `unsuppress` форма выходит из scope → `visible=true` (корректный
round-trip). Несколько форм одного FormSet разделяют `form_id` (секция).

> Практически: видимость форм открыта в ~99% случаев → section-level достаточно
> для v1. Per-form visibility (form_id → конкретный SuppressIf-scope по байтовому
> offset) — TODO будущего цикла; механика уже частично есть (`find_suppress_if_
  scopes` возвращает `{start,end}`, `unsuppress(scope)` умеет конкретный scope).

## Rename-маппинг `Setup*` → `Hii*`

**Proto** (`crates/uefi-proto/proto/engine.proto`):

| Было | Станет |
|------|--------|
| `rpc SetupListForms(SetupListFormsRequest) returns (SetupListFormsResponse)` | `rpc HiiListForms(HiiListFormsRequest) returns (HiiListFormsResponse)` |
| `rpc SetupSetFormVisibility(SetupSetFormVisibilityRequest) returns (Empty)` | `rpc HiiSetFormVisibility(HiiSetFormVisibilityRequest) returns (Empty)` |
| `rpc SetupListStrings(SetupListStringsRequest) returns (SetupListStringsResponse)` | `rpc HiiListStrings(HiiListStringsRequest) returns (HiiListStringsResponse)` |
| `rpc SetupFormSetAdd(SetupFormSetAddRequest) returns (SetupFormSetAddResponse)` | `rpc HiiFormSetAdd(HiiFormSetAddRequest) returns (HiiFormSetAddResponse)` |
| messages `SetupListFormsRequest/Response`, `SetupSetFormVisibilityRequest`, `SetupListStringsRequest/Response`, `SetupFormSetAddRequest/Response` | `Hii*` аналогично |

`FormInfo`, `StringInfo` — **без** префикса, без изменений. `build.rs`
serde-attributes обновить (`engine.FormInfo`/`engine.StringInfo` — без изменений).

**Engine:** `crate::setup::`/`crate::setup_advanced::` → `crate::hii::` (server.rs,
ops, везде). Handler'ы переименованы: `setup_list_forms` → `hii_list_forms` и т.д.

**CLI:** noun `setup` → `hii`: `hii form list`, `hii form set-visibility
<form_id>`, `hii string list`. `commands/setup.rs` → `commands/hii.rs`;
`client.rs` `setup_list_forms` → `hii_list_forms`; `main.rs` диспетч;
`output.rs` `print_forms`/`print_strings` без изменений (operates на
`FormInfo`/`StringInfo`).

**TUI/Gateway/WebUI/Mocks:** все client-вызовы (`setup_list_forms`,
`setup_set_form_visibility`, `setup_list_strings`, `SetupFormSetAdd`) и
mock-stub'ы (`*/tests/mock_server.rs`) переименованы. WebUI fetch-вызовы (даже
через gateway) обновляются в рамках этого rename (механически), хотя глубокий
WebUI-rework — отдельный цикл.

## Data flow

```
CLI: hii form list
  → client.hii_list_forms(image_id)
  → gRPC HiiListForms
  → server.hii_list_forms: get_or_load_image → hii::collect_forms(&image)
       ├─ обходит дерево, на form-package-секциях → parse_form_package
       ├─ visible = !suppressed (scope-stack)
       └─ title резолвится через hii::collect_strings (StringId→text map)
  → HiiListFormsResponse { forms: Vec<FormInfo> }
  → output::print_forms (text/json)

CLI: hii form set-visibility <form_id> true
  → client.hii_set_form_visibility(image_id, form_id, true)
  → gRPC HiiSetFormVisibility
  → server: set_item_visibility(&mut image, form_id, true)
       ├─ parse_target(form_id) → Target::GuidSection
       ├─ find_item_mut (новый GuidSection arm) → Section-узел
       └─ find_suppress_if_scopes + unsuppress → mark_rebuild_to_root
  → flush_image (write-through)
```

## Testing

**Pure-парсеры (TDD, unit, synthetic bytes):**
- `parse_form_package`: synthetic IFR = `PackageHeader` + `IfrFormSet(guid,title)`
  + `IfrForm(id=1)` + `IfrForm(id=2)` (одна внутри `SUPPRESS_IF`) + `END`'ы.
  Assert: guid, 2 формы, suppressed-флаг per-form.
- `parse_string_package`: synthetic string-package (переиспользовать
  `make_string_package` fixture из `string_pack.rs` tests) — assert
  `(string_id, text)` пары + language.
- `is_form_package` / edge-cases (truncation, нет FORM_SET_OP → None).

**`collect_forms`/`collect_strings` на synthetic `Image`** (маленькое дерево с
form-package + string-package секциями).

**Targeting:** `find_item_mut` для `Target::GuidSection` (mutable-доступ к
section-узлу); `set_item_visibility` с GUID-target round-trip.

**Real-image** (`crates/uefi-engine/tests/real_image.rs`, `#[ignore]`):
`refs/fw/HNX99TF_200525_original_E5C88C6F.bin` — `collect_forms` находит **≥2
form-пакетов** (Setup + Platform/IntelRCSetup), FormSet GUID'ы присутствуют;
`collect_strings` — `strings.len() > 0`, `language` ≈ English (`en`/`en-US`/
`eng`). Базовые ожидания сверить с UEFITool на этапе реализации.

**RPC handler'ы** (если инфраструктура позволяет).

**CLI content-assertions** (soft-note из `TODO.md:89`): `hii form list` stdout
содержит title формы; не только exit-code.

## Error handling

- `collect_forms`/`collect_strings`: **best-effort, infallible** — read-only;
  malformed package skip + `tracing::warn`, return пустой vec/без этой записи.
  Не валим весь список из-за одного битого пакета.
- `set_item_visibility`: `NotFound` если target не резолвится; `NotASetupItem`
  если узел не Section.
- RPC: image-not-found → `Status::not_found` (консистентно с другими handler'ами).
- `HiiFormSetAdd` (бывший `SetupFormSetAdd`): error-path без изменений, только
  тип ошибки `HiiError`.

## Известные ограничения / TODO

- **language exact-offset** — подтвердить против `refs/IFRExtractor-RS` (см.
  String-reader). Если layout нестандартный — fallback `language = ""`.
- **Title resolution global-map** — StringId→text строится из первого
  string-package (primary). Если form-package ссылается на StringId из другого
  package-list'а — fallback `""`. Для AMI/edk2 (один package-list на FFS)
  корректно.
- **Per-form visibility** — section-level в v1 (см. Семантика видимости).
- **Compression-aware robustness** — `collect_*` читает распакованное дерево;
  если form/string-секция внутри LZMA, она доступна как `FfsNode.children`
  (парсер расжал). Барьера для **чтения** нет.

## Реализация: конвенция по фазам (meta)

> Спецификация обычно не описывает план реализации, но здесь фиксируется
> сознательное решение о декомпозиции, принятое при планировании.

План B разбивается на **отдельные план-документы** (по одному на фазу) в
`docs/superpowers/plans/`. Каждая фаза:

- **3–7 задач** (допустимо до 7–9; если не укладываемся — декомпозиция по месту).
- **Каждая задача = 5–15 шагов** (TDD-порядок: тесты → реализация → проверка).
- Если фаза явно не помещается — принимаем решение о дополнительной декомпозиции
  прямо в процессе, фиксируя в плане.

**Предлагаемые фазы (каждая = свой plan-документ):**

1. **Merge + module rename** — `setup/` + `setup_advanced/` → `hii/` (плоская
   иерархия), `HiiError` enum, `lib.rs`. Mechanical refactor, всё компилируется.
2. **Proto/CLI rename `Setup*`→`Hii*`** — proto RPC+messages, CLI noun
   `setup`→`hii`, engine handlers, cli/tui/gateway clients, все mock_server.rs,
   webui fetch. Mechanical.
3. **IFR-reader + `HiiListForms`** — `ifr.rs` opcode-walker (`parse_form_package`),
   `forms.rs` (`collect_forms`, `is_form_package`, GUID-target, title resolution),
   RPC handler оживляется, pure-parser unit-тесты.
4. **String-reader + `HiiListStrings`** — `strings.rs` (`parse_string_package`,
   SIBT-walker, language), RPC handler оживляется, pure-parser unit-тесты.
5. **Targeting-extension + validation** — `find_item_mut` Guid/GuidSection arms,
   `set_item_visibility` GUID-target, real-image `#[ignore]` тесты (Setup +
   Platform/IntelRCSetup), CLI content-assertions.

Фазы 3 и 4 независимы по модулю, но обе зависят от 1 (merge) и 2 (rename).
Фаза 5 зависит от 3 (form_id для set-visibility). Порядок: 1 → 2 → (3, 4) → 5.
Конкретный порядок задач/шагов — в соответствующем plan-документе (writing-plans).

## Ссылки

- Proto: `crates/uefi-proto/proto/engine.proto:144-166` (`FormInfo`, `StringInfo`).
- Stub'ы RPC: `crates/uefi-engine/src/rpc/server.rs:648-666`.
- Существующий suppress-if: `crates/uefi-engine/src/setup/ifr.rs:7`.
- `set_item_visibility`: `crates/uefi-engine/src/setup/mod.rs:17-48`.
- `find_item_mut` (Path-only): `crates/uefi-engine/src/parser/target.rs:87-105`.
- `Target::GuidSection`: `crates/uefi-engine/src/parser/target.rs:15-38`.
- String-pack writer (`scan_sibt`): `crates/uefi-engine/src/setup_advanced/string_pack.rs:92`.
- `add_setup_formset` (merge-source): `crates/uefi-engine/src/setup_advanced/mod.rs:35`.
- IFR-структуры r-efi: `r-efi-7.0.0/src/hii.rs` (`IfrOpHeader:218`, `IfrFormSet:584`,
  `IfrForm:552`, `IfrSuppressIf:1115`, opcodes `:261-301`, `PACKAGE_FORMS:29`,
  `PACKAGE_STRINGS:30`).
- CLI-скаффолд: `crates/uefi-cli/src/commands/setup.rs`, `client.rs:276-301`,
  `output.rs:112-153`.
- Референсы для реализации: `../refs/IFRExtractor-RS/src/uefi_parser.rs` (string
  package language, IFR walk), `../refs/UEFITool-ai-fork/common/ffsparser.{h,cpp}`.
