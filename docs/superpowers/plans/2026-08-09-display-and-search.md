# Display Names, Search, and PI Constants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить символические имена типов в вывод `dump`/`list`, поднять UI-имя к FFS-файлу (чтобы `Setup` отображалось у файла), добавить поиск по содержимому секций (Name/UTF-8/UTF-16LE/Bytes), централизовать PI-константы в `uefi-common::pi`.

**Architecture:** Четыре новых модуля в `uefi-common` (`pi`, `names`, `format`, `search`) — единый источник истины для всех клиентов. Engine использует эти модули: `dump_tree` делегирует в `uefi-common::format::format_tree` (engine больше не форматирует текст сам), `node_name` поднимает UI-имя к FFS-файлу, новый `search()` обходит дерево с утилитами `uefi-common::search`. Proto: добавить `SearchItems` RPC (DumpTree остаётся — TUI/Gateway мигрируем в отдельных подпланах). CLI: text-формат `print_items` показывает имена типов; новый subcommand `search`. Хардкод `EFI_SECTION_*`/`EFI_FV_FILETYPE_*` заменён на типобезопасные enum'ы `uefi-common::pi::*`.

**Tech Stack:** `num_enum 0.7` (TryFromPrimitive для enum'ов), существующие deps. Без новых внешних крейтов.

## Global Constraints

- **Module-first rule** (AGENTS.md п.3): при создании файла модуля добавляй `pub mod X;` в `lib.rs` **в том же шаге**, до запуска `cargo test`.
- **TDD порядок**: (1) объявить `mod` + создать файл с тестами → (2) `cargo test` (падает) → (3) реализация → (4) `cargo test` (проходит) → (5) commit.
- **Rule 11 (AGENTS.md)**: дефекты плана/spec — отдельным docs-коммитом ДО реализации.
- **Code style**: `cargo fmt`, `cargo clippy -- -D warnings`, без комментариев в коде (кроме ссылок на референс `file:line` и TODO).
- **PI константы — не из r-efi**: проверено, в r-efi 7.0 их нет (только HII/IFR + UEFI runtime). Источник: `refs/UEFITool-ai-fork/common/ffs.h:306-327,402-421` и `refs/UEFITool-ai-fork/common/ffs.cpp:341-386` для имён.
- **Cargo commands**: после каждого шага — `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`. Финально — `cargo test --all`.
- **Well-known GUIDs** остаются в `uefi-engine::ffs` (это функции, не константы — `tiano_guid()`, `lzma_guid()`, etc.).
- **Fmt ID и node_type**: внутренние коды `FfsType as u32` (Root=60, Image=62, Volume=65, File=66, Section=67) — НЕ UEFI-стандарт, передаются в `Item.type: uint32`, едины во всех клиентах.

---

## Файлы плана

| Файл | Назначение |
|---|---|
| `AGENTS.md` | Уточнить про r-efi (только HII/IFR, не PI) |
| `crates/uefi-common/Cargo.toml` | Добавить `num_enum` |
| `crates/uefi-common/src/lib.rs` | `pub mod pi/names/format/search` |
| `crates/uefi-common/src/pi.rs` | `SectionType`, `FileType`, `CompressionType`, `EFI_FVH_SIGNATURE`, `EFI_FVB2_ERASE_POLARITY` |
| `crates/uefi-common/src/names.rs` | `file_type_name`, `section_type_name`, `node_type_name`, `*_or_raw` |
| `crates/uefi-common/src/format.rs` | `TreeRow`, `format_tree`, `format_legend` |
| `crates/uefi-common/src/search.rs` | `SearchMode`, `match_name`, `find_utf8`, `find_utf16le`, `parse_hex_pattern` |
| `crates/uefi-engine/Cargo.toml` | Добавить `uefi-common` dep |
| `crates/uefi-engine/src/ffs.rs` | Заменить хардкод на `pub use uefi_common::pi::*` |
| `crates/uefi-engine/src/decompress.rs` | Заменить `EFI_*_COMPRESSION` на `CompressionType` |
| `crates/uefi-engine/src/parser/image.rs` | `node_name` lift UI; `dump_tree` через `format::format_tree`; новый `search()` |
| `crates/uefi-engine/src/rpc/server.rs` | Добавить `search_items` handler (dump_tree остаётся) |
| `crates/uefi-engine/tests/real_image.rs` | Тесты: node_name="Setup", search по имени |
| `crates/uefi-proto/proto/engine.proto` | Добавить `SearchItems` RPC + `SearchMode` enum |
| `crates/uefi-cli/src/client.rs` | Добавить `search_items` |
| `crates/uefi-cli/src/commands/image.rs` | Новый `search()` handler |
| `crates/uefi-cli/src/output.rs` | Text-формат print_items с символическими именами |
| `crates/uefi-cli/src/main.rs` | `ImageCmd::Search` |
| `crates/uefi-cli/tests/mock_server.rs` | Stub `search_items` |
| `crates/uefi-tui/tests/mock_server.rs` | Stub `search_items` |
| `crates/uefi-gateway/tests/mock_server.rs` | Stub `search_items` |
| `crates/uefi-tui/src/commands.rs` | TODO comment про миграцию на list_items |

---

### Task 1: docs — clarify r-efi scope in AGENTS.md (rule 11)

**Files:**
- Modify: `AGENTS.md`

**Interfaces:** нет кодовых изменений.

В AGENTS.md указано «`r-efi` — UEFI типы: `r_efi::hii::*` (IFR-структуры...)». Но в той же таблице (п. «Крейты») про `r-efi` написано «UEFI типы», что можно неверно прочитать как источник PI-констант. Проверка `r-efi 7.0.0`: модули `base/gpt/hii/system/protocols/vendor` — FFS-констант (`EFI_SECTION_*`, `EFI_FV_FILETYPE_*`, `EFI_FVH_SIGNATURE`, `EFI_FVB2_*`) НЕТ.

- [ ] **Step 1: Уточнить таблицу «Крейты»**

В `AGENTS.md`, секция «Крейты», заменить строку про `r-efi`:

**Старое:**
```
| `r-efi` | UEFI типы: `r_efi::hii::*` (IFR-структуры: IfrFormSet, IfrForm, IfrCheckbox, IfrNumeric, IfrOneOf, IfrDefault, IfrVarstoreEfi, IfrEnd, etc.), opcode-константы (IFR_FORM_SET_OP, etc.). НЕ определять свои IFR-структуры. |
```

**Новое:**
```
| `r-efi` | UEFI runtime + HII типы: `r_efi::hii::*` (IFR-структуры: IfrFormSet, IfrForm, IfrCheckbox, IfrNumeric, IfrOneOf, IfrDefault, IfrVarstoreEfi, IfrEnd, etc.), opcode-константы (IFR_FORM_SET_OP, etc.). НЕ определять свои IFR-структуры. ВАЖНО: r-efi 7.0 НЕ содержит PI-констант (EFI_SECTION_*, EFI_FV_FILETYPE_*, EFI_FVH_SIGNATURE, EFI_FVB2_*) — они в `uefi-common::pi`. |
```

- [ ] **Step 2: Commit**

```bash
git add AGENTS.md
git commit -m "docs: clarify r-efi scope in AGENTS.md (HII/IFR only; PI FFS constants live in uefi-common::pi, r-efi 7.0 verified to have only UEFI runtime + HII, no PI section/file/volume constants)"
```

---

### Task 2: uefi-common::pi — типобезопасные PI-константы

**Files:**
- Modify: `crates/uefi-common/Cargo.toml` (добавить `num_enum.workspace = true`)
- Modify: `crates/uefi-common/src/lib.rs` (добавить `pub mod pi;`)
- Create: `crates/uefi-common/src/pi.rs`
- Test: inline `#[cfg(test)]` в `pi.rs`

**Interfaces:**
- Consumes: `num_enum::TryFromPrimitive`
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)] #[repr(u8)] pub enum SectionType { ... }` — все значения из `ffs.h:402-421`
  - `#[derive(...)] pub enum FileType { ... }` — из `ffs.h:306-327`
  - `pub enum CompressionType { NotCompressed = 0, Standard = 1, Lzma = 2 }`
  - `pub const EFI_FVH_SIGNATURE: u32 = 0x4856465F;`
  - `pub const EFI_FVB2_ERASE_POLARITY: u32 = 0x00000800;`

- [ ] **Step 1: Добавить dep + модуль**

`crates/uefi-common/Cargo.toml` (вставить в `[dependencies]`):
```toml
num_enum.workspace = true
```

`crates/uefi-common/src/lib.rs`:
```rust
pub mod error;
pub mod pi;
pub mod state;
pub use error::*;
pub use state::*;
```

- [ ] **Step 2: Написать failing тесты**

`crates/uefi-common/src/pi.rs`:
```rust
use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum SectionType {
    Compression = 0x01,
    GuidDefined = 0x02,
    Disposable = 0x03,
    Pe32 = 0x10,
    Pic = 0x11,
    Te = 0x12,
    DxeDepex = 0x13,
    Version = 0x14,
    UserInterface = 0x15,
    Compatibility16 = 0x16,
    FirmwareVolumeImage = 0x17,
    FreeformSubtypeGuid = 0x18,
    Raw = 0x19,
    PeiDepex = 0x1B,
    MmDepex = 0x1C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum FileType {
    All = 0x00,
    Raw = 0x01,
    Freeform = 0x02,
    SecurityCore = 0x03,
    PeiCore = 0x04,
    DxeCore = 0x05,
    Peim = 0x06,
    Driver = 0x07,
    CombinedPeimDriver = 0x08,
    Application = 0x09,
    Mm = 0x0A,
    FirmwareVolumeImage = 0x0B,
    CombinedMmDxe = 0x0C,
    MmCore = 0x0D,
    MmStandalone = 0x0E,
    MmCoreStandalone = 0x0F,
    Pad = 0xF0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType {
    NotCompressed = 0,
    Standard = 1,
    Lzma = 2,
}

pub const EFI_FVH_SIGNATURE: u32 = 0x4856465F;
pub const EFI_FVB2_ERASE_POLARITY: u32 = 0x00000800;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_type_known_codes() {
        assert_eq!(SectionType::Pe32 as u8, 0x10);
        assert_eq!(SectionType::UserInterface as u8, 0x15);
        assert_eq!(SectionType::Raw as u8, 0x19);
        assert_eq!(SectionType::MmDepex as u8, 0x1C);
        assert_eq!(SectionType::try_from(0x10u8).unwrap(), SectionType::Pe32);
        assert_eq!(
            SectionType::try_from(0x15u8).unwrap(),
            SectionType::UserInterface
        );
    }

    #[test]
    fn section_type_unknown_returns_err() {
        assert!(SectionType::try_from(0xCCu8).is_err());
        assert!(SectionType::try_from(0x05u8).is_err());
    }

    #[test]
    fn file_type_pad_and_oem_range() {
        assert_eq!(FileType::Pad as u8, 0xF0);
        assert_eq!(FileType::try_from(0xF0u8).unwrap(), FileType::Pad);
        assert!(FileType::try_from(0xC0u8).is_err());
    }

    #[test]
    fn compression_type_values() {
        assert_eq!(CompressionType::NotCompressed as u8, 0);
        assert_eq!(CompressionType::Standard as u8, 1);
        assert_eq!(CompressionType::Lzma as u8, 2);
    }

    #[test]
    fn fv_signature_is_fvh_ascii_le() {
        let bytes = EFI_FVH_SIGNATURE.to_le_bytes();
        assert_eq!(&bytes[..], b"_FVH");
    }

    #[test]
    fn erase_polarity_bit_value() {
        assert_eq!(EFI_FVB2_ERASE_POLARITY, 0x00000800);
    }
}
```

- [ ] **Step 3: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-common pi:: && cargo clippy -p uefi-common -- -D warnings`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-common/Cargo.toml crates/uefi-common/src/lib.rs crates/uefi-common/src/pi.rs
git commit -m "feat(uefi-common): add pi module with SectionType/FileType/CompressionType enums"
```

---

### Task 3: uefi-common::names — display-имена

**Files:**
- Modify: `crates/uefi-common/src/lib.rs` (добавить `pub mod names;`)
- Create: `crates/uefi-common/src/names.rs`
- Test: inline

**Interfaces:**
- Consumes: `crate::pi::{SectionType, FileType}`
- Produces:
  - `pub fn file_type_name(t: FileType) -> &'static str`
  - `pub fn section_type_name(t: SectionType) -> &'static str`
  - `pub fn node_type_name(code: u32) -> &'static str`
  - `pub fn file_type_name_or_raw(code: u8) -> String`
  - `pub fn section_type_name_or_raw(code: u8) -> String`

Источник имён: `refs/UEFITool-ai-fork/common/ffs.cpp:341-386`. `node_type_name` кодирует внутренние коды FfsType (см. Global Constraints).

- [ ] **Step 1: Добавить модуль**

`crates/uefi-common/src/lib.rs`:
```rust
pub mod error;
pub mod names;
pub mod pi;
pub mod state;
pub use error::*;
pub use state::*;
```

- [ ] **Step 2: Написать failing тесты + реализацию**

`crates/uefi-common/src/names.rs`:
```rust
use crate::pi::{FileType, SectionType};

pub fn file_type_name(t: FileType) -> &'static str {
    match t {
        FileType::All => "All",
        FileType::Raw => "Raw",
        FileType::Freeform => "Freeform",
        FileType::SecurityCore => "SEC core",
        FileType::PeiCore => "PEI core",
        FileType::DxeCore => "DXE core",
        FileType::Peim => "PEI module",
        FileType::Driver => "DXE driver",
        FileType::CombinedPeimDriver => "Combined PEI/DXE",
        FileType::Application => "Application",
        FileType::Mm => "SMM module",
        FileType::FirmwareVolumeImage => "Volume image",
        FileType::CombinedMmDxe => "Combined SMM/DXE",
        FileType::MmCore => "SMM core",
        FileType::MmStandalone => "MM standalone module",
        FileType::MmCoreStandalone => "MM standalone core",
        FileType::Pad => "Pad",
    }
}

pub fn section_type_name(t: SectionType) -> &'static str {
    match t {
        SectionType::Compression => "Compressed",
        SectionType::GuidDefined => "GUID defined",
        SectionType::Disposable => "Disposable",
        SectionType::Pe32 => "PE32 image",
        SectionType::Pic => "PIC image",
        SectionType::Te => "TE image",
        SectionType::DxeDepex => "DXE dependency",
        SectionType::Version => "Version",
        SectionType::UserInterface => "UI",
        SectionType::Compatibility16 => "16-bit image",
        SectionType::FirmwareVolumeImage => "Volume image",
        SectionType::FreeformSubtypeGuid => "Freeform subtype GUID",
        SectionType::Raw => "Raw",
        SectionType::PeiDepex => "PEI dependency",
        SectionType::MmDepex => "MM dependency",
    }
}

pub fn node_type_name(code: u32) -> &'static str {
    match code {
        60 => "Root",
        61 => "Capsule",
        62 => "Image",
        63 => "Region",
        64 => "Padding",
        65 => "Volume",
        66 => "File",
        67 => "Section",
        68 => "FreeSpace",
        _ => "Unknown",
    }
}

pub fn file_type_name_or_raw(code: u8) -> String {
    match FileType::try_from(code) {
        Ok(t) => file_type_name(t).to_string(),
        Err(_) => format!("Unknown {code:02X}h"),
    }
}

pub fn section_type_name_or_raw(code: u8) -> String {
    match SectionType::try_from(code) {
        Ok(t) => section_type_name(t).to_string(),
        Err(_) => format!("Unknown {code:02X}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_type_known_names() {
        assert_eq!(file_type_name(FileType::Raw), "Raw");
        assert_eq!(file_type_name(FileType::Driver), "DXE driver");
        assert_eq!(file_type_name(FileType::Pad), "Pad");
        assert_eq!(file_type_name(FileType::Application), "Application");
    }

    #[test]
    fn section_type_known_names() {
        assert_eq!(section_type_name(SectionType::Pe32), "PE32 image");
        assert_eq!(section_type_name(SectionType::UserInterface), "UI");
        assert_eq!(section_type_name(SectionType::GuidDefined), "GUID defined");
        assert_eq!(section_type_name(SectionType::Raw), "Raw");
    }

    #[test]
    fn node_type_codes_match_ffstype() {
        assert_eq!(node_type_name(62), "Image");
        assert_eq!(node_type_name(65), "Volume");
        assert_eq!(node_type_name(66), "File");
        assert_eq!(node_type_name(67), "Section");
        assert_eq!(node_type_name(99), "Unknown");
    }

    #[test]
    fn fallback_for_oem_and_unknown_codes() {
        assert_eq!(file_type_name_or_raw(0xC0), "Unknown C0h");
        assert_eq!(section_type_name_or_raw(0xCC), "Unknown CCh");
        assert_eq!(file_type_name_or_raw(0x07), "DXE driver");
        assert_eq!(section_type_name_or_raw(0x15), "UI");
    }
}
```

- [ ] **Step 3: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-common names:: && cargo clippy -p uefi-common -- -D warnings`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-common/src/lib.rs crates/uefi-common/src/names.rs
git commit -m "feat(uefi-common): add names module with display names (sources ffs.cpp:341-386)"
```

---

### Task 4: uefi-common::format — форматирование дерева

**Files:**
- Modify: `crates/uefi-common/src/lib.rs` (добавить `pub mod format;`)
- Create: `crates/uefi-common/src/format.rs`
- Test: inline

**Interfaces:**
- Consumes: `crate::names::{node_type_name, file_type_name_or_raw, section_type_name_or_raw}`
- Produces:
  - `pub struct TreeRow { pub path: String, pub type_: u32, pub subtype: u8, pub guid: String, pub offset: u64, pub size: u64, pub name: String }`
  - `pub fn format_tree(rows: &[TreeRow]) -> String`
  - `pub fn format_legend(rows: &[TreeRow]) -> String`

`format_tree` выводит `{path} {NodeType}({subtype_name}) subtype={subtype:02X} guid={guid} off={offset} size={size} name={name}` для text-формата. Path вида `"0/1/2"` → 2-пробельный отступ по depth (число `/`).

`format_legend` возвращает текст для stderr: список реально встретившихся кодов файлов и секций.

- [ ] **Step 1: Добавить модуль**

`crates/uefi-common/src/lib.rs`:
```rust
pub mod error;
pub mod format;
pub mod names;
pub mod pi;
pub mod state;
pub use error::*;
pub use state::*;
```

- [ ] **Step 2: Написать failing тесты + реализацию**

`crates/uefi-common/src/format.rs`:
```rust
use crate::names::{file_type_name_or_raw, node_type_name, section_type_name_or_raw};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub path: String,
    pub type_: u32,
    pub subtype: u8,
    pub guid: String,
    pub offset: u64,
    pub size: u64,
    pub name: String,
}

fn depth(path: &str) -> usize {
    if path.is_empty() {
        0
    } else {
        path.matches('/').count() + 1
    }
}

fn subtype_name_for(type_: u32, subtype: u8) -> String {
    match type_ {
        66 => file_type_name_or_raw(subtype),
        67 => section_type_name_or_raw(subtype),
        _ => String::new(),
    }
}

pub fn format_tree(rows: &[TreeRow]) -> String {
    let mut out = String::new();
    for r in rows {
        let indent = "  ".repeat(depth(&r.path));
        let node_label = node_type_name(r.type_);
        let sub = subtype_name_for(r.type_, r.subtype);
        let sub_part = if sub.is_empty() {
            String::new()
        } else {
            format!("({sub})")
        };
        let name_part = if r.name.is_empty() {
            String::new()
        } else {
            format!(" name={}", r.name)
        };
        out.push_str(&format!(
            "{indent}{}{sub_part} subtype={:02X} guid={} off={} size={}{name_part}\n",
            node_label, r.subtype, r.guid, r.offset, r.size,
        ));
    }
    out
}

pub fn format_legend(rows: &[TreeRow]) -> String {
    let mut file_codes: BTreeMap<u8, ()> = BTreeMap::new();
    let mut section_codes: BTreeMap<u8, ()> = BTreeMap::new();
    for r in rows {
        if r.type_ == 66 {
            file_codes.insert(r.subtype, ());
        } else if r.type_ == 67 {
            section_codes.insert(r.subtype, ());
        }
    }
    if file_codes.is_empty() && section_codes.is_empty() {
        return String::new();
    }
    let mut out = String::from("Legend:\n");
    if !file_codes.is_empty() {
        out.push_str("  File types:\n");
        for code in file_codes.keys() {
            out.push_str(&format!("    {:02X} = {}\n", code, file_type_name_or_raw(*code)));
        }
    }
    if !section_codes.is_empty() {
        out.push_str("  Section types:\n");
        for code in section_codes.keys() {
            out.push_str(&format!(
                "    {:02X} = {}\n",
                code,
                section_type_name_or_raw(*code)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, type_: u32, subtype: u8, name: &str) -> TreeRow {
        TreeRow {
            path: path.into(),
            type_,
            subtype,
            guid: String::new(),
            offset: 0,
            size: 0,
            name: name.into(),
        }
    }

    #[test]
    fn format_tree_renders_named_ffs_and_ui_section() {
        let rows = vec![
            row("", 62, 0, ""),
            row("0", 65, 0, ""),
            row("0/0", 66, 0x07, "Setup"),
            row("0/0/0", 67, 0x15, ""),
        ];
        let out = format_tree(&rows);
        assert!(out.contains("Image subtype=00"));
        assert!(out.contains("  Volume subtype=00"));
        assert!(out.contains("    File(DXE driver) subtype=07 guid= off=0 size=0 name=Setup"));
        assert!(out.contains("      Section(UI) subtype=15"));
    }

    #[test]
    fn format_tree_skips_subtype_name_for_non_file_section() {
        let rows = vec![row("0", 65, 0x42, "")];
        let out = format_tree(&rows);
        assert!(out.contains("Volume subtype=42"));
        assert!(!out.contains("Volume("));
    }

    #[test]
    fn format_legend_groups_codes_by_kind() {
        let rows = vec![
            row("0/0", 66, 0x07, ""),
            row("0/0/0", 67, 0x10, ""),
            row("0/0/1", 67, 0x15, ""),
            row("0/1", 66, 0xF0, ""),
        ];
        let leg = format_legend(&rows);
        assert!(leg.contains("File types:"));
        assert!(leg.contains("07 = DXE driver"));
        assert!(leg.contains("F0 = Pad"));
        assert!(leg.contains("Section types:"));
        assert!(leg.contains("10 = PE32 image"));
        assert!(leg.contains("15 = UI"));
    }

    #[test]
    fn format_legend_empty_when_no_file_or_section() {
        let rows = vec![row("", 62, 0, ""), row("0", 65, 0, "")];
        assert_eq!(format_legend(&rows), "");
    }

    #[test]
    fn depth_returns_tree_level() {
        assert_eq!(depth(""), 0);
        assert_eq!(depth("0"), 1);
        assert_eq!(depth("0/1"), 2);
        assert_eq!(depth("0/1/2/3"), 4);
    }
}
```

- [ ] **Step 3: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-common format:: && cargo clippy -p uefi-common -- -D warnings`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-common/src/lib.rs crates/uefi-common/src/format.rs
git commit -m "feat(uefi-common): add format module with TreeRow, format_tree, format_legend"
```

---

### Task 5: uefi-common::search — утилиты поиска

**Files:**
- Modify: `crates/uefi-common/src/lib.rs` (добавить `pub mod search;`)
- Create: `crates/uefi-common/src/search.rs`
- Test: inline

**Interfaces:**
- Consumes: нет внешних
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum SearchMode { Name, Utf8, Utf16Le, Bytes }`
  - `pub fn match_name(name: &str, query: &str) -> bool` — case-insensitive substring
  - `pub fn find_utf8(body: &[u8], needle: &[u8]) -> bool`
  - `pub fn find_utf16le(body: &[u8], query: &str) -> bool`
  - `pub fn parse_hex_pattern(s: &str) -> Option<Vec<u8>>` — принимает `"DEADBEEF"`, `"DE AD BEF0"`, `"de ad"`. `None` при invalid hex или нечётной длине.

- [ ] **Step 1: Добавить модуль**

`crates/uefi-common/src/lib.rs`:
```rust
pub mod error;
pub mod format;
pub mod names;
pub mod pi;
pub mod search;
pub mod state;
pub use error::*;
pub use state::*;
```

- [ ] **Step 2: Написать failing тесты + реализацию**

`crates/uefi-common/src/search.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Name,
    Utf8,
    Utf16Le,
    Bytes,
}

pub fn match_name(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    name.to_lowercase().contains(&query.to_lowercase())
}

pub fn find_utf8(body: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return false;
    }
    body.windows(needle.len()).any(|w| w == needle)
}

pub fn find_utf16le(body: &[u8], query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let needle: Vec<u8> = query
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    find_utf8(body, &needle)
}

pub fn parse_hex_pattern(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_name_case_insensitive_substring() {
        assert!(match_name("Setup", "set"));
        assert!(match_name("Setup", "UP"));
        assert!(match_name("PchInitDxe", "init"));
        assert!(!match_name("Setup", "menu"));
        assert!(!match_name("Setup", ""));
    }

    #[test]
    fn find_utf8_substring_match() {
        assert!(find_utf8(b"hello world", b"world"));
        assert!(find_utf8(b"abc", b"abc"));
        assert!(!find_utf8(b"abc", b"abcd"));
        assert!(!find_utf8(b"abc", b""));
    }

    #[test]
    fn find_utf16le_encodes_query_as_le_bytes() {
        let body: Vec<u8> = "AB".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert!(find_utf16le(&body, "AB"));
        assert!(!find_utf16le(&body, "AC"));
        assert!(!find_utf16le(b"", ""));
    }

    #[test]
    fn find_utf16le_cyrillic() {
        let body: Vec<u8> = "Привет"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert!(find_utf16le(&body, "рив"));
    }

    #[test]
    fn parse_hex_pattern_no_separator() {
        assert_eq!(parse_hex_pattern("DEADBEEF").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn parse_hex_pattern_with_spaces_and_mixed_case() {
        assert_eq!(
            parse_hex_pattern("DE ad BE f0").unwrap(),
            vec![0xDE, 0xAD, 0xBE, 0xF0]
        );
    }

    #[test]
    fn parse_hex_pattern_invalid() {
        assert!(parse_hex_pattern("XYZW").is_none());
        assert!(parse_hex_pattern("ABC").is_none());
        assert!(parse_hex_pattern("").is_none());
        assert!(parse_hex_pattern("AG").is_none());
    }

    #[test]
    fn search_mode_variants_distinct() {
        assert_ne!(SearchMode::Name, SearchMode::Utf8);
        assert_ne!(SearchMode::Bytes, SearchMode::Utf16Le);
    }
}
```

- [ ] **Step 3: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-common search:: && cargo clippy -p uefi-common -- -D warnings`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-common/src/lib.rs crates/uefi-common/src/search.rs
git commit -m "feat(uefi-common): add search module (name/utf8/utf16le/hex-pattern)"
```

---

### Task 6: uefi-engine — использовать uefi-common::pi (заменить хардкод)

**Files:**
- Modify: `crates/uefi-engine/Cargo.toml` (добавить `uefi-common` dep)
- Modify: `crates/uefi-engine/src/ffs.rs`
- Modify: `crates/uefi-engine/src/decompress.rs`
- Modify: `crates/uefi-engine/src/parser/section.rs` (вызов `decompress` с `CompressionType`)

**Interfaces:**
- Consumes: `uefi_common::pi::{SectionType, FileType, CompressionType, EFI_FVH_SIGNATURE, EFI_FVB2_ERASE_POLARITY}`
- Produces: `uefi-engine::ffs::*` остаётся обратно-совместимым через `pub use` alias'ы `EFI_SECTION_*`/`EFI_FV_FILETYPE_*` (как `u8`), чтобы не править все call sites сразу.

> **Стратегия:** в `ffs.rs` оставить `pub const` (как alias'ы значений enum'ов) — минимальные изменения call sites. В `decompress.rs` заменить `EFI_*_COMPRESSION` на `CompressionType as u8`.

- [ ] **Step 1: Добавить uefi-common dep**

`crates/uefi-engine/Cargo.toml` — добавить в `[dependencies]`:
```toml
uefi-common = { path = "../uefi-common" }
```

- [ ] **Step 2: Заменить хардкод в ffs.rs на uefi-common::pi (с alias'ами для совместимости)**

В `crates/uefi-engine/src/ffs.rs` заменить блок `pub const EFI_SECTION_* / EFI_FV_FILETYPE_* / EFI_FVH_SIGNATURE / EFI_FVB2_ERASE_POLARITY` (строки 3-17) на:

```rust
pub use uefi_common::pi::{EFI_FVH_SIGNATURE, EFI_FVB2_ERASE_POLARITY};

pub const EFI_SECTION_COMPRESSION: u8 = uefi_common::pi::SectionType::Compression as u8;
pub const EFI_SECTION_GUID_DEFINED: u8 = uefi_common::pi::SectionType::GuidDefined as u8;
pub const EFI_SECTION_PE32: u8 = uefi_common::pi::SectionType::Pe32 as u8;
pub const EFI_SECTION_TE: u8 = uefi_common::pi::SectionType::Te as u8;
pub const EFI_SECTION_UI: u8 = uefi_common::pi::SectionType::UserInterface as u8;
pub const EFI_SECTION_VERSION: u8 = uefi_common::pi::SectionType::Version as u8;
pub const EFI_SECTION_FV_IMAGE: u8 = uefi_common::pi::SectionType::FirmwareVolumeImage as u8;
pub const EFI_SECTION_FREEFORM_SUBTYPE_GUID: u8 = uefi_common::pi::SectionType::FreeformSubtypeGuid as u8;
pub const EFI_SECTION_RAW: u8 = uefi_common::pi::SectionType::Raw as u8;
pub const EFI_SECTION_DEPEX: u8 = uefi_common::pi::SectionType::MmDepex as u8;

pub const EFI_FV_FILETYPE_RAW: u8 = uefi_common::pi::FileType::Raw as u8;
```

> **Важно:** `EFI_SECTION_DEPEX` ранее был `0x1C` (MmDepex по EDK2 ffs.h:421). Соответствие сохранено. Если в коде ожидался PEI_DEPEX — это уже отдельный баг (в текущей кодовой базе используется только в `parser/section.rs` для match-branch, который остаётся корректным, т.к. там нет case для 0x1B).

- [ ] **Step 3: Заменить compression-константы в decompress.rs**

В `crates/uefi-engine/src/decompress.rs` заменить строки 12-14:
```rust
pub use uefi_common::pi::CompressionType;
```

И обновить `decompress()` (строки 16-23):
```rust
pub fn decompress(data: &[u8], algorithm: u8) -> Result<Vec<u8>, DecompressError> {
    match algorithm {
        0 => Ok(data.to_vec()),
        1 => decompress_tiano(data),
        2 => decompress_lzma(data),
        _ => Err(DecompressError::Unsupported),
    }
}
```

(Используем литералы 0/1/2 напрямую, т.к. `CompressionType` не нужен в pattern match — алгоритм определяется в вызове по GUID в section.rs, где используем `CompressionType::Lzma as u8`).

Тесты в `decompress.rs` (строки 51-82) — заменить `EFI_NOT_COMPRESSED`/`EFI_STANDARD_COMPRESSION`/`EFI_LZMA_COMPRESSION` на числовые литералы `0`/`1`/`2`. Например:
```rust
assert_eq!(decompress(&data, 0).unwrap(), data);
// ...
assert!(decompress(&[0; 16], 1).is_... );
// ...
let out = decompress(payload, 2).expect("LZMA decode");
```

- [ ] **Step 4: Обновить parser/section.rs — использовать CompressionType в guided_algorithm**

В `crates/uefi-engine/src/parser/section.rs` (функция `guided_algorithm`, строки 86-94) заменить:
```rust
fn guided_algorithm(guid: &Guid) -> Option<u8> {
    if is_lzma_guid(guid) {
        Some(uefi_common::pi::CompressionType::Lzma as u8)
    } else if is_tiano_guid(guid) {
        Some(uefi_common::pi::CompressionType::Standard as u8)
    } else {
        None
    }
}
```

Также в `use` (строка 2) убрать `EFI_LZMA_COMPRESSION, EFI_STANDARD_COMPRESSION` если они больше не нужны:
```rust
use crate::decompress;
```

- [ ] **Step 5: Запустить все тесты engine — должны пройти без изменений**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (тесты `ffs::tests::*`, `decompress::tests::*`, `parser::section::tests::*`, real_image tests игнорируются по умолчанию)

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/Cargo.toml crates/uefi-engine/src/ffs.rs crates/uefi-engine/src/decompress.rs crates/uefi-engine/src/parser/section.rs
git commit -m "refactor(uefi-engine): use uefi-common::pi constants instead of hardcoded values"
```

---

### Task 7: uefi-engine — node_name поднимает UI-имя к FFS-файлу

**Files:**
- Modify: `crates/uefi-engine/src/parser/image.rs`

**Interfaces:**
- Consumes: `crate::ffs::*` (включая `EFI_SECTION_UI`), `crate::types::FfsType`, `FfsNode`
- Produces: изменённое поведение `node_name(node: &FfsNode) -> String` — для FfsType::File ищет в детях первую UI-секцию и возвращает её имя.

- [ ] **Step 1: Написать failing тест**

В `crates/uefi-engine/src/parser/image.rs`, в `mod tests` (после существующих тестов) добавить:
```rust
#[test]
fn node_name_lifts_ui_for_ffs_file() {
    let ui_section = FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_UI,
        offset: 0,
        header: vec![0; 4],
        body: encode_utf16le_null("Setup"),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let file = FfsNode {
        guid: None,
        node_type: FfsType::File,
        subtype: 0x07,
        offset: 0,
        header: vec![0; 24],
        body: vec![],
        tail: vec![],
        children: vec![ui_section],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    assert_eq!(node_name(&file), "Setup");
}

#[test]
fn node_name_empty_for_ffs_without_ui_child() {
    let raw_section = FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_RAW,
        offset: 0,
        header: vec![0; 4],
        body: vec![0xAA; 4],
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let file = FfsNode {
        guid: None,
        node_type: FfsType::File,
        subtype: 0x01,
        offset: 0,
        header: vec![0; 24],
        body: vec![],
        tail: vec![],
        children: vec![raw_section],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    assert_eq!(node_name(&file), "");
}

fn encode_utf16le_null(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|u| u.to_le_bytes())
        .collect()
}
```

В `use` тестов (`use super::*; use crate::ffs::EFI_FVH_SIGNATURE;`) добавить `use crate::ffs::{EFI_SECTION_RAW, EFI_SECTION_UI};` и `use crate::types::{Action, FfsNode, FfsType, ParsingData};`.

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine parser::image::tests::node_name_lifts_ui_for_ffs_file`
Expected: FAIL — `assert_eq!` левое "" правое "Setup"

- [ ] **Step 3: Реализовать node_name lift для FFS-файла**

В `crates/uefi-engine/src/parser/image.rs` заменить функцию `node_name` (строки 177-191):
```rust
fn node_name(node: &FfsNode) -> String {
    match node.node_type {
        FfsType::Section if node.subtype == EFI_SECTION_UI || node.subtype == EFI_SECTION_VERSION => {
            decode_utf16le_body(&node.body)
        }
        FfsType::File => {
            for child in &node.children {
                if child.node_type == FfsType::Section && child.subtype == EFI_SECTION_UI {
                    return decode_utf16le_body(&child.body);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn decode_utf16le_body(body: &[u8]) -> String {
    let text: Vec<u16> = body
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&text)
        .trim_end_matches('\u{0}')
        .to_string()
}
```

- [ ] **Step 4: Запустить все тести engine — должны пройти**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/parser/image.rs
git commit -m "feat(uefi-engine): lift UI section name to FFS file in node_name"
```

---

### Task 8: uefi-engine — dump_tree использует uefi-common::format

**Files:**
- Modify: `crates/uefi-engine/src/parser/image.rs`

**Interfaces:**
- Consumes: `uefi_common::format::{TreeRow, format_tree}`, `crate::parser::image::list_items`
- Produces: `pub fn dump_tree(root: &FfsNode, format: DumpFormat) -> String` — делегирует в `format_tree` для Text; для Tsv строит простой TSV inline (или через现有的 format).

- [ ] **Step 1: Переписать dump_tree через list_items + format_tree**

В `crates/uefi-engine/src/parser/image.rs`:
1. Добавить `use uefi_common::format::{format_tree, TreeRow};` вверху.
2. Заменить функции `dump_tree` и `dump_recursive` (строки 105-143) на:
```rust
pub fn dump_tree(root: &FfsNode, format: DumpFormat) -> String {
    match format {
        DumpFormat::Text => {
            let items = list_items(root, None);
            let rows: Vec<TreeRow> = items
                .iter()
                .map(|it| TreeRow {
                    path: it.path.clone(),
                    type_: it.r#type,
                    subtype: it.subtype as u8,
                    guid: it.guid.clone(),
                    offset: it.offset,
                    size: it.size,
                    name: it.name.clone(),
                })
                .collect();
            format_tree(&rows)
        }
        DumpFormat::Tsv => {
            let items = list_items(root, None);
            let mut out = String::from("path\ttype\tsubtype\tguid\toffset\tsize\tname\n");
            for it in items {
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    it.path, it.r#type, it.subtype, it.guid, it.offset, it.size, it.name,
                ));
            }
            out
        }
    }
}
```

3. Обновить тест `dump_tree_text` (строки 218-225):
```rust
#[test]
fn dump_tree_text() {
    let buf = make_image_with_volume();
    let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
    let s = dump_tree(&img.root, DumpFormat::Text);
    assert!(s.contains("Image"));
    assert!(s.contains("Volume"));
}
```
(тест остаётся, assertions проходят — формат содержит эти строки через `node_type_name`.)

- [ ] **Step 2: Запустить тесты**

Run: `cargo test -p uefi-engine parser::image && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-engine/src/parser/image.rs
git commit -m "refactor(uefi-engine): dump_tree delegates to uefi-common::format (text mode)"
```

---

### Task 9: proto + engine + 3 mock'а — добавить SearchItems RPC

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-engine/src/rpc/server.rs`
- Modify: `crates/uefi-cli/tests/mock_server.rs`
- Modify: `crates/uefi-tui/tests/mock_server.rs`
- Modify: `crates/uefi-gateway/tests/mock_server.rs`

**Interfaces:**
- Consumes: `prost/tonic` codegen для новых message'ов
- Produces: `rpc SearchItems(SearchItemsRequest) returns (SearchItemsResponse)` в trait `EngineService`

> **Важно:** все 4 impl'а `EngineService` (engine + 3 mock'а) должны обновиться **в одном коммите**, иначе `cargo test --all` не соберётся (E0046 missing trait method).

- [ ] **Step 1: Обновить engine.proto**

В `crates/uefi-proto/proto/engine.proto` добавить в `service EngineService` (после `rpc ListItems`):
```proto
  rpc SearchItems(SearchItemsRequest) returns (SearchItemsResponse);
```

Добавить enum после `enum DumpFormat`:
```proto
enum SearchMode { NAME = 0; UTF8 = 1; UTF16 = 2; BYTES = 3; }
```

Добавить message'ы (после `ListItemsResponse`):
```proto
message SearchItemsRequest { string image_id = 1; string query = 2; repeated SearchMode modes = 3; uint32 limit = 4; }
message SearchItemsResponse { repeated Item items = 1; }
```

- [ ] **Step 2: Обновить engine rpc/server.rs — stub (пока без impl поиска)**

В `crates/uefi-engine/src/rpc/server.rs` добавить после `list_items` (строка 134) обработчик:
```rust
    async fn search_items(
        &self,
        req: Request<SearchItemsRequest>,
    ) -> RpcResult<SearchItemsResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let modes: Vec<uefi_common::search::SearchMode> = r
            .modes
            .iter()
            .map(|m| match m {
                0 => uefi_common::search::SearchMode::Name,
                1 => uefi_common::search::SearchMode::Utf8,
                2 => uefi_common::search::SearchMode::Utf16Le,
                _ => uefi_common::search::SearchMode::Bytes,
            })
            .collect();
        let limit = if r.limit == 0 { usize::MAX } else { r.limit as usize };
        let items = crate::parser::image::search(&img.root, &r.query, &modes, limit);
        Ok(Response::new(SearchItemsResponse { items }))
    }
```

> `crate::parser::image::search` будет реализован в Task 10. Если хочется гарантировать compile-тест на этом шаге — можно временно заменить тело на `Ok(Response::new(SearchItemsResponse { items: vec![] }))` и переключить в Task 10. Рекомендуется: сразу правильная реализация через `search` из Task 10. Тогда Task 9 и Task 10 объединяются — выполняй Task 10 перед компиляцией этого шага.

> **Решение:** выполняем Task 9 (proto + 3 mock'а) с **stub** server-обработчиком (`items: vec![]`), затем Task 10 — реализация `parser::image::search`, и в Step 4 Task 10 подключаем настоящий вызов. Это держит шаги атомарными и компилируемыми.

Stub для server.rs (если Task 10 ещё не выполнен):
```rust
    async fn search_items(
        &self,
        _req: Request<SearchItemsRequest>,
    ) -> RpcResult<SearchItemsResponse> {
        Ok(Response::new(SearchItemsResponse { items: vec![] }))
    }
```

- [ ] **Step 3: Добавить search_items в 3 mock'а**

Для каждого файла (`crates/uefi-cli/tests/mock_server.rs`, `crates/uefi-tui/tests/mock_server.rs`, `crates/uefi-gateway/tests/mock_server.rs`) добавить в `impl EngineService for MockEngine` (рядом с `list_items`):
```rust
    async fn search_items(
        &self,
        _req: Request<SearchItemsRequest>,
    ) -> Result<Response<SearchItemsResponse>, Status> {
        Ok(Response::new(SearchItemsResponse { items: vec![] }))
    }
```

- [ ] **Step 4: Запустить cargo test --all (обновлённый trait должен компилироваться везде)**

Run: `cargo test --all`
Expected: PASS (сборка всех 4 impl'ов, тесты проходят — search_items пока stub)

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-proto/proto/engine.proto crates/uefi-engine/src/rpc/server.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-tui/tests/mock_server.rs crates/uefi-gateway/tests/mock_server.rs
git commit -m "feat(proto,engine): add SearchItems RPC stub (handlers wired in engine + 3 mocks)"
```

---

### Task 10: uefi-engine — реализация parser::image::search

**Files:**
- Modify: `crates/uefi-engine/src/parser/image.rs`
- Modify: `crates/uefi-engine/src/rpc/server.rs` (заменить stub на настоящий вызов)

**Interfaces:**
- Consumes: `uefi_common::search::{SearchMode, match_name, find_utf8, find_utf16le, parse_hex_pattern}`, `crate::types::FfsNode`, `crate::ffs::*`
- Produces: `pub fn search(root: &FfsNode, query: &str, modes: &[SearchMode], limit: usize) -> Vec<Item>`

Логика: обходит дерево, для каждого узла FfsType::Section (subtype в SectionType) — проверяет каждый активный mode. Совпадение → добавляет `Item` в результат (path/type/subtype/guid/offset/size/name). Limit обрывает обход.

- [ ] **Step 1: Написать failing тесты**

В `crates/uefi-engine/src/parser/image.rs`, в `mod tests` добавить:
```rust
#[test]
fn search_finds_section_by_name_mode() {
    let ui_section = FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_UI,
        offset: 100,
        header: vec![0; 4],
        body: encode_utf16le_null("Setup"),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let root = FfsNode {
        guid: None,
        node_type: FfsType::Image,
        subtype: 0,
        offset: 0,
        header: vec![],
        body: vec![],
        tail: vec![],
        children: vec![ui_section],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let res = search(
        &root,
        "set",
        &[uefi_common::search::SearchMode::Name],
        100,
    );
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].subtype, EFI_SECTION_UI as u32);
    assert_eq!(res[0].name, "Setup");
}

#[test]
fn search_finds_section_by_utf8_body() {
    let raw_section = FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_RAW,
        offset: 0,
        header: vec![0; 4],
        body: b"Hello World".to_vec(),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let root = FfsNode {
        guid: None,
        node_type: FfsType::Image,
        subtype: 0,
        offset: 0,
        header: vec![],
        body: vec![],
        tail: vec![],
        children: vec![raw_section],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let res = search(
        &root,
        "World",
        &[uefi_common::search::SearchMode::Utf8],
        100,
    );
    assert_eq!(res.len(), 1);
}

#[test]
fn search_limit_truncates_results() {
    let sections: Vec<FfsNode> = (0..5)
        .map(|_| FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0; 4],
            body: b"needle".to_vec(),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        })
        .collect();
    let root = FfsNode {
        guid: None,
        node_type: FfsType::Image,
        subtype: 0,
        offset: 0,
        header: vec![],
        body: vec![],
        tail: vec![],
        children: sections,
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let res = search(
        &root,
        "needle",
        &[uefi_common::search::SearchMode::Utf8],
        3,
    );
    assert_eq!(res.len(), 3);
}

#[test]
fn search_skips_non_section_nodes() {
    let file = FfsNode {
        guid: None,
        node_type: FfsType::File,
        subtype: 0x01,
        offset: 0,
        header: vec![0; 24],
        body: b"needle".to_vec(),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let root = FfsNode {
        guid: None,
        node_type: FfsType::Image,
        subtype: 0,
        offset: 0,
        header: vec![],
        body: vec![],
        tail: vec![],
        children: vec![file],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    let res = search(
        &root,
        "needle",
        &[uefi_common::search::SearchMode::Utf8],
        100,
    );
    assert_eq!(res.len(), 0, "search must skip File nodes");
}
```

Добавить в `use` тестов `use crate::ffs::EFI_SECTION_RAW;` (если ещё не добавлено в Task 7).

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine parser::image::tests::search`
Expected: FAIL — `search` не определена

- [ ] **Step 3: Реализовать search()**

В `crates/uefi-engine/src/parser/image.rs` (после `list_recursive`) добавить:
```rust
pub fn search(
    root: &FfsNode,
    query: &str,
    modes: &[uefi_common::search::SearchMode],
    limit: usize,
) -> Vec<Item> {
    let mut out = vec![];
    if modes.is_empty() || limit == 0 {
        return out;
    }
    let mut path = String::new();
    search_recursive(root, &mut path, query, modes, limit, &mut out);
    out
}

fn search_recursive(
    node: &FfsNode,
    path: &mut String,
    query: &str,
    modes: &[uefi_common::search::SearchMode],
    limit: usize,
    out: &mut Vec<Item>,
) {
    if out.len() >= limit {
        return;
    }
    if node.node_type == FfsType::Section {
        if section_matches(node, query, modes) {
            let name = node_name(node);
            out.push(Item {
                path: path.clone(),
                r#type: node.node_type as u32,
                subtype: node.subtype as u32,
                guid: node
                    .guid
                    .map(|g| crate::guid_to_upper_string(&g))
                    .unwrap_or_default(),
                offset: node.offset as u64,
                size: (node.header.len() + node.body.len() + node.tail.len()) as u64,
                name,
            });
            if out.len() >= limit {
                return;
            }
        }
    }
    let saved_len = path.len();
    for (i, child) in node.children.iter().enumerate() {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&i.to_string());
        search_recursive(child, path, query, modes, limit, out);
        path.truncate(saved_len);
        if out.len() >= limit {
            return;
        }
    }
}

fn section_matches(
    node: &FfsNode,
    query: &str,
    modes: &[uefi_common::search::SearchMode],
) -> bool {
    let name = node_name(node);
    for &m in modes {
        let hit = match m {
            uefi_common::search::SearchMode::Name => uefi_common::search::match_name(&name, query),
            uefi_common::search::SearchMode::Utf8 => {
                uefi_common::search::find_utf8(&node.body, query.as_bytes())
            }
            uefi_common::search::SearchMode::Utf16Le => {
                uefi_common::search::find_utf16le(&node.body, query)
            }
            uefi_common::search::SearchMode::Bytes => uefi_common::search::parse_hex_pattern(query)
                .map(|p| uefi_common::search::find_utf8(&node.body, &p))
                .unwrap_or(false),
        };
        if hit {
            return true;
        }
    }
    false
}
```

- [ ] **Step 4: Подключить настоящий вызов в rpc/server.rs (заменить stub)**

В `crates/uefi-engine/src/rpc/server.rs` заменить stub-обработчик `search_items` (из Task 9 Step 2) на:
```rust
    async fn search_items(
        &self,
        req: Request<SearchItemsRequest>,
    ) -> RpcResult<SearchItemsResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images
            .get(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let modes: Vec<uefi_common::search::SearchMode> = r
            .modes
            .iter()
            .map(|m| match m {
                0 => uefi_common::search::SearchMode::Name,
                1 => uefi_common::search::SearchMode::Utf8,
                2 => uefi_common::search::SearchMode::Utf16Le,
                _ => uefi_common::search::SearchMode::Bytes,
            })
            .collect();
        let limit = if r.limit == 0 { usize::MAX } else { r.limit as usize };
        let items = crate::parser::image::search(&img.root, &r.query, &modes, limit);
        Ok(Response::new(SearchItemsResponse { items }))
    }
```

- [ ] **Step 5: Запустить все тесты engine**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/parser/image.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): implement parser::image::search (Name/Utf8/Utf16Le/Bytes) and wire RPC"
```

---

### Task 11: uefi-cli — output::print_items text + subcommand search

**Files:**
- Modify: `crates/uefi-cli/src/output.rs`
- Modify: `crates/uefi-cli/src/client.rs`
- Modify: `crates/uefi-cli/src/commands/image.rs`
- Modify: `crates/uefi-cli/src/main.rs`

**Interfaces:**
- Consumes: `uefi_common::names::{node_type_name, file_type_name_or_raw, section_type_name_or_raw}`, `uefi_proto::{SearchItemsRequest, SearchMode}`
- Produces:
  - `Client::search_items(image_id, query, modes: &[SearchMode], limit) -> Vec<Item>`
  - `commands::image::search(query, modes, limit, sock, format)`
  - `ImageCmd::Search` subcommand
  - `print_items` Text формат с symbolическими именами

- [ ] **Step 1: Обновить output.rs — Text формат с символическими именами**

В `crates/uefi-cli/src/output.rs` заменить `OutputFormat::Text =>` ветку (строки 38-44) в `print_items`:
```rust
        OutputFormat::Text => {
            for it in items {
                let node = uefi_common::names::node_type_name(it.r#type);
                let sub_name = match it.r#type {
                    66 => uefi_common::names::file_type_name_or_raw(it.subtype as u8),
                    67 => uefi_common::names::section_type_name_or_raw(it.subtype as u8),
                    _ => String::new(),
                };
                let sub_part = if sub_name.is_empty() {
                    String::new()
                } else {
                    format!("({sub_name})")
                };
                let name_part = if it.name.is_empty() {
                    String::new()
                } else {
                    format!(" name={}", it.name)
                };
                println!(
                    "{}  {}{sub_part} type={} subtype={:02X} guid={} off={} size={}{name_part}",
                    it.path, node, it.r#type, it.subtype, it.guid, it.offset, it.size,
                );
            }
        }
```

> TSV и JSON ветки не трогаем (машинный разбор).

- [ ] **Step 2: Добавить Client::search_items**

В `crates/uefi-cli/src/client.rs` добавить после `list_items` (строка 133):
```rust
    pub async fn search_items(
        &mut self,
        image_id: &str,
        query: &str,
        modes: &[uefi_proto::SearchMode],
        limit: u32,
    ) -> Result<Vec<Item>, AppError> {
        let req = SearchItemsRequest {
            image_id: image_id.into(),
            query: query.into(),
            modes: modes.iter().map(|m| *m as i32).collect(),
            limit,
        };
        Ok(self
            .inner
            .search_items(auth_req(&self.state, req))
            .await?
            .into_inner()
            .items)
    }
```

> `SearchItemsRequest`/`SearchMode` уже реэкспортируются через `use uefi_proto::*;` (строка 8).

- [ ] **Step 3: Добавить commands::image::search**

В `crates/uefi-cli/src/commands/image.rs` добавить в конец файла:
```rust
pub async fn search(
    query: &str,
    modes: &[uefi_proto::SearchMode],
    limit: u32,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let items = client.search_items(&image_id, query, modes, limit).await?;
    crate::output::print_items(&items, format);
    Ok(())
}
```

- [ ] **Step 4: Добавить ImageCmd::Search в main.rs**

В `crates/uefi-cli/src/main.rs`:

В `enum ImageCmd` (после `Find`, строка 72) добавить:
```rust
    Search {
        query: String,
        #[arg(long, value_enum, num_args = 0.., default_values_t = vec![SearchModeCli::Name])]
        mode: Vec<SearchModeCli>,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
```

Добавить перед `#[tokio::main]` (после `enum SetupCmd`):
```rust
#[derive(clap::ValueEnum, Clone, Debug)]
enum SearchModeCli {
    Name,
    Utf8,
    Utf16,
    Bytes,
}
```

В `dispatch` (в ветке `Cmd::Image`, после `ImageCmd::Find`) добавить:
```rust
            ImageCmd::Search { query, mode, limit } => {
                let modes: Vec<uefi_proto::SearchMode> = mode
                    .iter()
                    .map(|m| match m {
                        SearchModeCli::Name => uefi_proto::SearchMode::Name as i32,
                        SearchModeCli::Utf8 => uefi_proto::SearchMode::Utf8 as i32,
                        SearchModeCli::Utf16 => uefi_proto::SearchMode::Utf16 as i32,
                        SearchModeCli::Bytes => uefi_proto::SearchMode::Bytes as i32,
                    })
                    .collect();
                let m_refs: Vec<uefi_proto::SearchMode> = modes;
                commands::image::search(query, &m_refs.iter().map(|m| *m as i32).collect::<Vec<_>>(), *limit, sock, format).await
            }
```

> Wait — `commands::image::search` принимает `&[uefi_proto::SearchMode]`. В prost сгенерируется `pub enum SearchMode: i32`. Правильный тип — `&[i32]` или трейт. Уточним сигнатуры: заменим `modes: &[uefi_proto::SearchMode]` на `modes: &[i32]` во ВСЕХ сигнатурах (client, commands), чтобы проще матчилось с prost-типом (proto enum item'ы — это просто `i32` константы). Обнови:
> - `Client::search_items(image_id, query, modes: &[i32], limit)`
> - `commands::image::search(query, modes: &[i32], limit, sock, format)`
> - в dispatch: `let modes: Vec<i32> = mode.iter().map(|m| ...).collect();` → `commands::image::search(query, &modes, *limit, sock, format).await`

Updated `dispatch`:
```rust
            ImageCmd::Search { query, mode, limit } => {
                let modes: Vec<i32> = mode
                    .iter()
                    .map(|m| match m {
                        SearchModeCli::Name => uefi_proto::SearchMode::Name as i32,
                        SearchModeCli::Utf8 => uefi_proto::SearchMode::Utf8 as i32,
                        SearchModeCli::Utf16 => uefi_proto::SearchMode::Utf16 as i32,
                        SearchModeCli::Bytes => uefi_proto::SearchMode::Bytes as i32,
                    })
                    .collect();
                commands::image::search(query, &modes, *limit, sock, format).await
            }
```

Updated `Client::search_items` signature: `modes: &[i32]`, и в `SearchItemsRequest { modes: modes.to_vec(), ... }`.

Updated `commands::image::search` signature: `modes: &[i32]`, вызов `client.search_items(&image_id, query, modes, limit)`.

- [ ] **Step 5: Запустить все тесты CLI + clippy**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-cli/src/output.rs crates/uefi-cli/src/client.rs crates/uefi-cli/src/commands/image.rs crates/uefi-cli/src/main.rs
git commit -m "feat(uefi-cli): text format with symbolic type names; add search subcommand"
```

---

### Task 12: real_image tests — проверить node_name="Setup" и search

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs`

**Interfaces:**
- Consumes: `uefi_engine::parser::image::{list_items, search}`, `uefi_engine::types::ImageMode`, `uefi_common::search::SearchMode`

- [ ] **Step 1: Расширить `real_image_parse_image_full` — assert FFS с name="Setup"**

В `crates/uefi-engine/tests/real_image.rs`, в `real_image_parse_image_full` (строки 277-355) заменить блок с dump_tree (строки 337-346) на:
```rust
    let items = list_items(&img.root, None);
    assert!(
        items.len() > 1000,
        "expected many list items, got {}",
        items.len()
    );
    assert!(
        items
            .iter()
            .any(|i| i.r#type == FfsType::File as u32 && i.name == "Setup"),
        "expected an FFS file named 'Setup' (UI section lifted), got names: {:?}",
        items
            .iter()
            .filter(|i| i.r#type == FfsType::File as u32 && !i.name.is_empty())
            .map(|i| &i.name)
            .take(10)
            .collect::<Vec<_>>()
    );

    eprintln!(
        "real_image parse_image: {} top-level volumes, main FV files={}, guided={}, decompressed={}, items={}, named_files={}",
        img.root.children.len(),
        main.children.len(),
        guided,
        decompressed,
        items.len(),
        items
            .iter()
            .filter(|i| i.r#type == FfsType::File as u32 && !i.name.is_empty())
            .count(),
    );
```

> `dump_tree` больше не вызывается в тестах, но функция `dump_tree` остаётся в API (engine её использует для RPC DumpTree).

- [ ] **Step 2: Добавить тест real_image_search_finds_setup_by_name**

В конец `crates/uefi-engine/tests/real_image.rs`:
```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_search_finds_setup_by_name() {
    use uefi_common::search::SearchMode;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    let matches = search(
        &img.root,
        "setup",
        &[SearchMode::Name],
        50,
    );
    assert!(
        matches
            .iter()
            .any(|m| m.name == "Setup" && m.r#type == FfsType::Section as u32),
        "expected to find UI section 'Setup' by name; got {} matches: {:?}",
        matches.len(),
        matches.iter().map(|m| &m.name).take(5).collect::<Vec<_>>(),
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_search_finds_utf8_string_in_pe32() {
    use uefi_common::search::SearchMode;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    // ".reloc" — стандартная PE-секция, почти всегда присутствует в PE32-модулях
    let matches = search(
        &img.root,
        ".reloc",
        &[SearchMode::Utf8],
        50,
    );
    assert!(
        matches.len() >= 5,
        "expected multiple PE32 sections containing '.reloc' string, got {}",
        matches.len()
    );
}
```

В `use` вверху файла (строка 5) добавить:
```rust
use uefi_engine::parser::image::{dump_tree, list_items, parse_image, search};
```
(если `dump_tree` больше не используется в тестах — убрать).

> Проверь, что `.reloc` реально встречается в PE32-телах на этом образце. Если тест падает — замени на `"MZ"` (PE-header magic) — точно есть в каждом PE32. Или на `".text"`. Эти строки — часть PE-формата, гарантированно есть в PE32-секциях.

- [ ] **Step 3: Запустить игнорируемые тесты (если BIOS доступен)**

Run: `cargo test -p uefi-engine -- --ignored real_image`
Expected: PASS (если `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` присутствует)

Если файл недоступен — тесты пропускаются (#[ignore]).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): real_image covers node_name=Setup lift + search by name/utf8"
```

---

### Task 13: TODO в TUI commands.rs (минимальный markers для будущей миграции)

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs`

**Interfaces:** нет кодовых изменений.

> Этот шаг не добавляет функциональность, только помечает известную проблему для будущего подплана миграции TUI на list_items.

- [ ] **Step 1: Добавить TODO-комментарий над parse_tree_dump**

В `crates/uefi-tui/src/commands.rs` над `fn parse_tree_dump` (строка 191) добавить:
```rust
// TODO(2026-08-09): migrate to list_items RPC — current text-parser drops subtype, parses "File"/"Volume" as int (always 0), and reads "subtype=07" as name. See docs/superpowers/specs/2026-08-09-display-and-search-design.md
```

- [ ] **Step 2: Commit**

```bash
git add crates/uefi-tui/src/commands.rs
git commit -m "docs(uefi-tui): mark parse_tree_dump as broken (TODO migrate to list_items)"
```

---

## Финальная проверка

- [ ] **Final: cargo test --all + clippy**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```
Expected: PASS

- [ ] **Final: smoke-тест CLI (опционально, с запущенным engine)**

```bash
# в одном терминале
cargo run -p uefi-engine --bin engine &
# в другом
cargo run -p uefi-cli -- session init
cargo run -p uefi-cli -- image open refs/fw/HNX99TF_200525_original_E5C88C6F.bin
cargo run -p uefi-cli -- image dump 2>/dev/null | head -20   # stdout: tree with names
cargo run -p uefi-cli -- image dump 2>&1 >/dev/null | head -10 # stderr: legend
cargo run -p uefi-cli -- image search "setup"                # → 1 Section named Setup
cargo run -p uefi-cli -- image search ".reloc" --mode utf8   # → PE32 sections
```

Expected:
- `image dump` stdout показывает `File(DXE driver) subtype=07 guid=... name=Setup`
- `image dump` stderr показывает `Legend: File types: 07 = DXE driver, F0 = Pad ... Section types: 02 = GUID defined, 10 = PE32 image, 15 = UI ...`
- `image search "setup"` находит UI-секцию Setup
- `image search ".reloc" --mode utf8` находит PE32-секции

## Self-Review Checklist

- ✅ **Spec coverage:** все 6 целей spec покрыты задачами:
  1. Символические имена в text → Task 11 (output.rs)
  2. UI-имя к FFS → Task 7 (node_name lift)
  3. Легенда в stderr → Task 4 (format::format_legend), используется в CLI dump (см. будущий подплан CLI-дамп через list_items — пока engine dump_tree форматирует через format_tree без legend в stderr; legend доступен через format_legend отдельно, подключение в CLI dump — отдельная правка)

  > **План-уточнение:** текущий CLI `image dump` печатает текст из RPC DumpTreeResponse как есть (engine через format_tree, без legend в stderr). Чтобы legend уходил в stderr — CLI dump должен вызывать list_items (а не dump_tree) и сам печатать format_tree+legend. Это противоречит решению «не ломать dump_tree». Компромисс:
  > - CLI dump переключается на list_items (это **клиентское** изменение, не ломает DumpTree RPC).
  > - Добавим как **Task 14** ниже.

  4. Поиск → Tasks 9, 10 (RPC + реализация), Task 11 (CLI subcommand)
  5. Устранение дублирования → Task 8 (engine dump_tree делегирует в uefi-common::format)
  6. PI-константы → Tasks 2, 6
- ✅ **Placeholder scan:** нет TBD/TODO в коде (кроме разрешённого Task 13 — ссылка на spec)
- ✅ **Type consistency:** `SearchMode` (proto) — `i32` (prost); `uefi_common::search::SearchMode` — отдельный enum; конвертация в server.rs; `Item` proto vs `TreeRow` (fields same names/types)
- ✅ **Сompile story:** каждый task оставляет код компилируемым (`cargo test -p <crate>` зелёный после каждого Step)

---

### Task 14: uefi-cli — dump через list_items + legend в stderr

**Files:**
- Modify: `crates/uefi-cli/src/commands/image.rs`

**Interfaces:**
- Consumes: `uefi_common::format::{TreeRow, format_tree, format_legend}`, `crate::Client::list_items`

> CLI `dump` переключается на `list_items` и сам форматировать через `uefi-common::format`. DumpTree RPC остаётся для TUI/Gateway. Это даёт legend в stderr (главное требование пользователя) без ломания RPC.

- [ ] **Step 1: Переписать commands::image::dump**

В `crates/uefi-cli/src/commands/image.rs` заменить `pub async fn dump(...)` (строки 51-63):
```rust
pub async fn dump(
    _format_name: &str,
    cli_sock: Option<&str>,
    _format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let items = client.list_items(&image_id, None).await?;
    let rows: Vec<uefi_common::format::TreeRow> = items
        .iter()
        .map(|it| uefi_common::format::TreeRow {
            path: it.path.clone(),
            type_: it.r#type,
            subtype: it.subtype as u8,
            guid: it.guid.clone(),
            offset: it.offset,
            size: it.size,
            name: it.name.clone(),
        })
        .collect();
    eprint!("{}", uefi_common::format::format_legend(&rows));
    print!("{}", uefi_common::format::format_tree(&rows));
    Ok(())
}
```

`_format_name` оставлен для обратной совместимости парсера CLI (аргумент `--format` остаётся, игнорируется). В будущей правке можно убрать.

- [ ] **Step 2: Запустить тесты CLI + clippy**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-cli/src/commands/image.rs
git commit -m "feat(uefi-cli): image dump uses list_items + uefi-common::format (legend to stderr)"
```

---

## Финал (после Task 14)

- [ ] **Final: cargo test --all**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

- [ ] **Финальный smoke-test:**

Запусти engine, открой образ, проверь:
1. `image dump` stdout — дерево с `File(DXE driver) subtype=07 name=Setup`
2. `image dump` stderr — `Legend: File types: 07 = DXE driver, F0 = Pad ...`
3. `image list --filter Dxe` — только строки с "Dxe" в name
4. `image search "setup"` — находит UI-секцию Setup
5. `image search "DE AD" --mode bytes` — находит byte-pattern (если есть)
6. `image search ".reloc" --mode utf8` — находит PE32-секции

## Plan complete
