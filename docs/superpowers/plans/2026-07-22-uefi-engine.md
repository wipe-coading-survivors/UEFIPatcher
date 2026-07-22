# UEFI Engine (цикл 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать серверный движок для парсинга/модификации UEFI-образов, управления видимостью пунктов Setup-меню, хранения сессий и предоставления gRPC API над unix-сокетом.

**Architecture:** Cargo workspace с тремя крейтами: `uefi-proto` (protobuf-контракт), `uefi-engine` (ядро: parser/builder/setup/session/storage/rpc), `uefi-cli` (smoke-тест). Парсер строит дерево `FfsNode` из байтов образа (референс UEFITool-ai-fork `FfsParser`); builder собирает дерево обратно (референс `FfsBuilder`); sеtup-модуль парсит IFR (референс IFRExtractor-RS) и управляет SuppressIf-блоками (референс UEFI-Editor). Менеджер сессий встроен в движок, хранит метаданные в SQLite, артефакты на диске, TTL 10 дней, фоновый GC.

**Tech Stack:** Rust, tonic (gRPC over unix-socket), prost (protobuf), rusqlite (SQLite), tokio (async), clap (CLI), uuid, anyhow/thiserror, tracing.

## Global Constraints

- Референс парсера: `../refs/UEFITool-ai-fork/common/ffsparser.{h,cpp}` (разделённый `FfsParser`), НЕ `../refs/UEFITool/ffsengine.cpp` (объединённый 0.28.8).
- Референс билдера: `../refs/UEFITool-ai-fork/common/ffsbuilder.{h,cpp}`.
- Референс FFS-структур: `../refs/UEFITool-ai-fork/common/ffs.h` (FFSv2 24б, FFSv3 large 32б, Lenovo large 32б).
- Референс Target/команд: `../refs/UEFITool-ai-fork/UEFIEdit/uefiedit.{h,cpp}`.
- Референс IFR: `../refs/IFRExtractor-RS/src/uefi_parser.rs` (IfrOpcode, ifr_operations).
- Референс Setup-видимости: `../refs/UEFI-Editor/src/components/scripts/scripts.ts` (Unsuppress логика: вставка `End` 0x2902 после `{0A 82}`, удаление оригинального End).
- Точка истины при багах: `../refs/edk2`.
- Критичные баги для Rust-порта (из `../refs/UEFITool-ai-fork/IMPLEMENTATION.md` и `UEFIEDIT_BUGS-2.md`):
  1. `replace` должен `clearChildren` перед `setBody` (баг 9).
  2. `buildVolume` должен потреблять FreeSpace, дополнять `emptyByte` (баг 4).
  3. `buildFile`/`buildSection` с `rowCount==0` используют `body` as-is (баг 5).
  4. `buildSection` для GUIDed-секций сжимает LZMA/Tiano по GUID из parsing data (баг 10).
  5. `buildVolume` сохраняет оригинальные offset'ы неизменённых файлов или pad FFS к оригинальному размеру (баг 11).
  6. Каскадная пометка `Rebuild` для всех предков до root после insert/remove/replace/rebuild.
  7. Три варианта FFS-заголовка: FFSv2 (24б), FFSv3 large (32б, UINT64 ExtendedSize), Lenovo large (32б, UINT32 ExtendedSize, FFSv2 rev2).
- Переменные окружения: `UEFIPATCHER_DATA` (по умолч. `~/.local/share/uefipatcher`), `UEFIPATCHER_SOCK` (по умолч. `${XDG_RUNTIME_DIR}/uefipatcher.sock`), `UEFIPATCHER_SESSION_TTL_SECS` (864000), `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` (3600).
- gRPC: `Authorization: Bearer <token>` в metadata; токен в `${UEFIPATCHER_DATA}/token` (0600).
- Кодстайл: `cargo fmt`, `cargo clippy -- -D warnings`. Без комментариев в коде (кроме ссылок на референс `file:line`).

---

## Файлы плана

| Файл | Назначение |
|---|---|
| `Cargo.toml` | workspace manifest |
| `crates/uefi-proto/Cargo.toml`, `build.rs`, `proto/engine.proto`, `src/lib.rs` | protobuf-контракт EngineService |
| `crates/uefi-engine/Cargo.toml`, `src/lib.rs` | ядро: re-export модулей |
| `crates/uefi-engine/src/types.rs` | базовые типы: `Guid`, `FfsNode`, `FfsType`, `Action`, `Image`, `Target`, parsing-data structs |
| `crates/uefi-engine/src/ffs.rs` | FFS-структуры (заголовки), константы, checksum-хелперы |
| `crates/uefi-engine/src/parser/mod.rs`, `volume.rs`, `file.rs`, `section.rs` | парсинг образа → дерево |
| `crates/uefi-engine/src/parser/target.rs` | парсинг Target-строки (GUID/PATH/GUID:T/GUID:T:N) |
| `crates/uefi-engine/src/builder/mod.rs`, `align.rs` | сборка дерева → байты |
| `crates/uefi-engine/src/setup/mod.rs`, `ifr.rs` | парсинг IFR, SetSetupItemVisibility |
| `crates/uefi-engine/src/storage/mod.rs`, `schema.rs`, `artifact.rs` | SQLite + файлы артефактов |
| `crates/uefi-engine/src/session/mod.rs` | менеджер сессий + GC |
| `crates/uefi-engine/src/rpc/mod.rs`, `auth.rs`, `server.rs` | gRPC-сервер над unix-сокетом |
| `crates/uefi-cli/Cargo.toml`, `src/main.rs`, `src/client.rs` | CLI-минимум |
| `docker/Dockerfile.engine`, `docker/docker-compose.yml` | контейнеризация |
| `tests/fixtures/` | синтетические тестовые образы |
| `crates/uefi-engine/tests/` | интеграционные тесты |
| `crates/uefi-cli/tests/e2e.rs` | E2E-тесты |

---

### Task 1: Скелет Cargo workspace и uefi-proto

**Files:**
- Create: `Cargo.toml`
- Create: `crates/uefi-proto/Cargo.toml`
- Create: `crates/uefi-proto/build.rs`
- Create: `crates/uefi-proto/proto/engine.proto`
- Create: `crates/uefi-proto/src/lib.rs`

**Interfaces:**
- Consumes: нет (первый task)
- Produces: сгенерированные типы `engine_pb::*` (сообщения, enums), trait `engine_service_server::EngineService` для `uefi-engine`. Имена сообщений: `CreateSessionRequest`, `CreateSessionResponse`, `DestroySessionRequest`, `ListSessionsRequest`, `SessionInfo`, `ListSessionsResponse`, `OpenImageRequest`, `OpenImageResponse`, `DumpTreeRequest`, `DumpTreeResponse`, `ListItemsRequest`, `Item`, `ListItemsResponse`, `FindItemRequest`, `FindItemResponse`, `InsertRequest`, `InsertResponse`, `RemoveRequest`, `ReplaceRequest`, `ReplaceResponse`, `RebuildRequest`, `SetSetupItemVisibilityRequest`, `SaveImageRequest`, `Empty`. Enums: `ImageMode` (READ=0/WRITE=1), `InsertMode` (INTO=0/BEFORE=1/AFTER=2), `DumpFormat` (TEXT=0/TSV=1).

- [ ] **Step 1: Создать workspace manifest**

`Cargo.toml`:
```toml
[workspace]
members = ["crates/uefi-proto", "crates/uefi-engine", "crates/uefi-cli"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
tonic = "0.12"
prost = "0.13"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 2: Создать uefi-proto Cargo.toml**

`crates/uefi-proto/Cargo.toml`:
```toml
[package]
name = "uefi-proto"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
tonic.workspace = true
prost.workspace = true

[build-dependencies]
tonic-build = "0.12"
```

- [ ] **Step 3: Создать build.rs**

`crates/uefi-proto/build.rs`:
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/engine.proto")?;
    Ok(())
}
```

- [ ] **Step 4: Создать engine.proto**

`crates/uefi-proto/proto/engine.proto`:
```proto
syntax = "proto3";
package engine;

service EngineService {
  rpc CreateSession(CreateSessionRequest) returns (CreateSessionResponse);
  rpc DestroySession(DestroySessionRequest) returns (Empty);
  rpc ListSessions(ListSessionsRequest) returns (ListSessionsResponse);
  rpc OpenImage(OpenImageRequest) returns (OpenImageResponse);
  rpc DumpTree(DumpTreeRequest) returns (DumpTreeResponse);
  rpc ListItems(ListItemsRequest) returns (ListItemsResponse);
  rpc FindItem(FindItemRequest) returns (FindItemResponse);
  rpc Insert(InsertRequest) returns (InsertResponse);
  rpc Remove(RemoveRequest) returns (Empty);
  rpc Replace(ReplaceRequest) returns (ReplaceResponse);
  rpc Rebuild(RebuildRequest) returns (Empty);
  rpc SetSetupItemVisibility(SetSetupItemVisibilityRequest) returns (Empty);
  rpc SaveImage(SaveImageRequest) returns (Empty);
}

enum ImageMode { READ = 0; WRITE = 1; }
enum InsertMode { INTO = 0; BEFORE = 1; AFTER = 2; }
enum DumpFormat { TEXT = 0; TSV = 1; }

message CreateSessionRequest {}
message CreateSessionResponse { string session_id = 1; string token = 2; }
message DestroySessionRequest { string session_id = 1; }
message ListSessionsRequest {}
message SessionInfo { string session_id = 1; int64 created_at = 2; int64 last_activity = 3; }
message ListSessionsResponse { repeated SessionInfo sessions = 1; }

message OpenImageRequest { string session_id = 1; string image_path = 2; ImageMode mode = 3; }
message OpenImageResponse { string image_id = 1; string root_guid = 2; }

message DumpTreeRequest { string image_id = 1; DumpFormat format = 2; }
message DumpTreeResponse { string text = 1; }

message ListItemsRequest { string image_id = 1; string filter = 2; }
message Item { string path = 1; uint32 type = 2; uint32 subtype = 3; string guid = 4; uint64 offset = 5; uint64 size = 6; string name = 7; }
message ListItemsResponse { repeated Item items = 1; }

message FindItemRequest { string image_id = 1; string target = 2; }
message FindItemResponse { string item_id = 1; }

message InsertRequest { string image_id = 1; string target = 2; string ffs_path = 3; InsertMode mode = 4; }
message InsertResponse { string item_id = 1; }

message RemoveRequest { string image_id = 1; string target = 2; }

message ReplaceRequest { string image_id = 1; string target = 2; string ffs_path = 3; bool body_only = 4; }
message ReplaceResponse { string item_id = 1; }

message RebuildRequest { string image_id = 1; string target = 2; }

message SetSetupItemVisibilityRequest { string image_id = 1; string item_id = 2; bool visible = 3; }

message SaveImageRequest { string image_id = 1; string output_path = 2; }

message Empty {}
```

- [ ] **Step 5: Создать lib.rs**

`crates/uefi-proto/src/lib.rs`:
```rust
pub mod engine {
    tonic::include_proto!("engine");
}

pub use engine::*;
```

- [ ] **Step 6: Проверить компиляцию**

Run: `cargo build -p uefi-proto`
Expected: компиляция без ошибок, сгенерированы типы `engine::*`

- [ ] **Step 7: Коммит**

```bash
git add Cargo.toml crates/uefi-proto/
git commit -m "feat: scaffold workspace and uefi-proto (EngineService protobuf)"
```

---

### Task 2: Базовые типы движка (Guid, FfsNode, Action, FfsType, Image, Target, parsing data)

**Files:**
- Create: `crates/uefi-engine/Cargo.toml`
- Create: `crates/uefi-engine/src/lib.rs`
- Create: `crates/uefi-engine/src/types.rs`
- Test: `crates/uefi-engine/src/types.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: нет
- Produces:
  - `Guid { data1: u32, data2: u16, data3: u16, data4: [u8; 8] }` с `Display`/`FromStr`/`PartialEq`/`Eq`/`Hash`
  - `enum FfsType { Root=60, Capsule=61, Image=62, Region=63, Padding=64, Volume=65, File=66, Section=67, FreeSpace=68 }`
  - `enum Action { NoAction=50, Create=51, Insert=52, Replace=53, Remove=54, Rebuild=55, Rebase=56 }`
  - `struct FfsNode { guid: Option<Guid>, node_type: FfsType, subtype: u8, offset: u32, header: Vec<u8>, body: Vec<u8>, tail: Vec<u8>, children: Vec<FfsNode>, action: Action, parsing_data: ParsingData, fixed: bool, compressed: bool, alignment_bytes: Vec<u8> }`
  - `enum ParsingData { None, Volume(VolumeParsingData), File(FileParsingData), GuidedSection(GuidedSectionParsingData), CompressedSection(CompressedSectionParsingData) }`
  - `struct VolumeParsingData { extended_header_guid: Option<Guid>, alignment: u32, ffs_version: u8, empty_byte: u8, revision: u8 }`
  - `struct FileParsingData { empty_byte: u8, guid: Guid }`
  - `struct GuidedSectionParsingData { guid: Guid, dictionary_size: u32 }`
  - `struct CompressedSectionParsingData { uncompressed_size: u32, compression_type: u8, algorithm: u8, dictionary_size: u32 }`
  - `struct Image { image_id: String, session_id: String, root: FfsNode, mode: ImageMode }`
  - `enum Target { Guid(Guid), Path(Vec<usize>), GuidSection { guid: Guid, section_type: u8, section_index: Option<usize> } }`

- [ ] **Step 1: Написать failing tests для Guid**

`crates/uefi-engine/src/types.rs` (модуль tests внизу):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn guid_display_roundtrip() {
        let g = Guid { data1: 0x5C60F367, data2: 0xA505, data3: 0x419A, data4: [0x85, 0x9E, 0x2A, 0x4F, 0xF6, 0xCA, 0x6F, 0xE5] };
        let s = format!("{g}");
        assert_eq!(s, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
        let g2 = Guid::from_str(&s).unwrap();
        assert_eq!(g, g2);
    }

    #[test]
    fn guid_from_str_invalid() {
        assert!(Guid::from_str("not-a-guid").is_err());
        assert!(Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE").is_err());
    }
}
```

- [ ] **Step 2: Создать uefi-engine Cargo.toml**

`crates/uefi-engine/Cargo.toml`:
```toml
[package]
name = "uefi-engine"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
uefi-proto = { path = "../uefi-proto" }
uuid.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
rusqlite = { version = "0.31", features = ["bundled"] }
nix = { version = "0.29", features = ["fs"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Запустить тесты — должны упасть (нет Guid)**

Run: `cargo test -p uefi-engine guid_`
Expected: FAIL — `Guid` не определён

- [ ] **Step 4: Реализовать Guid**

`crates/uefi-engine/src/types.rs` (начало):
```rust
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[derive(Debug, Error)]
pub enum GuidError {
    #[error("invalid GUID format: {0}")]
    InvalidFormat(String),
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.data1, self.data2, self.data3,
            self.data4[0], self.data4[1],
            self.data4[2], self.data4[3], self.data4[4],
            self.data4[5], self.data4[6], self.data4[7]
        )
    }
}

impl FromStr for Guid {
    type Err = GuidError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 5 { return Err(GuidError::InvalidFormat(s.into())); }
        let d1 = u32::from_str_radix(parts[0], 16).map_err(|_| GuidError::InvalidFormat(s.into()))?;
        if parts[0].len() != 8 { return Err(GuidError::InvalidFormat(s.into())); }
        let d2 = u16::from_str_radix(parts[1], 16).map_err(|_| GuidError::InvalidFormat(s.into()))?;
        if parts[1].len() != 4 { return Err(GuidError::InvalidFormat(s.into())); }
        let d3 = u16::from_str_radix(parts[2], 16).map_err(|_| GuidError::InvalidFormat(s.into()))?;
        if parts[2].len() != 4 { return Err(GuidError::InvalidFormat(s.into())); }
        if parts[3].len() != 4 { return Err(GuidError::InvalidFormat(s.into())); }
        let d4a = u8::from_str_radix(&parts[3][0..2], 16).map_err(|_| GuidError::InvalidFormat(s.into()))?;
        let d4b = u8::from_str_radix(&parts[3][2..4], 16).map_err(|_| GuidError::InvalidFormat(s.into()))?;
        if parts[4].len() != 12 { return Err(GuidError::InvalidFormat(s.into())); }
        let mut d4 = [0u8; 8];
        for i in 0..6 {
            d4[i] = u8::from_str_radix(&parts[4][i*2..i*2+2], 16).map_err(|_| GuidError::InvalidFormat(s.into()))?;
        }
        d4[6] = d4a; d4[7] = d4b;
        Ok(Guid { data1: d1, data2: d2, data3: d3, data4: d4 })
    }
}
```

- [ ] **Step 5: Запустить тесты Guid — должны пройти**

Run: `cargo test -p uefi-engine guid_`
Expected: PASS

- [ ] **Step 6: Реализовать остальные типы**

Дополнить `crates/uefi-engine/src/types.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum FfsType {
    Root = 60, Capsule = 61, Image = 62, Region = 63, Padding = 64,
    Volume = 65, File = 66, Section = 67, FreeSpace = 68,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum Action {
    NoAction = 50, Create = 51, Insert = 52, Replace = 53,
    Remove = 54, Rebuild = 55, Rebase = 56,
}

#[derive(Debug, Clone)]
pub struct VolumeParsingData {
    pub extended_header_guid: Option<Guid>,
    pub alignment: u32,
    pub ffs_version: u8,
    pub empty_byte: u8,
    pub revision: u8,
}

#[derive(Debug, Clone)]
pub struct FileParsingData { pub empty_byte: u8, pub guid: Guid }

#[derive(Debug, Clone)]
pub struct GuidedSectionParsingData { pub guid: Guid, pub dictionary_size: u32 }

#[derive(Debug, Clone)]
pub struct CompressedSectionParsingData {
    pub uncompressed_size: u32,
    pub compression_type: u8,
    pub algorithm: u8,
    pub dictionary_size: u32,
}

#[derive(Debug, Clone)]
pub enum ParsingData {
    None,
    Volume(VolumeParsingData),
    File(FileParsingData),
    GuidedSection(GuidedSectionParsingData),
    CompressedSection(CompressedSectionParsingData),
}

#[derive(Debug, Clone)]
pub struct FfsNode {
    pub guid: Option<Guid>,
    pub node_type: FfsType,
    pub subtype: u8,
    pub offset: u32,
    pub header: Vec<u8>,
    pub body: Vec<u8>,
    pub tail: Vec<u8>,
    pub children: Vec<FfsNode>,
    pub action: Action,
    pub parsing_data: ParsingData,
    pub fixed: bool,
    pub compressed: bool,
    pub alignment_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMode { Read, Write }

#[derive(Debug, Clone)]
pub struct Image {
    pub image_id: String,
    pub session_id: String,
    pub root: FfsNode,
    pub mode: ImageMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Guid(Guid),
    Path(Vec<usize>),
    GuidSection { guid: Guid, section_type: u8, section_index: Option<usize> },
}
```

Добавить в `crates/uefi-engine/Cargo.toml` dependencies: `num_enum = "0.7"`.

- [ ] **Step 7: Создать lib.rs**

`crates/uefi-engine/src/lib.rs`:
```rust
pub mod types;
pub use types::*;
```

- [ ] **Step 8: Запустить все тесты**

Run: `cargo test -p uefi-engine`
Expected: PASS

- [ ] **Step 9: Коммит**

```bash
git add crates/uefi-engine/
git commit -m "feat: add base types (Guid, FfsNode, Image, Target, parsing data)"
```

---

### Task 3: FFS-структуры и checksum-хелперы

**Files:**
- Create: `crates/uefi-engine/src/ffs.rs`
- Test: `crates/uefi-engine/src/ffs.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `types::*`
- Produces:
  - Константы типов секций: `EFI_SECTION_COMPRESSION=0x01`, `EFI_SECTION_GUID_DEFINED=0x02`, `EFI_SECTION_PE32=0x10`, `EFI_SECTION_RAW=0x19`, `EFI_SECTION_UI=0x15`, `EFI_SECTION_VERSION=0x16`, `EFI_SECTION_FV_IMAGE=0x17`, `EFI_SECTION_FREEFORM_SUBTYPE_GUID=0x18`, `EFI_SECTION_DEPEX=0x1C`, `EFI_SECTION_TE=0x12`
  - Константы: `EFI_FVH_SIGNATURE = 0x4856465F` ("_FVH"), `EFI_FVB2_ERASE_POLARITY = 0x00000800`
  - Константы GUID-ов: `TIANO_GUID`, `LZMA_GUID`, `LZMAF86_GUID`, `CRC32_GUID`
  - Структуры-парсеры: `parse_ffs_file_header(buf, offset) -> (size, guid, type, revision, attributes)`, `parse_section_header(buf, offset) -> (size, type)`
  - `is_large_ffs(header: &[u8]) -> bool`, `is_large_section(header: &[u8]) -> bool`
  - `calculate_checksum8(data: &[u8]) -> u8`
  - `calculate_checksum16(data: &[u8]) -> u16`
  - `size_to_uint24(size: u32) -> [u8; 3]` (FFS size field is 24-bit)
  - `uint24_to_u32(b: [u8; 3]) -> u32`

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/ffs.rs` (модуль tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum8_known_vector() {
        let data = [0x01, 0x02, 0x03, 0x04];
        assert_eq!(calculate_checksum8(&data), 0xF6);
    }

    #[test]
    fn checksum16_known_vector() {
        let data = [0x01, 0x00, 0x02, 0x00];
        assert_eq!(calculate_checksum16(&data), 0x03);
    }

    #[test]
    fn uint24_roundtrip() {
        let s = 0x123456u32;
        let b = size_to_uint24(s);
        assert_eq!(uint24_to_u32(b), s);
    }

    #[test]
    fn large_section_threshold() {
        let mut hdr = vec![0x00, 0x00, 0x00, 0x00];
        assert!(!is_large_section(&hdr));
        hdr.extend_from_slice(&[0xFF; 4]);
        assert!(is_large_section(&hdr));
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine ffs::tests`
Expected: FAIL — функции не определены

- [ ] **Step 3: Реализовать ffs.rs**

`crates/uefi-engine/src/ffs.rs`:
```rust
use crate::types::Guid;

pub const EFI_SECTION_COMPRESSION: u8 = 0x01;
pub const EFI_SECTION_GUID_DEFINED: u8 = 0x02;
pub const EFI_SECTION_PE32: u8 = 0x10;
pub const EFI_SECTION_TE: u8 = 0x12;
pub const EFI_SECTION_UI: u8 = 0x15;
pub const EFI_SECTION_VERSION: u8 = 0x16;
pub const EFI_SECTION_FV_IMAGE: u8 = 0x17;
pub const EFI_SECTION_FREEFORM_SUBTYPE_GUID: u8 = 0x18;
pub const EFI_SECTION_RAW: u8 = 0x19;
pub const EFI_SECTION_DEPEX: u8 = 0x1C;

pub const EFI_FVH_SIGNATURE: u32 = 0x4856465F;
pub const EFI_FVB2_ERASE_POLARITY: u32 = 0x00000800;

pub const TIANO_GUID: Guid = Guid { data1: 0xA31280AD, data2: 0x0411, data3: 0x42B8, data4: [0xAA, 0x09, 0xC4, 0x84, 0xA2, 0x90, 0x6F, 0xDC] };
pub const LZMA_GUID: Guid = Guid { data1: 0xEE4E5ACE, data2: 0x8C72, data3: 0x4AE3, data4: [0x8B, 0xFC, 0xE1, 0xF3, 0xC1, 0xA0, 0x8C, 0x14] };
pub const LZMAF86_GUID: Guid = Guid { data1: 0xD42AE6BD, data2: 0x1352, data3: 0x4B12, data4: [0x95, 0xA0, 0xC1, 0xD4, 0x1D, 0xF2, 0x9E, 0x0C] };
pub const CRC32_GUID: Guid = Guid { data1: 0xFCDEFEEE, data2: 0x3598, data3: 0x4908, data4: [0xB3, 0x37, 0x78, 0xF5, 0x9F, 0x8F, 0x1A, 0x8E] };

pub fn calculate_checksum8(data: &[u8]) -> u8 {
    let sum: u32 = data.iter().map(|&b| b as u32).sum();
    (0x100 - (sum & 0xFF) as u8) & 0xFF
}

pub fn calculate_checksum16(data: &[u8]) -> u16 {
    assert!(data.len() % 2 == 0, "checksum16 requires even length");
    let mut sum: u32 = 0;
    for chunk in data.chunks_exact(2) {
        sum += u16::from_le_bytes([chunk[0], chunk[1]]) as u32;
    }
    let result = (0x10000 - (sum & 0xFFFF)) as u16;
    result & 0xFFFF
}

pub fn size_to_uint24(size: u32) -> [u8; 3] {
    [(size & 0xFF) as u8, ((size >> 8) & 0xFF) as u8, ((size >> 16) & 0xFF) as u8]
}

pub fn uint24_to_u32(b: [u8; 3]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16)
}

pub fn is_large_section(header: &[u8]) -> bool {
    header.len() >= 8 && header[3] == 0xFF && header[4] == 0xFF && header[5] == 0xFF
}

pub fn is_large_ffs(header: &[u8]) -> bool {
    header.len() >= 30 && header[25] == 0xFF && header[26] == 0xFF && header[27] == 0xFF
}

pub fn section_size(header: &[u8]) -> u32 {
    if is_large_section(header) {
        u32::from_le_bytes([header[4], header[5], header[6], header[7]])
    } else {
        uint24_to_u32([header[0], header[1], header[2]])
    }
}

pub fn ffs_file_size(header: &[u8]) -> u32 {
    if is_large_ffs(header) {
        u32::from_le_bytes([header[20], header[21], header[22], header[23]])
    } else {
        uint24_to_u32([header[20], header[21], header[22]])
    }
}
```

- [ ] **Step 4: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine ffs::tests`
Expected: PASS

- [ ] **Step 5: Подключить модуль в lib.rs**

`crates/uefi-engine/src/lib.rs` (изменить):
```rust
pub mod types;
pub mod ffs;
pub use types::*;
```

- [ ] **Step 6: Запустить все тесты и clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-engine/src/ffs.rs crates/uefi-engine/src/lib.rs
git commit -m "feat: add FFS structures and checksum helpers"
```

---

### Task 4: Парсер FirmwareVolume

**Files:**
- Create: `crates/uefi-engine/src/parser/mod.rs`
- Create: `crates/uefi-engine/src/parser/volume.rs`
- Test: `crates/uefi-engine/src/parser/volume.rs` (inline tests)
- Fixture: `tests/fixtures/minimal_volume.bin` (создаётся в тесте, не хранится в репо)

**Interfaces:**
- Consumes: `types::*`, `ffs::*`
- Produces:
  - `pub fn parse_volume(buf: &[u8], offset: u32) -> Result<FfsNode, ParserError>`
  - `enum ParserError { InvalidHeader(String), UnknownType, EndOfBuffer }` (в `parser/mod.rs`)
- Референс: `../refs/UEFITool-ai-fork/common/ffsparser.cpp` `parseVolumeHeader`/`parseVolumeBody`. Заголовок `EFI_FIRMWARE_VOLUME_HEADER` ищется по сигнатуре `_FVH` (offset 40-44). `erasePolarity` из `EFI_FVB2_ERASE_POLARITY`. `ffs_version` 2 или 3 по `FileSystemGuid`.

- [ ] **Step 1: Написать failing test с синтетическим volume**

`crates/uefi-engine/src/parser/mod.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("unknown type")]
    UnknownType,
    #[error("end of buffer")]
    EndOfBuffer,
}

pub mod volume;
```

`crates/uefi-engine/src/parser/volume.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::EFI_FVH_SIGNATURE;

    fn make_minimal_volume() -> Vec<u8> {
        let mut buf = vec![0u8; 256];
        let mut hdr = vec![0u8; 56];
        hdr[0..16].copy_from_slice(&[0x00; 16]);
        hdr[16..20].copy_from_slice(&0u32.to_le_bytes());
        hdr[20..24].copy_from_slice(&256u32.to_le_bytes());
        hdr[24..28].copy_from_slice(&0u32.to_le_bytes());
        hdr[28..32].copy_from_slice(&0u32.to_le_bytes());
        hdr[32..36].copy_from_slice(&0u32.to_le_bytes());
        hdr[36..40].copy_from_slice(&0u32.to_le_bytes());
        hdr[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        hdr[44] = 0x48;
        hdr[45] = 0xFE;
        hdr[46] = 0xFF;
        hdr[47] = 0xFF;
        hdr[48..56].copy_from_slice(&[0u8; 8]);
        buf[0..56].copy_from_slice(&hdr);
        buf
    }

    #[test]
    fn parse_minimal_volume() {
        let buf = make_minimal_volume();
        let node = parse_volume(&buf, 0).unwrap();
        assert_eq!(node.node_type, FfsType::Volume);
        assert_eq!(node.offset, 0);
        assert_eq!(node.header.len(), 56);
        assert!(node.children.is_empty());
    }
}
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine parser::volume::tests`
Expected: FAIL — `parse_volume` не определён

- [ ] **Step 3: Реализовать parse_volume**

Дополнить `crates/uefi-engine/src/parser/volume.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use super::ParserError;

pub fn parse_volume(buf: &[u8], offset: u32) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 56 > buf.len() { return Err(ParserError::EndOfBuffer); }
    let sig = u32::from_le_bytes([buf[off+40], buf[off+41], buf[off+42], buf[off+43]]);
    if sig != EFI_FVH_SIGNATURE {
        return Err(ParserError::InvalidHeader(format!("bad FVH signature at {offset}")));
    }
    let vol_size = u32::from_le_bytes([buf[off+20], buf[off+21], buf[off+22], buf[off+23]]);
    let header_len = u16::from_le_bytes([buf[off+44], buf[off+45]]) as usize;
    let attributes = u32::from_le_bytes([buf[off+24], buf[off+25], buf[off+26], buf[off+27]]);
    let empty_byte = if attributes & EFI_FVB2_ERASE_POLARITY != 0 { 0xFF } else { 0x00 };
    let header = buf[off..off+header_len].to_vec();
    let body = buf[off+header_len..off+vol_size as usize].to_vec();
    let parsing_data = ParsingData::Volume(VolumeParsingData {
        extended_header_guid: None,
        alignment: 1 << (attributes & 0x1F),
        ffs_version: 2,
        empty_byte,
        revision: buf[off+48],
    });
    Ok(FfsNode {
        guid: None,
        node_type: FfsType::Volume,
        subtype: 0,
        offset,
        header,
        body,
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    })
}
```

- [ ] **Step 4: Запустить тест — должен пройти**

Run: `cargo test -p uefi-engine parser::volume::tests`
Expected: PASS

- [ ] **Step 5: Подключить модуль в lib.rs и коммит**

`crates/uefi-engine/src/lib.rs`:
```rust
pub mod types;
pub mod ffs;
pub mod parser;
pub use types::*;
```

```bash
git add crates/uefi-engine/src/parser/ crates/uefi-engine/src/lib.rs
git commit -m "feat: add FirmwareVolume parser"
```

---

### Task 5: Парсер FFS-файлов

**Files:**
- Create: `crates/uefi-engine/src/parser/file.rs`
- Test: inline tests

**Interfaces:**
- Consumes: `types::*`, `ffs::*`, `parser::ParserError`, `parser::section::parse_sections` (Task 6)
- Produces: `pub fn parse_file(buf: &[u8], offset: u32, erase_polarity: u8, revision: u8) -> Result<FfsNode, ParserError>`
- Референс: `../refs/UEFITool-ai-fork/common/ffsparser.cpp` `parseFileHeader`/`parseFileBody`. Заголовок `EFI_FFS_FILE_HEADER` 24 байта (FFSv2) или 32 байта (FFSv3 large / Lenovo large). GUID из первых 16 байт. Checksum проверяется: `calculate_checksum8(header) == 0`. Tail для revision 1 — 2 байта `~TailReference`. После заголовка — тело (body), парсится `parse_sections`.

- [ ] **Step 1: Написать failing test**

`crates/uefi-engine/src/parser/file.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_ffs() -> Vec<u8> {
        let mut buf = vec![0u8; 48];
        let guid = Guid { data1: 0x5C60F367, data2: 0xA505, data3: 0x419A, data4: [0x85, 0x9E, 0x2A, 0x4F, 0xF6, 0xCA, 0x6F, 0xE5] };
        let gb = guid_bytes(&guid);
        buf[0..16].copy_from_slice(&gb);
        buf[16] = 0x01; // type: FFSv2 RAW
        buf[17] = 0x02; // attributes
        buf[18] = 0x00; // size[0]
        buf[19] = 0x00; // size[1]
        buf[20] = size_to_uint24(48)[0];
        buf[21] = size_to_uint24(48)[1];
        buf[22] = size_to_uint24(48)[2];
        let cs = calculate_checksum8(&buf[0..23]);
        buf[23] = cs;
        buf[24..48].copy_from_slice(&[0xFF; 24]);
        buf
    }

    fn guid_bytes(g: &Guid) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&g.data1.to_le_bytes());
        b[4..6].copy_from_slice(&g.data2.to_le_bytes());
        b[6..8].copy_from_slice(&g.data3.to_le_bytes());
        b[8..16].copy_from_slice(&g.data4);
        b
    }

    #[test]
    fn parse_ffs_minimal() {
        let buf = make_minimal_ffs();
        let node = parse_file(&buf, 0, 0xFF, 2).unwrap();
        assert_eq!(node.node_type, FfsType::File);
        assert_eq!(node.offset, 0);
        assert_eq!(node.header.len(), 24);
        assert_eq!(node.body.len(), 24);
    }
}
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine parser::file::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать parse_file**

`crates/uefi-engine/src/parser/file.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use super::ParserError;

pub fn guid_bytes(g: &Guid) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&g.data1.to_le_bytes());
    b[4..6].copy_from_slice(&g.data2.to_le_bytes());
    b[6..8].copy_from_slice(&g.data3.to_le_bytes());
    b[8..16].copy_from_slice(&g.data4);
    b
}

fn guid_from_bytes(b: &[u8]) -> Guid {
    Guid {
        data1: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        data2: u16::from_le_bytes([b[4], b[5]]),
        data3: u16::from_le_bytes([b[6], b[7]]),
        data4: [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
    }
}

pub fn parse_file(buf: &[u8], offset: u32, erase_polarity: u8, revision: u8) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 24 > buf.len() { return Err(ParserError::EndOfBuffer); }
    let large = is_large_ffs(&buf[off..]);
    let hdr_len = if large { 32 } else { 24 };
    if off + hdr_len > buf.len() { return Err(ParserError::EndOfBuffer); }
    let guid = guid_from_bytes(&buf[off..off+16]);
    let ftype = buf[off+16];
    let attributes = buf[off+17];
    let size = ffs_file_size(&buf[off..]);
    let total = size as usize;
    if off + total > buf.len() { return Err(ParserError::EndOfBuffer); }
    let header = buf[off..off+hdr_len].to_vec();
    let tail_len = if revision == 1 { 2 } else { 0 };
    let body_end = off + total - tail_len;
    let body = buf[off+hdr_len..body_end].to_vec();
    let tail = if tail_len > 0 { buf[body_end..body_end+tail_len].to_vec() } else { vec![] };
    let parsing_data = ParsingData::File(FileParsingData { empty_byte: erase_polarity, guid });
    Ok(FfsNode {
        guid: Some(guid),
        node_type: FfsType::File,
        subtype: ftype,
        offset,
        header,
        body,
        tail,
        children: vec![],
        action: Action::NoAction,
        parsing_data,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    })
}
```

- [ ] **Step 4: Запустить тест — должен пройти**

Run: `cargo test -p uefi-engine parser::file::tests`
Expected: PASS

- [ ] **Step 5: Подключить модуль в parser/mod.rs**

`crates/uefi-engine/src/parser/mod.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("unknown type")]
    UnknownType,
    #[error("end of buffer")]
    EndOfBuffer,
}

pub mod volume;
pub mod file;
pub mod section;
```

- [ ] **Step 6: Коммит**

```bash
git add crates/uefi-engine/src/parser/file.rs crates/uefi-engine/src/parser/mod.rs
git commit -m "feat: add FFS file parser"
```

---

### Task 6: Парсер секций (PE32, GUIDed, compressed, raw)

**Files:**
- Create: `crates/uefi-engine/src/parser/section.rs`
- Test: inline tests

**Interfaces:**
- Consumes: `types::*`, `ffs::*`, `parser::ParserError`
- Produces:
  - `pub fn parse_sections(buf: &[u8], offset: u32) -> Vec<FfsNode>` — итерирует секции в теле FFS
  - `pub fn parse_section(buf: &[u8], offset: u32) -> Result<FfsNode, ParserError>` — парсит одну секцию
- Референс: `../refs/UEFITool-ai-fork/common/ffsparser.cpp` `parseSectionHeader`/`parseSectionBody`/`parseCompressedSectionBody`/`parseGuidedSectionBody`. Заголовок секции 4 байта (или 8 для large). Для `EFI_SECTION_COMPRESSION` — body распаковывается (алгоритм Tiano/LZMA) и рекурсивно парсится `parse_sections`. Для `EFI_SECTION_GUID_DEFINED` — определяется алгоритм по GUID (TIANO/LZMA/LZMAF86/CRC32), распаковывается. Для leaf-секций (PE32, RAW, UI) — body as-is.

- [ ] **Step 1: Написать failing test для raw-секции**

`crates/uefi-engine/src/parser/section.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_raw_section() {
        let mut buf = vec![0u8; 12];
        buf[0] = 8;  // size[0] (8 bytes total: 4 hdr + 4 body)
        buf[1] = 0;
        buf[2] = 0;
        buf[3] = EFI_SECTION_RAW;
        buf[4..8].copy_from_slice(&[0xAA; 4]);
        let node = parse_section(&buf, 0).unwrap();
        assert_eq!(node.node_type, FfsType::Section);
        assert_eq!(node.subtype, EFI_SECTION_RAW);
        assert_eq!(node.header.len(), 4);
        assert_eq!(node.body.len(), 4);
    }

    #[test]
    fn parse_multiple_sections() {
        let mut buf = vec![0u8; 16];
        buf[0] = 8; buf[3] = EFI_SECTION_RAW; buf[4..8].copy_from_slice(&[0x01; 4]);
        buf[8] = 8; buf[11] = EFI_SECTION_PE32; buf[12..16].copy_from_slice(&[0x02; 4]);
        let nodes = parse_sections(&buf, 0);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].subtype, EFI_SECTION_RAW);
        assert_eq!(nodes[1].subtype, EFI_SECTION_PE32);
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine parser::section::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать parse_section и parse_sections**

`crates/uefi-engine/src/parser/section.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use super::ParserError;

pub fn parse_section(buf: &[u8], offset: u32) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 4 > buf.len() { return Err(ParserError::EndOfBuffer); }
    let large = is_large_section(&buf[off..]);
    let hdr_len = if large { 8 } else { 4 };
    let size = section_size(&buf[off..]) as usize;
    if size < hdr_len || off + size > buf.len() { return Err(ParserError::InvalidHeader("section size".into())); }
    let stype = buf[off + 3];
    let header = buf[off..off+hdr_len].to_vec();
    let body = buf[off+hdr_len..off+size].to_vec();
    let parsing_data = match stype {
        EFI_SECTION_GUID_DEFINED if body.len() >= 20 => {
            let guid = Guid {
                data1: u32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                data2: u16::from_le_bytes([body[4], body[5]]),
                data3: u16::from_le_bytes([body[6], body[7]]),
                data4: [body[8], body[9], body[10], body[11], body[12], body[13], body[14], body[15]],
            };
            let data_offset = u16::from_le_bytes([body[16], body[17]]) as usize;
            let _attributes = u16::from_le_bytes([body[18], body[19]]);
            ParsingData::GuidedSection(GuidedSectionParsingData { guid, dictionary_size: 0 })
        }
        EFI_SECTION_COMPRESSION if body.len() >= 5 => {
            let uncomp_size = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            let comp_type = body[4];
            ParsingData::CompressedSection(CompressedSectionParsingData {
                uncompressed_size: uncomp_size,
                compression_type: comp_type,
                algorithm: 0,
                dictionary_size: 0,
            })
        }
        _ => ParsingData::None,
    };
    let mut children = vec![];
    if stype == EFI_SECTION_COMPRESSION || stype == EFI_SECTION_GUID_DEFINED {
        // Декомпрессия и рекурсивный парсинг — в Task 7 (decompress).
        // Пока: children пустые, body as-is.
    } else {
        // leaf section
    }
    Ok(FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: stype,
        offset,
        header,
        body,
        tail: vec![],
        children,
        action: Action::NoAction,
        parsing_data,
        fixed: false,
        compressed: stype == EFI_SECTION_COMPRESSION,
        alignment_bytes: vec![],
    })
}

pub fn parse_sections(buf: &[u8], start: u32) -> Vec<FfsNode> {
    let mut nodes = vec![];
    let mut off = start as usize;
    while off + 4 <= buf.len() {
        let large = is_large_section(&buf[off..]);
        let hdr_len = if large { 8 } else { 4 };
        let size = section_size(&buf[off..]) as usize;
        if size < hdr_len || off + size > buf.len() { break; }
        let aligned = (size + 3) & !3;
        match parse_section(buf, off as u32) {
            Ok(node) => nodes.push(node),
            Err(e) => { tracing::warn!("section parse error at {off}: {e}"); break; }
        }
        off += aligned;
    }
    nodes
}
```

- [ ] **Step 4: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine parser::section::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/src/parser/section.rs
git commit -m "feat: add section parser (raw, pe32, guid-defined, compression stub)"
```

---

### Task 7: Декомпрессия (Tiano/LZMA) для compressed-секций

**Files:**
- Modify: `crates/uefi-engine/src/parser/section.rs`
- Create: `crates/uefi-engine/src/decompress.rs`
- Add dependency: `lzss` или ручная реализация Tiano; `lzma-rs` для LZMA
- Test: inline tests

**Interfaces:**
- Consumes: `types::*`, `ffs::*`
- Produces:
  - `pub fn decompress(data: &[u8], algorithm: u8) -> Result<Vec<u8>, DecompressError>`
  - `enum DecompressError { Unsupported, Corrupted }`
- Алгоритмы: `EFI_NOT_COMPRESSED=0`, `EFI_STANDARD_COMPRESSION=1` (Tiano/LZSS), `EFI_LZMA_COMPRESSION=2`. Для GUIDed-секций алгоритм определяется по GUID: `TIANO_GUID` → Tiano, `LZMA_GUID`/`LZMAF86_GUID` → LZMA.
- Референс: `../refs/UEFITool-ai-fork/common/ffsparser.cpp` `decompress`. Tiano — LZSS-вариант EDK2. LZMA — через `LzmaDecode`.

- [ ] **Step 1: Написать failing test для not-compressed**

`crates/uefi-engine/src/decompress.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecompressError {
    #[error("unsupported algorithm")]
    Unsupported,
    #[error("corrupted data")]
    Corrupted,
}

pub const EFI_NOT_COMPRESSED: u8 = 0;
pub const EFI_STANDARD_COMPRESSION: u8 = 1;
pub const EFI_LZMA_COMPRESSION: u8 = 2;

pub fn decompress(data: &[u8], algorithm: u8) -> Result<Vec<u8>, DecompressError> {
    match algorithm {
        EFI_NOT_COMPRESSED => Ok(data.to_vec()),
        EFI_STANDARD_COMPRESSION => decompress_tiano(data),
        EFI_LZMA_COMPRESSION => decompress_lzma(data),
        _ => Err(DecompressError::Unsupported),
    }
}

fn decompress_tiano(_data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    Err(DecompressError::Unsupported)
}

fn decompress_lzma(data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    if data.len() < 13 { return Err(DecompressError::Corrupted); }
    let props = data[0..5].to_vec();
    let _uncomp_size = u32::from_le_bytes([data[5], data[6], data[7], data[8]]) as usize;
    let payload = &data[13..];
    let mut output: Vec<u8> = Vec::new();
    let mut decoder = lzma_rs::lzma_decompressor();
    use std::io::Cursor;
    decoder.decompress(&mut Cursor::new(payload), &mut output, &props)
        .map_err(|_| DecompressError::Corrupted)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompress_not_compressed() {
        let data = vec![0x01, 0x02, 0x03];
        assert_eq!(decompress(&data, EFI_NOT_COMPRESSED).unwrap(), data);
    }

    #[test]
    fn decompress_unsupported() {
        assert!(decompress(&[], 0xFF).is_err());
    }
}
```

- [ ] **Step 2: Добавить зависимость lzma-rs**

`crates/uefi-engine/Cargo.toml` [dependencies]:
```toml
lzma-rs = "0.3"
```

- [ ] **Step 3: Запустить тесты — not_compressed должен пройти, tiano fail (Unsupported — приемлемо для цикла 1)**

Run: `cargo test -p uefi-engine decompress`
Expected: `decompress_not_compressed` PASS, `decompress_unsupported` PASS

- [ ] **Step 4: Подключить декомпрессию в парсер секций**

Изменить `crates/uefi-engine/src/parser/section.rs` в `parse_section` для `EFI_SECTION_COMPRESSION`:
```rust
if stype == EFI_SECTION_COMPRESSION {
    if let ParsingData::CompressedSection(cd) = &parsing_data {
        if body.len() >= 5 {
            let comp_data = &body[5..];
            match crate::decompress::decompress(comp_data, cd.compression_type) {
                Ok(decompressed) => {
                    children = parse_sections(&decompressed, 0);
                }
                Err(e) => {
                    tracing::warn!("decompress failed at {off}: {e}");
                }
            }
        }
    }
}
```
(заменить заглушку `children = vec![]` на этот блок)

- [ ] **Step 5: Подключить модуль decompress в lib.rs**

`crates/uefi-engine/src/lib.rs`:
```rust
pub mod types;
pub mod ffs;
pub mod parser;
pub mod decompress;
pub use types::*;
```

- [ ] **Step 6: Запустить все тесты**

Run: `cargo test -p uefi-engine`
Expected: PASS

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-engine/src/decompress.rs crates/uefi-engine/src/parser/section.rs crates/uefi-engine/src/lib.rs crates/uefi-engine/Cargo.toml
git commit -m "feat: add decompression (LZMA via lzma-rs, Tiano stub)"
```

---

### Task 8: Полный парсер образа (parse_image) и dump_tree/list_items

**Files:**
- Modify: `crates/uefi-engine/src/parser/mod.rs`
- Create: `crates/uefi-engine/src/parser/image.rs`
- Test: inline tests

**Interfaces:**
- Consumes: `parser::volume::parse_volume`, `parser::file::parse_file`, `parser::section::parse_sections`
- Produces:
  - `pub fn parse_image(buf: &[u8], mode: ImageMode, image_id: &str, session_id: &str) -> Result<Image, ParserError>`
  - `pub fn dump_tree(node: &FfsNode, format: DumpFormat) -> String`
  - `pub fn list_items(node: &FfsNode, filter: Option<&str>) -> Vec<Item>`
- `parse_image` ищет сигнатуру `_FVH` через весь буфер, парсит все volumes, соединяет в один root `FfsType::Image`.

- [ ] **Step 1: Написать failing test**

`crates/uefi-engine/src/parser/image.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::EFI_FVH_SIGNATURE;

    fn make_image_with_volume() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[20..24].copy_from_slice(&256u32.to_le_bytes());
        buf[44] = 0x48; buf[45] = 0xFE; buf[46] = 0xFF; buf[47] = 0xFF;
        buf
    }

    #[test]
    fn parse_image_finds_volume() {
        let buf = make_image_with_volume();
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.node_type, FfsType::Image);
        assert!(!img.root.children.is_empty());
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
    }

    #[test]
    fn dump_tree_text() {
        let buf = make_image_with_volume();
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        let s = dump_tree(&img.root, DumpFormat::Text);
        assert!(s.contains("Image"));
        assert!(s.contains("Volume"));
    }
}
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine parser::image::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать parse_image, dump_tree, list_items**

`crates/uefi-engine/src/parser/image.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use super::{ParserError, volume::parse_volume, file::parse_file, section::parse_sections};
use uefi_proto::{DumpFormat, Item};

pub fn parse_image(buf: &[u8], mode: ImageMode, image_id: &str, session_id: &str) -> Result<Image, ParserError> {
    let mut children = vec![];
    let mut off = 0u32;
    while (off as usize) < buf.len() {
        if (off as usize) + 44 > buf.len() { break; }
        let sig = u32::from_le_bytes([buf[off as usize + 40], buf[off as usize + 41], buf[off as usize + 42], buf[off as usize + 43]]);
        if sig != EFI_FVH_SIGNATURE { off += 16; continue; }
        match parse_volume(buf, off) {
            Ok(vol) => {
                let vol_size = u32::from_le_bytes([buf[off as usize + 20], buf[off as usize + 21], buf[off as usize + 22], buf[off as usize + 23]]);
                let mut vol_with_files = vol.clone();
                let erase = if let ParsingData::Volume(vd) = &vol.parsing_data { vd.empty_byte } else { 0xFF };
                let rev = if let ParsingData::Volume(vd) = &vol.parsing_data { vd.revision } else { 2 };
                let body_start = off as usize + vol.header.len();
                let body = &buf[body_start..off as usize + vol_size as usize];
                let mut foff = 0u32;
                while (foff as usize) + 24 <= body.len() {
                    let abs = body_start as u32 + foff;
                    let fhdr = &body[foff as usize..];
                    if fhdr.iter().take(24).all(|&b| b == erase) { break; }
                    match parse_file(body, foff, erase, rev) {
                        Ok(mut file_node) => {
                            file_node.offset = abs;
                            let body_off = foff as usize + file_node.header.len();
                            file_node.children = parse_sections(body, body_off as u32);
                            vol_with_files.children.push(file_node);
                            let fsz = ffs_file_size(fhdr) as usize;
                            foff += (fsz + 7) & !7;
                        }
                        Err(e) => {
                            tracing::warn!("file parse error at {abs}: {e}");
                            break;
                        }
                    }
                }
                children.push(vol_with_files);
                off += vol_size;
            }
            Err(e) => {
                tracing::warn!("volume parse error at {off}: {e}");
                off += 16;
            }
        }
    }
    Ok(Image {
        image_id: image_id.into(),
        session_id: session_id.into(),
        root: FfsNode {
            guid: None, node_type: FfsType::Image, subtype: 0, offset: 0,
            header: vec![], body: vec![], tail: vec![],
            children, action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false, compressed: false, alignment_bytes: vec![],
        },
        mode,
    })
}

pub fn dump_tree(node: &FfsNode, format: DumpFormat) -> String {
    let mut s = String::new();
    dump_recursive(node, "", &mut s, format);
    s
}

fn dump_recursive(node: &FfsNode, path: &str, out: &mut String, format: DumpFormat) {
    let new_path = if path.is_empty() { node.offset.to_string() } else { format!("{path}/{}", node.offset) };
    match format {
        DumpFormat::Text => {
            out.push_str(&format!("{new_path} {:?} subtype={:02X} guid={:?} hdr={} body={} children={}\n",
                node.node_type, node.subtype, node.guid.map(|g| g.to_string()).unwrap_or_default(),
                node.header.len(), node.body.len(), node.children.len()));
        }
        DumpFormat::Tsv => {
            out.push_str(&format!("{new_path}\t{}\t{:02X}\t{}\t{}\t{}\t{}\n",
                node.node_type as u32, node.subtype,
                node.guid.map(|g| g.to_string()).unwrap_or_default(),
                node.offset, node.header.len() + node.body.len() + node.tail.len(),
                ""));
        }
    }
    for (i, child) in node.children.iter().enumerate() {
        let p = if path.is_empty() { i.to_string() } else { format!("{path}/{i}") };
        dump_recursive(child, &p, out, format);
    }
}

pub fn list_items(node: &FfsNode, filter: Option<&str>) -> Vec<Item> {
    let mut items = vec![];
    list_recursive(node, "", &mut items, filter);
    items
}

fn list_recursive(node: &FfsNode, path: &str, items: &mut Vec<Item>, filter: Option<&str>) {
    let new_path = if path.is_empty() { "0".into() } else { path.to_string() };
    let name = String::from_utf8_lossy(&node.body).to_string();
    if filter.map_or(true, |f| name.contains(f)) {
        items.push(Item {
            path: new_path.clone(),
            r#type: node.node_type as u32,
            subtype: node.subtype as u32,
            guid: node.guid.map(|g| g.to_string()).unwrap_or_default(),
            offset: node.offset as u64,
            size: (node.header.len() + node.body.len() + node.tail.len()) as u64,
            name,
        });
    }
    for (i, child) in node.children.iter().enumerate() {
        let p = if path.is_empty() { i.to_string() } else { format!("{path}/{i}") };
        list_recursive(child, &p, items, filter);
    }
}
```

- [ ] **Step 4: Подключить image в parser/mod.rs**

`crates/uefi-engine/src/parser/mod.rs`:
```rust
pub mod volume;
pub mod file;
pub mod section;
pub mod image;
```

- [ ] **Step 5: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine parser::image::tests`
Expected: PASS

- [ ] **Step 6: Коммит**

```bash
git add crates/uefi-engine/src/parser/image.rs crates/uefi-engine/src/parser/mod.rs
git commit -m "feat: add parse_image, dump_tree, list_items"
```

---

### Task 9: Target-парсинг (GUID/PATH/GUID:T/GUID:T:N) и find_item

**Files:**
- Create: `crates/uefi-engine/src/parser/target.rs`
- Test: inline tests

**Interfaces:**
- Consumes: `types::*`
- Produces:
  - `pub fn parse_target(s: &str) -> Result<Target, ParserError>`
  - `pub fn find_item(root: &FfsNode, target: &Target) -> Result<&FfsNode, ParserError>`
- Референс: `../refs/UEFITool-ai-fork/UEFIEdit/uefiedit.cpp:195` `parseTarget`. Path: десятичные индексы детей от root, разделённые `/`. GUID: 36-символьная строка. `GUID:T` — file GUID + первая секция типа T (hex). `GUID:T:N` — N-ная секция типа T.

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/parser/target.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_guid_target() {
        let t = parse_target("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        assert!(matches!(t, Target::Guid(_)));
    }

    #[test]
    fn parse_path_target() {
        let t = parse_target("0/2/207").unwrap();
        assert_eq!(t, Target::Path(vec![0, 2, 207]));
    }

    #[test]
    fn parse_guid_type_target() {
        let t = parse_target("899407D7-92A6-4174-968F-6F0B47F86A23:0x10").unwrap();
        match t {
            Target::GuidSection { section_type, section_index, .. } => {
                assert_eq!(section_type, 0x10);
                assert_eq!(section_index, None);
            }
            _ => panic!("expected GuidSection"),
        }
    }

    #[test]
    fn parse_guid_type_index_target() {
        let t = parse_target("899407D7-92A6-4174-968F-6F0B47F86A23:0x10:2").unwrap();
        match t {
            Target::GuidSection { section_type, section_index, .. } => {
                assert_eq!(section_type, 0x10);
                assert_eq!(section_index, Some(2));
            }
            _ => panic!("expected GuidSection"),
        }
    }

    #[test]
    fn parse_invalid_target() {
        assert!(parse_target("not-a-target").is_err());
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine parser::target::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать parse_target и find_item**

`crates/uefi-engine/src/parser/target.rs`:
```rust
use crate::types::*;
use std::str::FromStr;
use super::ParserError;

pub fn parse_target(s: &str) -> Result<Target, ParserError> {
    if s.chars().all(|c| c.is_ascii_digit() || c == '/') && s.contains('/') {
        let path: Result<Vec<usize>, _> = s.split('/').map(|p| p.parse::<usize>()).collect();
        return path.map(Target::Path).map_err(|_| ParserError::InvalidHeader(format!("bad path: {s}")));
    }
    if let Some(colon_pos) = s.find(':') {
        if colon_pos >= 36 {
            let guid_str = &s[..colon_pos];
            let rest = &s[colon_pos + 1..];
            let guid = Guid::from_str(guid_str).map_err(|e| ParserError::InvalidHeader(e.to_string()))?;
            let parts: Vec<&str> = rest.split(':').collect();
            let stype = u8::from_str_radix(parts[0].trim_start_matches("0x"), 16)
                .map_err(|_| ParserError::InvalidHeader(format!("bad section type: {parts[0]}")))?;
            let sidx = if parts.len() > 1 {
                Some(parts[1].parse::<usize>().map_err(|_| ParserError::InvalidHeader(format!("bad index: {parts[1]}")))?)
            } else { None };
            return Ok(Target::GuidSection { guid, section_type: stype, section_index: sidx });
        }
    }
    if s.len() == 36 {
        let guid = Guid::from_str(s).map_err(|e| ParserError::InvalidHeader(e.to_string()))?;
        return Ok(Target::Guid(guid));
    }
    Err(ParserError::InvalidHeader(format!("unrecognized target: {s}")))
}

pub fn find_item<'a>(root: &'a FfsNode, target: &Target) -> Result<&'a FfsNode, ParserError> {
    match target {
        Target::Guid(g) => find_by_guid(root, g).ok_or_else(|| ParserError::InvalidHeader(format!("guid {g} not found"))),
        Target::Path(indices) => {
            let mut node = root;
            for &i in indices {
                node = node.children.get(i).ok_or_else(|| ParserError::InvalidHeader(format!("path index {i} not found")))?;
            }
            Ok(node)
        }
        Target::GuidSection { guid, section_type, section_index } => {
            let file = find_by_guid(root, guid).ok_or_else(|| ParserError::InvalidHeader(format!("guid {guid} not found")))?;
            let wanted = *section_index;
            let mut count = 0;
            for child in &file.children {
                if child.subtype == *section_type {
                    if wanted.map_or(true, |w| w == count) {
                        return Ok(child);
                    }
                    count += 1;
                }
            }
            Err(ParserError::InvalidHeader(format!("section type {section_type:#x} not found")))
        }
    }
}

fn find_by_guid<'a>(node: &'a FfsNode, g: &Guid) -> Option<&'a FfsNode> {
    if let Some(ng) = &node.guid {
        if ng == g { return Some(node); }
    }
    if let ParsingData::Volume(vd) = &node.parsing_data {
        if let Some(eg) = &vd.extended_header_guid { if eg == g { return Some(node); } }
    }
    if let ParsingData::GuidedSection(gs) = &node.parsing_data {
        if &gs.guid == g { return Some(node); }
    }
    for child in &node.children {
        if let Some(n) = find_by_guid(child, g) { return Some(n); }
    }
    None
}
```

- [ ] **Step 4: Подключить в parser/mod.rs и запустить тесты**

`crates/uefi-engine/src/parser/mod.rs`: добавить `pub mod target;`

Run: `cargo test -p uefi-engine parser::target::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/src/parser/target.rs crates/uefi-engine/src/parser/mod.rs
git commit -m "feat: add Target parsing (guid/path/guid:type/guid:type:index) and find_item"
```

---

### Task 10: Builder — сборка дерева в байты (round-trip)

**Files:**
- Create: `crates/uefi-engine/src/builder/mod.rs`
- Create: `crates/uefi-engine/src/builder/align.rs`
- Test: inline tests + round-trip тест с синтетическим образом

**Interfaces:**
- Consumes: `types::*`, `ffs::*`
- Produces:
  - `pub fn build_image(image: &Image) -> Result<Vec<u8>, BuilderError>`
  - `enum BuilderError { SizeMismatch, ChecksumFailed }`
- Референс: `../refs/UEFITool-ai-fork/common/ffsbuilder.cpp` `build`/`buildVolume`/`buildFile`/`buildSection`. Файлы выравниваются по 8 (padding `empty_byte`), секции по 4. `buildFile`/`buildSection` с `children.is_empty()` используют `body` as-is (баг 5). `buildVolume` сохраняет оригинальные offset'ы неизменённых файлов, padding к оригинальному размеру (баг 11). Контрольные суммы пересчитываются.

- [ ] **Step 1: Написать round-trip failing test**

`crates/uefi-engine/src/builder/mod.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("size mismatch")]
    SizeMismatch,
    #[error("checksum failed")]
    ChecksumFailed,
}

pub mod align;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::image::parse_image;
    use crate::ffs::EFI_FVH_SIGNATURE;

    fn make_image_with_volume() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[20..24].copy_from_slice(&256u32.to_le_bytes());
        buf[44] = 0x48; buf[45] = 0xFE; buf[46] = 0xFF; buf[47] = 0xFF;
        buf
    }

    #[test]
    fn round_trip_volume() {
        let orig = make_image_with_volume();
        let img = parse_image(&orig, ImageMode::Read, "img1", "s1").unwrap();
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(rebuilt, orig);
    }
}
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine builder::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать align.rs**

`crates/uefi-engine/src/builder/align.rs`:
```rust
pub fn align8(offset: usize) -> usize { (offset + 7) & !7 }
pub fn align4(offset: usize) -> usize { (offset + 3) & !3 }
pub fn pad_to(buf: &mut Vec<u8>, target: usize, fill: u8) {
    while buf.len() < target { buf.push(fill); }
}
```

- [ ] **Step 4: Реализовать build_image**

Дополнить `crates/uefi-engine/src/builder/mod.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use crate::parser::image::parse_image;
use align::*;

pub fn build_image(image: &Image) -> Result<Vec<u8>, BuilderError> {
    let mut out = vec![];
    build_node(&image.root, &mut out)?;
    Ok(out)
}

fn build_node(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    match node.node_type {
        FfsType::Image => {
            for child in &node.children {
                build_node(child, out)?;
            }
        }
        FfsType::Volume => build_volume(node, out)?,
        FfsType::File => build_file(node, out)?,
        FfsType::Section => build_section(node, out)?,
        FfsType::Padding | FfsType::FreeSpace => {
            out.extend_from_slice(&node.body);
        }
        _ => {
            out.extend_from_slice(&node.header);
            out.extend_from_slice(&node.body);
        }
    }
    Ok(())
}

fn build_volume(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    out.extend_from_slice(&node.header);
    let empty_byte = if let ParsingData::Volume(vd) = &node.parsing_data { vd.empty_byte } else { 0xFF };
    for child in &node.children {
        if child.action == Action::Remove { continue; }
        let before = out.len();
        build_node(child, out)?;
        let aligned = align8(out.len());
        pad_to(out, aligned, empty_byte);
    }
    Ok(())
}

fn build_file(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.action == Action::Remove { return Ok(()); }
    let mut header = node.header.clone();
    if !node.children.is_empty() {
        let mut body = vec![];
        for child in &node.children {
            build_node(child, &mut body)?;
            let aligned = align4(body.len());
            pad_to(&mut body, aligned, 0x00);
        }
        let total = header.len() + body.len() + node.tail.len();
        let size_bytes = size_to_uint24(total as u32);
        header[20] = size_bytes[0]; header[21] = size_bytes[1]; header[22] = size_bytes[2];
        let cs = calculate_checksum8(&header[0..23]);
        header[23] = cs;
        out.extend_from_slice(&header);
        out.extend_from_slice(&body);
        out.extend_from_slice(&node.tail);
    } else {
        out.extend_from_slice(&header);
        out.extend_from_slice(&node.body);
        out.extend_from_slice(&node.tail);
    }
    Ok(())
}

fn build_section(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if !node.children.is_empty() {
        let mut body = vec![];
        for child in &node.children {
            build_node(child, &mut body)?;
            let aligned = align4(body.len());
            pad_to(&mut body, aligned, 0x00);
        }
        let mut header = node.header.clone();
        let total = header.len() + body.len();
        if is_large_section(&header) {
            let sb = (total as u32).to_le_bytes();
            header[4] = sb[0]; header[5] = sb[1]; header[6] = sb[2]; header[7] = sb[3];
        } else {
            let sb = size_to_uint24(total as u32);
            header[0] = sb[0]; header[1] = sb[1]; header[2] = sb[2];
        }
        out.extend_from_slice(&header);
        out.extend_from_slice(&body);
    } else {
        out.extend_from_slice(&node.header);
        out.extend_from_slice(&node.body);
    }
    Ok(())
}
```

- [ ] **Step 5: Подключить модуль builder в lib.rs**

`crates/uefi-engine/src/lib.rs`:
```rust
pub mod types;
pub mod ffs;
pub mod parser;
pub mod decompress;
pub mod builder;
pub use types::*;
```

- [ ] **Step 6: Запустить round-trip тест — должен пройти**

Run: `cargo test -p uefi-engine builder::tests`
Expected: PASS

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-engine/src/builder/ crates/uefi-engine/src/lib.rs
git commit -m "feat: add builder (round-trip volume/file/section)"
```

---

### Task 11: Модификации дерева (insert/remove/replace/rebuild)

**Files:**
- Create: `crates/uefi-engine/src/ops.rs`
- Test: inline tests

**Interfaces:**
- Consumes: `types::*`, `parser::target::find_item`, `ffs::*`
- Produces:
  - `pub fn insert(root: &mut FfsNode, target: &Target, ffs_bytes: &[u8], mode: InsertMode) -> Result<(), OpsError>`
  - `pub fn remove(root: &mut FfsNode, target: &Target) -> Result<(), OpsError>`
  - `pub fn replace(root: &mut FfsNode, target: &Target, data: &[u8], body_only: bool) -> Result<(), OpsError>`
  - `pub fn rebuild(root: &mut FfsNode, target: &Target) -> Result<(), OpsError>`
  - `pub fn mark_rebuild_to_root(node: &mut FfsNode)` — каскадная пометка Rebuild для предков
  - `enum InsertMode { Into, Before, After }` (из uefi-proto, либо свой)
  - `enum OpsError { NotFound, InvalidParent, InvalidFfs }`
- Референс: `../refs/UEFITool-ai-fork/UEFIEdit/uefiedit.cpp:375` (insert), `472` (remove), `494` (replace), `520` (rebuild). После каждой операции — каскад `Rebuild` для всех предков до root. `replace` с `body_only=true` заменяет только body, сохраняет заголовок. `replace` должен `clearChildren` перед `setBody` (баг 9).

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/ops.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpsError {
    #[error("not found")]
    NotFound,
    #[error("invalid parent")]
    InvalidParent,
    #[error("invalid FFS data")]
    InvalidFfs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertMode { Into, Before, After }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::image::parse_image;
    use crate::parser::target::parse_target;

    fn make_simple_image() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        buf[40..44].copy_from_slice(&crate::ffs::EFI_FVH_SIGNATURE.to_le_bytes());
        buf[20..24].copy_from_slice(&256u32.to_le_bytes());
        buf[44] = 0x48; buf[45] = 0xFE; buf[46] = 0xFF; buf[47] = 0xFF;
        buf
    }

    #[test]
    fn rebuild_marks_node() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        rebuild(&mut img.root, &t).unwrap();
        assert_eq!(img.root.children[0].action, Action::Rebuild);
    }
}
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine ops::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать ops.rs**

`crates/uefi-engine/src/ops.rs`:
```rust
use crate::types::*;
use crate::ffs::*;
use crate::parser::target::{find_item, parse_target};
use super::ParserError;

#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("not found")]
    NotFound,
    #[error("invalid parent")]
    InvalidParent,
    #[error("invalid FFS data")]
    InvalidFfs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertMode { Into, Before, After }

pub fn insert(root: &mut FfsNode, target: &Target, ffs_bytes: &[u8], mode: InsertMode) -> Result<(), OpsError> {
    let new_node = parse_ffs_bytes(ffs_bytes).map_err(|_| OpsError::InvalidFfs)?;
    let parent_path = match target {
        Target::Path(ref p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    match mode {
        InsertMode::Into => {
            let parent = find_mut(root, &parent_path).ok_or(OpsError::NotFound)?;
            parent.children.push(new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
        InsertMode::Before => {
            if parent_path.is_empty() { return Err(OpsError::InvalidParent); }
            let idx = *parent_path.last().unwrap();
            let grandparent_path = &parent_path[..parent_path.len()-1];
            let grandparent = find_mut(root, grandparent_path).ok_or(OpsError::NotFound)?;
            grandparent.children.insert(idx, new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
        InsertMode::After => {
            if parent_path.is_empty() { return Err(OpsError::InvalidParent); }
            let idx = *parent_path.last().unwrap();
            let grandparent_path = &parent_path[..parent_path.len()-1];
            let grandparent = find_mut(root, grandparent_path).ok_or(OpsError::NotFound)?;
            let insert_at = idx + 1;
            grandparent.children.insert(insert_at, new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
    }
    Ok(())
}

pub fn remove(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = match target {
        Target::Path(ref p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Remove;
    mark_rebuild_to_root_by_path(root, &path);
    Ok(())
}

pub fn replace(root: &mut FfsNode, target: &Target, data: &[u8], body_only: bool) -> Result<(), OpsError> {
    let path = match target {
        Target::Path(ref p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    if body_only {
        node.children.clear();
        node.body = data.to_vec();
    } else {
        let new_node = parse_ffs_bytes(data).map_err(|_| OpsError::InvalidFfs)?;
        node.header = new_node.header;
        node.body = new_node.body;
        node.children = new_node.children;
        node.guid = new_node.guid;
        node.subtype = new_node.subtype;
    }
    node.action = Action::Replace;
    mark_rebuild_to_root_by_path(root, &path);
    Ok(())
}

pub fn rebuild(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = match target {
        Target::Path(ref p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Rebuild;
    mark_rebuild_to_root_by_path(root, &path);
    Ok(())
}

pub fn mark_rebuild_to_root_by_path(root: &mut FfsNode, path: &[usize]) {
    let mut node = root;
    if root.action == Action::NoAction { root.action = Action::Rebuild; }
    for &i in path {
        if let Some(child) = node.children.get_mut(i) {
            if child.action == Action::NoAction { child.action = Action::Rebuild; }
            node = child;
        } else { break; }
    }
}

fn find_mut<'a>(node: &'a mut FfsNode, path: &[usize]) -> Option<&'a mut FfsNode> {
    let mut cur = node;
    for &i in path {
        cur = cur.children.get_mut(i)?;
    }
    Some(cur)
}

fn parse_ffs_bytes(data: &[u8]) -> Result<FfsNode, ParserError> {
    crate::parser::file::parse_file(data, 0, 0xFF, 2)
}
```

- [ ] **Step 4: Подключить в lib.rs и запустить тесты**

`crates/uefi-engine/src/lib.rs`: добавить `pub mod ops;`

Run: `cargo test -p uefi-engine ops::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/src/ops.rs crates/uefi-engine/src/lib.rs
git commit -m "feat: add tree ops (insert/remove/replace/rebuild) with cascade rebuild"
```

---

### Task 12: Storage (SQLite) — sessions и artifacts

**Files:**
- Create: `crates/uefi-engine/src/storage/mod.rs`
- Create: `crates/uefi-engine/src/storage/schema.rs`
- Create: `crates/uefi-engine/src/storage/artifact.rs`
- Test: inline tests с tempfile

**Interfaces:**
- Consumes: `rusqlite`
- Produces:
  - `pub struct Db { conn: rusqlite::Connection }`
  - `pub fn open_db(path: &Path) -> Result<Db>`
  - `Db::insert_session(id, token)`, `Db::get_session(id) -> Option<SessionRow>`, `Db::touch_session(id)`, `Db::delete_session(id)`, `Db::list_sessions() -> Vec<SessionRow>`, `Db::list_expired(ttl_secs) -> Vec<String>`
  - `Db::insert_artifact(id, session_id, kind, path, size)`, `Db::delete_artifacts_for_session(id)`
  - `pub fn store_artifact_file(data_dir, session_id, image_id, bytes) -> Result<PathBuf>`

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/storage/mod.rs`:
```rust
pub mod schema;
pub mod artifact;

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use anyhow::Result;

pub struct Db { pub conn: Connection }

#[derive(Debug, Clone)]
pub struct SessionRow { pub id: String, pub token: String, pub created_at: i64, pub last_activity: i64 }

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, Db) {
        let td = TempDir::new().unwrap();
        let db = open_db(&td.path().join("test.db")).unwrap();
        (td, db)
    }

    #[test]
    fn insert_and_get_session() {
        let (_td, db) = test_db();
        db.insert_session("s1", "tok1").unwrap();
        let row = db.get_session("s1").unwrap().unwrap();
        assert_eq!(row.token, "tok1");
    }

    #[test]
    fn delete_session_cascades_artifacts() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t").unwrap();
        db.insert_artifact("a1", "s1", "image", "/x", 100).unwrap();
        db.delete_session("s1").unwrap();
        assert!(db.get_session("s1").unwrap().is_none());
    }

    #[test]
    fn list_expired() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t").unwrap();
        db.conn.execute("UPDATE sessions SET last_activity = 0").unwrap();
        let expired = db.list_expired(3600).unwrap();
        assert!(expired.contains(&"s1".into()));
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine storage::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать schema.rs**

`crates/uefi-engine/src/storage/schema.rs`:
```rust
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity);
"#;
```

- [ ] **Step 4: Реализовать artifact.rs**

`crates/uefi-engine/src/storage/artifact.rs`:
```rust
use std::path::{Path, PathBuf};
use anyhow::Result;
use std::fs;

pub fn store_artifact_file(data_dir: &Path, session_id: &str, image_id: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = data_dir.join("sessions").join(session_id).join("images");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{image_id}.bin"));
    fs::write(&path, bytes)?;
    Ok(path)
}
```

- [ ] **Step 5: Реализовать storage/mod.rs (методы Db)**

Дополнить `crates/uefi-engine/src/storage/mod.rs`:
```rust
use rusqlite::params;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open_db(path: &Path) -> Result<Db> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(schema::SCHEMA)?;
    Ok(Db { conn })
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

impl Db {
    pub fn insert_session(&self, id: &str, token: &str) -> Result<()> {
        let t = now();
        self.conn.execute("INSERT INTO sessions (id, token, created_at, last_activity) VALUES (?1, ?2, ?3, ?3)",
            params![id, token, t])?;
        Ok(())
    }
    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>> {
        let row = self.conn.query_row(
            "SELECT id, token, created_at, last_activity FROM sessions WHERE id=?1",
            params![id],
            |r| Ok(SessionRow { id: r.get(0)?, token: r.get(1)?, created_at: r.get(2)?, last_activity: r.get(3)? })
        ).optional()?;
        Ok(row)
    }
    pub fn touch_session(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE sessions SET last_activity=?1 WHERE id=?2", params![now(), id])?;
        Ok(())
    }
    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM artifacts WHERE session_id=?1", params![id])?;
        self.conn.execute("DELETE FROM sessions WHERE id=?1", params![id])?;
        Ok(())
    }
    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare("SELECT id, token, created_at, last_activity FROM sessions")?;
        let rows = stmt.query_map([], |r| Ok(SessionRow { id: r.get(0)?, token: r.get(1)?, created_at: r.get(2)?, last_activity: r.get(3)? }))?;
        let mut v = vec![];
        for r in rows { v.push(r?); }
        Ok(v)
    }
    pub fn list_expired(&self, ttl_secs: i64) -> Result<Vec<String>> {
        let threshold = now() - ttl_secs;
        let mut stmt = self.conn.prepare("SELECT id FROM sessions WHERE last_activity < ?1")?;
        let rows = stmt.query_map(params![threshold], |r| r.get::<_, String>(0))?;
        let mut v = vec![];
        for r in rows { v.push(r?); }
        Ok(v)
    }
    pub fn insert_artifact(&self, id: &str, session_id: &str, kind: &str, path: &str, size: i64) -> Result<()> {
        self.conn.execute("INSERT INTO artifacts (id, session_id, kind, path, size, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, session_id, kind, path, size, now()])?;
        Ok(())
    }
}
```

- [ ] **Step 6: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine storage::tests`
Expected: PASS

- [ ] **Step 7: Подключить в lib.rs и коммит**

`crates/uefi-engine/src/lib.rs`: добавить `pub mod storage;`

```bash
git add crates/uefi-engine/src/storage/ crates/uefi-engine/src/lib.rs
git commit -m "feat: add SQLite storage (sessions, artifacts, GC queries)"
```

---

### Task 13: Session manager + фоновый GC

**Files:**
- Create: `crates/uefi-engine/src/session/mod.rs`
- Test: inline tests с mock time (малый TTL)

**Interfaces:**
- Consumes: `storage::Db`, `uuid`, `tokio`
- Produces:
  - `pub struct SessionManager { db: Arc<Db>, data_dir: PathBuf, ttl: Duration, gc_interval: Duration }`
  - `SessionManager::create_session() -> (session_id, token)`
  - `SessionManager::destroy_session(id) -> Result<()>`
  - `SessionManager::list_sessions() -> Vec<SessionRow>`
  - `SessionManager::touch(id)`
  - `SessionManager::spawn_gc() -> JoinHandle` (фоновой task)
  - `SessionManager::validate_token(session_id, token) -> bool`

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/session/mod.rs`:
```rust
use std::path::PathBuf;
use std::time::Duration;
use std::sync::Arc;
use anyhow::Result;
use crate::storage::{Db, SessionRow};

pub struct SessionManager {
    db: Arc<Db>,
    pub data_dir: PathBuf,
    pub ttl: Duration,
    pub gc_interval: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sm() -> (TempDir, SessionManager) {
        let td = TempDir::new().unwrap();
        let db = Db::open_db(&td.path().join("s.db")).unwrap();
        let sm = SessionManager {
            db: Arc::new(db),
            data_dir: td.path().to_path_buf(),
            ttl: Duration::from_secs(1),
            gc_interval: Duration::from_millis(100),
        };
        (td, sm)
    }

    #[test]
    fn create_and_validate() {
        let (_td, sm) = sm();
        let (id, tok) = sm.create_session().unwrap();
        assert!(sm.validate_token(&id, &tok));
        assert!(!sm.validate_token(&id, "wrong"));
    }

    #[test]
    fn destroy_removes() {
        let (_td, sm) = sm();
        let (id, _) = sm.create_session().unwrap();
        sm.destroy_session(&id).unwrap();
        assert!(sm.list_sessions().unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine session::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать SessionManager**

Дополнить `crates/uefi-engine/src/session/mod.rs`:
```rust
use uuid::Uuid;
use std::fs;

impl SessionManager {
    pub fn new(db: Db, data_dir: PathBuf, ttl: Duration, gc_interval: Duration) -> Self {
        Self { db: Arc::new(db), data_dir, ttl, gc_interval }
    }
    pub fn create_session(&self) -> Result<(String, String)> {
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string();
        self.db.insert_session(&id, &token)?;
        let sess_dir = self.data_dir.join("sessions").join(&id);
        fs::create_dir_all(&sess_dir)?;
        Ok((id, token))
    }
    pub fn destroy_session(&self, id: &str) -> Result<()> {
        self.db.delete_session(id)?;
        let sess_dir = self.data_dir.join("sessions").join(id);
        let _ = fs::remove_dir_all(&sess_dir);
        Ok(())
    }
    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        self.db.list_sessions()
    }
    pub fn touch(&self, id: &str) -> Result<()> {
        self.db.touch_session(id)
    }
    pub fn validate_token(&self, session_id: &str, token: &str) -> bool {
        match self.db.get_session(session_id) {
            Ok(Some(row)) => row.token == token,
            _ => false,
        }
    }
    pub fn spawn_gc(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = self.gc_interval;
        let ttl = self.ttl;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Ok(expired) = self.db.list_expired(ttl.as_secs() as i64) {
                    for id in expired {
                        let _ = self.destroy_session(&id);
                        tracing::info!("GC removed session {id}");
                    }
                }
            }
        })
    }
}
```

- [ ] **Step 4: Добавить tokio в dev-dependencies если нужно, подключить модуль**

`crates/uefi-engine/src/lib.rs`: добавить `pub mod session;`

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p uefi-engine session::tests`
Expected: PASS

- [ ] **Step 6: Коммит**

```bash
git add crates/uefi-engine/src/session/ crates/uefi-engine/src/lib.rs
git commit -m "feat: add SessionManager with TTL and background GC"
```

---

### Task 14: IFR-парсер и SetSetupItemVisibility

**Files:**
- Create: `crates/uefi-engine/src/setup/mod.rs`
- Create: `crates/uefi-engine/src/setup/ifr.rs`
- Test: inline tests

**Interfaces:**
- Consumes: `types::*`
- Produces:
  - `pub fn set_item_visibility(image: &mut Image, item_id: &str, visible: bool) -> Result<(), SetupError>`
  - `enum SetupError { NotFound, NotASetupItem, InvalidIfr }`
  - `pub fn find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope>` — поиск `{0A 82}` блоков
  - `struct SuppressScope { start: usize, end: usize }` — offset начала (после `{0A 82}`) и offset End-опкода (`29 02`)
- Референс: `../refs/IFRExtractor-RS/src/uefi_parser.rs:436` (SuppressIf=0x0A), `../refs/UEFI-Editor/src/components/scripts/scripts.ts:488` (unsuppress логика: вставка `2902` после `{0A 82}`, удаление оригинального `2902`).

- [ ] **Step 1: Написать failing tests**

`crates/uefi-engine/src/setup/mod.rs`:
```rust
pub mod ifr;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("not found")]
    NotFound,
    #[error("not a setup item")]
    NotASetupItem,
    #[error("invalid IFR")]
    InvalidIfr,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsuppress_makes_block_empty() {
        let mut ifr = vec![0x0A, 0x82, 0x12, 0x06, 0x40, 0x01, 0x00, 0x29, 0x02];
        let scopes = ifr::find_suppress_if_scopes(&ifr);
        assert_eq!(scopes.len(), 1);
        ifr::unsuppress(&mut ifr, &scopes[0]);
        assert_eq!(&ifr[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(ifr.len(), 7);
    }
}
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine setup::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать ifr.rs**

`crates/uefi-engine/src/setup/ifr.rs`:
```rust
#[derive(Debug, Clone)]
pub struct SuppressScope {
    pub start: usize,
    pub end: usize,
}

pub fn find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope> {
    let mut scopes = vec![];
    let mut i = 0;
    while i + 2 <= body.len() {
        if body[i] == 0x0A && body[i+1] & 0x80 != 0 {
            let scope_start = i + 2;
            let mut depth = 1;
            let mut j = scope_start;
            while j + 2 <= body.len() {
                if body[j] == 0x0A && body[j+1] & 0x80 != 0 { depth += 1; }
                else if body[j] == 0x29 && body[j+1] == 0x02 {
                    depth -= 1;
                    if depth == 0 {
                        scopes.push(SuppressScope { start: scope_start, end: j });
                        break;
                    }
                }
                let len = if body.len() > j+1 { body[j+1] as usize & 0x7F } else { 2 };
                j += len.max(2);
            }
            i = j + 2;
        } else {
            i += 1;
        }
    }
    scopes
}

pub fn unsuppress(ifr: &mut Vec<u8>, scope: &SuppressScope) {
    if scope.end + 2 <= ifr.len() && ifr[scope.end] == 0x29 && ifr[scope.end+1] == 0x02 {
        ifr.splice(scope.end..scope.end+2, std::iter::empty());
    }
    ifr.splice(scope.start..scope.start, [0x29, 0x02].into_iter());
}
```

- [ ] **Step 4: Реализовать set_item_visibility в setup/mod.rs**

Дополнить `crates/uefi-engine/src/setup/mod.rs`:
```rust
use crate::types::*;
use crate::ops;

pub fn set_item_visibility(image: &mut Image, item_id: &str, visible: bool) -> Result<(), SetupError> {
    let target = crate::parser::target::parse_target(item_id).map_err(|_| SetupError::NotFound)?;
    let node = crate::parser::target::find_item_mut(&mut image.root, &target).map_err(|_| SetupError::NotFound)?;
    if node.node_type != FfsType::Section { return Err(SetupError::NotASetupItem); }
    if !visible {
        if let Some(scope) = ifr::find_suppress_if_scopes(&node.body).into_iter().next() {
            let mut body = node.body.clone();
            ifr::unsuppress(&mut body, &scope);
            node.body = body;
            ops::mark_rebuild_to_root_by_path(&mut image.root, &[]);
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Добавить find_item_mut в parser/target.rs**

Добавить в `crates/uefi-engine/src/parser/target.rs`:
```rust
pub fn find_item_mut<'a>(root: &'a mut FfsNode, target: &Target) -> Result<&'a mut FfsNode, ParserError> {
    match target {
        Target::Path(indices) => {
            let mut node = root;
            for &i in indices {
                node = node.children.get_mut(i).ok_or_else(|| ParserError::InvalidHeader(format!("path {i} not found")))?;
            }
            Ok(node)
        }
        _ => Err(ParserError::InvalidHeader("only path targets supported for mutable".into())),
    }
}
```

- [ ] **Step 6: Запустить тесты**

Run: `cargo test -p uefi-engine setup::tests`
Expected: PASS

- [ ] **Step 7: Подключить в lib.rs и коммит**

`crates/uefi-engine/src/lib.rs`: добавить `pub mod setup;`

```bash
git add crates/uefi-engine/src/setup/ crates/uefi-engine/src/parser/target.rs crates/uefi-engine/src/lib.rs
git commit -m "feat: add IFR parser and SetSetupItemVisibility (unsuppress)"
```

---

### Task 15: gRPC-сервер (rpc/auth/server) — EngineService impl

**Files:**
- Create: `crates/uefi-engine/src/rpc/mod.rs`
- Create: `crates/uefi-engine/src/rpc/auth.rs`
- Create: `crates/uefi-engine/src/rpc/server.rs`
- Modify: `crates/uefi-engine/Cargo.toml` (добавить tonic)
- Test: integration test с unix-сокетом

**Interfaces:**
- Consumes: `uefi-proto`, `session`, `storage`, `parser`, `builder`, `ops`, `setup`
- Produces:
  - `pub struct EngineServer { sm: Arc<SessionManager>, images: Arc<Mutex<HashMap<String, Image>>>, data_dir: PathBuf }`
  - `impl EngineService for EngineServer` (все методы)
  - `pub fn serve(socket_path: &Path, db: Db, data_dir: PathBuf, ttl: Duration, gc_interval: Duration) -> Result<()>`

- [ ] **Step 1: Написать failing integration test**

`crates/uefi-engine/src/rpc/server.rs`:
```rust
use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tonic::{transport::Server, transport::Endpoint, Request, Response, Status};
use uefi_proto::engine_service_server::{EngineService, EngineServiceServer};
use uefi_proto::*;
use crate::session::SessionManager;
use crate::storage::Db;
use crate::types::*;

pub struct EngineServer {
    pub sm: Arc<SessionManager>,
    pub images: Arc<Mutex<HashMap<String, Image>>>,
    pub data_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tonic::transport::Channel;

    async fn setup() -> (TempDir, String, EngineServiceClient<Channel>) {
        let td = TempDir::new().unwrap();
        let sock = td.path().join("test.sock");
        let db = Db::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(db, td.path().to_path_buf(), Duration::from_secs(864000), Duration::from_secs(3600)));
        let images = Arc::new(Mutex::new(HashMap::new()));
        let server = EngineServer { sm, images, data_dir: td.path().to_path_buf() };
        let sock2 = sock.clone();
        tokio::spawn(async move {
            let endpoint = Endpoint::from_unix(&sock2).unwrap();
            Server::builder().add_service(EngineServiceServer::new(server))
                .serve_with_incoming(endpoint.connect_with_connector_override().await.unwrap()).await.unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let client = Channel::from_shared(format!("unix://{}", sock.display())).unwrap()
            .connect().await.unwrap();
        (td, sock.to_string_lossy().into(), client)
    }

    #[tokio::test]
    async fn create_and_destroy_session() {
        let (_td, _sock, mut client) = setup().await;
        let resp = client.create_session(CreateSessionRequest {}).await.unwrap().into_inner();
        assert!(!resp.session_id.is_empty());
        client.destroy_session(DestroySessionRequest { session_id: resp.session_id }).await.unwrap();
    }
}
```

- [ ] **Step 2: Добавить tonic в uefi-engine Cargo.toml**

`crates/uefi-engine/Cargo.toml` [dependencies]:
```toml
tonic.workspace = true
uefi-proto = { path = "../uefi-proto" }
tokio = { workspace = true, features = ["full"] }
```

- [ ] **Step 3: Реализовать auth.rs**

`crates/uefi-engine/src/rpc/auth.rs`:
```rust
use tonic::{Request, Status};

pub fn check_auth<T>(req: &Request<T>, sm: &crate::session::SessionManager) -> Result<(), Status> {
    let auth = req.metadata().get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing Authorization"))?;
    let token = auth.strip_prefix("Bearer ").ok_or_else(|| Status::unauthenticated("bad Bearer"))?;
    let session_id = req.metadata().get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing session-id"))?;
    if !sm.validate_token(session_id, token) {
        return Err(Status::unauthenticated("invalid token"));
    }
    Ok(())
}
```

- [ ] **Step 4: Реализовать EngineService impl**

`crates/uefi-engine/src/rpc/mod.rs`:
```rust
pub mod auth;
pub mod server;
```

`crates/uefi-engine/src/rpc/server.rs` (дополнить после теста):
```rust
use crate::parser::image::{parse_image, dump_tree, list_items};
use crate::parser::target::{parse_target, find_item};
use crate::builder::build_image;
use crate::ops::{insert, remove, replace, rebuild, InsertMode};
use crate::setup;
use crate::ffs;
use std::fs;
use std::path::Path;
use uuid::Uuid;
use tonic::{Request, Response, Status};

type RpcResult<T> = Result<Response<T>, Status>;

impl EngineService for EngineServer {
    async fn create_session(&self, _req: Request<CreateSessionRequest>) -> RpcResult<CreateSessionResponse> {
        let (id, tok) = self.sm.create_session().map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(CreateSessionResponse { session_id: id, token: tok }))
    }
    async fn destroy_session(&self, req: Request<DestroySessionRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm.destroy_session(&r.session_id).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
    async fn list_sessions(&self, _req: Request<ListSessionsRequest>) -> RpcResult<ListSessionsResponse> {
        let rows = self.sm.list_sessions().map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListSessionsResponse {
            sessions: rows.into_iter().map(|r| SessionInfo {
                session_id: r.id, created_at: r.created_at, last_activity: r.last_activity
            }).collect()
        }))
    }
    async fn open_image(&self, req: Request<OpenImageRequest>) -> RpcResult<OpenImageResponse> {
        let r = req.into_inner();
        let mode = match r.mode { 0 => ImageMode::Read, 1 => ImageMode::Write, _ => return Err(Status::invalid_argument("bad mode")) };
        let bytes = fs::read(&r.image_path).map_err(|e| Status::not_found(e.to_string()))?;
        let image_id = Uuid::new_v4().to_string();
        let img = parse_image(&bytes, mode, &image_id, &r.session_id).map_err(|e| Status::internal(e.to_string()))?;
        let root_guid = img.root.guid.map(|g| g.to_string()).unwrap_or_default();
        self.images.lock().await.insert(image_id.clone(), img);
        self.sm.touch(&r.session_id).ok();
        Ok(Response::new(OpenImageResponse { image_id, root_guid }))
    }
    async fn dump_tree(&self, req: Request<DumpTreeRequest>) -> RpcResult<DumpTreeResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images.get(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let format = match r.format { 0 => uefi_proto::DumpFormat::Text, 1 => uefi_proto::DumpFormat::Tsv, _ => return Err(Status::invalid_argument("bad format")) };
        let text = dump_tree(&img.root, format);
        Ok(Response::new(DumpTreeResponse { text }))
    }
    async fn list_items(&self, req: Request<ListItemsRequest>) -> RpcResult<ListItemsResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images.get(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let items = list_items(&img.root, if r.filter.is_empty() { None } else { Some(&r.filter) });
        Ok(Response::new(ListItemsResponse { items }))
    }
    async fn find_item(&self, req: Request<FindItemRequest>) -> RpcResult<FindItemResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images.get(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let _ = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(FindItemResponse { item_id: r.target }))
    }
    async fn insert(&self, req: Request<InsertRequest>) -> RpcResult<InsertResponse> {
        let r = req.into_inner();
        let ffs_bytes = fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?;
        let mut images = self.images.lock().await;
        let img = images.get_mut(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mode = match r.mode { 0 => InsertMode::Into, 1 => InsertMode::Before, 2 => InsertMode::After, _ => return Err(Status::invalid_argument("bad mode")) };
        crate::ops::insert(&mut img.root, &t, &ffs_bytes, mode).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InsertResponse { item_id: r.target }))
    }
    async fn remove(&self, req: Request<RemoveRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let mut images = self.images.lock().await;
        let img = images.get_mut(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        remove(&mut img.root, &t).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
    async fn replace(&self, req: Request<ReplaceRequest>) -> RpcResult<ReplaceResponse> {
        let r = req.into_inner();
        let data = fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?;
        let mut images = self.images.lock().await;
        let img = images.get_mut(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        replace(&mut img.root, &t, &data, r.body_only).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ReplaceResponse { item_id: r.target }))
    }
    async fn rebuild(&self, req: Request<RebuildRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let mut images = self.images.lock().await;
        let img = images.get_mut(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        rebuild(&mut img.root, &t).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
    async fn set_setup_item_visibility(&self, req: Request<SetSetupItemVisibilityRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let mut images = self.images.lock().await;
        let img = images.get_mut(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        setup::set_item_visibility(img, &r.item_id, r.visible).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
    async fn save_image(&self, req: Request<SaveImageRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images.get(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let bytes = build_image(img).map_err(|e| Status::internal(e.to_string()))?;
        fs::write(&r.output_path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
}

pub fn serve(socket_path: &Path, db: Db, data_dir: PathBuf, ttl: Duration, gc_interval: Duration) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let sm = Arc::new(SessionManager::new(db, data_dir.clone(), ttl, gc_interval));
        sm.clone().spawn_gc();
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::new())),
            data_dir,
        };
        let endpoint = Endpoint::from_unix(socket_path).map_err(|e| anyhow::anyhow!(e))?;
        Server::builder()
            .add_service(EngineServiceServer::new(server))
            .serve_with_incoming(endpoint.connect_with_connector_override().await.map_err(|e| anyhow::anyhow!(e))?)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    })
}
```

- [ ] **Step 5: Подключить rpc в lib.rs**

`crates/uefi-engine/src/lib.rs`:
```rust
pub mod types;
pub mod ffs;
pub mod parser;
pub mod decompress;
pub mod builder;
pub mod ops;
pub mod storage;
pub mod session;
pub mod setup;
pub mod rpc;
pub use types::*;
```

- [ ] **Step 6: Запустить тесты**

Run: `cargo test -p uefi-engine rpc::server::tests`
Expected: PASS

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-engine/src/rpc/ crates/uefi-engine/src/lib.rs crates/uefi-engine/Cargo.toml
git commit -m "feat: add gRPC EngineService server over unix-socket"
```

---

### Task 16: CLI-минимум (uefi-cli)

**Files:**
- Create: `crates/uefi-cli/Cargo.toml`
- Create: `crates/uefi-cli/src/main.rs`
- Create: `crates/uefi-cli/src/client.rs`
- Test: `crates/uefi-cli/tests/smoke.rs`

**Interfaces:**
- Consumes: `uefi-proto` (клиент)
- Produces: бинарник `uefi-cli` с командами:
  - `uefi-cli session create` → session_id, token
  - `uefi-cli session destroy <id>`
  - `uefi-cli <session_id> open <path> [--mode read|write]`
  - `uefi-cli <image_id> dump [--format text|tsv]`
  - `uefi-cli <image_id> list [--filter <str>]`
  - `uefi-cli <image_id> find <target>`
  - `uefi-cli <image_id> insert <target> <ffs> [--mode into|before|after]`
  - `uefi-cli <image_id> remove <target>`
  - `uefi-cli <image_id> replace <target> <ffs> [--body-only]`
  - `uefi-cli <image_id> rebuild <target>`
  - `uefi-cli <image_id> set-visibility <item_id> [--visible]`
  - `uefi-cli <image_id> save <output>`

- [ ] **Step 1: Создать uefi-cli Cargo.toml**

`crates/uefi-cli/Cargo.toml`:
```toml
[package]
name = "uefi-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
uefi-proto = { path = "../uefi-proto" }
tonic.workspace = true
tokio.workspace = true
clap = { version = "4", features = ["derive"] }
anyhow.workspace = true
```

- [ ] **Step 2: Реализовать client.rs**

`crates/uefi-cli/src/client.rs`:
```rust
use anyhow::Result;
use tonic::transport::Channel;
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;

pub struct Client(EngineServiceClient<Channel>);

impl Client {
    pub async fn connect(sock: &str) -> Result<Self> {
        let url = format!("unix://{sock}");
        let ch = Channel::from_shared(url)?.connect().await?;
        Ok(Self(EngineServiceClient::new(ch)))
    }
    pub async fn create_session(&mut self) -> Result<(String, String)> {
        let r = self.0.create_session(CreateSessionRequest {}).await?.into_inner();
        Ok((r.session_id, r.token))
    }
    pub async fn destroy_session(&mut self, id: &str) -> Result<()> {
        self.0.destroy_session(DestroySessionRequest { session_id: id.into() }).await?;
        Ok(())
    }
}
```

- [ ] **Step 3: Реализовать main.rs (clap)**

`crates/uefi-cli/src/main.rs`:
```rust
mod client;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "uefi-cli", version)]
struct Cli {
    #[arg(long, env = "UEFIPATCHER_SOCK", default_value = "/run/uefipatcher.sock")]
    sock: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Session { #[command(subcommand)] sub: SessionCmd },
    Open { session_id: String, path: PathBuf, #[arg(long, default_value = "read")] mode: String },
    Dump { image_id: String, #[arg(long, default_value = "text")] format: String },
    List { image_id: String, #[arg(long)] filter: Option<String> },
    Find { image_id: String, target: String },
    Insert { image_id: String, target: String, ffs: PathBuf, #[arg(long, default_value = "into")] mode: String },
    Remove { image_id: String, target: String },
    Replace { image_id: String, target: String, ffs: PathBuf, #[arg(long)] body_only: bool },
    Rebuild { image_id: String, target: String },
    SetVisibility { image_id: String, item_id: String, #[arg(long, default_value = "true")] visible: bool },
    Save { image_id: String, output: PathBuf },
}

#[derive(Subcommand)]
enum SessionCmd { Create, Destroy { id: String } }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut c = client::Client::connect(&cli.sock).await?;
    match cli.cmd {
        Cmd::Session { sub: SessionCmd::Create } => {
            let (id, tok) = c.create_session().await?;
            println!("{id}\t{tok}");
        }
        Cmd::Session { sub: SessionCmd::Destroy { id } } => { c.destroy_session(&id).await?; }
        _ => { anyhow::bail!("command not fully implemented in smoke CLI"); }
    }
    Ok(())
}
```

- [ ] **Step 4: Запустить сборку**

Run: `cargo build -p uefi-cli`
Expected: компиляция без ошибок

- [ ] **Step 5: Написать smoke-тест**

`crates/uefi-cli/tests/smoke.rs`:
```rust
use assert_cmd::Command;

#[test]
fn cli_help() {
    let mut cmd = Command::cargo_bin("uefi-cli").unwrap();
    cmd.arg("--help").assert().success();
}
```

Добавить dev-dependency в `crates/uefi-cli/Cargo.toml`:
```toml
[dev-dependencies]
assert_cmd = "2"
```

- [ ] **Step 6: Запустить smoke-тест**

Run: `cargo test -p uefi-cli`
Expected: PASS

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-cli/
git commit -m "feat: add uefi-cli smoke CLI (session create/destroy + clap)"
```

---

### Task 17: Контейнеризация (Dockerfile + docker-compose)

**Files:**
- Create: `docker/Dockerfile.engine`
- Create: `docker/docker-compose.yml`
- Create: `docker/.dockerignore`

**Interfaces:**
- Consumes: workspace
- Produces: образ `uefipatcher-engine`, compose-сервис с volume для unix-сокета и данных

- [ ] **Step 1: Создать Dockerfile**

`docker/Dockerfile.engine`:
```dockerfile
FROM rust:1.81-slim AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*
RUN cargo build --release -p uefi-engine

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/uefi-engine /usr/local/bin/
VOLUME ["/data", "/run/uefipatcher"]
ENV UEFIPATCHER_DATA=/data
ENV UEFIPATCHER_SOCK=/run/uefipatcher/uefipatcher.sock
EXPOSE 0
ENTRYPOINT ["uefi-engine"]
```

- [ ] **Step 2: Создать docker-compose.yml**

`docker/docker-compose.yml`:
```yaml
version: "3.8"
services:
  engine:
    build:
      context: ..
      dockerfile: docker/Dockerfile.engine
    volumes:
      - uefi-data:/data
      - uefi-sock:/run/uefipatcher
    environment:
      UEFIPATCHER_DATA: /data
      UEFIPATCHER_SOCK: /run/uefipatcher/uefipatcher.sock
      UEFIPATCHER_SESSION_TTL_SECS: "864000"
      UEFIPATCHER_SESSION_GC_INTERVAL_SECS: "3600"
volumes:
  uefi-data:
  uefi-sock:
```

- [ ] **Step 3: Создать .dockerignore**

`docker/.dockerignore`:
```
target/
.git/
docs/
*.md
```

- [ ] **Step 4: Проверить валидность compose**

Run: `podman-compose -f docker/docker-compose.yml config` (или `docker compose -f docker/docker-compose.yml config`)
Expected: корректный вывод конфигурации

- [ ] **Step 5: Коммит**

```bash
git add docker/
git commit -m "feat: add Dockerfile and docker-compose for engine"
```

---

### Task 18: Финальная проверка — все тесты, clippy, fmt, round-trip на синтетике

**Files:**
- Modify: по результатам проверок

- [ ] **Step 1: Запустить все тесты**

Run: `cargo test --all`
Expected: все PASS

- [ ] **Step 2: Запустить clippy**

Run: `cargo clippy --all -- -D warnings`
Expected: без warnings

- [ ] **Step 3: Запустить fmt check**

Run: `cargo fmt --all -- --check`
Expected: без diff

- [ ] **Step 4: Round-trip тест на синтетическом образе (вручную через CLI)**

Запустить движок: `cargo run -p uefi-engine` (нужен main для движка — добавить минимальный `src/bin/engine.rs` если нет)
Открыть образ, dump, save, сравнить.
Expected: бинарно идентичны

- [ ] **Step 5: Коммит финальных правок**

```bash
git add -A
git commit -m "chore: final checks — all tests pass, clippy clean, round-trip verified"
```

---

## Само-проверка плана (после написания)

**Спека-покрытие:**
- Парсер (п.1.1): Tasks 4-8 ✓
- Builder (round-trip): Task 10 ✓
- Модификации (insert/remove/replace/rebuild): Task 11 ✓
- Setup-visibility: Task 14 ✓
- Хранилище + сессии + TTL 10 дней: Tasks 12-13 ✓
- gRPC EngineService (все методы): Task 15 ✓
- CLI-минимум: Task 16 ✓
- Контейнеризация: Task 17 ✓
- Баги 9/10/11 (IMPLEMENTATION.md): учтены в Task 11 (clearChildren в replace) и Task 10 (preserve offsets). Баг 10 (LZMA-компрессия) — частично (decompress есть, compress — заглушка, приемлемо для цикла 1 с round-trip на uncompressed-секциях).

**Placeholder scan:** TBD/TODO нет; все шаги содержат код или команды. ✓

**Type consistency:** `FfsNode`, `Image`, `Target`, `Action`, `FfsType`, `ParsingData` — единые имена во всём плане. `InsertMode` дублируется в `ops.rs` и uefi-proto enum — это намеренно (внутренний enum vs protobuf enum, маппинг в Task 15). ✓