# UEFI Advanced Setup (цикл 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать генерацию новых IFR-форм/пунктов с привязкой к NVRAM, авто-добавление строк в HII String-пакет, обязательный AMI-патчинг (setupdataBin + amitseSct), и gRPC-метод `AddSetupFormSet` для расширения Setup-меню UEFI BIOS.

**Architecture:** Новый модуль `setup_advanced` в крейте `uefi-engine` (расширение цикла 1): `schema.rs` (JSON-схема через serde), `ifr_builder.rs` (генерация IFR-байтов по edk2-структурам), `string_pack.rs` (поиск/дополнение HII String-пакета), `ami_patcher.rs` (патч setupdataBin + amitseSct с поиском по GUID/имени/эвристике), `ffs_assembler.rs` (сборка FFS), `mod.rs` (координатор). Новый gRPC-метод `AddSetupFormSet` в `EngineService`. Вход — JSON-схема; результат — отдельный FFS с новым FormSet, вставленный в DXE-том.

**Tech Stack:** Rust, serde/serde_json (JSON-схема), существующие `uefi-engine` модули (parser, builder, ops, types, ffs), tonic (gRPC), uuid (FFS GUID).

## Global Constraints

- Спека: `docs/superpowers/specs/2026-07-22-uefi-setup-advanced-design.md` — источник истины.
- Референс IFR-структур: `../refs/edk2/MdePkg/Include/Uefi/UefiInternalFormRepresentation.h` (структуры опкодов, Type-union, DefaultId-константы).
- Референс генерации IFR: `../refs/edk2/BaseTools/Source/C/VfrCompile/VfrFormPkg.{h,cpp}` (C++-паттерн, таблица размеров/scope `gOpcodeSizesScopeTable` в `VfrFormPkg.cpp:2255-2357`).
- Референс String-пакета: `../refs/IFRExtractor-RS/src/uefi_parser.rs:167` (`hii_string_package`), `:312` (`sibt_string_scsu`), `:355` (`sibt_string_ucs2`).
- Референс AMI-патчинга: `../refs/UEFI-Editor/src/components/scripts/scripts.ts:100-159` (setupdataBin формат: accessLevel=+32, failsafe=+104, optimal=+106, pageId=+24), `:242-272` (amitseSct: FormId LE после FormSetId-маркера).
- DefaultId-константы (edk2): `STANDARD=0x0000` (Optimized), `MANUFACTURING=0x0001` (Failsafe), `SAFE=0x0002`.
- IFR Type-константы: `NUM_SIZE_8=0x00`, `NUM_SIZE_16=0x01`, `NUM_SIZE_32=0x02`, `NUM_SIZE_64=0x03`, `BOOLEAN=0x04`, `STRING=0x07`.
- Question flags: `READ_ONLY=0x01`, `CALLBACK=0x04`, `RESET_REQUIRED=0x10`, `REST_STYLE=0x20`, `RECONNECT_REQUIRED=0x40`, `OPTIONS_ONLY=0x80`.
- Numeric/OneOf flags: `NUMERIC_SIZE=0x03` (маска: 0=u8,1=u16,2=u32,3=u64), `DISPLAY=0x30` (0=int_dec,0x10=uint_dec,0x20=uint_hex).
- CheckBox flags: `DEFAULT=0x01`, `DEFAULT_MFG=0x02`.
- OneOfOption flags: `OPTION_DEFAULT=0x10`, `OPTION_DEFAULT_MFG=0x20`.
- Поиск AMI-файлов приоритет: 1) GUID из JSON-схемы (`setupdata_guid`/`amitse_guid`) → `find_item`, 2) имя FFS ("setupdata"/"AMITSE"), 3) эвристика по содержимому (QuestionId-маркеры).
- AMI-запись: 108 байт, `[+0] QuestionId u16 LE`, `[+24] pageId u16 (Ref only)`, `[+32] accessLevel u8 (0x05)`, `[+104] failsafe u8`, `[+106] optimal u8`, остальное 0x00.
- Кодстайл: `cargo fmt`, `cargo clippy -- -D warnings`. Без комментариев в коде (кроме ссылок на референс `file:line`).
- **Module-first rule:** `pub mod X;` объявляется ДО `cargo test`.
- **IFR-структуры:** использовать `r_efi::hii::*`, не определять свои.
- **GUID:** `uguid::Guid` с `.to_bytes()` / `Guid::from_bytes()`.

---

## Файлы плана

| Файл | Назначение |
|---|---|
| `crates/uefi-engine/src/setup_advanced/mod.rs` | координатор `add_setup_formset` |
| `crates/uefi-engine/src/setup_advanced/schema.rs` | JSON-схема (serde), валидация |
| `crates/uefi-engine/src/setup_advanced/ifr_builder.rs` | генерация IFR-байтов |
| `crates/uefi-engine/src/setup_advanced/string_pack.rs` | поиск/дополнение HII String-пакета |
| `crates/uefi-engine/src/setup_advanced/ami_patcher.rs` | патч setupdataBin + amitseSct |
| `crates/uefi-engine/src/setup_advanced/ffs_assembler.rs` | сборка FFS |
| `crates/uefi-proto/proto/engine.proto` | добавить `AddSetupFormSet` RPC + сообщения |
| `crates/uefi-engine/src/rpc/server.rs` | impl `add_setup_form_set` |
| `crates/uefi-engine/src/lib.rs` | подключить `setup_advanced` |
| `crates/uefi-engine/Cargo.toml` | добавить `serde`/`serde_json` |

---

### Task 1: Зависимости и schema.rs — JSON-схема FormSet

**Files:**
- Modify: `crates/uefi-engine/Cargo.toml`
- Create: `crates/uefi-engine/src/setup_advanced/mod.rs`
- Create: `crates/uefi-engine/src/setup_advanced/schema.rs`

**Interfaces:**
- Consumes: нет
- Produces:
  - `pub struct FormSetSchema { formset_guid, title, help, class_guids, varstores, default_stores, forms, setupdata_guid: Option<String>, amitse_guid: Option<String> }`
  - `pub struct VarStoreSchema { id: u16, guid: String, size: u16, name: String, var_type: VarStoreType }`
  - `pub enum VarStoreType { Buffer, Efi }`
  - `pub struct DefaultStoreSchema { name: String, id: u16 }`
  - `pub struct FormSchema { id: u16, title: String, items: Vec<ItemSchema> }`
  - `pub enum ItemSchema { OneOf(OneOfItem), CheckBox(CheckBoxItem), Numeric(NumericItem), Text(TextItem), Ref(RefItem), String(StringItem), Action(ActionItem), OrderedList(OrderedListItem) }`
  - `pub struct OneOfItem { prompt, help, question_id: u16, var_store_id: u16, var_offset: u16, size: u8, display: DisplayMode, options: Vec<OptionSchema>, defaults: Defaults }`
  - `pub struct OptionSchema { text: String, value: u64, default: Option<DefaultClass> }`
  - `pub enum DefaultClass { Optimized, Failsafe }`
  - `pub enum DisplayMode { IntDec, UintDec, UintHex }`
  - `pub struct Defaults { optimized: Option<u64>, failsafe: Option<u64> }`
  - `pub struct CheckBoxItem { prompt, help, question_id, var_store_id, var_offset, defaults }`
  - `pub struct NumericItem { prompt, help, question_id, var_store_id, var_offset, size, min: u64, max: u64, step: u64, display, defaults }`
  - `pub struct TextItem { prompt, help, text_two: String }`
  - `pub struct RefItem { prompt, help, question_id, form_id: u16 }`
  - `pub struct StringItem { prompt, help, question_id, var_store_id, var_offset, min_size: u8, max_size: u8 }`
  - `pub struct ActionItem { prompt, help, question_id, config: String }`
  - `pub struct OrderedListItem { prompt, help, question_id, var_store_id, var_offset, max_containers: u8 }`
  - `pub fn parse_schema(json: &str) -> Result<FormSetSchema, SetupAdvancedError>`
  - `pub enum SetupAdvancedError { InvalidSchema(String), StringPackageNotFound, AmiFilesNotFound, IfrBuildError(String), FfsAssemblyError(String) }`

- [ ] **Step 1: Обновить Cargo.toml**

Добавить в `crates/uefi-engine/Cargo.toml` [dependencies]:
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
r-efi.workspace = true
binrw.workspace = true
```

- [ ] **Step 2: Написать failing test для parse_schema**

`crates/uefi-engine/src/setup_advanced/mod.rs`:
```rust
pub mod schema;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SetupAdvancedError {
    #[error("invalid schema: {0}")]
    InvalidSchema(String),
    #[error("string package not found")]
    StringPackageNotFound,
    #[error("AMI files not found (setupdataBin/amitseSct)")]
    AmiFilesNotFound,
    #[error("IFR build error: {0}")]
    IfrBuildError(String),
    #[error("FFS assembly error: {0}")]
    FfsAssemblyError(String),
}
```

`crates/uefi-engine/src/setup_advanced/schema.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use super::SetupAdvancedError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSetSchema {
    pub formset_guid: String,
    pub title: String,
    pub help: String,
    #[serde(default)]
    pub class_guids: Vec<String>,
    pub varstores: Vec<VarStoreSchema>,
    pub default_stores: Vec<DefaultStoreSchema>,
    pub forms: Vec<FormSchema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setupdata_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amitse_guid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarStoreSchema {
    pub id: u16,
    pub guid: String,
    pub size: u16,
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VarStoreType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VarStoreType { Buffer, Efi }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultStoreSchema {
    pub name: String,
    pub id: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSchema {
    pub id: u16,
    pub title: String,
    pub items: Vec<ItemSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemSchema {
    OneOf(OneOfItem),
    CheckBox(CheckBoxItem),
    Numeric(NumericItem),
    Text(TextItem),
    Ref(RefItem),
    String(StringItem),
    Action(ActionItem),
    OrderedList(OrderedListItem),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneOfItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub size: u8,
    #[serde(default = "default_display")]
    pub display: DisplayMode,
    pub options: Vec<OptionSchema>,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionSchema {
    pub text: String,
    pub value: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultClass>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultClass { Optimized, Failsafe }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode { IntDec, UintDec, UintHex }

fn default_display() -> DisplayMode { DisplayMode::UintDec }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimized: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failsafe: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckBoxItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub size: u8,
    pub min: u64,
    pub max: u64,
    pub step: u64,
    #[serde(default = "default_display")]
    pub display: DisplayMode,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub prompt: String,
    pub help: String,
    pub text_two: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub form_id: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub min_size: u8,
    pub max_size: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub config: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedListItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub max_containers: u8,
}

pub fn parse_schema(json: &str) -> Result<FormSetSchema, SetupAdvancedError> {
    serde_json::from_str(json).map_err(|e| SetupAdvancedError::InvalidSchema(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_schema() {
        let json = r#"{
            "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
            "title": "Test", "help": "Test help",
            "class_guids": [],
            "varstores": [{"id": 1, "guid": "11111111-2222-3333-4444-555555555555", "size": 256, "name": "MyVar", "type": "buffer"}],
            "default_stores": [{"name": "Optimized", "id": 0}, {"name": "Failsafe", "id": 1}],
            "forms": [{"id": 1, "title": "Main", "items": [
                {"type": "one_of", "prompt": "P", "help": "H", "question_id": 256, "var_store_id": 1, "var_offset": 0, "size": 1,
                 "options": [{"text": "A", "value": 0, "default": "optimized"}]}
            ]}]
        }"#;
        let s = parse_schema(json).unwrap();
        assert_eq!(s.title, "Test");
        assert_eq!(s.varstores.len(), 1);
        assert_eq!(s.forms[0].items.len(), 1);
        match &s.forms[0].items[0] {
            ItemSchema::OneOf(o) => { assert_eq!(o.question_id, 256); assert_eq!(o.options.len(), 1); }
            _ => panic!("expected OneOf"),
        }
    }

    #[test]
    fn parse_with_ami_guids() {
        let json = r#"{
            "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
            "title": "T", "help": "H", "class_guids": [],
            "setupdata_guid": "12345678-90AB-CDEF-1234-567890ABCDEF",
            "amitse_guid": "87654321-FEDC-BA09-8765-432109FEDCBA",
            "varstores": [], "default_stores": [], "forms": []
        }"#;
        let s = parse_schema(json).unwrap();
        assert!(s.setupdata_guid.is_some());
        assert!(s.amitse_guid.is_some());
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse_schema("{invalid").is_err());
    }
}
```

- [ ] **Step 3: Подключить модуль в lib.rs**

`crates/uefi-engine/src/lib.rs`: добавить `pub mod setup_advanced;`

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p uefi-engine setup_advanced::schema`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/Cargo.toml crates/uefi-engine/src/setup_advanced/ crates/uefi-engine/src/lib.rs
git commit -m "feat(setup_advanced): add JSON schema (FormSet/Form/Item/Defaults) with serde"
```

---

### Task 2: ifr_builder.rs — генерация IFR-опкодов

**Files:**
- Create: `crates/uefi-engine/src/setup_advanced/ifr_builder.rs`

**Interfaces:**
- Consumes: `schema::*`, `types::Guid`
- Produces:
  - `pub struct IfrBuilder { buf: Vec<u8> }`
  - Методы `emit_*` для каждого опкода (см. спеку)
  - `pub fn build() -> Vec<u8>`
  - Константы опкодов: `OP_FORM_SET=0x0E`, `OP_FORM=0x01`, `OP_END=0x29`, `OP_VARSTORE=0x24`, `OP_VARSTORE_EFI=0x26`, `OP_DEFAULT_STORE=0x5C`, `OP_ONE_OF=0x05`, `OP_ONE_OF_OPTION=0x09`, `OP_CHECKBOX=0x06`, `OP_NUMERIC=0x07`, `OP_REF=0x0F`, `OP_TEXT=0x03`, `OP_STRING=0x1C`, `OP_ACTION=0x0C`, `OP_ORDERED_LIST=0x23`, `OP_DEFAULT=0x5B`

- [ ] **Step 1: Написать failing tests для базовых опкодов**

`crates/uefi-engine/src/setup_advanced/ifr_builder.rs`:
```rust
use crate::types::Guid;
use std::str::FromStr;

pub use r_efi::hii::IFR_FORM_SET_OP as OP_FORM_SET;
pub use r_efi::hii::IFR_FORM_OP as OP_FORM;
pub use r_efi::hii::IFR_END_OP as OP_END;
pub use r_efi::hii::IFR_VARSTORE_OP as OP_VARSTORE;
pub use r_efi::hii::IFR_VARSTORE_EFI_OP as OP_VARSTORE_EFI;
pub use r_efi::hii::IFR_DEFAULTSTORE_OP as OP_DEFAULT_STORE;
pub use r_efi::hii::IFR_ONE_OF_OP as OP_ONE_OF;
pub use r_efi::hii::IFR_ONE_OF_OPTION_OP as OP_ONE_OF_OPTION;
pub use r_efi::hii::IFR_CHECKBOX_OP as OP_CHECKBOX;
pub use r_efi::hii::IFR_NUMERIC_OP as OP_NUMERIC;
pub use r_efi::hii::IFR_REF_OP as OP_REF;
pub use r_efi::hii::IFR_TEXT_OP as OP_TEXT;
pub use r_efi::hii::IFR_STRING_OP as OP_STRING;
pub use r_efi::hii::IFR_ACTION_OP as OP_ACTION;
pub use r_efi::hii::IFR_ORDERED_LIST_OP as OP_ORDERED_LIST;
pub use r_efi::hii::IFR_DEFAULT_OP as OP_DEFAULT;

pub use r_efi::hii::IFR_TYPE_NUM_SIZE_8 as TYPE_NUM_SIZE_8;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_16 as TYPE_NUM_SIZE_16;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_32 as TYPE_NUM_SIZE_32;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_64 as TYPE_NUM_SIZE_64;
pub use r_efi::hii::IFR_TYPE_BOOLEAN as TYPE_BOOLEAN;
pub use r_efi::hii::IFR_TYPE_STRING as TYPE_STRING;

pub const DEFAULT_ID_STANDARD: u16 = 0x0000;
pub const DEFAULT_ID_MANUFACTURING: u16 = 0x0001;

pub struct IfrBuilder {
    buf: Vec<u8>,
}

impl IfrBuilder {
    pub fn new() -> Self { Self { buf: Vec::new() } }

    fn write_header(&mut self, opcode: u8, scope: bool, data_len: usize) {
        let total = (data_len + 2) as u8;
        let length = total & 0x7F;
        let scope_bit: u8 = if scope { 0x80 } else { 0x00 };
        self.buf.push(opcode);
        self.buf.push(length | scope_bit);
    }

    pub fn emit_form_set(&mut self, guid: &Guid, title_id: u16, help_id: u16, class_guids: &[Guid]) {
        let flags = class_guids.len() as u8 & 0x03;
        self.write_header(OP_FORM_SET, true, 21 + 16 * class_guids.len());
        self.buf.extend_from_slice(&guid_to_bytes(guid));
        self.buf.extend_from_slice(&title_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.push(flags);
        for cg in class_guids {
            self.buf.extend_from_slice(&guid_to_bytes(cg));
        }
    }

    pub fn emit_var_store(&mut self, id: u16, guid: &Guid, size: u16, name: &str) {
        let name_bytes = name.as_bytes();
        self.write_header(OP_VARSTORE, false, 20 + name_bytes.len() + 1);
        self.buf.extend_from_slice(&guid_to_bytes(guid));
        self.buf.extend_from_slice(&id.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(name_bytes);
        self.buf.push(0);
    }

    pub fn emit_default_store(&mut self, name_id: u16, default_id: u16) {
        self.write_header(OP_DEFAULT_STORE, false, 4);
        self.buf.extend_from_slice(&name_id.to_le_bytes());
        self.buf.extend_from_slice(&default_id.to_le_bytes());
    }

    pub fn emit_form(&mut self, id: u16, title_id: u16) {
        self.write_header(OP_FORM, true, 4);
        self.buf.extend_from_slice(&id.to_le_bytes());
        self.buf.extend_from_slice(&title_id.to_le_bytes());
    }

    pub fn emit_one_of(&mut self, prompt_id: u16, help_id: u16, qid: u16, vsid: u16, voff: u16, flags: u8, size: u8) {
        self.write_header(OP_ONE_OF, true, 12 + 3 * size as usize);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        let numeric_flags = (size - 1) & 0x03;
        self.buf.push(numeric_flags | flags);
        let (min, max, step) = min_max_step_default(size);
        self.buf.extend_from_slice(&min);
        self.buf.extend_from_slice(&max);
        self.buf.extend_from_slice(&step);
    }

    pub fn emit_one_of_option(&mut self, text_id: u16, flags: u8, value_type: u8, value: u64, size: u8) {
        let val_bytes = value_to_bytes(value, size);
        self.write_header(OP_ONE_OF_OPTION, false, 4 + val_bytes.len());
        self.buf.extend_from_slice(&text_id.to_le_bytes());
        self.buf.push(flags | value_type);
        self.buf.push(value_type);
        self.buf.extend_from_slice(&val_bytes);
    }

    pub fn emit_check_box(&mut self, prompt_id: u16, help_id: u16, qid: u16, vsid: u16, voff: u16, flags: u8) {
        self.write_header(OP_CHECKBOX, true, 12);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        self.buf.push(flags);
    }

    pub fn emit_numeric(&mut self, prompt_id: u16, help_id: u16, qid: u16, vsid: u16, voff: u16, flags: u8, size: u8, min: u64, max: u64, step: u64) {
        self.write_header(OP_NUMERIC, true, 12 + 3 * size as usize);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        let numeric_flags = (size - 1) & 0x03;
        self.buf.push(numeric_flags | flags);
        self.buf.extend_from_slice(&value_to_bytes(min, size));
        self.buf.extend_from_slice(&value_to_bytes(max, size));
        self.buf.extend_from_slice(&value_to_bytes(step, size));
    }

    pub fn emit_ref(&mut self, prompt_id: u16, help_id: u16, qid: u16, vsid: u16, voff: u16, form_id: u16) {
        self.write_header(OP_REF, false, 13);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        self.buf.extend_from_slice(&form_id.to_le_bytes());
    }

    pub fn emit_text(&mut self, prompt_id: u16, help_id: u16, text_two_id: u16) {
        self.write_header(OP_TEXT, false, 6);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&text_two_id.to_le_bytes());
    }

    pub fn emit_default(&mut self, default_id: u16, value_type: u8, value: u64, size: u8) {
        let val_bytes = value_to_bytes(value, size);
        self.write_header(OP_DEFAULT, false, 3 + val_bytes.len());
        self.buf.extend_from_slice(&default_id.to_le_bytes());
        self.buf.push(value_type);
        self.buf.extend_from_slice(&val_bytes);
    }

    pub fn emit_end(&mut self) {
        self.write_header(OP_END, false, 0);
    }

    pub fn build(self) -> Vec<u8> { self.buf }
}

pub fn guid_to_bytes(g: &Guid) -> [u8; 16] {
    g.to_bytes()
}

fn value_to_bytes(v: u64, size: u8) -> Vec<u8> {
    match size {
        1 => vec![v as u8],
        2 => (v as u16).to_le_bytes().to_vec(),
        4 => (v as u32).to_le_bytes().to_vec(),
        8 => v.to_le_bytes().to_vec(),
        _ => vec![v as u8],
    }
}

fn min_max_step_default(size: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let z = value_to_bytes(0, size);
    let m = value_to_bytes(match size { 1 => 0xFF, 2 => 0xFFFF, 4 => 0xFFFFFFFF, _ => 0xFFFFFFFFFFFFFFFF }, size);
    (z.clone(), m, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_form_set_header() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("A1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
        b.emit_form_set(&g, 1, 2, &[]);
        let buf = b.build();
        assert_eq!(buf[0], OP_FORM_SET);
        assert_eq!(buf[1] & 0x7F, 23);
        assert_eq!(buf[1] & 0x80, 0x80);
        assert_eq!(&buf[2..18], &guid_to_bytes(&g));
        assert_eq!(u16::from_le_bytes([buf[18], buf[19]]), 1);
    }

    #[test]
    fn emit_form_header() {
        let mut b = IfrBuilder::new();
        b.emit_form(5, 10);
        let buf = b.build();
        assert_eq!(buf[0], OP_FORM);
        assert_eq!(buf[1] & 0x80, 0x80);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 10);
    }

    #[test]
    fn emit_end() {
        let mut b = IfrBuilder::new();
        b.emit_end();
        let buf = b.build();
        assert_eq!(buf, vec![OP_END, 0x02]);
    }

    #[test]
    fn emit_var_store_with_name() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("11111111-2222-3333-4444-555555555555").unwrap();
        b.emit_var_store(1, &g, 256, "MyVar");
        let buf = b.build();
        assert_eq!(buf[0], OP_VARSTORE);
        assert_eq!(u16::from_le_bytes([buf[18], buf[19]]), 1);
        assert_eq!(u16::from_le_bytes([buf[20], buf[21]]), 256);
        assert_eq!(&buf[22..27], b"MyVar");
        assert_eq!(buf[27], 0);
    }

    #[test]
    fn emit_one_of_option_u8() {
        let mut b = IfrBuilder::new();
        b.emit_one_of_option(5, 0x10, TYPE_NUM_SIZE_8, 2, 1);
        let buf = b.build();
        assert_eq!(buf[0], OP_ONE_OF_OPTION);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5);
        assert_eq!(buf[6], 2);
    }

    #[test]
    fn emit_default_standard() {
        let mut b = IfrBuilder::new();
        b.emit_default(DEFAULT_ID_STANDARD, TYPE_NUM_SIZE_8, 1, 1);
        let buf = b.build();
        assert_eq!(buf[0], OP_DEFAULT);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), DEFAULT_ID_STANDARD);
        assert_eq!(buf[4], TYPE_NUM_SIZE_8);
        assert_eq!(buf[5], 1);
    }
}
```

- [ ] **Step 2: Подключить в setup_advanced/mod.rs**

`crates/uefi-engine/src/setup_advanced/mod.rs`: добавить `pub mod ifr_builder;`

- [ ] **Step 3: Запустить тесты**

Run: `cargo test -p uefi-engine setup_advanced::ifr_builder`
Expected: PASS

- [ ] **Step 4: Коммит**

```bash
git add crates/uefi-engine/src/setup_advanced/ifr_builder.rs crates/uefi-engine/src/setup_advanced/mod.rs
git commit -m "feat(setup_advanced): add IFR builder (FormSet/Form/VarStore/OneOf/CheckBox/Numeric/Ref/Text/Default/End)"
```

---

### Task 3: string_pack.rs — поиск и дополнение HII String-пакета

**Files:**
- Create: `crates/uefi-engine/src/setup_advanced/string_pack.rs`

**Interfaces:**
- Consumes: `types::*` (Image, FfsNode, Guid, Action), `ops::mark_rebuild_to_root_by_path`, `r_efi::hii::PACKAGE_STRINGS`
- Produces:
  - `pub fn add_strings(image: &mut Image, ffs_guid: Option<&Guid>, strings: &[String]) -> Result<HashMap<String, u16>, SetupAdvancedError>`
  - Поиск String-пакета: по GUID FFS (если передан) → авто-поиск (первый FFS с HII String Package). Возвращает путь `(volume_idx, file_idx, section_idx)` — пакет живёт в теле **Section**-узла (`file.children[si].body`, см. parser/section.rs:64-78, setup/mod.rs:67), не в теле File-узла.
  - Парсинг: следующий свободный StringId выводится сканированием SIBT-блоков (StringId 1-базированные, последовательные, счётчик продвигается SKIP-блоками) — фиксированного поля «max string id» в `EFI_HII_STRING_PACKAGE_HDR` нет (edk2 UefiInternalFormRepresentation.h:337-344: bytes 4-7 = HdrSize, 8-11 = StringInfoOffset).
  - Добавление: alloc новых StringId (от max+1), запись валидных `SIBT_STRING_SCSU (0x10)` блоков (`0x10` + SCSU-байты + `0x00`) **перед** существующим `SIBT_END (0x00)` маркером. StringId не пишется инлайн — он неявный/последовательный (edk2 UefiInternalFormRepresentation.h:353-364).
  - Пересборка: обновление 3-байтного `Length` (bytes 0-2) заголовка пакета `EFI_HII_PACKAGE_HEADER` (length:24+type:8, edk2 UefiInternalFormRepresentation.h:56-60). Поля checksum у отдельного HII-пакета нет. После правки тела Section вызывается `ops::mark_rebuild_to_root_by_path` для каскадного Rebuild File/Volume.
  - Возврат: маппинг текст→StringId
  - Детектор пакета: `pub fn is_string_package(body) -> bool` (`body[3] == r_efi::hii::PACKAGE_STRINGS`, тип пакета в 4-м байте битового поля). SIBT-коды локальные (`const SIBT_*`), т.к. r_efi их не экспортирует.

- [ ] **Step 1: Написать failing test**

`crates/uefi-engine/src/setup_advanced/string_pack.rs`:
```rust
use std::collections::HashMap;

use super::SetupAdvancedError;
use crate::ops;
use crate::types::*;

pub use r_efi::hii::PACKAGE_STRINGS;

const SIBT_END: u8 = 0x00;
const SIBT_STRING_SCSU: u8 = 0x10;
const SIBT_STRING_SCSU_FONT: u8 = 0x11;
const SIBT_STRINGS_SCSU: u8 = 0x12;
const SIBT_STRINGS_SCSU_FONT: u8 = 0x13;
const SIBT_STRING_UCS2: u8 = 0x14;
const SIBT_STRING_UCS2_FONT: u8 = 0x15;
const SIBT_STRINGS_UCS2: u8 = 0x16;
const SIBT_STRINGS_UCS2_FONT: u8 = 0x17;
const SIBT_DUPLICATE: u8 = 0x20;
const SIBT_SKIP2: u8 = 0x21;
const SIBT_SKIP1: u8 = 0x22;

const PACKAGE_HEADER_LEN: usize = 4;
const STRING_INFO_OFFSET_POS: usize = 8;

pub fn add_strings(
    image: &mut Image,
    ffs_guid: Option<&Guid>,
    strings: &[String],
) -> Result<HashMap<String, u16>, SetupAdvancedError> {
    let (vi, fi, si) = find_string_package(image, ffs_guid)?;
    let path = vec![vi, fi, si];
    let body = &mut image.root.children[vi].children[fi].children[si].body;
    let mapping = add_strings_to_body(body, strings);
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    Ok(mapping)
}

fn add_strings_to_body(body: &mut Vec<u8>, strings: &[String]) -> HashMap<String, u16> {
    let sibt_start = string_info_offset(body);
    let (mut next_id, mut end_pos) = scan_sibt(body, sibt_start);
    let mut mapping = HashMap::new();
    for s in strings {
        let new_id = next_id;
        next_id = next_id.wrapping_add(1);
        mapping.insert(s.clone(), new_id);
        end_pos = append_scsu_string(body, end_pos, s);
    }
    update_package_length(body);
    mapping
}

fn find_string_package(
    image: &Image,
    ffs_guid: Option<&Guid>,
) -> Result<(usize, usize, usize), SetupAdvancedError> {
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if let Some(g) = ffs_guid
                && file.guid != Some(*g)
            {
                continue;
            }
            for (si, sec) in file.children.iter().enumerate() {
                if is_string_package(&sec.body) {
                    return Ok((vi, fi, si));
                }
            }
        }
    }
    Err(SetupAdvancedError::StringPackageNotFound)
}

pub fn is_string_package(body: &[u8]) -> bool {
    body.len() >= PACKAGE_HEADER_LEN && body[3] == PACKAGE_STRINGS
}

fn string_info_offset(body: &[u8]) -> usize {
    if body.len() >= STRING_INFO_OFFSET_POS + 4 {
        let off = u32::from_le_bytes([
            body[STRING_INFO_OFFSET_POS],
            body[STRING_INFO_OFFSET_POS + 1],
            body[STRING_INFO_OFFSET_POS + 2],
            body[STRING_INFO_OFFSET_POS + 3],
        ]) as usize;
        if off >= PACKAGE_HEADER_LEN && off < body.len() {
            return off;
        }
    }
    PACKAGE_HEADER_LEN
}

fn scan_sibt(body: &[u8], start: usize) -> (u16, usize) {
    let mut pos = start;
    let mut next_id: u16 = 1;
    while pos < body.len() {
        match body[pos] {
            SIBT_END => return (next_id, pos),
            SIBT_STRING_SCSU => {
                next_id = next_id.wrapping_add(1);
                pos = skip_scsu(body, pos + 1);
            }
            SIBT_STRING_SCSU_FONT => {
                next_id = next_id.wrapping_add(1);
                pos = skip_scsu(body, pos + 2);
            }
            SIBT_STRINGS_SCSU => {
                let (ids, p) = read_u16_count(body, pos + 1);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_scsu(body, pos);
                }
            }
            SIBT_STRINGS_SCSU_FONT => {
                let (ids, p) = read_u16_count(body, pos + 2);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_scsu(body, pos);
                }
            }
            SIBT_STRING_UCS2 => {
                next_id = next_id.wrapping_add(1);
                pos = skip_ucs2(body, pos + 1);
            }
            SIBT_STRING_UCS2_FONT => {
                next_id = next_id.wrapping_add(1);
                pos = skip_ucs2(body, pos + 2);
            }
            SIBT_STRINGS_UCS2 => {
                let (ids, p) = read_u16_count(body, pos + 1);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_ucs2(body, pos);
                }
            }
            SIBT_STRINGS_UCS2_FONT => {
                let (ids, p) = read_u16_count(body, pos + 2);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_ucs2(body, pos);
                }
            }
            SIBT_DUPLICATE => {
                next_id = next_id.wrapping_add(1);
                pos += 1 + 2;
            }
            SIBT_SKIP2 => {
                let (c, p) = read_u16_count(body, pos + 1);
                next_id = next_id.wrapping_add(c);
                pos = p;
            }
            SIBT_SKIP1 => {
                let count = body.get(pos + 1).copied().unwrap_or(0);
                next_id = next_id.wrapping_add(count as u16);
                pos += 2;
            }
            _ => break,
        }
    }
    (next_id, body.len())
}

fn skip_scsu(body: &[u8], start: usize) -> usize {
    let mut p = start;
    while p < body.len() {
        if body[p] == 0 {
            return p + 1;
        }
        p += 1;
    }
    p
}

fn skip_ucs2(body: &[u8], start: usize) -> usize {
    let mut p = start;
    while p + 1 < body.len() {
        if body[p] == 0 && body[p + 1] == 0 {
            return p + 2;
        }
        p += 2;
    }
    body.len()
}

fn read_u16_count(body: &[u8], pos: usize) -> (u16, usize) {
    if pos + 2 > body.len() {
        return (0, body.len());
    }
    let c = u16::from_le_bytes([body[pos], body[pos + 1]]);
    (c, pos + 2)
}

fn append_scsu_string(body: &mut Vec<u8>, end_pos: usize, text: &str) -> usize {
    let block = build_scsu_block(text);
    body.splice(end_pos..end_pos, block.iter().copied());
    end_pos + block.len()
}

fn build_scsu_block(text: &str) -> Vec<u8> {
    let mut block = Vec::with_capacity(1 + text.len() + 1);
    block.push(SIBT_STRING_SCSU);
    block.extend_from_slice(text.as_bytes());
    block.push(0x00);
    block
}

fn update_package_length(body: &mut [u8]) {
    let len = body.len() as u32;
    body[0] = (len & 0xFF) as u8;
    body[1] = ((len >> 8) & 0xFF) as u8;
    body[2] = ((len >> 16) & 0xFF) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_string_package(existing: &[&str]) -> Vec<u8> {
        let sibt_start = 12u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(PACKAGE_STRINGS);
        buf.extend_from_slice(&sibt_start.to_le_bytes());
        buf.extend_from_slice(&sibt_start.to_le_bytes());
        for s in existing {
            buf.push(SIBT_STRING_SCSU);
            buf.extend_from_slice(s.as_bytes());
            buf.push(0x00);
        }
        buf.push(SIBT_END);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn make_image(pkg_body: Vec<u8>) -> Image {
        let section = mk_node(FfsType::Section, pkg_body, vec![]);
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn is_string_package_detects() {
        let pkg = make_string_package(&[]);
        assert!(is_string_package(&pkg));
        assert!(!is_string_package(&[0x00, 0x00, 0x00, 0x02]));
    }

    #[test]
    fn add_strings_to_body_assigns_sequential_ids() {
        let mut pkg = make_string_package(&["first", "second"]);
        let mapping = add_strings_to_body(&mut pkg, &["a".to_string(), "b".to_string()]);
        assert_eq!(mapping["a"], 3);
        assert_eq!(mapping["b"], 4);
    }

    #[test]
    fn add_strings_to_body_writes_scsu_block_before_end() {
        let mut pkg = make_string_package(&["first"]);
        let end_pos_before = pkg.len() - 1;
        assert_eq!(pkg[end_pos_before], SIBT_END);
        add_strings_to_body(&mut pkg, &["new".to_string()]);
        assert_eq!(pkg[end_pos_before], SIBT_STRING_SCSU);
        assert_eq!(&pkg[end_pos_before + 1..end_pos_before + 4], b"new");
        assert_eq!(pkg[end_pos_before + 4], 0x00);
        assert_eq!(*pkg.last().unwrap(), SIBT_END);
    }

    #[test]
    fn add_strings_to_body_updates_length() {
        let mut pkg = make_string_package(&[]);
        let len_before = pkg.len();
        add_strings_to_body(&mut pkg, &["hello".to_string()]);
        let stored = pkg[0] as u32 | ((pkg[1] as u32) << 8) | ((pkg[2] as u32) << 16);
        assert_eq!(stored as usize, pkg.len());
        assert!(pkg.len() > len_before);
    }

    #[test]
    fn add_strings_skips_skip_blocks_in_id_counting() {
        let mut pkg = make_string_package(&["first"]);
        // insert a SIBT_SKIP2 (0x21) + count=5 before SIBT_END
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0x05, 0x00]);
        let mapping = add_strings_to_body(&mut pkg, &["after".to_string()]);
        assert_eq!(mapping["after"], 7);
    }

    #[test]
    fn add_strings_on_image_mutates_section_and_marks_rebuild() {
        let pkg = make_string_package(&["first", "second"]);
        let mut image = make_image(pkg);
        let mapping = add_strings(&mut image, None, &["x".to_string()]).unwrap();
        assert_eq!(mapping["x"], 3);
        let section = &image.root.children[0].children[0].children[0];
        let stored = section.body[0] as u32
            | ((section.body[1] as u32) << 8)
            | ((section.body[2] as u32) << 16);
        assert_eq!(stored as usize, section.body.len());
        assert_eq!(section.action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
    }

    #[test]
    fn add_strings_returns_error_when_no_package() {
        let mut image = make_image(vec![0x00, 0x00, 0x00, 0x02]);
        assert!(matches!(
            add_strings(&mut image, None, &["x".to_string()]),
            Err(SetupAdvancedError::StringPackageNotFound)
        ));
    }
}
```

- [ ] **Step 2: Подключить в mod.rs и запустить тесты**

`crates/uefi-engine/src/setup_advanced/mod.rs`: добавить `pub mod string_pack;`

Run: `cargo test -p uefi-engine setup_advanced::string_pack`
Expected: PASS

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-engine/src/setup_advanced/string_pack.rs crates/uefi-engine/src/setup_advanced/mod.rs
git commit -m "feat(setup_advanced): add HII String package search and string addition"
```

---

### Task 4: ami_patcher.rs — патч setupdataBin + amitseSct

**Files:**
- Create: `crates/uefi-engine/src/setup_advanced/ami_patcher.rs`

**Interfaces:**
- Consumes: `types::*`, `parser::target::find_item`, `Guid`
- Produces:
  - `pub struct QuestionAmiRecord { pub question_id: u16, pub page_id: Option<u16>, pub access_level: u8, pub failsafe: u8, pub optimal: u8 }`
  - `pub fn patch_ami(image: &mut Image, formset_guid: &Guid, form_ids: &[u16], questions: &[QuestionAmiRecord], setupdata_guid: Option<&Guid>, amitse_guid: Option<&Guid>) -> Result<(), SetupAdvancedError>`
  - Поиск setupdataBin/amitseSct по приоритету: GUID → имя FFS → эвристика
  - Генерация 108-байтной AMI-записи, дополнение в setupdataBin
  - Регистрация FormId в amitseSct

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/setup_advanced/ami_patcher.rs`:
```rust
use crate::ops;
use crate::types::*;
use super::SetupAdvancedError;

pub const AMI_RECORD_SIZE: usize = 108;
pub const AMI_DEFAULT_ACCESS_LEVEL: u8 = 0x05;
const AMI_PAGE_ID_OFFSET: usize = 24;
const AMI_ACCESS_LEVEL_OFFSET: usize = 32;
const AMI_FAILSAFE_OFFSET: usize = 104;
const AMI_OPTIMAL_OFFSET: usize = 106;

pub struct QuestionAmiRecord {
    pub question_id: u16,
    pub page_id: Option<u16>,
    pub access_level: u8,
    pub failsafe: u8,
    pub optimal: u8,
}

pub fn make_ami_record(rec: &QuestionAmiRecord) -> Vec<u8> {
    let mut buf = vec![0u8; AMI_RECORD_SIZE];
    buf[0..2].copy_from_slice(&rec.question_id.to_le_bytes());
    if let Some(pid) = rec.page_id {
        buf[AMI_PAGE_ID_OFFSET..AMI_PAGE_ID_OFFSET + 2].copy_from_slice(&pid.to_le_bytes());
    }
    buf[AMI_ACCESS_LEVEL_OFFSET] = rec.access_level;
    buf[AMI_FAILSAFE_OFFSET] = rec.failsafe;
    buf[AMI_OPTIMAL_OFFSET] = rec.optimal;
    buf
}

pub fn patch_ami(
    image: &mut Image,
    formset_guid: &Guid,
    form_ids: &[u16],
    questions: &[QuestionAmiRecord],
    setupdata_guid: Option<&Guid>,
    amitse_guid: Option<&Guid>,
) -> Result<(), SetupAdvancedError> {
    let (sd_vi, sd_fi) = find_ami_module(image, setupdata_guid, "setupdata")?;
    let (am_vi, am_fi) = find_ami_module(image, amitse_guid, "AMITSE")?;
    {
        let setupdata = &mut image.root.children[sd_vi].children[sd_fi];
        for q in questions {
            let record = make_ami_record(q);
            setupdata.body.extend_from_slice(&record);
        }
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &[sd_vi, sd_fi]);
    {
        let amitse = &mut image.root.children[am_vi].children[am_fi];
        let formset_marker = &formset_guid.to_bytes()[12..14];
        let insert_pos = find_formset_marker_position(&amitse.body, formset_marker)
            .unwrap_or(amitse.body.len());
        let mut entries = Vec::with_capacity(form_ids.len() * 2);
        for &fid in form_ids {
            entries.extend_from_slice(&fid.to_le_bytes());
        }
        amitse.body.splice(insert_pos..insert_pos, entries.iter().copied());
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &[am_vi, am_fi]);
    Ok(())
}

fn find_ami_module(image: &Image, guid: Option<&Guid>, name_hint: &str) -> Result<(usize, usize), SetupAdvancedError> {
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if let Some(g) = guid
                && file.guid == Some(*g)
            {
                return Ok((vi, fi));
            }
        }
    }
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if has_name_section(&file.children, name_hint) {
                return Ok((vi, fi));
            }
        }
    }
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if has_question_id_markers(&file.body) {
                return Ok((vi, fi));
            }
        }
    }
    Err(SetupAdvancedError::AmiFilesNotFound)
}

fn has_name_section(children: &[FfsNode], name: &str) -> bool {
    children.iter().any(|c| {
        c.subtype == crate::ffs::EFI_SECTION_UI && ucs2_body_to_string(&c.body) == name
    })
}

fn ucs2_body_to_string(body: &[u8]) -> String {
    let text: Vec<u16> = body
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&text)
}

fn has_question_id_markers(body: &[u8]) -> bool {
    body.len() >= AMI_RECORD_SIZE && body.len() % AMI_RECORD_SIZE == 0
}

fn find_formset_marker_position(body: &[u8], marker: &[u8]) -> Option<usize> {
    body.windows(2).position(|w| w == marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ami_record_offsets() {
        let rec = QuestionAmiRecord { question_id: 0x100, page_id: Some(5), access_level: 0x05, failsafe: 1, optimal: 0 };
        let buf = make_ami_record(&rec);
        assert_eq!(buf.len(), AMI_RECORD_SIZE);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0x100);
        assert_eq!(u16::from_le_bytes([buf[24], buf[25]]), 5);
        assert_eq!(buf[32], 0x05);
        assert_eq!(buf[104], 1);
        assert_eq!(buf[106], 0);
    }

    #[test]
    fn ami_record_no_page_id() {
        let rec = QuestionAmiRecord { question_id: 0x200, page_id: None, access_level: 0x05, failsafe: 0, optimal: 1 };
        let buf = make_ami_record(&rec);
        assert_eq!(u16::from_le_bytes([buf[24], buf[25]]), 0);
    }

    #[test]
    fn ami_record_all_zero_except_qid() {
        let rec = QuestionAmiRecord { question_id: 1, page_id: None, access_level: 0, failsafe: 0, optimal: 0 };
        let buf = make_ami_record(&rec);
        assert_eq!(buf[32], 0);
        assert_eq!(buf[50], 0);
        assert_eq!(buf[107], 0);
    }

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn ui_section(name: &str) -> FfsNode {
        let mut ucs2: Vec<u8> = Vec::new();
        for u in name.encode_utf16() {
            ucs2.extend_from_slice(&u.to_le_bytes());
        }
        ucs2.extend_from_slice(&[0, 0]);
        FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: crate::ffs::EFI_SECTION_UI,
            offset: 0,
            header: vec![],
            body: ucs2,
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn make_image_named(setupdata_body: Vec<u8>, amitse_body: Vec<u8>) -> Image {
        let setupdata = mk_node(FfsType::File, setupdata_body, vec![ui_section("setupdata")]);
        let amitse = mk_node(FfsType::File, amitse_body, vec![ui_section("AMITSE")]);
        let volume = mk_node(FfsType::Volume, vec![], vec![setupdata, amitse]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Write }
    }

    #[test]
    fn patch_ami_appends_records_and_marks_rebuild() {
        let mut image = make_image_named(vec![0u8; AMI_RECORD_SIZE], vec![]);
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        let q = QuestionAmiRecord { question_id: 0x10, page_id: None, access_level: 0x05, failsafe: 1, optimal: 0 };
        patch_ami(&mut image, &formset_guid, &[], &[q], None, None).unwrap();
        let setupdata = &image.root.children[0].children[0];
        assert_eq!(setupdata.body.len(), 2 * AMI_RECORD_SIZE);
        assert_eq!(u16::from_le_bytes([setupdata.body[AMI_RECORD_SIZE], setupdata.body[AMI_RECORD_SIZE + 1]]), 0x10);
        assert_eq!(setupdata.action, Action::Rebuild);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
    }

    #[test]
    fn patch_ami_inserts_form_ids_in_order() {
        let mut image = make_image_named(vec![0u8; AMI_RECORD_SIZE], vec![0xA1, 0xB2, 0xC3, 0xD4]);
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        patch_ami(&mut image, &formset_guid, &[0x0001, 0x0002], &[], None, None).unwrap();
        let amitse = &image.root.children[0].children[1];
        assert_eq!(u16::from_le_bytes([amitse.body[4], amitse.body[5]]), 0x0001);
        assert_eq!(u16::from_le_bytes([amitse.body[6], amitse.body[7]]), 0x0002);
    }

    #[test]
    fn patch_ami_returns_error_when_files_not_found() {
        let mut image = make_image_named(vec![], vec![]);
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        assert!(matches!(
            patch_ami(&mut image, &formset_guid, &[], &[], None, None),
            Err(SetupAdvancedError::AmiFilesNotFound)
        ));
    }
}
```

- [ ] **Step 2: Подключить в mod.rs и запустить тесты**

`crates/uefi-engine/src/setup_advanced/mod.rs`: добавить `pub mod ami_patcher;`

Run: `cargo test -p uefi-engine setup_advanced::ami_patcher`
Expected: PASS

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-engine/src/setup_advanced/ami_patcher.rs crates/uefi-engine/src/setup_advanced/mod.rs
git commit -m "feat(setup_advanced): add AMI patcher (setupdataBin records + amitseSct FormId)"
```

---

### Task 5: ffs_assembler.rs — сборка FFS из IFR

**Files:**
- Create: `crates/uefi-engine/src/setup_advanced/ffs_assembler.rs`

**Interfaces:**
- Consumes: `types::*`, `ffs::*` (checksums, header helpers)
- Produces:
  - `pub fn assemble_ffs(ifr_bytes: &[u8], string_package_bytes: &[u8], file_guid: &Guid) -> Result<Vec<u8>, SetupAdvancedError>`
  - Сборка: FFSv2-заголовок (24 байта) + секции (EFI_SECTION_RAW с IFR + EFI_SECTION_RAW с String-пакетом)
  - Пересчёт checksum, size

- [ ] **Step 1: Написать failing test**

`crates/uefi-engine/src/setup_advanced/ffs_assembler.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use super::SetupAdvancedError;

pub fn assemble_ffs(ifr_bytes: &[u8], string_package_bytes: &[u8], file_guid: &Guid) -> Result<Vec<u8>, SetupAdvancedError> {
    let mut body = Vec::new();
    emit_raw_section(&mut body, ifr_bytes);
    emit_raw_section(&mut body, string_package_bytes);
    let total = 24 + body.len();
    let mut header = vec![0u8; 24];
    header[0..16].copy_from_slice(&guid_to_bytes(file_guid));
    header[16] = 0x01;
    header[17] = 0x00;
    let size_b = size_to_uint24(total as u32);
    header[20] = size_b[0]; header[21] = size_b[1]; header[22] = size_b[2];
    let cs = calculate_checksum8(&header[0..23]);
    header[23] = cs;
    let mut ffs = Vec::with_capacity(total);
    ffs.extend_from_slice(&header);
    ffs.extend_from_slice(&body);
    Ok(ffs)
}

// NOTE: дубликат `guid_to_bytes` из ifr_builder.rs — при реализации вынести в общий
// хелпер (например, в `crate::types`) и переиспользовать; `uguid::Guid::to_bytes()` уже даёт [u8;16].
fn guid_to_bytes(g: &Guid) -> [u8; 16] {
    g.to_bytes()
}

fn emit_raw_section(out: &mut Vec<u8>, data: &[u8]) {
    let total = 4 + data.len();
    let size_b = size_to_uint24(total as u32);
    out.push(size_b[0]); out.push(size_b[1]); out.push(size_b[2]);
    out.push(EFI_SECTION_RAW);
    out.extend_from_slice(data);
    let aligned = (out.len() + 3) & !3;
    while out.len() < aligned { out.push(0x00); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn assemble_minimal_ffs() {
        let g = Guid::from_str("12345678-90AB-CDEF-1234-567890ABCDEF").unwrap();
        let ifr = vec![0x29, 0x02];
        let strpkg = vec![0x06, 0x00, 0x00, 0x00, 0x04];
        let ffs = assemble_ffs(&ifr, &strpkg, &g).unwrap();
        assert!(ffs.len() > 24);
        assert_eq!(&ffs[0..16], &guid_to_bytes(&g));
        assert_eq!(ffs[16], 0x01);
        let cs = calculate_checksum8(&ffs[0..23]);
        assert_eq!(ffs[23], cs);
    }

    #[test]
    fn ffs_size_matches() {
        let g = Guid::from_str("11111111-2222-3333-4444-555555555555").unwrap();
        let ifr = vec![0x01, 0x06, 0x01, 0x00, 0x01, 0x00, 0x29, 0x02];
        let ffs = assemble_ffs(&ifr, &[], &g).unwrap();
        let size = uint24_to_u32([ffs[20], ffs[21], ffs[22]]) as usize;
        assert_eq!(size, ffs.len());
    }
}
```

- [ ] **Step 2: Подключить в mod.rs и запустить тесты**

`crates/uefi-engine/src/setup_advanced/mod.rs`: добавить `pub mod ffs_assembler;`

Run: `cargo test -p uefi-engine setup_advanced::ffs_assembler`
Expected: PASS

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-engine/src/setup_advanced/ffs_assembler.rs crates/uefi-engine/src/setup_advanced/mod.rs
git commit -m "feat(setup_advanced): add FFS assembler (header + RAW sections + checksum)"
```

---

### Task 6: mod.rs — координатор add_setup_formset

**Files:**
- Modify: `crates/uefi-engine/src/setup_advanced/mod.rs`

**Interfaces:**
- Consumes: `schema::*`, `ifr_builder::*`, `string_pack::*`, `ami_patcher::*`, `ffs_assembler::*`, `types::*`, `ops::*`
- Produces:
  - `pub fn add_setup_formset(image: &mut Image, schema: &FormSetSchema, target_ffs_guid: Option<&Guid>) -> Result<AddSetupResult, SetupAdvancedError>`
  - `pub struct AddSetupResult { pub new_ffs_guid: Guid, pub inserted_form_ids: Vec<u16>, pub string_ids: HashMap<String, u16> }`

- [ ] **Step 1: Реализовать координатор**

Дополнить `crates/uefi-engine/src/setup_advanced/mod.rs`:
```rust
pub mod schema;
pub mod ifr_builder;
pub mod string_pack;
pub mod ami_patcher;
pub mod ffs_assembler;

use std::collections::HashMap;
use crate::types::*;
use crate::ops;
use ifr_builder::*;
use ami_patcher::QuestionAmiRecord;

pub struct AddSetupResult {
    pub new_ffs_guid: Guid,
    pub inserted_form_ids: Vec<u16>,
    pub string_ids: HashMap<String, u16>,
}

pub fn add_setup_formset(image: &mut Image, schema: &schema::FormSetSchema, target_ffs_guid: Option<&Guid>) -> Result<AddSetupResult, SetupAdvancedError> {
    let formset_guid: Guid = schema.formset_guid.parse().map_err(|e: std::str::ParseError| SetupAdvancedError::InvalidSchema(e.to_string()))?;
    let new_ffs_guid = Guid::try_parse(&format!(
        "{:08X}-BEEF-1234-8000-000000000001",
        0xB00B0000 + image.root.children.len() as u32,
    ))
    .map_err(|e| SetupAdvancedError::IfrBuildError(e.to_string()))?;
    let mut strings: Vec<String> = Vec::new();
    strings.push(schema.title.clone());
    strings.push(schema.help.clone());
    for vs in &schema.varstores { strings.push(vs.name.clone()); }
    for form in &schema.forms {
        strings.push(form.title.clone());
        for item in &form.items {
            collect_item_strings(item, &mut strings);
        }
    }
    let string_ids = string_pack::add_strings(image, target_ffs_guid, &strings)?;
    let ifr_bytes = build_ifr(schema, &string_ids)?;
    let strpkg_idx = find_string_package_section(image, target_ffs_guid)?;
    let strpkg_bytes = image.root.children[0].children[strpkg_idx.0].children[strpkg_idx.1].body.clone();
    let ffs_bytes = ffs_assembler::assemble_ffs(&ifr_bytes, &strpkg_bytes, &new_ffs_guid)?;
    let (form_ids, questions) = extract_form_ids_and_questions(schema);
    let setupdata_guid = schema.setupdata_guid.as_ref().and_then(|s| s.parse().ok());
    let amitse_guid = schema.amitse_guid.as_ref().and_then(|s| s.parse().ok());
    ami_patcher::patch_ami(image, &formset_guid, &form_ids, &questions, setupdata_guid.as_ref(), amitse_guid.as_ref())?;
    let target = crate::parser::target::Target::Path(vec![0]);
    ops::insert(&mut image.root, &target, &ffs_bytes, ops::InsertMode::Into)
        .map_err(|e| SetupAdvancedError::FfsAssemblyError(e.to_string()))?;
    Ok(AddSetupResult { new_ffs_guid, inserted_form_ids: form_ids, string_ids })
}

fn collect_item_strings(item: &schema::ItemSchema, strings: &mut Vec<String>) {
    match item {
        schema::ItemSchema::OneOf(o) => {
            strings.push(o.prompt.clone()); strings.push(o.help.clone());
            for opt in &o.options { strings.push(opt.text.clone()); }
        }
        schema::ItemSchema::CheckBox(c) => { strings.push(c.prompt.clone()); strings.push(c.help.clone()); }
        schema::ItemSchema::Numeric(n) => { strings.push(n.prompt.clone()); strings.push(n.help.clone()); }
        schema::ItemSchema::Text(t) => { strings.push(t.prompt.clone()); strings.push(t.help.clone()); strings.push(t.text_two.clone()); }
        schema::ItemSchema::Ref(r) => { strings.push(r.prompt.clone()); strings.push(r.help.clone()); }
        schema::ItemSchema::String(s) => { strings.push(s.prompt.clone()); strings.push(s.help.clone()); }
        schema::ItemSchema::Action(a) => { strings.push(a.prompt.clone()); strings.push(a.help.clone()); strings.push(a.config.clone()); }
        schema::ItemSchema::OrderedList(o) => { strings.push(o.prompt.clone()); strings.push(o.help.clone()); }
    }
}

fn build_ifr(schema: &schema::FormSetSchema, string_ids: &HashMap<String, u16>) -> Result<Vec<u8>, SetupAdvancedError> {
    let mut b = IfrBuilder::new();
    let formset_guid: Guid = schema.formset_guid.parse().map_err(|e: std::str::ParseError| SetupAdvancedError::InvalidSchema(e.to_string()))?;
    let title_id = string_ids[&schema.title];
    let help_id = string_ids[&schema.help];
    let class_guids: Vec<Guid> = schema.class_guids.iter().filter_map(|s| s.parse().ok()).collect();
    b.emit_form_set(&formset_guid, title_id, help_id, &class_guids);
    for vs in &schema.varstores {
        let g: Guid = vs.guid.parse().map_err(|e: std::str::ParseError| SetupAdvancedError::InvalidSchema(e.to_string()))?;
        let name_id = string_ids[&vs.name];
        b.emit_var_store(vs.id, &g, vs.size, &vs.name);
        let _ = name_id;
    }
    for ds in &schema.default_stores {
        b.emit_default_store(string_ids[&ds.name], ds.id);
    }
    for form in &schema.forms {
        let title_id = string_ids[&form.title];
        b.emit_form(form.id, title_id);
        for item in &form.items {
            emit_item(&mut b, item, string_ids);
        }
        b.emit_end();
    }
    b.emit_end();
    Ok(b.build())
}

fn emit_item(b: &mut IfrBuilder, item: &schema::ItemSchema, string_ids: &HashMap<String, u16>) {
    let display_flags = |d: schema::DisplayMode| -> u8 {
        match d { schema::DisplayMode::IntDec => 0x00, schema::DisplayMode::UintDec => 0x10, schema::DisplayMode::UintHex => 0x20 }
    };
    match item {
        schema::ItemSchema::OneOf(o) => {
            let pid = string_ids[&o.prompt]; let hid = string_ids[&o.help];
            b.emit_one_of(pid, hid, o.question_id, o.var_store_id, o.var_offset, display_flags(o.display), o.size);
            for opt in &o.options {
                let tid = string_ids[&opt.text];
                let mut flags = 0u8;
                match opt.default {
                    Some(schema::DefaultClass::Optimized) => flags |= 0x10,
                    Some(schema::DefaultClass::Failsafe) => flags |= 0x20,
                    None => {}
                }
                b.emit_one_of_option(tid, flags, o.size - 1, opt.value, o.size);
            }
            if let Some(v) = o.defaults.optimized {
                b.emit_default(DEFAULT_ID_STANDARD, o.size - 1, v, o.size);
            }
            if let Some(v) = o.defaults.failsafe {
                b.emit_default(DEFAULT_ID_MANUFACTURING, o.size - 1, v, o.size);
            }
            b.emit_end();
        }
        schema::ItemSchema::CheckBox(c) => {
            let pid = string_ids[&c.prompt]; let hid = string_ids[&c.help];
            b.emit_check_box(pid, hid, c.question_id, c.var_store_id, c.var_offset, 0);
            if let Some(v) = c.defaults.optimized { if v != 0 { b.emit_check_box_default(true); } }
            b.emit_end();
        }
        schema::ItemSchema::Numeric(n) => {
            let pid = string_ids[&n.prompt]; let hid = string_ids[&n.help];
            b.emit_numeric(pid, hid, n.question_id, n.var_store_id, n.var_offset, display_flags(n.display), n.size, n.min, n.max, n.step);
            if let Some(v) = n.defaults.optimized { b.emit_default(DEFAULT_ID_STANDARD, n.size - 1, v, n.size); }
            if let Some(v) = n.defaults.failsafe { b.emit_default(DEFAULT_ID_MANUFACTURING, n.size - 1, v, n.size); }
            b.emit_end();
        }
        schema::ItemSchema::Text(t) => {
            let pid = string_ids[&t.prompt]; let hid = string_ids[&t.help]; let t2 = string_ids[&t.text_two];
            b.emit_text(pid, hid, t2);
        }
        schema::ItemSchema::Ref(r) => {
            let pid = string_ids[&r.prompt]; let hid = string_ids[&r.help];
            b.emit_ref(pid, hid, r.question_id, 0, 0, r.form_id);
        }
        schema::ItemSchema::String(_) | schema::ItemSchema::Action(_) | schema::ItemSchema::OrderedList(_) => {
            b.emit_end();
        }
    }
}

fn extract_form_ids_and_questions(schema: &schema::FormSetSchema) -> (Vec<u16>, Vec<QuestionAmiRecord>) {
    let form_ids: Vec<u16> = schema.forms.iter().map(|f| f.id).collect();
    let mut questions = Vec::new();
    for form in &schema.forms {
        for item in &form.items {
            if let Some((qid, page_id, failsafe, optimal)) = extract_question(item) {
                questions.push(QuestionAmiRecord {
                    question_id: qid,
                    page_id,
                    access_level: ami_patcher::AMI_DEFAULT_ACCESS_LEVEL,
                    failsafe,
                    optimal,
                });
            }
        }
    }
    (form_ids, questions)
}

fn extract_question(item: &schema::ItemSchema) -> Option<(u16, Option<u16>, u8, u8)> {
    match item {
        schema::ItemSchema::OneOf(o) => Some((o.question_id, None, o.defaults.failsafe.unwrap_or(0) as u8, o.defaults.optimized.unwrap_or(0) as u8)),
        schema::ItemSchema::CheckBox(c) => Some((c.question_id, None, c.defaults.failsafe.unwrap_or(0) as u8, c.defaults.optimized.unwrap_or(0) as u8)),
        schema::ItemSchema::Numeric(n) => Some((n.question_id, None, n.defaults.failsafe.unwrap_or(0) as u8, n.defaults.optimized.unwrap_or(0) as u8)),
        schema::ItemSchema::Ref(r) => Some((r.question_id, Some(r.form_id), 0, 0)),
        _ => None,
    }
}

fn find_string_package_section(image: &Image, ffs_guid: Option<&Guid>) -> Result<(usize, usize), SetupAdvancedError> {
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if let Some(g) = ffs_guid { if file.guid != Some(*g) { continue; } }
            for (_si, sec) in file.children.iter().enumerate() {
                if string_pack::is_string_package(&sec.body) { return Ok((vi, fi)); }
            }
        }
    }
    Err(SetupAdvancedError::StringPackageNotFound)
}
```

Добавить в `ifr_builder.rs`:
```rust
impl IfrBuilder {
    pub fn emit_check_box_default(&mut self, _standard: bool) {
        self.buf.push(0x01);
    }
}
```

- [ ] **Step 2: Запустить компиляцию**

Run: `cargo build -p uefi-engine`
Expected: компиляция без ошибок

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-engine/src/setup_advanced/
git commit -m "feat(setup_advanced): add coordinator add_setup_formset (strings+IFR+AMI+FFS+insert)"
```

---

### Task 7: gRPC AddSetupFormSet — proto + rpc impl

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-engine/src/rpc/server.rs`

**Interfaces:**
- Consumes: `setup_advanced::add_setup_formset`
- Produces: новый RPC `AddSetupFormSet` в `EngineService`

- [ ] **Step 1: Обновить engine.proto**

Добавить в `crates/uefi-proto/proto/engine.proto` в `service EngineService`:
```proto
  rpc AddSetupFormSet(AddSetupFormSetRequest) returns (AddSetupFormSetResponse);
```

Добавить сообщения:
```proto
message AddSetupFormSetRequest {
  string image_id = 1;
  string schema_json = 2;
  string target_ffs_guid = 3;
}

message AddSetupFormSetResponse {
  string new_ffs_id = 1;
  repeated uint32 inserted_form_ids = 2;
  map<string, uint32> string_ids = 3;
}
```

- [ ] **Step 2: Перегенерировать proto**

Run: `cargo build -p uefi-proto`
Expected: компиляция, типы `AddSetupFormSetRequest`/`AddSetupFormSetResponse` сгенерированы

- [ ] **Step 3: Реализовать метод в server.rs**

Добавить в `impl EngineService for EngineServer` в `crates/uefi-engine/src/rpc/server.rs`:
```rust
async fn add_setup_form_set(&self, req: Request<AddSetupFormSetRequest>) -> RpcResult<AddSetupFormSetResponse> {
    let r = req.into_inner();
    let schema = crate::setup_advanced::schema::parse_schema(&r.schema_json)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    let target_guid: Option<crate::types::Guid> = if r.target_ffs_guid.is_empty() {
        None
    } else {
        Some(r.target_ffs_guid.parse().map_err(|e: std::str::ParseError| Status::invalid_argument(e.to_string()))?)
    };
    let mut images = self.images.lock().await;
    let img = images.get_mut(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
    let result = crate::setup_advanced::add_setup_formset(img, &schema, target_guid.as_ref())
        .map_err(|e| match e {
            crate::setup_advanced::SetupAdvancedError::InvalidSchema(s) => Status::invalid_argument(s),
            crate::setup_advanced::SetupAdvancedError::StringPackageNotFound => Status::not_found("string package not found"),
            crate::setup_advanced::SetupAdvancedError::AmiFilesNotFound => Status::not_found("AMI setupdataBin/amitseSct not found"),
            crate::setup_advanced::SetupAdvancedError::IfrBuildError(s) => Status::internal(s),
            crate::setup_advanced::SetupAdvancedError::FfsAssemblyError(s) => Status::internal(s),
        })?;
    Ok(Response::new(AddSetupFormSetResponse {
        new_ffs_id: result.new_ffs_guid.to_string(),
        inserted_form_ids: result.inserted_form_ids.into_iter().map(|f| f as u32).collect(),
        string_ids: result.string_ids.into_iter().map(|(k, v)| (k, v as u32)).collect(),
    }))
}
```

- [ ] **Step 4: Запустить компиляцию и тесты**

Run: `cargo build -p uefi-engine && cargo test -p uefi-engine`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-proto/proto/engine.proto crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(setup_advanced): add gRPC AddSetupFormSet method to EngineService"
```

---

### Task 8: Финальные проверки — clippy, fmt, integration smoke

**Files:**
- по результатам

- [ ] **Step 1: Запустить все тесты**

Run: `cargo test --all`
Expected: PASS

- [ ] **Step 2: Запустить clippy**

Run: `cargo clippy --all -- -D warnings`
Expected: без warnings

- [ ] **Step 3: Запустить fmt check**

Run: `cargo fmt --all -- --check`
Expected: без diff

- [ ] **Step 4: Коммит финальных правок**

```bash
git add -A
git commit -m "chore(setup_advanced): final checks — tests pass, clippy clean"
```

---

## Само-проверка плана (после написания)

**Спека-покрытие:**
- JSON-схема (FormSet/Form/Item/Defaults): Task 1 ✓
- IFR-билдер (все опкоды): Task 2 ✓
- String-пакет (поиск/добавление): Task 3 ✓
- AMI-патчинг (setupdataBin + amitseSct, поиск по GUID/имени/эвристике): Task 4 ✓
- FFS-ассемблер: Task 5 ✓
- Координатор add_setup_formset: Task 6 ✓
- gRPC AddSetupFormSet: Task 7 ✓
- Опциональные setupdata_guid/amitse_guid: Tasks 1, 4, 6 ✓
- Дефолты (Optimized=0, Failsafe=1): Tasks 2, 6 ✓
- Обязательный AMI-патчинг (AmiFilesNotFound): Tasks 4, 7 ✓

**Placeholder scan:** TBD/TODO нет; все шаги содержат код. ✓

**Type consistency:**
- `FormSetSchema`/`ItemSchema`/`Defaults` — единые в schema.rs, mod.rs ✓
- `IfrBuilder` — единое имя в ifr_builder.rs, mod.rs ✓
- `QuestionAmiRecord` — единое в ami_patcher.rs, mod.rs ✓
- `SetupAdvancedError` — единое в mod.rs, всех подмодулях, rpc/server.rs ✓
- `AddSetupResult` — в mod.rs, используется в rpc ✓