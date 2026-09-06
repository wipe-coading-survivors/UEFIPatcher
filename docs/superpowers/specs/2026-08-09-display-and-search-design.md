# Display names, Search, and PI constants — Design

**Date:** 2026-08-09
**Scope:** `uefi-common` (new modules), `uefi-engine`, `uefi-cli`, `uefi-proto`, `AGENTS.md`
**Motivation:** На реальном BIOS-образе `dump`/`list` показывают числовые коды (`subtype=15`, `type=66`) вместо символических имён (`UI`, `File`), а FFS-файлы выводятся без своего UI-имени (`1/28 File subtype=07 guid=899407D7-...` вместо `name=Setup`). Также отсутствует поиск по тексту в теле секций. UEFITool решает эти задачи через `fileTypeToUString`/`sectionTypeToUString` (`refs/UEFITool-ai-fork/common/ffs.cpp:341-386`) и search-by-string.

## Goals

1. **Символические имена типов** в `text`-выводе `dump`/`list` (`type=File subtype=15(UI)`), TSV/JSON — без изменений (машинный разбор).
2. **Имя UI-секции поднято к FFS-файлу** — `1/28 File subtype=07 name=Setup` (как UEFITool).
3. **Легенда встретившихся кодов** в `stderr` после `dump`.
4. **Поиск** по имени и по содержимому секций (UTF-8, UTF-16LE, byte-pattern).
5. **Устранение дублирования** `dump_tree` (engine) vs `list_items` (клиент) — форматирование уходит в `uefi-common::format`.
6. **Типобезопасные PI-константы** в одном месте (`uefi-common::pi`) с заменой хардкода `EFI_SECTION_*`/`EFI_FV_FILETYPE_*` в `uefi-engine::ffs` и `decompress`.

## Non-goals

- Изменение парсера FFS/Volume (только `node_name` для FFS-файла).
- Изменение TUI/WebUI — они используют `list_items` напрямую; получат улучшенные `name` бесплатно; прямой search они подключают позже по необходимости.
- Regexp-поиск; поддержка wildcard в name (только substring case-insensitive).
- Сжатый поиск (search inside compressed bodies) — поиск идёт по секциям как они есть в дереве; если тело сжато, искать по decompressed-телу не делаем (YAGNI).

## Architecture

### Слой `uefi-common` (3 новых модуля)

#### `uefi-common::pi` — PI-константы

Типобезопасные enum'ы (`#[repr(u8)]` + `num_enum::TryFromPrimitive` + `Debug` + `Display`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum SectionType {
    Compression = 0x01, GuidDefined = 0x02, Disposable = 0x03,
    Pe32 = 0x10, Pic = 0x11, Te = 0x12, DxeDepex = 0x13,
    Version = 0x14, UserInterface = 0x15, Compatibility16 = 0x16,
    FirmwareVolumeImage = 0x17, FreeformSubtypeGuid = 0x18,
    Raw = 0x19, PeiDepex = 0x1B, MmDepex = 0x1C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum FileType {
    All = 0x00, Raw = 0x01, Freeform = 0x02, SecurityCore = 0x03,
    PeiCore = 0x04, DxeCore = 0x05, Peim = 0x06, Driver = 0x07,
    CombinedPeimDriver = 0x08, Application = 0x09, Mm = 0x0A,
    FirmwareVolumeImage = 0x0B, CombinedMmDxe = 0x0C, MmCore = 0x0D,
    MmStandalone = 0x0E, MmCoreStandalone = 0x0F, Pad = 0xF0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType { NotCompressed = 0, Standard = 1, Lzma = 2 }

pub const EFI_FVH_SIGNATURE: u32 = 0x4856465F;
pub const EFI_FVB2_ERASE_POLARITY: u32 = 0x00000800;
```

Источник значений: `refs/UEFITool-ai-fork/common/ffs.h:306-327,402-421`. `EFI_FVH_SIGNATURE`/`EFI_FVB2_ERASE_POLARITY` — `pub const` (битовые флаги, не enum).

> **План-дефект AGENTS.md** (правило 11): в AGENTS.md заявлено «использовать `r_efi::hii::*` для IFR-структур и opcode-констант», но про FFS-константы там же сказано использовать r-efi — это неверно. Проверка `r-efi 7.0.0`: модули `base`, `gpt`, `hii`, `system`, `protocols`, `vendor` — FFS-констант (`EFI_SECTION_*`, `EFI_FV_FILETYPE_*`, `EFI_FVH_SIGNATURE`, `EFI_FVB2_*`) НЕТ. Это UEFI runtime/HII; FFS — это PI-спецификация. Отдельный коммит `docs: clarify r-efi scope in AGENTS.md (HII/IFR only; FFS PI constants live in uefi-common::pi, not r-efi which has only UEFI runtime + HII, no PI section/file/volume constants)` ДО реализации.

#### `uefi-common::names` — display-имена

```rust
pub fn file_type_name(t: FileType) -> &'static str;       // "Raw", "DXE driver", "Pad", ...
pub fn section_type_name(t: SectionType) -> &'static str; // "PE32 image", "UI", "GUID defined", ...
pub fn node_type_name(t: u32) -> &'static str;            // 0x3E=Image, 0x41=Volume, 0x42=File, 0x43=Section
pub fn file_type_name_or_raw(code: u8) -> String;         // fallback "Unknown %02Xh"
pub fn section_type_name_or_raw(code: u8) -> String;
```

Таблицы — точный перенос `fileTypeToUString`/`sectionTypeToUString` из `ffs.cpp:341-386`. `node_type_name` кодирует **внутренние** значения `FfsType as u32` движка (Root=60, Capsule=61, Image=62, Region=63, Padding=64, Volume=65, File=66, Section=67, FreeSpace=68) — это НЕ UEFI-стандарт, передаётся в `Item.type: uint32`. Коды фиксированы контрактом proto и совпадают во всех клиентах. `*_or_raw` для нестандартных кодов (OEM file types 0xC0–0xDF и т.п.) возвращает `format!("Unknown {code:02X}h")`.

#### `uefi-common::format` — форматирование дерева (вне engine)

```rust
pub struct TreeRow {
    pub path: String,
    pub type_: u32,        // FfsType as u32
    pub subtype: u8,
    pub guid: String,
    pub offset: u64,
    pub size: u64,
    pub name: String,
}

pub fn format_tree(rows: &[TreeRow]) -> String;
pub fn format_legend(rows: &[TreeRow]) -> String;
```

- `format_tree` строит древовидный текст с отступами по path-индексам (`0/1/2`), выводит `File(DXE driver) subtype=07 guid=... name=Setup`, для секций — `Section(UI) subtype=15`.
- `format_legend` возвращает таблицу реально встретившихся кодов (например `15 = UI, 02 = GUID defined, 07 = DXE driver, F0 = Pad`) для stderr.
- `From<proto::Item> for TreeRow` реализуется в `uefi-cli` (uefi-common не зависит от proto).

#### `uefi-common::search` — утилиты поиска

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode { Name, Utf8, Utf16Le, Bytes }

pub fn match_name(name: &str, query: &str) -> bool;              // case-insensitive substring
pub fn find_utf8(body: &[u8], needle: &[u8]) -> bool;            // memmem
pub fn find_utf16le(body: &[u8], query: &str) -> bool;           // encode query as UTF-16LE, memmem
pub fn parse_hex_pattern(s: &str) -> Option<Vec<u8>>;            // "DE AD BEF0" -> vec![0xDE,0xAD,0xBE,0xF0]
```

- `match_name` — `name.to_lowercase().contains(&query.to_lowercase())`.
- `find_utf16le` — `query.encode_utf16().collect::<Vec<u16>>()` → as little-endian bytes → `find_utf8`-style search.
- `parse_hex_pattern` принимает hex с разделителями пробелом/без; возвращает `None` при invalid hex.

### Engine changes

**`parser/image.rs`:**

- `node_name(node)`:
  - Для FfsType::File — обходит `node.children`, находит первую секцию с `subtype == SectionType::UserInterface as u8` и возвращает её UI-имя (через существующий механизм чтения UTF-16LE из body). Если UI-секции нет — пустая строка.
  - Для Section с `subtype == UserInterface || Version` — без изменений (UTF-16LE decode).
  - Для остальных узлов — пустая строка.
- Удалить `dump_tree` и `dump_recursive` (форматирование перенесено в `uefi-common::format`).
- Добавить `pub fn search(root: &FfsNode, query: &str, modes: &[SearchMode], limit: usize) -> Vec<Item>`:
  - Обходит дерево, для каждого узла FfsType::Section (остальные пропускает):
    - `modes.contains(Name)` → `match_name(&node_name(node), query)`
    - `modes.contains(Utf8)` → `find_utf8(&node.body, query.as_bytes())`
    - `modes.contains(Utf16Le)` → `find_utf16le(&node.body, query)`
    - `modes.contains(Bytes)` → `parse_hex_pattern(query).map(|p| find_utf8(&node.body, &p)).unwrap_or(false)`
  - Совпадение по любому из активных mode — попадает в результат. Лимит обрывает обход на первом N совпадении (0 = без лимита).
  - Возвращает `Item` (path, type, subtype, guid, offset, size, name).

**`ffs.rs`:** удалить хардкод `EFI_SECTION_*`, `EFI_FV_FILETYPE_*`, `EFI_FVH_SIGNATURE`, `EFI_FVB2_ERASE_POLARITY` — переэкспортить из `uefi_common::pi` (для обратной совместимости внутреннего кода оставить `pub use uefi_common::pi::{SectionType as SectionTypePi, ...}` или обновить call sites). Well-known GUID-функции (`tiano_guid()`, `lzma_guid()`, `lzma_hp_guid()`, `lzma_ms_guid()`, `lzmaf86_guid()`, `crc32_guid()`) — оставляются (это не константы).

**`decompress.rs`:** `EFI_NOT_COMPRESSED`/`EFI_STANDARD_COMPRESSION`/`EFI_LZMA_COMPRESSION` — заменить на `CompressionType::NotCompressed as u8` etc. (или `pub use uefi_common::pi::CompressionType`).

**`rpc/server.rs`:** удалить `async fn dump_tree`, добавить `async fn search_items` (после `list_items`). Подключить новый trait-метод во всех impl-блоках (mock'и в cli/tui/gateway).

### Proto `engine.proto`

Удалить:
```proto
rpc DumpTree(DumpTreeRequest) returns (DumpTreeResponse);
enum DumpFormat { TEXT = 0; TSV = 1; }
message DumpTreeRequest { string image_id = 1; DumpFormat format = 2; }
message DumpTreeResponse { string text = 1; }
```

Добавить:
```proto
rpc SearchItems(SearchItemsRequest) returns (SearchItemsResponse);
enum SearchMode { NAME = 0; UTF8 = 1; UTF16 = 2; BYTES = 3; }
message SearchItemsRequest { string image_id = 1; string query = 2; repeated SearchMode modes = 3; uint32 limit = 4; }
message SearchItemsResponse { repeated Item items = 1; }
```

**Совместимость:** proto-контракт внутренний (engine ↔ cli ↔ gateway ↔ tui ↔ webui), внешних потребителей нет; обновляем синхронно по всему workspace.

### CLI changes

- `commands/image.rs::dump`:
  ```rust
  let items = client.list_items(&image_id, "").await?;
  let rows: Vec<TreeRow> = items.iter().map(Into::into).collect();
  eprint!("{}", format_legend(&rows));   // stderr
  print!("{}", format_tree(&rows));      // stdout
  ```
- `commands/image.rs::search` (новый subcommand):
  ```
  uefi-cli search <query> [--mode name|utf8|utf16|bytes]... [--limit N] [--format text|tsv|json]
  ```
  Без `--mode` — по умолчанию `[Name]`. Вызывает `search_items`, печатает через `print_items`.
- `output.rs::print_items`:
  - `OutputFormat::Text` — добавить имена: `File(DXE driver) subtype=07 guid=... name=Setup`, `Section(UI) subtype=15`.
  - `OutputFormat::Tsv` / `OutputFormat::Json` — без изменений.
- `client.rs`: удалить `dump_tree`, добавить `search_items(image_id, query, modes, limit) -> Vec<Item>`.

### `main.rs` CLI

- Удалить аргумент `--format text|tsv` у subcommand `dump` (format больше не нужен — вывод фиксированный text + stderr legend).
- Добавить subcommand `search` с аргументами выше.

## Testing

- **`uefi-common::names`** — юнит-тесты на каждую функцию: `file_type_name(FileType::Driver) == "DXE driver"`, `section_type_name(SectionType::UserInterface) == "UI"`, fallback для `0xC0` → `"Unknown C0h"`.
- **`uefi-common::format`** — тесты `format_tree`/`format_legend` на синтетическом наборе `TreeRow` (image + volume + file + section + named FFS).
- **`uefi-common::search`** — тесты на `match_name` (case-insensitive), `find_utf8`, `find_utf16le` (RU/EN), `parse_hex_pattern` (valid/invalid).
- **`uefi-common::pi`** — `TryFromPrimitive` для известных кодов, `Unknown` для `0xCC`.
- **`uefi-engine::parser::image::tests`**:
  - `node_name_ffs_uses_ui_child` — синтетическое дерево с FFS, у которого child UI "Hello" → `node_name(ffs) == "Hello"`.
  - Перенести `list_items_root_and_volume` без изменений.
  - Удалить `dump_tree_text` (перенесён в uefi-common).
  - Добавить `search_finds_by_name` / `search_finds_by_utf8_in_body` на синтетике.
- **`uefi-engine::rpc::server::tests`**: переписать `rpc_open_save_round_trip` — вместо `dump_tree` дёргать `list_items` и проверять `items.len() > 0` + наличие FFS.
- **`tests/real_image.rs`**:
  - `real_image_parse_image_full` — заменить проверки `tree.contains("File")` на `items.iter().any(|i| i.r#type == FfsType::File as u32)`; добавить `assert!(items.iter().any(|i| i.name == "Setup"))`.
  - Добавить `real_image_search_finds_setup_by_name` — `search(root, "setup", &[Name], 10)` → `≥1` совпадения с `name=="Setup"`.
  - Добавить `real_image_search_finds_utf16_in_pe32` — поиск строки, которая гарантированно есть в PE32 (например, `L"Dxe"`).
- **`uefi-cli`**: при наличии e2e тестов — обновить subcommand parsing.

## Migration order

1. `docs: clarify r-efi scope in AGENTS.md` (отдельный коммит ДО кода — правило 11).
2. `feat(uefi-common): add pi, names, format, search modules` (TDD: tests + impl в одном коммите на модуль).
3. `refactor(uefi-engine): use uefi-common::pi constants` (замена хардкода, без изменения поведения).
4. `feat(uefi-engine): lift UI section name to FFS file (node_name)`.
5. `feat(proto,engine,cli): replace DumpTree RPC with SearchItems RPC` (,proto + server + clients одномоментно, т.к. контракт общий).
6. `feat(cli): use uefi-common::format for dump, add search subcommand`.

Каждый шаг: `cargo test -p <crate>` + `cargo clippy -p <crate> -- -D warnings`. Финально: `cargo test --all`.

## Risks

- **Breaking proto change** — все impl `EngineService` trait должны одновременно получить `search_items` и потерять `dump_tree`. Список impl: `rpc/server.rs` + 3 mock'а (`uefi-cli`, `uefi-tui`, `uefi-gateway`). Шаг 5 обновляет все за один коммит + `cargo test --all`.
- **`node_name` для FFS-файла** читает UI-секцию детей. Если UI-секция внутри compressed/guided-секции (наследник глубже 1), текущий парсер уже разложил её в `children` — обходим рекурсивно (но вглубь только до первой UI). Глубокий поиск не делаем (YAGNI, совпадает с UEFITool).
- **Поиск `Utf16Le` с/query вне BMP** — `encode_utf16` выдаст surrogate-пары, корректно матчится. Test с RU-строкой.
- **`parse_hex_pattern` ambiguity** — если режим `Bytes`, а пользователь передал `"Setup"` (не hex), возвращаем `None` → 0 совпадений → пользователю понятное сообщение «invalid hex pattern». CLI отдельно валидирует.
