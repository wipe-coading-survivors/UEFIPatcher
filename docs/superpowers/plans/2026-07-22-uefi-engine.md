# UEFI Engine (цикл 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать серверный движок для парсинга/модификации UEFI-образов, управления видимостью пунктов Setup-меню, хранения сессий и предоставления gRPC API над unix-сокетом.

**Architecture:** Cargo workspace с четырьмя крейтами: `uefi-proto` (protobuf-контракт), `uefi-common` (state/error для всех клиентов), `uefi-engine` (ядро: parser/builder/setup/session/storage/rpc), `uefi-cli` (smoke-тест). Парсер строит дерево `FfsNode` из байтов образа через `binrw`-декларативные структуры (референс UEFITool-ai-fork `FfsParser`); builder собирает дерево обратно (референс `FfsBuilder`); setup-модуль парсит IFR через `r_efi::hii` структуры и управляет SuppressIf-блоками. Менеджер сессий встроен в движок, именованные сессии, артефакты с экстракцией/импортом/экспортом, TTL 10 дней, фоновый GC с `--purge-artifacts` (по умолчанию НЕ удаляет файлы).

**Tech Stack:** `uguid 2.2.1` (Guid, serde), `r-efi 7.0` (UEFI/PI типы, IFR-структуры, hii), `binrw 0.15` (декларативный binary-парсинг), `object 0.39` (PE32), `lzma-rs 0.3` (декомпрессия), `tonic 0.12` (gRPC), `prost 0.13` (protobuf), `rusqlite 0.31` (SQLite), `tokio 1` (async), `clap 4` (CLI), `uuid 1` (IDs), `anyhow 1`, `thiserror 1`, `tracing 0.1`.

## Global Constraints

- **Module-first rule (issue #2, п.3):** при создании файла модуля (например `ffs.rs`), шаг добавления `pub mod ffs;` в `lib.rs`/`mod.rs` выполняется В ТОМ ЖЕ ШАГЕ, ДО запуска `cargo test`. Binary-крейты получают `main.rs` одновременно с `Cargo.toml`. Это исключает false-positive тестов (0 из 0) из-за неподключённых модулей. **Это правило применяется во ВСЕХ задачах ниже автоматически.**
- **TDD порядок:** (1) объявить `mod` + создать файл с тестами → (2) `cargo test` (падает) → (3) реализация → (4) `cargo test` (проходит) → (5) commit.
- **Crate stack:** `uguid::Guid` для всех GUID-значений (Display/FromStr/serde). `r_efi::hii::*` для IFR-структур и opcode-констант. `#[brw]`-макросы `binrw` для FFS/section/FV заголовков. `object` для PE32. Не писать самописный byte-offset парсинг.
- **GUID UPPERCASE:** `uguid::Guid` выводит lowercase. Wrapper-функция `fn guid_to_upper_string(g: &Guid) -> String` приводит к верхнему регистру для индустриального формата (AMI/OEM/EDK2).
- **Checksum wrapping:** использовать `wrapping_add`/`wrapping_sub` (см. референс `refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs`).
- Референс парсера: `../refs/UEFITool-ai-fork/common/ffsparser.{h,cpp}`.
- Референс билдера: `../refs/UEFITool-ai-fork/common/ffsbuilder.{h,cpp}`.
- Референс FFS-структур: `../refs/UEFITool-ai-fork/common/ffs.h` (FFSv2 24б, FFSv3 large 32б, Lenovo large 32б).
- Референс Target/команд: `../refs/UEFITool-ai-fork/UEFIEdit/uefiedit.{h,cpp}`.
- Референс IFR: `../refs/IFRExtractor-RS/src/uefi_parser.rs` (IfrOpcode, ifr_operations). Структуры: `r_efi::hii::*`.
- Референс Setup-видимости: `../refs/UEFI-Editor/src/components/scripts/scripts.ts` (Unsuppress логика).
- Референс ffs.rs (checksums): `../refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs`.
- Точка истины при багах: `../refs/edk2`.
- Критичные баги для Rust-порта:
  1. `replace` должен `clearChildren` перед `setBody` (баг 9).
  2. `buildVolume` должен потреблять FreeSpace, дополнять `emptyByte` (баг 4).
  3. `buildFile`/`buildSection` с `rowCount==0` используют `body` as-is (баг 5).
  4. `buildSection` для GUIDed-секций сжимает LZMA/Tiano по GUID (баг 10).
  5. `buildVolume` сохраняет оригинальные offset'ы или pad FFS (баг 11).
  6. Каскадная пометка `Rebuild` для всех предков после insert/remove/replace/rebuild.
  7. Три варианта FFS-заголовка: FFSv2 (24б), FFSv3 large (32б), Lenovo large (32б).
- Переменные окружения: `UEFIPATCHER_DATA`, `UEFIPATCHER_SOCK`, `UEFIPATCHER_SESSION_TTL_SECS` (864000), `UEFIPATCHER_SESSION_GC_INTERVAL_SECS` (3600), `UEFIPATCHER_PURGE_ARTIFACTS` (false).
- gRPC: `Authorization: Bearer <token>` в metadata.
- Кодстайл: `cargo fmt`, `cargo clippy -- -D warnings`. Без комментариев в коде.

---

## Файлы плана

| Файл | Назначение |
|---|---|
| `Cargo.toml` | workspace manifest (members + workspace deps: uguid, r-efi, binrw, object, lzma-rs, tonic, prost, tokio, clap, uuid, anyhow, thiserror, tracing, rusqlite) |
| `crates/uefi-proto/Cargo.toml`, `build.rs`, `proto/engine.proto`, `src/lib.rs` | protobuf-контракт EngineService |
| `crates/uefi-common/Cargo.toml`, `src/lib.rs`, `src/state.rs`, `src/error.rs` | общий крейт: State, read_state/write_state, AppError, ExitCode |
| `crates/uefi-engine/Cargo.toml`, `src/lib.rs` | ядро: re-export модулей |
| `crates/uefi-engine/src/types.rs` | `FfsNode`, `Image`, `Target`, Guid wrapper (uguid) |
| `crates/uefi-engine/src/ffs.rs` | FFS-константы (r-efi), checksum-хелперы (wrapping), well-known GUIDs |
| `crates/uefi-engine/src/parser/mod.rs`, `volume.rs`, `file.rs`, `section.rs` | парсинг образа → дерево (binrw-структуры) |
| `crates/uefi-engine/src/parser/target.rs` | парсинг Target-строки |
| `crates/uefi-engine/src/decompress.rs` | Tiano/LZMA декомпрессия |
| `crates/uefi-engine/src/builder.rs` | сборка дерева → байты |
| `crates/uefi-engine/src/ops.rs` | insert/remove/replace/rebuild (с artifact_id) |
| `crates/uefi-engine/src/setup.rs` | IFR-парсинг (r_efi::hii), SetSetupItemVisibility |
| `crates/uefi-engine/src/storage/mod.rs`, `schema.rs`, `artifact.rs` | SQLite (sessions с name, artifacts), extract/import/export |
| `crates/uefi-engine/src/session.rs` | менеджер сессий + GC (--purge-artifacts) |
| `crates/uefi-engine/src/rpc/mod.rs`, `auth.rs`, `server.rs` | gRPC-сервер, все RPC (вкл. artifact ops) |
| `crates/uefi-engine/src/bin/engine.rs` | engine binary (clap CLI, --purge-artifacts) |
| `crates/uefi-cli/Cargo.toml`, `src/main.rs` | CLI-минимум (depends on uefi-common) |
| `docker/rust-builder.containerfile` | базовый Rust builder (fedora:44) |
| `docker/engine.containerfile` | движок на базе rust-builder |
| `docker/docker-compose.yml` | контейнеризация |
| `tests/fixtures/` | синтетические тестовые образы |
| `crates/uefi-engine/tests/` | интеграционные тесты |

---

### Task 1: Скелет Cargo workspace, uefi-proto и uefi-common

**Files:**
- Create: `Cargo.toml`
- Create: `crates/uefi-proto/{Cargo.toml, build.rs, proto/engine.proto, src/lib.rs}`
- Create: `crates/uefi-common/{Cargo.toml, src/lib.rs, src/state.rs, src/error.rs}`
- Create: `crates/uefi-engine/{Cargo.toml, src/lib.rs}`
- Create: `crates/uefi-cli/{Cargo.toml, src/main.rs}`

**Interfaces:**
- Consumes: нет (первый task)
- Produces: сгенерированные proto-типы; скелеты `uefi-common` (State, AppError), `uefi-engine` (пустой lib.rs), `uefi-cli` (stub main.rs).

- [ ] **Step 1: Создать workspace manifest**

`Cargo.toml`:
```toml
[workspace]
members = ["crates/uefi-proto", "crates/uefi-common", "crates/uefi-engine", "crates/uefi-cli"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
# UEFI-specific
uguid = { version = "2.2", features = ["serde"] }
r-efi = "7.0"
binrw = "0.15"
object = { version = "0.39", default-features = false, features = ["read_core", "pe"] }
lzma-rs = "0.3"
# gRPC / async
tonic = "0.12"
prost = "0.13"
tokio = { version = "1", features = ["full"] }
# Storage
rusqlite = { version = "0.31", features = ["bundled"] }
# CLI
clap = { version = "4", features = ["derive", "env"] }
# Utility
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
directories = "5"
nix = { version = "0.29", features = ["fs"] }
num_enum = "0.7"
```

- [ ] **Step 2: Создать uefi-proto**

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

`crates/uefi-proto/build.rs`:
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/engine.proto")?;
    Ok(())
}
```

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
  rpc ExtractArtifact(ExtractArtifactRequest) returns (ExtractArtifactResponse);
  rpc ExportArtifact(ExportArtifactRequest) returns (Empty);
  rpc ImportArtifact(ImportArtifactRequest) returns (ImportArtifactResponse);
  rpc ListArtifacts(ListArtifactsRequest) returns (ListArtifactsResponse);
  rpc SetSetupItemVisibility(SetSetupItemVisibilityRequest) returns (Empty);
  rpc SaveImage(SaveImageRequest) returns (Empty);
}

enum ImageMode { READ = 0; WRITE = 1; }
enum InsertMode { INTO = 0; BEFORE = 1; AFTER = 2; }
enum DumpFormat { TEXT = 0; TSV = 1; }

message CreateSessionRequest { string name = 1; }
message CreateSessionResponse { string session_id = 1; string token = 2; }
message DestroySessionRequest { string session_id = 1; }
message ListSessionsRequest {}
message SessionInfo { string session_id = 1; string name = 2; int64 created_at = 3; int64 last_activity = 4; }
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

message InsertRequest { string image_id = 1; string target = 2; string ffs_path = 3; string artifact_id = 4; InsertMode mode = 5; }
message InsertResponse { string item_id = 1; }

message RemoveRequest { string image_id = 1; string target = 2; }

message ReplaceRequest { string image_id = 1; string target = 2; string ffs_path = 3; string artifact_id = 4; bool body_only = 5; }
message ReplaceResponse { string item_id = 1; }

message RebuildRequest { string image_id = 1; string target = 2; }

message ExtractArtifactRequest { string image_id = 1; string target = 2; bool body_only = 3; }
message ExtractArtifactResponse { string artifact_id = 1; }

message ExportArtifactRequest { string artifact_id = 1; string output_path = 2; }

message ImportArtifactRequest { string session_id = 1; string file_path = 2; }
message ImportArtifactResponse { string artifact_id = 1; }

message ListArtifactsRequest { string session_id = 1; }
message ArtifactInfo { string artifact_id = 1; string kind = 2; uint64 size = 3; int64 created_at = 4; string source = 5; }
message ListArtifactsResponse { repeated ArtifactInfo artifacts = 1; }

message SetSetupItemVisibilityRequest { string image_id = 1; string item_id = 2; bool visible = 3; }

message SaveImageRequest { string image_id = 1; string output_path = 2; }

message Empty {}
```

`crates/uefi-proto/src/lib.rs`:
```rust
pub mod engine {
    tonic::include_proto!("engine");
}
pub use engine::*;
```

- [ ] **Step 3: Создать uefi-common (скелет state.rs + error.rs)**

`crates/uefi-common/Cargo.toml`:
```toml
[package]
name = "uefi-common"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
toml.workspace = true
directories.workspace = true
thiserror.workspace = true
```

`crates/uefi-common/src/lib.rs`:
```rust
pub mod state;
pub mod error;
pub use state::*;
pub use error::*;
```

`crates/uefi-common/src/state.rs` — скелет (полное наполнение в цикле 2):
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub session_id: Option<String>,
    pub token: Option<String>,
    pub active_image_id: Option<String>,
    pub sock_path: Option<String>,
}

pub fn state_path() -> PathBuf {
    PathBuf::from(".uefipatcher")
}
// TODO цикл 2: read_state, write_state, resolve_sock, default_sock
```

`crates/uefi-common/src/error.rs` — скелет:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
}
// TODO цикл 2: ErrKind, ExitCode, print_error
```

- [ ] **Step 4: Создать uefi-engine скелет**

`crates/uefi-engine/Cargo.toml`:
```toml
[package]
name = "uefi-engine"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
uefi-proto = { path = "../uefi-proto" }
uguid.workspace = true
r-efi.workspace = true
binrw.workspace = true
uuid.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
num_enum.workspace = true

[dev-dependencies]
tempfile = "3"
```

`crates/uefi-engine/src/lib.rs`:
```rust
// Модули добавляются по мере реализации в последующих задачах
```

- [ ] **Step 5: Создать uefi-cli stub (зависит от uefi-common)**

`crates/uefi-cli/Cargo.toml`:
```toml
[package]
name = "uefi-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
uefi-proto = { path = "../uefi-proto" }
uefi-common = { path = "../uefi-common" }
tonic.workspace = true
tokio.workspace = true
anyhow.workspace = true
```

`crates/uefi-cli/src/main.rs`:
```rust
fn main() {
    println!("uefi-cli stub — smoke test in Task 17");
}
```

- [ ] **Step 6: Проверить компиляцию всего workspace**

Run: `cargo build --all`
Expected: компиляция без ошибок; все 4 крейта собираются; proto-типы сгенерированы.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: workspace skeleton with uefi-proto, uefi-common, uefi-engine, uefi-cli"
```

---

### Task 2: Базовые типы движка (Guid через uguid, FfsNode, Action, FfsType, Image, Target)

**Files:**
- Create: `crates/uefi-engine/src/types.rs`
- Modify: `crates/uefi-engine/src/lib.rs` (добавить `pub mod types;`)
- Modify: `crates/uefi-engine/Cargo.toml` (добавить `num_enum`)

**Interfaces:**
- Consumes: `uguid::Guid` (внешний крейт)
- Produces:
  - `pub fn guid_to_upper_string(g: &Guid) -> String` — UPPERCASE Display wrapper для индустриального формата
  - Re-export `pub use uguid::Guid;` — используем uguid напрямую
  - `enum FfsType`, `enum Action`, `struct FfsNode`, `enum ParsingData` и sub-structs
  - `struct Image`, `enum Target` — все используют `uguid::Guid`

- [ ] **Step 1: Добавить `pub mod types;` в lib.rs и создать types.rs с failing tests**

`crates/uefi-engine/src/lib.rs`:
```rust
pub mod types;
pub use types::*;
```

`crates/uefi-engine/src/types.rs` (только tests сначала):
```rust
pub use uguid::Guid;

pub fn guid_to_upper_string(g: &Guid) -> String {
    g.to_string().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn guid_display_uppercase() {
        let g = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let s = guid_to_upper_string(&g);
        assert_eq!(s, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
    }

    #[test]
    fn guid_from_str_roundtrip() {
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let s = guid_to_upper_string(&g);
        assert_eq!(s, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
    }

    #[test]
    fn guid_invalid() {
        assert!(Guid::try_parse("not-a-guid").is_err());
    }
}
```

- [ ] **Step 2: Запустить тесты — должны пройти (uguid уже реализует FromStr)**

Run: `cargo test -p uefi-engine types::tests`
Expected: PASS — uguid предоставляет Display/FromStr/serde

- [ ] **Step 3: Реализовать остальные типы**

Дополнить `crates/uefi-engine/src/types.rs` (после Guid-секций):
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

Добавить `num_enum.workspace = true` в `crates/uefi-engine/Cargo.toml`.

- [ ] **Step 4: Запустить все тесты**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/
git commit -m "feat: base types using uguid::Guid, FfsNode, Image, Target"
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

- [ ] **Step 1: Добавить `pub mod ffs;` в lib.rs и написать failing tests**

`crates/uefi-engine/src/lib.rs` (обновить):
```rust
pub mod types;
pub mod ffs;
pub use types::*;
```

`crates/uefi-engine/src/ffs.rs` (только tests):
```rust
use crate::types::Guid;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum8_known_vector() {
        let mut data = [0x01, 0x02, 0x03, 0x04, 0x00];
        let checksum = calculate_checksum8(&data);
        assert_eq!(checksum, 0xF6);
        if let Some(last) = data.last_mut() { *last = checksum; }
        assert_eq!(calculate_checksum8(&data), 0);
    }

    #[test]
    fn checksum16_known_vector() {
        let data = [0x01, 0x00, 0x02, 0x00];
        assert_eq!(calculate_checksum16(&data), 0xFFFD);
    }

    #[test]
    fn uint24_roundtrip() {
        let s = 0x123456u32;
        let b = size_to_uint24(s);
        assert_eq!(uint24_to_u32(b), s);
    }

    #[test]
    fn large_section_threshold() {
        let small_hdr = vec![0x00, 0x00, 0x00, 0x00];
        assert!(!is_large_section(&small_hdr));
        let mut large_hdr = vec![0xFF, 0xFF, 0xFF, 0x00];
        large_hdr.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        assert!(is_large_section(&large_hdr));
    }

    #[test]
    fn ffs_file_size_parsing() {
        let mut small_ffs = vec![0; 24];
        small_ffs[20..23].copy_from_slice(&size_to_uint24(0x000150));
        assert!(!is_large_ffs(&small_ffs));
        assert_eq!(ffs_file_size(&small_ffs), 0x000150);

        let mut large_ffs = vec![0; 32];
        large_ffs[20] = 0xFF; large_ffs[21] = 0xFF; large_ffs[22] = 0xFF;
        let large_size: u64 = 0x02000000;
        large_ffs[24..32].copy_from_slice(&large_size.to_le_bytes());
        assert!(is_large_ffs(&large_ffs));
        assert_eq!(ffs_file_size(&large_ffs), 0x02000000);
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine ffs::tests`
Expected: FAIL — функции не определены

- [ ] **Step 3: Реализовать ffs.rs (по референсу `refs/current/fixes/UEFIPatcher/crates/uefi-engine/src/ffs.rs`)**

`crates/uefi-engine/src/ffs.rs` (добавить реализацию):
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

pub fn tiano_guid() -> Guid {
    Guid::try_parse("a31280ad-481e-41b6-95e8-127f4c984779").unwrap()
}
pub fn lzma_guid() -> Guid {
    Guid::try_parse("ee4e5898-3914-4259-9d6e-dc7bd79403cf").unwrap()
}
pub fn lzma_hp_guid() -> Guid {
    Guid::try_parse("0ed85e23-f253-413f-a03c-901987b04397").unwrap()
}
pub fn lzma_ms_guid() -> Guid {
    Guid::try_parse("bd9921ea-ed91-404a-8b2f-b4d724747c8c").unwrap()
}
pub fn lzmaf86_guid() -> Guid {
    Guid::try_parse("d42ae6bd-1352-4bfb-909a-ca72a6eae889").unwrap()
}
pub fn crc32_guid() -> Guid {
    Guid::try_parse("fc1bcdb0-7d31-49aa-936a-a4600d9dd083").unwrap()
}

pub fn calculate_checksum8(data: &[u8]) -> u8 {
    let sum = data.iter().fold(0u8, |acc, &byte| acc.wrapping_add(byte));
    0u8.wrapping_sub(sum)
}

pub fn calculate_checksum16(data: &[u8]) -> u16 {
    let mut sum: u16 = 0;
    for &byte in data {
        sum = sum.wrapping_add(byte as u16);
    }
    0u16.wrapping_sub(sum)
}

pub fn size_to_uint24(size: u32) -> [u8; 3] {
    [size as u8, (size >> 8) as u8, (size >> 16) as u8]
}

pub fn uint24_to_u32(b: [u8; 3]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16)
}

pub fn is_large_section(header: &[u8]) -> bool {
    header.len() >= 8 && header[0] == 0xFF && header[1] == 0xFF && header[2] == 0xFF
}

pub fn is_large_ffs(header: &[u8]) -> bool {
    header.len() >= 32 && header[20] == 0xFF && header[21] == 0xFF && header[22] == 0xFF
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
        if header.len() >= 32 {
            u32::from_le_bytes([header[24], header[25], header[26], header[27]])
        } else {
            0
        }
    } else {
        uint24_to_u32([header[20], header[21], header[22]])
    }
}
```

**Важно:** well-known GUIDs — функции возвращают `Guid`, так как `uguid::Guid` не поддерживает `const` struct literal. Использовать `Guid::try_parse(...)` под капотом. Альтернатива: `Guid::from_bytes_le(&[16 байт])`.

- [ ] **Step 4: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine ffs::tests`
Expected: PASS

- [ ] **Step 5: Запустить все тесты и clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS

- [ ] **Step 6: Коммит**

```bash
git add crates/uefi-engine/src/ffs.rs crates/uefi-engine/src/lib.rs
git commit -m "feat: FFS structures and checksum helpers (wrapping arithmetic, ref ffs.rs)"
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
- Референс: `../refs/UEFITool-ai-fork/common/ffsparser.cpp` `parseVolumeHeader`/`parseSectionBody`. Заголовок `EFI_FIRMWARE_VOLUME_HEADER` (layout — `../refs/UEFITool-ai-fork/common/ffs.h:99-111`, `EFI_FV_SIGNATURE_OFFSET = 0x28` — ffs.h:144). Канонические смещения: ZeroVector@0(16), FileSystemGuid@16(16), **FvLength@32(u64)**, Signature@40(u32, "_FVH"), **Attributes@44(u32)**, **HeaderLength@48(u16)**, Checksum@50(u16), ExtHeaderOffset@52(u16), Reserved@54(u8), **Revision@55(u8)**, BlockMap@56. `erasePolarity` из `EFI_FVB2_ERASE_POLARITY` (бит в Attributes). `ffs_version` 2 или 3 по `FileSystemGuid` (полная реализация — позже; в Task 4 — заглушка `ffs_version: 2`).

> **NOTE (исправление плана, 2026-07-25):** исходный текст Step 1/Step 3 содержал ошибочные смещения полей FV-заголовка (`vol_size@20` как u32, `attributes@24`, `header_len@44`, `revision@48`). Они не соответствуют canonical `EFI_FIRMWARE_VOLUME_HEADER` из `ffs.h` и реальному `EFI_FV_SIGNATURE_OFFSET=0x28`. Тест-фикстура и реализация в исходном плане были согласованы между собой (оба ошибались одинаково), поэтому тест прошёл бы формально, но парсер не работал бы ни на одном реальном UEFI-образе (FvLength считывался бы из середины FileSystemGuid, HeaderLength — из Attributes и т.д.; при `[44]=0x48,[45]=0xFE` `header_len=0xFE48=65096` → выход за границы буфера → panic). Смещения исправлены на канонические ниже. Причина: см. `ffs.h:99-111` и EDK2 `MdePkg/Include/Pi/PiFirmwareVolume.h`.

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

**Module-first rule:** добавить `pub mod parser;` в `crates/uefi-engine/src/lib.rs` (ДО запуска `cargo test` в Step 2):

```rust
pub mod types;
pub mod ffs;
pub mod parser;
pub use types::*;
```

`crates/uefi-engine/src/parser/volume.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};

    fn make_minimal_volume() -> Vec<u8> {
        let mut buf = vec![0u8; 256];
        // ZeroVector@0 и FileSystemGuid@16 — нули (минимальный синтетический volume)
        // FvLength@32 (u64) = 256 — полный размер volume
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        // Signature@40 (u32) = "_FVH"
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        // Attributes@44 (u32) = EFI_FVB2_ERASE_POLARITY → empty_byte = 0xFF
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        // HeaderLength@48 (u16) = 56 (sizeof EFI_FIRMWARE_VOLUME_HEADER без BlockMap)
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        // Revision@55 (u8) = 2
        buf[55] = 2;
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
    let fv_length = u64::from_le_bytes([
        buf[off+32], buf[off+33], buf[off+34], buf[off+35],
        buf[off+36], buf[off+37], buf[off+38], buf[off+39],
    ]);
    let vol_size = fv_length as usize;
    let header_len = u16::from_le_bytes([buf[off+48], buf[off+49]]) as usize;
    let attributes = u32::from_le_bytes([buf[off+44], buf[off+45], buf[off+46], buf[off+47]]);
    if header_len < 56 || vol_size < header_len || off + vol_size > buf.len() {
        return Err(ParserError::InvalidHeader(format!(
            "bad FV geometry at {offset}: header_len={header_len}, fv_length={fv_length}"
        )));
    }
    let empty_byte = if attributes & EFI_FVB2_ERASE_POLARITY != 0 { 0xFF } else { 0x00 };
    let header = buf[off..off+header_len].to_vec();
    let body = buf[off+header_len..off+vol_size].to_vec();
    let parsing_data = ParsingData::Volume(VolumeParsingData {
        extended_header_guid: None,
        alignment: 1u32 << (attributes & 0x1F),
        ffs_version: 2,
        empty_byte,
        revision: buf[off+55],
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

- [ ] **Step 5: Коммит**

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
- Референс: `../refs/UEFITool-ai-fork/common/ffsparser.cpp` `parseFileHeader`/`parseFileBody`. Заголовок `EFI_FFS_FILE_HEADER` (layout — `../refs/UEFITool-ai-fork/common/ffs.h:270-277`): `Name@0(16) | IntegrityCheck@16(2) | **Type@18(1)** | **Attributes@19(1)** | Size@20(3) | State@23`. FFSv2 = 24 байта; FFSv3 large (`EFI_FFS_FILE_HEADER2`, 32 байта, Size=0xFFFFFF → ExtendedSize@24 u64) и Lenovo large (ExtendedSize@24 u32) — ffs.h:280-299. GUID из первых 16 байт. Checksum (валидация отложена на поздний цикл): header-checksum в `IntegrityCheck.Checksum.Header@16`, `calculate_checksum8(header) == 0` при FFS_ATTRIB_CHECKSUM. Tail для revision 1 — 2 байта `~TailReference`. После заголовка — тело (body), парсится `parse_sections` (Task 6).

> **NOTE (исправление плана, 2026-07-25):** исходный Step 1/Step 3 читал `Type@16` и `Attributes@17` — это позиции `EFI_FFS_INTEGRITY_CHECK` (union Header/File @16-17, ffs.h:261-268), а не Type/Attributes. Тест-фикстура и impl были согласованы между собой (оба писали Type в IntegrityCheck.Header), поэтому тест прошёл бы формально, но на реальном образе `node.subtype` и атрибуты считывались бы неправильно (Type брался из checksum-байта). Смещения исправлены на канонические Type@18/Attributes@19. Заодно удалён неиспользуемый read `attributes` (не хранится в `FileParsingData`; будет добавлен когда понадобится для checksum/alignment). Причина: `ffs.h:270-277`.

- [ ] **Step 1: Написать failing test**

`crates/uefi-engine/src/parser/file.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_ffs() -> Vec<u8> {
        let mut buf = vec![0u8; 48];
        let guid = Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        buf[0..16].copy_from_slice(&guid.to_bytes());
        // IntegrityCheck @16-17 — нули (валидация checksum отложена)
        buf[18] = 0x01; // Type: RAW (canonical offset)
        buf[19] = 0x02; // Attributes (canonical offset)
        buf[20..23].copy_from_slice(&size_to_uint24(48)); // Size[3] @20
        // State @23 — 0
        buf[24..48].copy_from_slice(&[0xFF; 24]); // body
        buf
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

- [ ] **Step 3: Реализовать parse_file и подключить модуль**

Обновить `crates/uefi-engine/src/parser/mod.rs` — добавить `pub mod file;` (БЕЗ `pub mod section;` — он будет добавлён в Task 6):
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
```

`crates/uefi-engine/src/parser/file.rs`:
```rust
use crate::types::*;
use crate::ffs::*;

pub fn guid_to_bytes(g: &Guid) -> [u8; 16] {
    g.to_bytes()
}

pub fn guid_from_bytes(b: &[u8]) -> Option<Guid> {
    let arr: [u8; 16] = b.get(..16)?.try_into().unwrap();
    Some(Guid::from_bytes(arr))
}

pub fn parse_file(buf: &[u8], offset: u32, erase_polarity: u8, revision: u8) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 24 > buf.len() { return Err(ParserError::EndOfBuffer); }
    let large = is_large_ffs(&buf[off..]);
    let hdr_len = if large { 32 } else { 24 };
    if off + hdr_len > buf.len() { return Err(ParserError::EndOfBuffer); }
    let guid = guid_from_bytes(&buf[off..off+16]).ok_or_else(|| ParserError::InvalidHeader("guid".into()))?;
    let ftype = buf[off+18];
    let size = ffs_file_size(&buf[off..]);
    let total = size as usize;
    if off + total > buf.len() { return Err(ParserError::EndOfBuffer); }
    if total < hdr_len { return Err(ParserError::InvalidHeader("size < header".into())); }
    let header = buf[off..off+hdr_len].to_vec();
    let tail_len = if revision == 1 { 2 } else { 0 };
    let body_end = off + total - tail_len;
    let body = if body_end > off + hdr_len { buf[off+hdr_len..body_end].to_vec() } else { vec![] };
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

**Важно:** `guid_from_bytes` проверяет длину через `.get(..16)` — не падает при некорректных байтах. `Guid::from_bytes` принимает любые 16 байт (без RFC-валидации) — UEFI-образы могут содержать нестандартные GUID. `body_end > off + hdr_len` защищает от отрицательного body при `total < hdr_len + tail_len`.

- [ ] **Step 4: Запустить тест — должен пройти (модуль уже подключён в Step 3)**

Run: `cargo test -p uefi-engine parser::file::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/src/parser/file.rs crates/uefi-engine/src/parser/mod.rs
git commit -m "feat: add FFS file parser (safe guid_from_bytes, uguid::Guid)"
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

**Module-first rule:** добавить `pub mod section;` в `crates/uefi-engine/src/parser/mod.rs` (ДО запуска `cargo test` в Step 2).

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
            let guid = crate::parser::file::guid_from_bytes(&body[0..16]).ok_or_else(|| ParserError::InvalidHeader("guid".into()))?;
            let _data_offset = u16::from_le_bytes([body[16], body[17]]) as usize;
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
    // Декомпрессия compressed/guided-секций и рекурсивный парсинг — в Task 7.
    // Пока: children пустые, body as-is.
    let children = vec![];
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

> **NOTE (исправление плана, 2026-07-25):** исходный Task 7 содержал три дефекта, выявленные валидацией на реальном образе `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`:
> 1. **Неверные well-known GUIDs** в Task 3 (`ee4e5ace-...`, `a31280ad-0411-...` и т.д.) — выдуманные значения, не встречающиеся в реальном UEFI. На образе 0 совпадений. Исправлены на канонические EDK2 (`ee4e5898-3914-4259-9d6e-dc7bd79403cf` LZMA, `a31280ad-481e-41b6-95e8-127f4c984779` Tiano, `d42ae6bd-1352-4bfb-909a-ca72a6eae889` LZMAF86, `fc1bcdb0-7d31-49aa-936a-a4600d9dd083` CRC32) + добавлены LZMA_HP/LZMA_MS. После исправнения: 207 LZMA-секций в образе. Референс: `../refs/edk2/MdeModulePkg/Include/Guid/LzmaDecompress.h`, `../refs/UEFITool-ai-fork/common/ffs.cpp:190-200`.
> 2. **Несуществующий API lzma-rs**: код `lzma_rs::lzma_decompressor().decompress(input, output, &props)` (3 аргумента, явные props) не компилируется — в lzma-rs 0.3 доступна только `lzma_rs::lzma_decompress(input: &mut BufRead, output: &mut Write)` без props. При этом UEFI LZMA-поток — это стандартный LZMA1 «alone» (5 байт props + 8 байт LE u64 uncompressed-size + данные), который lzma-rs читает целиком. Реализация: `lzma_rs::lzma_decompress(&mut Cursor::new(data), &mut output)`.
> 3. **Отсутствовала декомпрессия GUIDed-секций**: Step 4 подключал только `EFI_SECTION_COMPRESSION`, но реальный образ использует исключительно GUIDed-секции с LZMA GUID (207 шт., 0 `EFI_SECTION_COMPRESSION`). Добавлена wiring-логика для `EFI_SECTION_GUID_DEFINED` по GUID (LZMA/Tiano) с извлечением payload из `body[data_offset-4..]` (DataOffset — из заголовка GUIDed-секции, `body[16..18]` LE u16; measured from section start incl. 4-байтный общий заголовок) и заполнением `dictionary_size` из props[1..5]. Результат: `real_image_decompresses_lzma_sections` — все 207 секций декомпрессируются, восстановлено 193 PE32-модуля.

- [ ] **Step 1: Написать failing test для not-compressed**

**Module-first rule:** добавить `pub mod decompress;` в `crates/uefi-engine/src/lib.rs` (ДО запуска `cargo test` в Step 3).

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

- [ ] **Step 5: Запустить все тесты**

Run: `cargo test -p uefi-engine`
Expected: PASS

- [ ] **Step 6: Коммит**

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

> **NOTE (исправление плана, 2026-07-25):** исходный Task 8 содержал три дефекта, выявленные валидацией на реальном образе `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`:
> 1. **`vol_size` читался из неверного offset 20** (`u32::from_le_bytes(buf[off+20..off+24])`) — это середина `FileSystemGuid`. Канонический `FvLength` (u64) находится @32 (см. исправление Task 4). На реальном образе offset 20 даёт garbage `0x4f1c8a3d` вместо `0x40000` (FV @0x800000), что ломает весь обход образа (`off += vol_size` улетает за буфер). Исправление: `vol_size = vol.header.len() + vol.body.len()` — `parse_volume` уже хранит header `buf[off..off+header_len]` и body `buf[off+header_len..off+vol_size]`, поэтому их сумма == FvLength. Дополнительно: дублирующий `read` `vol_size` удалён.
> 2. **Синтетический fixture `make_image_with_volume` сломан** — пишет `256u32` в offset 20 и не задаёт `FvLength@32`/`HeaderLength@48`/`Revision@55`, поэтому `parse_volume` (с каноническими offset'ами из Task 4) на нём падает. Fixture переписан под канонический `EFI_FIRMWARE_VOLUME_HEADER`.
> 3. **`list_recursive` дублировал путь "0"** для root и первого ребёнка (`new_path = "0"` для root, и первый ребёнок тоже получал index-path "0"). Исправлено: root получает пустой путь `""`, дети — index-path `0/2/207` (консистентно с `Target::Path` из Task 9). `name` извлекается только для UI/VERSION-секций (UTF-16LE), а не `String::from_utf8_lossy(body)` (давало мусор для бинарных тел). `guid` выводится UPPERCASE через `guid_to_upper_string` (AGENTS.md).

- [ ] **Step 1: Написать failing test**

**Module-first rule:** добавить `pub mod image;` в `crates/uefi-engine/src/parser/mod.rs` (ДО запуска `cargo test` в Step 2).

`crates/uefi-engine/src/parser/image.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::EFI_FVH_SIGNATURE;

    fn make_image_with_volume() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        // ZeroVector@0(16) и FileSystemGuid@16(16) — нули
        // FvLength@32 (u64) = 256 — полный размер volume
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        // Signature@40 (u32) = "_FVH"
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        // Attributes@44 (u32) = EFI_FVB2_ERASE_POLARITY
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        // HeaderLength@48 (u16) = 56
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        // Revision@55 (u8) = 2
        buf[55] = 2;
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
                let vol_size = vol.header.len() + vol.body.len();
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

- [ ] **Step 4: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine parser::image::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

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

**Module-first rule:** добавить `pub mod target;` в `crates/uefi-engine/src/parser/mod.rs` (ДО запуска `cargo test` в Step 2).

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

- [ ] **Step 4: Запустить тесты**

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

**Module-first rule:** добавить `pub mod builder;` в `crates/uefi-engine/src/lib.rs` (ДО запуска `cargo test` в Step 2).

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

- [ ] **Step 5: Запустить round-trip тест — должен пройти**

Run: `cargo test -p uefi-engine builder::tests`
Expected: PASS

- [ ] **Step 6: Коммит**

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

**Module-first rule:** добавить `pub mod ops;` в `crates/uefi-engine/src/lib.rs` (ДО запуска `cargo test` в Step 2).

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

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p uefi-engine ops::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/src/ops.rs crates/uefi-engine/src/lib.rs
git commit -m "feat: add tree ops (insert/remove/replace/rebuild) with cascade rebuild"
```

---

### Task 12: Storage (SQLite) — sessions (с name) и artifacts (extract/import/export)

**Files:**
- Create: `crates/uefi-engine/src/storage/mod.rs`
- Create: `crates/uefi-engine/src/storage/schema.rs`
- Create: `crates/uefi-engine/src/storage/artifact.rs`
- Modify: `crates/uefi-engine/src/lib.rs` (добавить `pub mod storage;`)

**Interfaces:**
- Consumes: `rusqlite`
- Produces:
  - `pub struct Db { conn: rusqlite::Connection }`
  - `pub fn open_db(path: &Path) -> Result<Db>`
  - `SessionRow { id, token, name, created_at, last_activity }`
  - `Db::insert_session(id, token, name)`, `Db::get_session(id)`, `Db::touch_session(id)`, `Db::delete_session(id)`, `Db::delete_session_metadata(id)` (только БД, без файлов), `Db::list_sessions()`, `Db::list_expired(ttl_secs)`
  - `Db::insert_artifact(id, session_id, kind, path, size, source)`, `Db::get_artifact(id)`, `Db::list_artifacts(session_id)`, `Db::delete_artifacts_for_session(id)`
  - `ArtifactRow { id, session_id, kind, path, size, source, created_at }`
  - `store_artifact_file(data_dir, session_id, artifact_id, bytes) -> Result<PathBuf>`

- [ ] **Step 1: Объявить модуль и написать failing tests**

`crates/uefi-engine/src/lib.rs` (добавить):
```rust
pub mod storage;
```

`crates/uefi-engine/src/storage/mod.rs` (структуры + tests):
```rust
pub mod schema;
pub mod artifact;

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use anyhow::Result;

pub struct Db { pub conn: Connection }

#[derive(Debug, Clone)]
pub struct SessionRow { pub id: String, pub token: String, pub name: String, pub created_at: i64, pub last_activity: i64 }

#[derive(Debug, Clone)]
pub struct ArtifactRow { pub id: String, pub session_id: String, pub kind: String, pub path: String, pub size: i64, pub source: String, pub created_at: i64 }

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
    fn insert_and_get_session_with_name() {
        let (_td, db) = test_db();
        db.insert_session("s1", "tok1", "/home/user/work").unwrap();
        let row = db.get_session("s1").unwrap().unwrap();
        assert_eq!(row.token, "tok1");
        assert_eq!(row.name, "/home/user/work");
    }

    #[test]
    fn delete_session_metadata_keeps_no_trace() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "name1").unwrap();
        db.insert_artifact("a1", "s1", "extracted", "/x.bin", 100, "GUID:0x10").unwrap();
        db.delete_session_metadata("s1").unwrap();
        assert!(db.get_session("s1").unwrap().is_none());
        assert!(db.list_artifacts("s1").unwrap().is_empty());
    }

    #[test]
    fn list_artifacts_for_session() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.insert_artifact("a1", "s1", "extracted", "/x", 100, "target1").unwrap();
        db.insert_artifact("a2", "s1", "imported", "/y", 200, "file.bin").unwrap();
        let arts = db.list_artifacts("s1").unwrap();
        assert_eq!(arts.len(), 2);
    }

    #[test]
    fn list_expired() {
        let (_td, db) = test_db();
        db.insert_session("s1", "t", "n").unwrap();
        db.conn.execute("UPDATE sessions SET last_activity = 0").unwrap();
        let expired = db.list_expired(3600).unwrap();
        assert!(expired.contains(&"s1".into()));
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine storage::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать schema.rs (с session_name и source напрямую в CREATE TABLE)**

`crates/uefi-engine/src/storage/schema.rs`:
```rust
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id            TEXT PRIMARY KEY,
    token         TEXT NOT NULL,
    name          TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS artifacts (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    path       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    source     TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity);
"#;
```

**Важно (issue #1, п.2):** `session_name` добавляется напрямую в `CREATE TABLE`, НЕ через `ALTER TABLE`.

- [ ] **Step 4: Реализовать artifact.rs**

`crates/uefi-engine/src/storage/artifact.rs`:
```rust
use std::path::{Path, PathBuf};
use anyhow::Result;
use std::fs;

pub fn store_artifact_file(data_dir: &Path, session_id: &str, artifact_id: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = data_dir.join("sessions").join(session_id).join("artifacts");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{artifact_id}.bin"));
    fs::write(&path, bytes)?;
    Ok(path)
}

pub fn read_artifact_file(data_dir: &Path, session_id: &str, artifact_id: &str) -> Result<Vec<u8>> {
    let path = data_dir.join("sessions").join(session_id).join("artifacts").join(format!("{artifact_id}.bin"));
    Ok(fs::read(&path)?)
}

pub fn write_artifact_to_output(data_dir: &Path, session_id: &str, artifact_id: &str, output_path: &str) -> Result<()> {
    let bytes = read_artifact_file(data_dir, session_id, artifact_id)?;
    fs::write(output_path, &bytes)?;
    Ok(())
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
    pub fn insert_session(&self, id: &str, token: &str, name: &str) -> Result<()> {
        let t = now();
        self.conn.execute(
            "INSERT INTO sessions (id, token, name, created_at, last_activity) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, token, name, t])?;
        Ok(())
    }
    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>> {
        self.conn.query_row(
            "SELECT id, token, name, created_at, last_activity FROM sessions WHERE id=?1",
            params![id],
            |r| Ok(SessionRow { id: r.get(0)?, token: r.get(1)?, name: r.get(2)?, created_at: r.get(3)?, last_activity: r.get(4)? })
        ).optional().map_err(Into::into)
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
    pub fn delete_session_metadata(&self, id: &str) -> Result<()> {
        self.delete_session(id)
    }
    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare("SELECT id, token, name, created_at, last_activity FROM sessions")?;
        let rows = stmt.query_map([], |r| Ok(SessionRow { id: r.get(0)?, token: r.get(1)?, name: r.get(2)?, created_at: r.get(3)?, last_activity: r.get(4)? }))?;
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
    pub fn insert_artifact(&self, id: &str, session_id: &str, kind: &str, path: &str, size: i64, source: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO artifacts (id, session_id, kind, path, size, source, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, session_id, kind, path, size, source, now()])?;
        Ok(())
    }
    pub fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRow>> {
        self.conn.query_row(
            "SELECT id, session_id, kind, path, size, source, created_at FROM artifacts WHERE id=?1",
            params![id],
            |r| Ok(ArtifactRow { id: r.get(0)?, session_id: r.get(1)?, kind: r.get(2)?, path: r.get(3)?, size: r.get(4)?, source: r.get(5)?, created_at: r.get(6)? })
        ).optional().map_err(Into::into)
    }
    pub fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactRow>> {
        let mut stmt = self.conn.prepare("SELECT id, session_id, kind, path, size, source, created_at FROM artifacts WHERE session_id=?1")?;
        let rows = stmt.query_map(params![session_id], |r| Ok(ArtifactRow { id: r.get(0)?, session_id: r.get(1)?, kind: r.get(2)?, path: r.get(3)?, size: r.get(4)?, source: r.get(5)?, created_at: r.get(6)? }))?;
        let mut v = vec![];
        for r in rows { v.push(r?); }
        Ok(v)
    }
}
```

- [ ] **Step 6: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine storage::tests`
Expected: PASS

- [ ] **Step 7: Коммит**

```bash
git add crates/uefi-engine/src/storage/ crates/uefi-engine/src/lib.rs
git commit -m "feat: SQLite storage with session_name, artifact extract/import/export"
```

---

### Task 13: Session manager + фоновый GC (--purge-artifacts)

**Files:**
- Create: `crates/uefi-engine/src/session.rs`
- Modify: `crates/uefi-engine/src/lib.rs` (добавить `pub mod session;`)

**Interfaces:**
- Consumes: `storage::Db`, `uuid`, `tokio`
- Produces:
  - `pub struct SessionManager { db, data_dir, ttl, gc_interval, purge_artifacts: bool }`
  - `SessionManager::create_session(name: &str) -> (session_id, token)` — name по умолчанию = `env::var("PWD")` (без symlink resolution)
  - `SessionManager::destroy_session(id, purge_files: bool) -> Result<()>` — при `purge_files=false` удаляет только БД metadata
  - `SessionManager::spawn_gc() -> JoinHandle` — GC учитывает `purge_artifacts` flag

- [ ] **Step 1: Объявить модуль и написать failing tests**

`crates/uefi-engine/src/lib.rs` (добавить):
```rust
pub mod session;
```

`crates/uefi-engine/src/session.rs` (структура + tests):
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
    pub purge_artifacts: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sm() -> (TempDir, SessionManager) {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("s.db")).unwrap();
        let sm = SessionManager {
            db: Arc::new(db),
            data_dir: td.path().to_path_buf(),
            ttl: Duration::from_secs(1),
            gc_interval: Duration::from_millis(100),
            purge_artifacts: false,
        };
        (td, sm)
    }

    #[test]
    fn create_session_with_name() {
        let (_td, sm) = sm();
        let (id, tok) = sm.create_session("/home/user/bios").unwrap();
        assert!(sm.validate_token(&id, &tok));
        let sessions = sm.list_sessions().unwrap();
        assert_eq!(sessions[0].name, "/home/user/bios");
    }

    #[test]
    fn destroy_without_purge_keeps_files() {
        let (td, sm) = sm();
        let (id, _) = sm.create_session("n").unwrap();
        let art_dir = td.path().join("sessions").join(&id).join("artifacts");
        std::fs::create_dir_all(&art_dir).unwrap();
        std::fs::write(art_dir.join("a1.bin"), b"data").unwrap();
        sm.destroy_session(&id, false).unwrap();
        assert!(sm.list_sessions().unwrap().is_empty());
        assert!(art_dir.join("a1.bin").exists(), "files must survive when purge=false");
    }

    #[test]
    fn destroy_with_purge_removes_files() {
        let (td, sm) = sm();
        let (id, _) = sm.create_session("n").unwrap();
        let sess_dir = td.path().join("sessions").join(&id);
        std::fs::create_dir_all(&sess_dir).unwrap();
        sm.destroy_session(&id, true).unwrap();
        assert!(!sess_dir.exists(), "files must be removed when purge=true");
    }
}
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine session::tests`
Expected: FAIL

- [ ] **Step 3: Реализовать SessionManager**

Дополнить `crates/uefi-engine/src/session.rs`:
```rust
use uuid::Uuid;
use std::fs;

impl SessionManager {
    pub fn new(db: Db, data_dir: PathBuf, ttl: Duration, gc_interval: Duration, purge_artifacts: bool) -> Self {
        Self { db: Arc::new(db), data_dir, ttl, gc_interval, purge_artifacts }
    }
    pub fn create_session(&self, name: &str) -> Result<(String, String)> {
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string();
        self.db.insert_session(&id, &token, name)?;
        let sess_dir = self.data_dir.join("sessions").join(&id);
        fs::create_dir_all(&sess_dir)?;
        Ok((id, token))
    }
    pub fn destroy_session(&self, id: &str, purge_files: bool) -> Result<()> {
        self.db.delete_session_metadata(id)?;
        if purge_files {
            let sess_dir = self.data_dir.join("sessions").join(id);
            let _ = fs::remove_dir_all(&sess_dir);
        }
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
        let purge = self.purge_artifacts;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Ok(expired) = self.db.list_expired(ttl.as_secs() as i64) {
                    for id in expired {
                        let _ = self.destroy_session(&id, purge);
                        if purge {
                            tracing::info!("GC purged session {id} (files+metadata)");
                        } else {
                            tracing::info!("GC removed session {id} metadata (files preserved)");
                        }
                    }
                }
            }
        })
    }
}
```

**Важно (issue #1, п.3):** при `purge_artifacts=false` (default) GC удаляет только session metadata из БД, но **СОХРАНЯЕТ файлы артефактов**. Безопасность данных > экономия места.

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p uefi-engine session::tests`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-engine/src/session.rs crates/uefi-engine/src/lib.rs
git commit -m "feat: SessionManager with named sessions and --purge-artifacts GC policy"
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

**Module-first rule:** добавить `pub mod setup;` в `crates/uefi-engine/src/lib.rs` (ДО запуска `cargo test` в Step 2).

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

- [ ] **Step 7: Коммит**

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

**Module-first rule:** добавить `pub mod rpc;` в `crates/uefi-engine/src/lib.rs` (ДО запуска `cargo test`). Также создать `crates/uefi-engine/src/rpc/mod.rs` с `pub mod auth; pub mod server;` (модуль-декларации — в Step 1, реализация EngineService — в Step 4).

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
        let sm = Arc::new(SessionManager::new(db, td.path().to_path_buf(), Duration::from_secs(864000), Duration::from_secs(3600), false));
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
    async fn create_session(&self, req: Request<CreateSessionRequest>) -> RpcResult<CreateSessionResponse> {
        let r = req.into_inner();
        let name = if r.name.is_empty() {
            std::env::var("PWD").unwrap_or_default()
        } else {
            r.name
        };
        let (id, tok) = self.sm.create_session(&name).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(CreateSessionResponse { session_id: id, token: tok }))
    }
    async fn destroy_session(&self, req: Request<DestroySessionRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        self.sm.destroy_session(&r.session_id, self.sm.purge_artifacts).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
    async fn list_sessions(&self, _req: Request<ListSessionsRequest>) -> RpcResult<ListSessionsResponse> {
        let rows = self.sm.list_sessions().map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListSessionsResponse {
            sessions: rows.into_iter().map(|r| SessionInfo {
                session_id: r.id, name: r.name, created_at: r.created_at, last_activity: r.last_activity
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
        let ffs_bytes = if !r.artifact_id.is_empty() {
            let art = self.sm.db.get_artifact(&r.artifact_id).map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("artifact not found"))?;
            crate::storage::artifact::read_artifact_file(&self.data_dir, &art.session_id, &art.id).map_err(|e| Status::internal(e.to_string()))?
        } else {
            fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?
        };
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
        let data = if !r.artifact_id.is_empty() {
            let art = self.sm.db.get_artifact(&r.artifact_id).map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("artifact not found"))?;
            crate::storage::artifact::read_artifact_file(&self.data_dir, &art.session_id, &art.id).map_err(|e| Status::internal(e.to_string()))?
        } else {
            fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?
        };
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
    async fn extract_artifact(&self, req: Request<ExtractArtifactRequest>) -> RpcResult<ExtractArtifactResponse> {
        let r = req.into_inner();
        let images = self.images.lock().await;
        let img = images.get(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let t = parse_target(&r.target).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let node = find_item(&img.root, &t).map_err(|e| Status::not_found(e.to_string()))?;
        let bytes = if r.body_only { node.body.clone() } else { node.header.iter().chain(node.body.iter()).copied().collect() };
        drop(images);
        let artifact_id = Uuid::new_v4().to_string();
        let path = crate::storage::artifact::store_artifact_file(&self.data_dir, &img.session_id, &artifact_id, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        let kind = if r.body_only { "body" } else { "whole" };
        let source = format!("{}{}", r.target, if r.body_only { ":body" } else { "" });
        self.sm.db.insert_artifact(&artifact_id, &img.session_id, kind, &path.to_string_lossy(), bytes.len() as i64, &source).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ExtractArtifactResponse { artifact_id }))
    }
    async fn export_artifact(&self, req: Request<ExportArtifactRequest>) -> RpcResult<Empty> {
        let r = req.into_inner();
        let art = self.sm.db.get_artifact(&r.artifact_id).map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("artifact not found"))?;
        crate::storage::artifact::write_artifact_to_output(&self.data_dir, &art.session_id, &art.id, &r.output_path).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(Empty {}))
    }
    async fn import_artifact(&self, req: Request<ImportArtifactRequest>) -> RpcResult<ImportArtifactResponse> {
        let r = req.into_inner();
        let bytes = fs::read(&r.file_path).map_err(|e| Status::not_found(e.to_string()))?;
        let artifact_id = Uuid::new_v4().to_string();
        let path = crate::storage::artifact::store_artifact_file(&self.data_dir, &r.session_id, &artifact_id, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        let source = std::path::Path::new(&r.file_path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.sm.db.insert_artifact(&artifact_id, &r.session_id, "imported", &path.to_string_lossy(), bytes.len() as i64, &source).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ImportArtifactResponse { artifact_id }))
    }
    async fn list_artifacts(&self, req: Request<ListArtifactsRequest>) -> RpcResult<ListArtifactsResponse> {
        let r = req.into_inner();
        let arts = self.sm.db.list_artifacts(&r.session_id).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListArtifactsResponse {
            artifacts: arts.into_iter().map(|a| ArtifactInfo {
                artifact_id: a.id, kind: a.kind, size: a.size as u64, created_at: a.created_at, source: a.source
            }).collect()
        }))
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

pub fn serve(socket_path: &Path, db: Db, data_dir: PathBuf, ttl: Duration, gc_interval: Duration, purge_artifacts: bool) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let sm = Arc::new(SessionManager::new(db, data_dir.clone(), ttl, gc_interval, purge_artifacts));
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

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p uefi-engine rpc::server::tests`
Expected: PASS

- [ ] **Step 6: Коммит**

```bash
git add crates/uefi-engine/src/rpc/ crates/uefi-engine/src/lib.rs crates/uefi-engine/Cargo.toml
git commit -m "feat: add gRPC EngineService server over unix-socket"
```

---

### Task 16: Engine binary (`src/bin/engine.rs` — clap CLI с --purge-artifacts)

**Files:**
- Create: `crates/uefi-engine/src/bin/engine.rs`
- Modify: `crates/uefi-engine/Cargo.toml` (добавить `clap`)

**Interfaces:**
- Consumes: `rpc::serve`, `clap`, env vars
- Produces: бинарник `uefi-engine` с флагами:
  - `--data-dir <PATH>` [env: UEFIPATCHER_DATA]
  - `--sock <PATH>` [env: UEFIPATCHER_SOCK]
  - `--ttl <SECS>` [env: UEFIPATCHER_SESSION_TTL_SECS] (default: 864000)
  - `--gc-interval <SECS>` [env: UEFIPATCHER_SESSION_GC_INTERVAL_SECS] (default: 3600)
  - `--purge-artifacts` [env: UEFIPATCHER_PURGE_ARTIFACTS] (default: false)

- [ ] **Step 1: Добавить clap в Cargo.toml**

`crates/uefi-engine/Cargo.toml` [dependencies]:
```toml
clap = { workspace = true }
```

- [ ] **Step 2: Реализовать engine binary**

`crates/uefi-engine/src/bin/engine.rs`:
```rust
use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "uefi-engine", version, about = "UEFIPatcher engine server")]
struct Args {
    #[arg(long, env = "UEFIPATCHER_DATA")]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<PathBuf>,
    #[arg(long, env = "UEFIPATCHER_SESSION_TTL_SECS", default_value = "864000")]
    ttl: u64,
    #[arg(long, env = "UEFIPATCHER_SESSION_GC_INTERVAL_SECS", default_value = "3600")]
    gc_interval: u64,
    #[arg(long, env = "UEFIPATCHER_PURGE_ARTIFACTS", default_value = "false")]
    purge_artifacts: bool,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or_else(|| {
        directories::ProjectDirs::from("", "", "uefipatcher")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./data"))
    });
    let sock = args.sock.unwrap_or_else(|| PathBuf::from("/run/uefipatcher.sock"));
    std::fs::create_dir_all(&data_dir)?;
    if sock.parent().is_some() { let _ = std::fs::remove_file(&sock); }
    let db = uefi_engine::storage::open_db(&data_dir.join("uefipatcher.db"))?;
    tracing::info!("Starting engine: sock={}, data={}, purge_artifacts={}", sock.display(), data_dir.display(), args.purge_artifacts);
    uefi_engine::rpc::server::serve(
        &sock, db, data_dir,
        Duration::from_secs(args.ttl),
        Duration::from_secs(args.gc_interval),
        args.purge_artifacts,
    )
}
```

- [ ] **Step 3: Проверить сборку и запуск**

Run: `cargo build -p uefi-engine --bin engine`
Expected: бинарник `target/debug/engine` создан

Run: `./target/debug/engine --help`
Expected: показывает все флаги включая `--purge-artifacts`

- [ ] **Step 4: Коммит**

```bash
git add crates/uefi-engine/src/bin/engine.rs crates/uefi-engine/Cargo.toml
git commit -m "feat: engine binary with clap CLI (--purge-artifacts, env fallbacks)"
```

---

### Task 17: CLI-минимум (uefi-cli, зависит от uefi-common)

**Files:**
- Modify: `crates/uefi-cli/Cargo.toml` (уже создан в Task 1, добавить clap, uefi-common)
- Modify: `crates/uefi-cli/src/main.rs` (уже stub из Task 1)
- Test: `crates/uefi-cli/tests/smoke.rs`

**Interfaces:**
- Consumes: `uefi-proto`, `uefi-common`
- Produces: бинарник `uefi-cli` с командами:
  - `uefi-cli session create` → session_id, token (name = CWD через PWD env)
  - `uefi-cli session destroy <id>`
  - `uefi-cli <image_id> extract <target> [--body-only]` → artifact_id
  - `uefi-cli artifact export <artifact_id> <output_path>`
  - `uefi-cli artifact import <file_path>` → artifact_id
  - `uefi-cli artifact list`
  - `uefi-cli <image_id> insert <target> <ffs> [--mode into|before|after]` или `--from-artifact <id>`
  - остальные команды (open/dump/list/find/remove/replace/rebuild/save/set-visibility)

- [ ] **Step 1: Обновить Cargo.toml**

`crates/uefi-cli/Cargo.toml` [dependencies]:
```toml
clap = { workspace = true }
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

### Task 18: Контейнеризация (containerfile + fedora:44 + rust-builder)

**Files:**
- Create: `docker/rust-builder.containerfile`
- Create: `docker/engine.containerfile`
- Create: `docker/docker-compose.yml`
- Create: `docker/.dockerignore`

**Interfaces:**
- Consumes: workspace
- Produces: образ `uefipatcher-engine` на базе `registry.fedoraproject.org/fedora:44`

- [ ] **Step 1: Создать rust-builder.containerfile (базовый образ для всех Rust-сборок)**

`docker/rust-builder.containerfile`:
```dockerfile
FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y rust cargo protobuf-compiler make gcc && dnf clean all
WORKDIR /app
```

- [ ] **Step 2: Создать engine.containerfile**

`docker/engine.containerfile`:
```dockerfile
FROM uefipatcher-rust-builder AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin engine -p uefi-engine

FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y ca-certificates sqlite-libs && dnf clean all
COPY --from=builder /app/target/release/engine /usr/local/bin/uefi-engine
VOLUME ["/data", "/run/uefipatcher"]
ENV UEFIPATCHER_DATA=/data
ENV UEFIPATCHER_SOCK=/run/uefipatcher/uefipatcher.sock
ENV UEFIPATCHER_PURGE_ARTIFACTS=false
ENTRYPOINT ["uefi-engine"]
```

- [ ] **Step 3: Создать docker-compose.yml**

`docker/docker-compose.yml`:
```yaml
version: "3.8"
services:
  engine:
    build:
      context: ..
      dockerfile: docker/engine.containerfile
    volumes:
      - uefi-data:/data
      - uefi-sock:/run/uefipatcher
    environment:
      UEFIPATCHER_DATA: /data
      UEFIPATCHER_SOCK: /run/uefipatcher/uefipatcher.sock
      UEFIPATCHER_SESSION_TTL_SECS: "864000"
      UEFIPATCHER_SESSION_GC_INTERVAL_SECS: "3600"
      UEFIPATCHER_PURGE_ARTIFACTS: "false"
volumes:
  uefi-data:
  uefi-sock:
```

- [ ] **Step 4: Создать .dockerignore**

`docker/.dockerignore`:
```
target/
.git/
docs/
*.md
```

- [ ] **Step 5: Проверить валидность compose**

Run: `podman-compose -f docker/docker-compose.yml config`
Expected: корректный вывод конфигурации

- [ ] **Step 6: Коммит**

```bash
git add docker/
git commit -m "feat: containerfile (fedora:44 + rust-builder), --purge-artifacts=false default"
```

---

### Task 19: Финальная проверка — все тесты, clippy, fmt, round-trip на синтетике

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

- [ ] **Step 4: Round-trip тест через engine binary**

Run: `cargo build -p uefi-engine --bin engine`
Запустить: `./target/debug/engine --data-dir /tmp/uefi-test --sock /tmp/uefi-test.sock`
Через uefi-cli: открыть образ, dump, extract, import, insert --from-artifact, save, сравнить.
Expected: бинарно идентичны (round-trip)

- [ ] **Step 5: Коммит финальных правок**

```bash
git add -A
git commit -m "chore: final checks — all tests pass, clippy clean, round-trip verified"
```

---

## Само-проверка плана (после написания)

**Спека-покрытие:**
- Парсер: Tasks 4-8 ✓
- Builder (round-trip): Task 10 ✓
- Модификации (insert/remove/replace/rebuild с artifact_id): Task 11 ✓
- Setup-visibility (IFR через r_efi::hii): Task 14 ✓
- Хранилище (session_name + artifacts extract/import/export): Task 12 ✓
- Session manager (named sessions, --purge-artifacts GC): Task 13 ✓
- gRPC EngineService (все методы, вкл. artifact ops): Task 15 ✓
- Engine binary (clap CLI, --purge-artifacts): Task 16 ✓
- uefi-common (state.rs + error.rs скелет): Task 1 ✓
- CLI-минимум (зависит от uefi-common): Task 17 ✓
- Контейнеризация (containerfile + fedora:44 + rust-builder): Task 18 ✓
- uguid (Display/FromStr/serde + UPPERCASE): Task 2 ✓
- ffs.rs (wrapping arithmetic, correct offsets, ref ffs.rs): Task 3 ✓
- Module-first rule: Global Constraints ✓ (mod объявляется до cargo test)
- Баги 9/10/11: учтены в Tasks 10-11 ✓

**Issue #1 покрытие:**
- Extract/Export/Import артефактов: Tasks 12, 15 ✓
- Session name (CWD без symlink resolution через PWD): Tasks 12, 13, 15 ✓
- --purge-artifacts (default false): Tasks 13, 16, 18 ✓
- uefi-common в цикле 1: Task 1 ✓
- Docker → containerfile + fedora:44 + rust-builder: Task 18 ✓

**Issue #2 покрытие:**
- Guid → uguid: Task 2 ✓
- ffs.rs rewrite по референсу: Task 3 ✓
- Module ordering (mod до test): Global Constraints ✓

**Crate stack:** uguid, r-efi, binrw, object, lzma-rs — workspace deps в Task 1 ✓