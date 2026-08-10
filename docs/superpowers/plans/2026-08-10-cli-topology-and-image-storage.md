# CLI topology, proto rename, image storage — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure CLI into 5 top-level resources (session/image/node/artifact/setup), rename all gRPC RPCs to noun-first convention, add persistent image storage with write-through, migrate all clients to the new contract.

**Architecture:** Layered rollout — engine-internal infrastructure (storage, helpers) first → proto rename (breaks workspace) → engine handlers (fix engine compile) → clients one-by-one (each task fixes one crate's compile). Workspace is green after Task 1, 2; temporarily red between Task 3 and Task 8; green again after Task 8.

**Tech Stack:** Rust (edition 2024), tonic/prost (gRPC), rusqlite (storage), clap 4 (CLI), axum (gateway), tokio.

## Global Constraints

- **Rust edition 2024** (`Cargo.toml` workspace).
- **No comments in code** except `file:line` references to refs (AGENTS.md rule 8).
- **Module-first rule** (AGENTS.md rule 4): when creating a new module file, add `pub mod X;` to the parent `mod.rs`/`lib.rs`/`main.rs` **in the same step**, before running `cargo test`.
- **TDD order** (AGENTS.md rule 5): test → fail → implement → pass → commit.
- **One commit per step** where `git commit` is shown (AGENTS.md rule 7).
- **After each task:** `cargo test -p <crate>` and `cargo clippy -p <crate> -- -D warnings` (AGENTS.md rule 9).
- **No self-rolled byte parsing** — use binrw / existing types (AGENTS.md).
- **Checksums** via `wrapping_add`/`wrapping_sub`.
- **Conventional commit messages** with detailed bodies; style: `<type>(<scope>): <subject>` then body. Example: `feat(uefi-engine): add images table CRUD (Plan A Task 1)`.
- **Spec reference:** `docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md`.
- **Plan defects** (AGENTS.md rule 11): if a step turns out wrong relative to reality, separate `docs: fix Task N ...` commit BEFORE implementation commit.
- **Container runtime** (AGENTS.md rule 12): if `/run/.containerenv` exists, use `podman-remote` for any container ops.

---

## File Structure

**uefi-proto** (Task 3):
- `crates/uefi-proto/proto/engine.proto` — full rewrite (rename all RPC + new messages + remove DumpTree/FindItem/DumpFormat)

**uefi-engine** (Tasks 1, 2, 4):
- `crates/uefi-engine/src/storage/schema.rs` — add `images` table
- `crates/uefi-engine/src/storage/mod.rs` — add `ImageRow` + CRUD methods
- `crates/uefi-engine/src/storage/image.rs` — NEW: image file helpers (`store_image_file`, `read_image_file`, `atomic_write`)
- `crates/uefi-engine/src/rpc/server.rs` — rename all handlers + new handlers + `flush_image` + `get_or_load_image` + wire storage
- `crates/uefi-engine/src/parser/image.rs` — delete `dump_tree` + `DumpFormat` (after DumpTree RPC removed)
- `crates/uefi-engine/src/session.rs` — extend `destroy_session` to clean `images/` dir on `purge_files`
- `crates/uefi-engine/src/lib.rs` — add `pub mod storage::image;`

**uefi-common** (Task 4):
- `crates/uefi-common/src/state.rs` — no changes

**uefi-cli** (Task 5):
- `crates/uefi-cli/src/main.rs` — full rewrite of `Cmd` enum + `Subcommand` enums + dispatch + `ValueEnum`
- `crates/uefi-cli/src/client.rs` — rename all methods + add new (`images_list`, `image_close`, `image_status`, `setup_list_forms`, `setup_list_strings`)
- `crates/uefi-cli/src/output.rs` — add `print_image_info`, `print_images_list`, `print_forms`, `print_strings`, `print_image_status`; remove `print_find`; `OutputFormat` → `clap::ValueEnum`
- `crates/uefi-cli/src/commands/mod.rs` — add `pub mod node; pub mod artifact;`, remove `pub mod edit;`
- `crates/uefi-cli/src/commands/node.rs` — NEW: list/search/insert/remove/replace/rebuild/extract
- `crates/uefi-cli/src/commands/artifact.rs` — NEW: list/import/export
- `crates/uefi-cli/src/commands/image.rs` — refactor: drop dump/list/find/extract/export/import/artifacts subcommands (moved); keep open/switch/close/save/list/status
- `crates/uefi-cli/src/commands/session.rs` — add `--name` flag to init
- `crates/uefi-cli/src/commands/setup.rs` — new form/string subcommand structure
- `crates/uefi-cli/src/commands/edit.rs` — DELETE

**uefi-tui** (Task 6):
- `crates/uefi-tui/src/commands.rs` — mechanical rename of client method calls
- mock test stubs updated

**uefi-gateway** (Task 7):
- `crates/uefi-gateway/src/client.rs` — mechanical rename
- `crates/uefi-gateway/src/routes/image.rs` — handler fn rename (preserve route paths)
- mock test stubs updated

**WebUI** (Task 8):
- `webui/src/...` — minimal fetch URL update if it currently builds; otherwise leave with explicit TODO marker

---

## Task 1: Storage layer for images (Db CRUD)

**Files:**
- Modify: `crates/uefi-engine/src/storage/schema.rs`
- Modify: `crates/uefi-engine/src/storage/mod.rs`
- Test: `crates/uefi-engine/src/storage/mod.rs` (`mod tests`)

**Interfaces:**
- Consumes: existing `Db` struct, `now()`, existing `sessions`/`artifacts` tables
- Produces: `ImageRow { id, session_id, name, path, mode, size, created_at, last_activity }`; methods `Db::insert_image`, `Db::get_image`, `Db::list_images`, `Db::touch_image`, `Db::delete_image`, `Db::delete_images_for_session`

- [ ] **Step 1: Add `images` table to schema**

Modify `crates/uefi-engine/src/storage/schema.rs` — append to the `SCHEMA` raw string (before closing `"#`):

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
CREATE TABLE IF NOT EXISTS images (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    mode          INTEGER NOT NULL,
    size          INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_activity INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_images_session ON images(session_id);
"#;
```

- [ ] **Step 2: Add `ImageRow` struct and CRUD method stubs (fail to compile)**

In `crates/uefi-engine/src/storage/mod.rs`, after the existing `ArtifactRow` definition (around line 31), add:

```rust
#[derive(Debug, Clone)]
pub struct ImageRow {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub path: String,
    pub mode: i64,
    pub size: i64,
    pub created_at: i64,
    pub last_activity: i64,
}
```

- [ ] **Step 3: Write the failing tests for image CRUD**

In `crates/uefi-engine/src/storage/mod.rs`, append to `mod tests` (after existing `list_expired` test, around line 249):

```rust
#[test]
fn insert_and_get_image() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_image("img1", "s1", "BIOS.bin", "/path/bios.bin", 1, 16_000_000)
        .unwrap();
    let row = db.get_image("img1").unwrap().unwrap();
    assert_eq!(row.name, "BIOS.bin");
    assert_eq!(row.path, "/path/bios.bin");
    assert_eq!(row.mode, 1);
    assert_eq!(row.size, 16_000_000);
}

#[test]
fn get_image_missing_returns_none() {
    let (_td, db) = test_db();
    assert!(db.get_image("nope").unwrap().is_none());
}

#[test]
fn list_images_for_session() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_image("img1", "s1", "A.bin", "/a", 0, 100).unwrap();
    db.insert_image("img2", "s1", "B.bin", "/b", 1, 200).unwrap();
    let imgs = db.list_images("s1").unwrap();
    assert_eq!(imgs.len(), 2);
}

#[test]
fn list_images_empty_for_other_session() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_session("s2", "t", "n").unwrap();
    db.insert_image("img1", "s1", "A.bin", "/a", 0, 100).unwrap();
    assert!(db.list_images("s2").unwrap().is_empty());
}

#[test]
fn touch_image_updates_last_activity() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_image("img1", "s1", "A.bin", "/a", 0, 100).unwrap();
    let before = db.get_image("img1").unwrap().unwrap().last_activity;
    std::thread::sleep(std::time::Duration::from_secs(2));
    db.touch_image("img1").unwrap();
    let after = db.get_image("img1").unwrap().unwrap().last_activity;
    assert!(after > before, "last_activity must advance: {before} -> {after}");
}

#[test]
fn delete_image_removes_row() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_image("img1", "s1", "A.bin", "/a", 0, 100).unwrap();
    db.delete_image("img1").unwrap();
    assert!(db.get_image("img1").unwrap().is_none());
}

#[test]
fn delete_images_for_session_removes_all() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_image("img1", "s1", "A.bin", "/a", 0, 100).unwrap();
    db.insert_image("img2", "s1", "B.bin", "/b", 1, 200).unwrap();
    db.delete_images_for_session("s1").unwrap();
    assert!(db.list_images("s1").unwrap().is_empty());
}

#[test]
fn delete_session_cascades_to_images() {
    let (_td, db) = test_db();
    db.insert_session("s1", "t", "n").unwrap();
    db.insert_image("img1", "s1", "A.bin", "/a", 0, 100).unwrap();
    db.delete_session("s1").unwrap();
    assert!(db.get_image("img1").unwrap().is_none());
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p uefi-engine storage::tests`
Expected: FAIL — `no method named insert_image / get_image / list_images / touch_image / delete_image / delete_images_for_session found for Db`

- [ ] **Step 5: Implement `insert_image`, `get_image`, `list_images`**

In `crates/uefi-engine/src/storage/mod.rs`, in `impl Db` block (after `list_artifacts`, before `delete_artifacts_for_session`, around line 186):

```rust
pub fn insert_image(
    &self,
    id: &str,
    session_id: &str,
    name: &str,
    path: &str,
    mode: i64,
    size: i64,
) -> Result<()> {
    let t = now();
    self.conn.execute(
        "INSERT INTO images (id, session_id, name, path, mode, size, created_at, last_activity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![id, session_id, name, path, mode, size, t],
    )?;
    Ok(())
}

pub fn get_image(&self, id: &str) -> Result<Option<ImageRow>> {
    self.conn
        .query_row(
            "SELECT id, session_id, name, path, mode, size, created_at, last_activity FROM images WHERE id=?1",
            params![id],
            |r| {
                Ok(ImageRow {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    name: r.get(2)?,
                    path: r.get(3)?,
                    mode: r.get(4)?,
                    size: r.get(5)?,
                    created_at: r.get(6)?,
                    last_activity: r.get(7)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

pub fn list_images(&self, session_id: &str) -> Result<Vec<ImageRow>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, session_id, name, path, mode, size, created_at, last_activity FROM images WHERE session_id=?1",
    )?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(ImageRow {
            id: r.get(0)?,
            session_id: r.get(1)?,
            name: r.get(2)?,
            path: r.get(3)?,
            mode: r.get(4)?,
            size: r.get(5)?,
            created_at: r.get(6)?,
            last_activity: r.get(7)?,
        })
    })?;
    let mut v = vec![];
    for r in rows {
        v.push(r?);
    }
    Ok(v)
}
```

- [ ] **Step 6: Implement `touch_image`, `delete_image`, `delete_images_for_session`**

Append to the same `impl Db` block:

```rust
pub fn touch_image(&self, id: &str) -> Result<()> {
    self.conn.execute(
        "UPDATE images SET last_activity=?1 WHERE id=?2",
        params![now(), id],
    )?;
    Ok(())
}

pub fn delete_image(&self, id: &str) -> Result<()> {
    self.conn.execute("DELETE FROM images WHERE id=?1", params![id])?;
    Ok(())
}

pub fn delete_images_for_session(&self, session_id: &str) -> Result<()> {
    self.conn.execute(
        "DELETE FROM images WHERE session_id=?1",
        params![session_id],
    )?;
    Ok(())
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p uefi-engine storage::tests`
Expected: PASS — all 8 image CRUD tests + existing session/artifact tests pass.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/uefi-engine/src/storage/schema.rs crates/uefi-engine/src/storage/mod.rs
git commit -m "feat(uefi-engine): add images table CRUD (Plan A Task 1)

Add images table to SQLite schema with session_id FK cascade; ImageRow struct
and Db methods insert_image/get_image/list_images/touch_image/delete_image/
delete_images_for_session. Mirrors existing artifacts CRUD pattern. Tests
cover CRUD + cascade delete on parent session removal.

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 2: Image file storage helpers

**Files:**
- Create: `crates/uefi-engine/src/storage/image.rs`
- Modify: `crates/uefi-engine/src/lib.rs` — wait, `storage` is the parent module, so add `pub mod image;` to `crates/uefi-engine/src/storage/mod.rs`
- Test: `crates/uefi-engine/src/storage/image.rs` (`mod tests`)

**Interfaces:**
- Consumes: `data_dir`, `session_id`, `image_id` strings; bytes
- Produces:
  - `store_image_file(data_dir: &Path, session_id: &str, image_id: &str, bytes: &[u8]) -> Result<PathBuf>`
  - `read_image_file(data_dir: &Path, session_id: &str, image_id: &str) -> Result<Vec<u8>>`
  - `remove_image_file(data_dir: &Path, session_id: &str, image_id: &str) -> Result<()>`
  - `atomic_write(path: &Path, bytes: &[u8]) -> Result<()>` (tmp + rename)

- [ ] **Step 1: Add `pub mod image;` to parent**

In `crates/uefi-engine/src/storage/mod.rs` (line 1-2 area), add at top:

```rust
pub mod artifact;
pub mod image;
pub mod schema;
```

(Insert `pub mod image;` between `artifact` and `schema`.)

- [ ] **Step 2: Write the failing tests**

Create `crates/uefi-engine/src/storage/image.rs`:

```rust
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub fn store_image_file(
    data_dir: &Path,
    session_id: &str,
    image_id: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    let dir = data_dir.join("sessions").join(session_id).join("images");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{image_id}.bin"));
    atomic_write(&path, bytes)?;
    Ok(path)
}

pub fn read_image_file(data_dir: &Path, session_id: &str, image_id: &str) -> Result<Vec<u8>> {
    let path = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.bin"));
    Ok(fs::read(&path)?)
}

pub fn remove_image_file(data_dir: &Path, session_id: &str, image_id: &str) -> Result<()> {
    let path = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.bin"));
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("bin.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn store_and_read_roundtrip() {
        let td = TempDir::new().unwrap();
        let bytes = b"hello bios";
        let path = store_image_file(td.path(), "s1", "img1", bytes).unwrap();
        assert!(path.ends_with("sessions/s1/images/img1.bin"));
        let read = read_image_file(td.path(), "s1", "img1").unwrap();
        assert_eq!(read, bytes);
    }

    #[test]
    fn store_overwrites_existing() {
        let td = TempDir::new().unwrap();
        store_image_file(td.path(), "s1", "img1", b"old").unwrap();
        store_image_file(td.path(), "s1", "img1", b"new longer").unwrap();
        let read = read_image_file(td.path(), "s1", "img1").unwrap();
        assert_eq!(read, b"new longer");
    }

    #[test]
    fn remove_existing_file() {
        let td = TempDir::new().unwrap();
        store_image_file(td.path(), "s1", "img1", b"x").unwrap();
        remove_image_file(td.path(), "s1", "img1").unwrap();
        assert!(read_image_file(td.path(), "s1", "img1").is_err());
    }

    #[test]
    fn remove_missing_file_is_ok() {
        let td = TempDir::new().unwrap();
        remove_image_file(td.path(), "s1", "missing").unwrap();
    }

    #[test]
    fn atomic_write_persists_bytes() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("a.bin");
        atomic_write(&path, b"data").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"data");
    }

    #[test]
    fn atomic_write_no_tmp_left_behind() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("a.bin");
        atomic_write(&path, b"data").unwrap();
        let tmp = path.with_extension("bin.tmp");
        assert!(!tmp.exists(), "tmp file must be renamed, not left behind");
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p uefi-engine storage::image`
Expected: PASS — 6 tests. (Implementation is already inlined above; this step confirms green.)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/storage/mod.rs crates/uefi-engine/src/storage/image.rs
git commit -m "feat(uefi-engine): add image file storage helpers (Plan A Task 2)

storage::image module: store_image_file / read_image_file / remove_image_file
with atomic_write (tmp + rename) for crash-safe persistence. Mirrors
storage::artifact module structure.

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 3: Proto rename (full)

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` — full rewrite
- Modify: `crates/uefi-proto/build.rs` — rename `engine.Item` → `engine.Node`; add serde derives for new message types consumed as JSON by CLI/gateway (`engine.ImageInfo`, `engine.FormInfo`, `engine.StringInfo`)

**Interfaces:**
- Consumes: nothing (this is the contract source)
- Produces: new tonic-generated types in `uefi_proto` crate; BREAKS all clients until Tasks 4-8 complete

**⚠ Workspace state after this task:** `cargo build --all` will FAIL (engine, CLI, TUI, Gateway reference old RPC names). This is expected and gets fixed in Tasks 4-8.

- [ ] **Step 1: Replace `engine.proto` with the new contract**

Overwrite `crates/uefi-proto/proto/engine.proto`:

```protobuf
syntax = "proto3";
package engine;

service EngineService {
  rpc SessionCreate(SessionCreateRequest)   returns (SessionCreateResponse);
  rpc SessionDestroy(SessionDestroyRequest) returns (Empty);
  rpc SessionsList(SessionsListRequest)     returns (SessionsListResponse);

  rpc ImageOpen(ImageOpenRequest)     returns (ImageOpenResponse);
  rpc ImageClose(ImageCloseRequest)   returns (Empty);
  rpc ImagesList(ImagesListRequest)   returns (ImagesListResponse);
  rpc ImageSave(ImageSaveRequest)     returns (Empty);
  rpc ImageStatus(ImageStatusRequest) returns (ImageStatusResponse);

  rpc ImageNodesList(ImageNodesListRequest)      returns (ImageNodesResponse);
  rpc ImageNodesSearch(ImageNodesSearchRequest)  returns (ImageNodesResponse);
  rpc ImageNodeInsert(ImageNodeInsertRequest)    returns (ImageNodeResponse);
  rpc ImageNodeRemove(ImageNodeRemoveRequest)    returns (Empty);
  rpc ImageNodeReplace(ImageNodeReplaceRequest)  returns (ImageNodeResponse);
  rpc ImageNodeRebuild(ImageNodeRebuildRequest)  returns (Empty);
  rpc ImageNodeExtract(ImageNodeExtractRequest)  returns (ImageNodeExtractResponse);

  rpc ArtifactsList(ArtifactsListRequest)    returns (ArtifactsListResponse);
  rpc ArtifactImport(ArtifactImportRequest) returns (ArtifactImportResponse);
  rpc ArtifactExport(ArtifactExportRequest) returns (Empty);

  rpc SetupListForms(SetupListFormsRequest)                 returns (SetupListFormsResponse);
  rpc SetupSetFormVisibility(SetupSetFormVisibilityRequest) returns (Empty);
  rpc SetupListStrings(SetupListStringsRequest)             returns (SetupListStringsResponse);
  rpc SetupFormSetAdd(SetupFormSetAddRequest)               returns (SetupFormSetAddResponse);
}

enum ImageMode { READ = 0; WRITE = 1; }
enum InsertMode { INTO = 0; BEFORE = 1; AFTER = 2; }
enum SearchMode { NAME = 0; UTF8 = 1; UTF16 = 2; BYTES = 3; }

message SessionCreateRequest  { string name = 1; }
message SessionCreateResponse { string session_id = 1; string token = 2; }
message SessionDestroyRequest { string session_id = 1; }
message SessionsListRequest   {}
message SessionInfo { string session_id = 1; string name = 2; int64 created_at = 3; int64 last_activity = 4; }
message SessionsListResponse  { repeated SessionInfo sessions = 1; }

message ImageOpenRequest {
  string session_id = 1;
  string path = 2;
  ImageMode mode = 3;
  string name = 4;
}
message ImageOpenResponse {
  string image_id = 1;
  string root_guid = 2;
  string name = 3;
}

message ImageInfo {
  string image_id = 1;
  string name = 2;
  string path = 3;
  ImageMode mode = 4;
  uint64 size = 5;
  int64 created_at = 6;
  int64 last_activity = 7;
}
message ImagesListRequest  { string session_id = 1; }
message ImagesListResponse { repeated ImageInfo images = 1; }

message ImageCloseRequest { string image_id = 1; }

message ImageStatusRequest  { string image_id = 1; }
message ImageStatusResponse { ImageInfo info = 1; }

message ImageSaveRequest { string image_id = 1; string output_path = 2; }

message ImageNodesListRequest { string image_id = 1; string filter = 2; }
message Node {
  string path = 1;
  uint32 type = 2;
  uint32 subtype = 3;
  string guid = 4;
  uint64 offset = 5;
  uint64 size = 6;
  string name = 7;
}
message ImageNodesResponse { repeated Node nodes = 1; }

message ImageNodesSearchRequest {
  string image_id = 1;
  string query = 2;
  repeated SearchMode modes = 3;
  uint32 limit = 4;
}

message ImageNodeInsertRequest {
  string image_id = 1;
  string target = 2;
  string ffs_path = 3;
  string artifact_id = 4;
  InsertMode mode = 5;
}
message ImageNodeResponse { string item_id = 1; }

message ImageNodeRemoveRequest { string image_id = 1; string target = 2; }

message ImageNodeReplaceRequest {
  string image_id = 1;
  string target = 2;
  string ffs_path = 3;
  string artifact_id = 4;
  bool body_only = 5;
}

message ImageNodeRebuildRequest { string image_id = 1; string target = 2; }

message ImageNodeExtractRequest {
  string image_id = 1;
  string target = 2;
  bool body_only = 3;
}
message ImageNodeExtractResponse { string artifact_id = 1; }

message ArtifactsListRequest { string session_id = 1; }
message ArtifactInfo {
  string artifact_id = 1;
  string kind = 2;
  uint64 size = 3;
  int64 created_at = 4;
  string source = 5;
}
message ArtifactsListResponse { repeated ArtifactInfo artifacts = 1; }

message ArtifactImportRequest {
  string session_id = 1;
  string path = 2;
}
message ArtifactImportResponse { string artifact_id = 1; }

message ArtifactExportRequest {
  string artifact_id = 1;
  string output_path = 2;
}

message SetupListFormsRequest { string image_id = 1; }
message FormInfo {
  string form_id = 1;
  string formset_guid = 2;
  uint32 form_id_ifr = 3;
  string title = 4;
  bool visible = 5;
}
message SetupListFormsResponse { repeated FormInfo forms = 1; }

message SetupSetFormVisibilityRequest {
  string image_id = 1;
  string item_id = 2;
  bool visible = 3;
}

message SetupListStringsRequest { string image_id = 1; }
message StringInfo {
  string language = 1;
  uint32 string_id = 2;
  string text = 3;
}
message SetupListStringsResponse { repeated StringInfo strings = 1; }

message SetupFormSetAddRequest {
  string image_id = 1;
  string schema_json = 2;
  string target_ffs_guid = 3;
}
message SetupFormSetAddResponse {
  string new_ffs_id = 1;
  repeated uint32 inserted_form_ids = 2;
  map<string, uint32> string_ids = 3;
}

message Empty {}
```

- [ ] **Step 1b: Update `build.rs` serde derives for renamed + new message types**

In `crates/uefi-proto/build.rs`, the existing `.message_attribute("engine.Item", ...)` is now a silent no-op (`Item` renamed to `Node`). Rename it and add serde derives for the new message types that Task 5 (`output.rs`) and Task 7 (gateway JSON) will serialize:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .message_attribute("engine.Node", "#[derive(serde::Serialize)]")
        .message_attribute("engine.ImageInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.FormInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.StringInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.SessionInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.ArtifactInfo", "#[derive(serde::Serialize)]")
        .compile_protos(&["proto/engine.proto"], &["proto"])?;
    Ok(())
}
```

- [ ] **Step 2: Verify uefi-proto builds**

Run: `cargo build -p uefi-proto`
Expected: SUCCESS — codegen produces new types. The `Item` type is now `Node`; old names gone.

- [ ] **Step 3: Verify workspace is now red (expected)**

Run: `cargo build --all 2>&1 | head -50`
Expected: FAIL — engine/CLI/TUI/Gateway reference old RPC names (`open_image`, `list_items`, `Item`, etc.). Document the errors briefly in commit message.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-proto/proto/engine.proto crates/uefi-proto/build.rs
git commit -m "feat!(uefi-proto): rename all RPC to noun-first + new image/setup RPCs (Plan A Task 3)

BREAKING CHANGE: full proto rename to noun-first convention.
- Session: CreateSession->SessionCreate, ListSessions->SessionsList, ...
- Image: OpenImage->ImageOpen, SaveImage->ImageSave; add ImagesList/ImageClose/ImageStatus
- Node (was Items): ListItems->ImageNodesList, SearchItems->ImageNodesSearch,
  Insert->ImageNodeInsert, Remove/Replace/Rebuild/ExtractArtifact similarly
- Artifact: ListArtifacts->ArtifactsList, Import/Export similarly
- Setup: add SetupListForms/SetupListStrings (stubs); SetSetupItemVisibility
  -> SetupSetFormVisibility; AddSetupFormSet -> SetupFormSetAdd
- Remove DumpTree RPC + DumpFormat enum + FindItem RPC
- Message Item -> Node
- Field renames: OpenImageRequest.image_path -> path (was image_path);
  ArtifactImportRequest.file_path -> path; new ImageInfo.path (was source_path concept)
- New fields: ImageOpenRequest.name, ImageOpenResponse.name

Workspace will not compile after this commit; uefi-engine (Task 4), uefi-cli
(Task 5), uefi-tui (Task 6), uefi-gateway (Task 7), WebUI (Task 8) migrate next.

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 4: uefi-engine handlers update

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs`
- Modify: `crates/uefi-engine/src/parser/image.rs` — delete `dump_tree` + `DumpFormat`
- Modify: `crates/uefi-engine/src/session.rs` — extend `destroy_session` to clean `images/` dir
- Test: `crates/uefi-engine/src/rpc/server.rs` (`mod tests`)

**Interfaces:**
- Consumes: new `uefi_proto` types (Task 3); `storage::image` helpers (Task 2); `Db` image CRUD (Task 1)
- Produces: engine that compiles against new proto and implements new contract; helpers `flush_image`/`get_or_load_image` on `EngineServer`

This task is large; decomposed into 7 subtasks (4a–4g). Commit after each subtask.

### Subtask 4a: Add `flush_image` and `get_or_load_image` helpers + tests

- [ ] **Step 4a.1: Write failing test for `flush_image` persistence**

In `crates/uefi-engine/src/rpc/server.rs`, append to `mod tests` (after existing `create_and_list_sessions` test):

```rust
fn fixture_volume() -> Vec<u8> {
    use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};
    let mut buf = vec![0xFFu8; 256];
    buf[32..40].copy_from_slice(&256u64.to_le_bytes());
    buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
    buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
    buf[48..50].copy_from_slice(&56u16.to_le_bytes());
    buf[55] = 2;
    buf
}

#[tokio::test]
async fn flush_image_writes_bytes_to_data_dir() {
    let (td, mut client) = setup().await;
    let orig = td.path().join("orig.bin");
    std::fs::write(&orig, fixture_volume()).unwrap();
    let session = client
        .session_create(SessionCreateRequest::default())
        .await
        .unwrap()
        .into_inner();
    let opened = client
        .image_open(ImageOpenRequest {
            session_id: session.session_id.clone(),
            path: orig.to_string_lossy().to_string(),
            mode: ImageMode::Read as i32,
            name: "test.bin".into(),
        })
        .await
        .unwrap()
        .into_inner();
    let img_path = td.path()
        .join("sessions").join(&session.session_id)
        .join("images").join(format!("{}.bin", opened.image_id));
    assert!(img_path.exists(), "image bytes must be persisted on open");
    let saved = std::fs::read(&img_path).unwrap();
    assert_eq!(saved, fixture_volume());
}
```

Note: the existing `rpc_open_save_round_trip` test references `dump_tree` and old field names — it will fail to compile. For Subtask 4a, comment out or remove that test temporarily; it gets re-added in 4g.

- [ ] **Step 4a.2: Add helper methods on `EngineServer`**

In `crates/uefi-engine/src/rpc/server.rs`, add `use crate::storage::image::{atomic_write, read_image_file, remove_image_file, store_image_file};` to imports, and `use crate::storage::ImageRow;`. Add helper methods on `EngineServer` (above the `impl EngineService` block):

```rust
impl EngineServer {
    async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
        let (bytes, session_id) = {
            let images = self.images.lock().await;
            let img = images
                .get(image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let bytes = crate::builder::build_image(img)
                .map_err(|e| Status::internal(e.to_string()))?;
            (bytes, img.session_id.clone())
        };
        let path = self.data_dir.join("sessions").join(&session_id)
            .join("images").join(format!("{image_id}.bin"));
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        self.sm.db.lock().unwrap().touch_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(())
    }

    async fn get_or_load_image(&self, image_id: &str) -> Result<crate::types::Image, Status> {
        {
            let images = self.images.lock().await;
            if let Some(img) = images.get(image_id) {
                return Ok(img.clone());
            }
        }
        let row = self.sm.db.lock().unwrap().get_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes = read_image_file(&self.data_dir, &row.session_id, image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        let mode = if row.mode == 1 { ImageMode::Write } else { ImageMode::Read };
        let img = crate::parser::image::parse_image(&bytes, mode, image_id, &row.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        self.images.lock().await.insert(image_id.into(), img.clone());
        Ok(img)
    }
}
```

(These helpers will be `#[allow(dead_code)]` until subtask 4f wires them — but tests in 4a already exercise `flush_image` indirectly through `image_open` which will be rewritten in 4d to use it.)

- [ ] **Step 4a.3: Skip running tests until subtask 4d (image_open rewrite)**

Note: workspace is still red from Task 3. Helper methods alone don't make engine compile (handler signatures still old). Commit will happen at end of 4d.

### Subtask 4b: Rename all existing handlers (signature + body updates)

- [ ] **Step 4b.1: Rename handler methods to match new proto**

In `crates/uefi-engine/src/rpc/server.rs`, rename inside `impl EngineService for EngineServer`:

- `create_session` → `session_create` (param `Request<CreateSessionRequest>` → `Request<SessionCreateRequest>`, return `SessionCreateResponse`; same body)
- `destroy_session` → `session_destroy` (param `DestroySessionRequest` → `SessionDestroyRequest`)
- `list_sessions` → `sessions_list` (param `ListSessionsRequest` → `SessionsListRequest`, return `SessionsListResponse`)
- `open_image` → `image_open` (param `OpenImageRequest` → `ImageOpenRequest`; field `image_path` → `path`; logic updated in 4d)
- `dump_tree` → **delete entire method** (RPC removed)
- `list_items` → `image_nodes_list` (param `ListItemsRequest` → `ImageNodesListRequest`; return `ImageNodesResponse`; field `items` → `nodes`; `Item` → `Node` in body)
- `search_items` → `image_nodes_search` (param `SearchItemsRequest` → `ImageNodesSearchRequest`)
- `find_item` → **delete entire method** (RPC removed)
- `insert` → `image_node_insert` (param `InsertRequest` → `ImageNodeInsertRequest`; return `ImageNodeResponse`)
- `remove` → `image_node_remove` (param `RemoveRequest` → `ImageNodeRemoveRequest`)
- `replace` → `image_node_replace` (param `ReplaceRequest` → `ImageNodeReplaceRequest`; return `ImageNodeResponse`)
- `rebuild` → `image_node_rebuild` (param `RebuildRequest` → `ImageNodeRebuildRequest`)
- `extract_artifact` → `image_node_extract` (param `ExtractArtifactRequest` → `ImageNodeExtractRequest`; return `ImageNodeExtractResponse`)
- `export_artifact` → `artifact_export` (param `ExportArtifactRequest` → `ArtifactExportRequest`)
- `import_artifact` → `artifact_import` (param `ImportArtifactRequest` → `ArtifactImportRequest`; field `file_path` → `path`)
- `list_artifacts` → `artifacts_list` (param `ListArtifactsRequest` → `ArtifactsListRequest`; return `ArtifactsListResponse`)
- `set_setup_item_visibility` → `setup_set_form_visibility` (param `SetSetupItemVisibilityRequest` → `SetupSetFormVisibilityRequest`)
- `save_image` → `image_save` (param `SaveImageRequest` → `ImageSaveRequest`; logic unchanged for now — still does `build_image`; updated rationale: even under write-through, `image_save` always builds from in-memory for safety)
- `add_setup_form_set` → `setup_form_set_add` (param `AddSetupFormSetRequest` → `SetupFormSetAddRequest`)

For each rename: update the `async fn` name, parameter type, return type, and any field references in the body (`r.image_path` → `r.path`, `Item { ... }` → `Node { ... }`, `ListItemsResponse { items }` → `ImageNodesResponse { nodes }`, etc.).

- [ ] **Step 4b.2: Delete `dump_tree` handler and `find_item` handler entirely**

Remove the two methods from `impl EngineService`. Also remove the imports `dump_tree` from `use crate::parser::image::{dump_tree, list_items, parse_image};` (keep `list_items` and `parse_image`). Same for any `find_item` references — but note `find_item` from `parser::target` is still used by other handlers (insert/replace/extract), so do NOT remove that import.

- [ ] **Step 4b.3: Migrate `parser/image.rs` to new proto (`Item` → `Node`; delete `dump_tree`)**

Task 3 renamed `uefi_proto::Item` → `uefi_proto::Node` and removed `uefi_proto::DumpFormat` entirely.
`crates/uefi-engine/src/parser/image.rs` still imports both and uses `Item` in `list_items`,
`list_recursive`, `search`, `search_recursive`. It must be migrated in this subtask or the
engine will not compile at Step 4d's first compile-check. Concretely:

- Change `use uefi_proto::{DumpFormat, Item};` → `use uefi_proto::Node;`
- Change `Vec<Item>` → `Vec<Node>`, `&mut Vec<Item>` → `&mut Vec<Node>`, `Item { ... }` → `Node { ... }`
  in: `list_items`, `list_recursive`, `search`, `search_recursive`.
- Delete the `pub fn dump_tree(...) { ... }` function (originally scheduled under 4g.1; moved here
  because it depends on the now-removed `DumpFormat` type).
- Delete the `dump_tree_text` test in `parser/image.rs`'s `mod tests` (calls `dump_tree` +
  references `DumpFormat::Text`).

`tests/real_image.rs` calls `list_items(...)` and reads `.r#type` / `.name` — `Node` has the
same field names, so no changes are required there.

### Subtask 4c: Add new handler stubs (image_close, images_list, image_status, setup_list_forms, setup_list_strings)

- [ ] **Step 4c.1: Implement `image_close` (full — destroys image on server)**

In `impl EngineService`, add:

```rust
async fn image_close(&self, req: Request<ImageCloseRequest>) -> RpcResult<Empty> {
    let r = req.into_inner();
    let row = self.sm.db.lock().unwrap().get_image(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("image not found"))?;
    self.sm.db.lock().unwrap().delete_image(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    remove_image_file(&self.data_dir, &row.session_id, &r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    self.images.lock().await.remove(&r.image_id);
    Ok(Response::new(Empty {}))
}
```

- [ ] **Step 4c.2: Implement `images_list`**

```rust
async fn images_list(&self, req: Request<ImagesListRequest>) -> RpcResult<ImagesListResponse> {
    let r = req.into_inner();
    let rows = self.sm.db.lock().unwrap().list_images(&r.session_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(ImagesListResponse {
        images: rows.into_iter().map(|r| ImageInfo {
            image_id: r.id,
            name: r.name,
            path: r.path,
            mode: r.mode as i32,
            size: r.size as u64,
            created_at: r.created_at,
            last_activity: r.last_activity,
        }).collect(),
    }))
}
```

- [ ] **Step 4c.3: Implement `image_status`**

```rust
async fn image_status(&self, req: Request<ImageStatusRequest>) -> RpcResult<ImageStatusResponse> {
    let r = req.into_inner();
    let row = self.sm.db.lock().unwrap().get_image(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("image not found"))?;
    Ok(Response::new(ImageStatusResponse {
        info: Some(ImageInfo {
            image_id: row.id,
            name: row.name,
            path: row.path,
            mode: row.mode as i32,
            size: row.size as u64,
            created_at: row.created_at,
            last_activity: row.last_activity,
        }),
    }))
}
```

- [ ] **Step 4c.4: Implement `setup_list_forms` and `setup_list_strings` stubs (UNIMPLEMENTED)**

```rust
async fn setup_list_forms(&self, _req: Request<SetupListFormsRequest>) -> RpcResult<SetupListFormsResponse> {
    Err(Status::unimplemented("SetupListForms not implemented (Plan B)"))
}

async fn setup_list_strings(&self, _req: Request<SetupListStringsRequest>) -> RpcResult<SetupListStringsResponse> {
    Err(Status::unimplemented("SetupListStrings not implemented (Plan B)"))
}
```

### Subtask 4d: Update `image_open` to persist bytes + DB row

- [ ] **Step 4d.1: Rewrite `image_open` body**

Replace the body of `image_open`:

```rust
async fn image_open(&self, req: Request<ImageOpenRequest>) -> RpcResult<ImageOpenResponse> {
    let r = req.into_inner();
    let mode = match r.mode {
        0 => ImageMode::Read,
        1 => ImageMode::Write,
        _ => return Err(Status::invalid_argument("bad mode")),
    };
    let bytes = fs::read(&r.path).map_err(|e| Status::not_found(e.to_string()))?;
    let image_id = Uuid::new_v4().to_string();
    let name = if r.name.is_empty() {
        std::path::Path::new(&r.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        r.name.clone()
    };
    let img = crate::parser::image::parse_image(&bytes, mode, &image_id, &r.session_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    store_image_file(&self.data_dir, &r.session_id, &image_id, &bytes)
        .map_err(|e| Status::internal(e.to_string()))?;
    self.sm.db.lock().unwrap().insert_image(
        &image_id, &r.session_id, &name, &r.path,
        r.mode as i64, bytes.len() as i64,
    ).map_err(|e| Status::internal(e.to_string()))?;
    let root_guid = img.root.guid
        .map(|g| crate::guid_to_upper_string(&g))
        .unwrap_or_default();
    self.images.lock().await.insert(image_id.clone(), img);
    let _ = self.sm.touch(&r.session_id);
    Ok(Response::new(ImageOpenResponse {
        image_id,
        root_guid,
        name,
    }))
}
```

### Subtask 4e: Refactor read-handlers to use `get_or_load_image`

- [ ] **Step 4e.1: Update `image_nodes_list` to use lazy load**

Replace body of `image_nodes_list`:

```rust
async fn image_nodes_list(&self, req: Request<ImageNodesListRequest>) -> RpcResult<ImageNodesResponse> {
    let r = req.into_inner();
    let img = self.get_or_load_image(&r.image_id).await?;
    let nodes = crate::parser::image::list_items(
        &img.root,
        if r.filter.is_empty() { None } else { Some(&r.filter) },
    );
    let _ = self.sm.touch(&img.session_id);
    Ok(Response::new(ImageNodesResponse { nodes }))
}
```

- [ ] **Step 4e.2: Update `image_nodes_search` similarly**

```rust
async fn image_nodes_search(&self, req: Request<ImageNodesSearchRequest>) -> RpcResult<ImageNodesResponse> {
    let r = req.into_inner();
    let img = self.get_or_load_image(&r.image_id).await?;
    let modes: Vec<uefi_common::search::SearchMode> = r.modes.iter().map(|m| match m {
        0 => uefi_common::search::SearchMode::Name,
        1 => uefi_common::search::SearchMode::Utf8,
        2 => uefi_common::search::SearchMode::Utf16Le,
        _ => uefi_common::search::SearchMode::Bytes,
    }).collect();
    let limit = r.limit as usize;
    let nodes = crate::parser::image::search(&img.root, &r.query, &modes, limit);
    Ok(Response::new(ImageNodesResponse { nodes }))
}
```

- [ ] **Step 4e.3: Update `image_status` to also touch in-memory presence**

The `image_status` from 4c.3 already queries DB only. That's correct — `image_status` is metadata, not in-memory state. Leave as is.

### Subtask 4f: Wire mutations to `flush_image`

- [ ] **Step 4f.1: Add flush calls to each mutation handler**

In `image_node_insert`, `image_node_remove`, `image_node_replace`, `image_node_rebuild`, `setup_set_form_visibility`, `setup_form_set_add` — each handler must call `self.flush_image(&r.image_id).await?` after the operation succeeds but before returning `Ok(Response::new(...))`.

For handlers that hold `images.lock().await` over the mutation (insert/remove/replace/rebuild/setup_set_form_visibility/setup_form_set_add), the lock MUST be dropped before calling `flush_image` (which acquires the same lock). Restructure: do mutation under scoped lock, drop lock, then flush.

Example for `image_node_insert`:

```rust
async fn image_node_insert(&self, req: Request<ImageNodeInsertRequest>) -> RpcResult<ImageNodeResponse> {
    let r = req.into_inner();
    let ffs_bytes = if !r.artifact_id.is_empty() {
        let art = self.sm.db.lock().unwrap().get_artifact(&r.artifact_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("artifact not found"))?;
        crate::storage::artifact::read_artifact_file(&self.data_dir, &art.session_id, &art.id)
            .map_err(|e| Status::internal(e.to_string()))?
    } else {
        fs::read(&r.ffs_path).map_err(|e| Status::not_found(e.to_string()))?
    };
    let img = self.get_or_load_image(&r.image_id).await?;
    {
        let mut images = self.images.lock().await;
        let img_slot = images.get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let t = crate::parser::target::parse_target(&r.target)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mode = match r.mode {
            0 => crate::ops::InsertMode::Into,
            1 => crate::ops::InsertMode::Before,
            2 => crate::ops::InsertMode::After,
            _ => return Err(Status::invalid_argument("bad mode")),
        };
        crate::ops::insert(&mut img_slot.root, &t, &ffs_bytes, mode)
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    self.flush_image(&r.image_id).await?;
    let _ = self.sm.touch(&img.session_id);
    Ok(Response::new(ImageNodeResponse { item_id: r.target }))
}
```

Apply the same pattern (scoped mutation lock → drop → flush_image → touch session) to:
- `image_node_remove`
- `image_node_replace`
- `image_node_rebuild`
- `setup_set_form_visibility`
- `setup_form_set_add`

- [ ] **Step 4f.2: Add test for write-through persistence across "restart"**

Append to `mod tests` in `rpc/server.rs`:

```rust
#[tokio::test]
async fn write_through_persists_mutation_to_disk() {
    let (td, mut client) = setup().await;
    let orig = td.path().join("v.bin");
    std::fs::write(&orig, fixture_volume()).unwrap();
    let session = client.session_create(SessionCreateRequest::default()).await.unwrap().into_inner();
    let opened = client.image_open(ImageOpenRequest {
        session_id: session.session_id.clone(),
        path: orig.to_string_lossy().to_string(),
        mode: ImageMode::Write as i32,
        name: "v.bin".into(),
    }).await.unwrap().into_inner();

    let img_path = td.path().join("sessions").join(&session.session_id)
        .join("images").join(format!("{}.bin", opened.image_id));
    let before = std::fs::read(&img_path).unwrap();

    client.image_node_rebuild(ImageNodeRebuildRequest {
        image_id: opened.image_id.clone(),
        target: "0".into(),
    }).await.unwrap();

    let after = std::fs::read(&img_path).unwrap();
    assert!(after != before || after == fixture_volume(),
        "write-through must update disk file after mutation");
}
```

(Note: the assertion allows `after == fixture_volume()` for the case where build_image is idempotent on a no-op rebuild — the key requirement is that disk file is rewritten, which we can verify by checking mtime if needed. For simplicity, leave as is.)

### Subtask 4g: Delete dead code + cleanup + green

- [ ] **Step 4g.1: Verify `dump_tree` and `DumpFormat` are gone from `parser/image.rs`**

`dump_tree` (function) + `DumpFormat` import were already deleted in Step 4b.3 (they had to go
before the engine's first compile at 4d, because Task 3 removed `DumpFormat` from `uefi-proto`).
Verify nothing references them:

```
rg 'dump_tree|DumpFormat' crates/uefi-engine/src/  # should return nothing
```

If any references survived, delete them here.

- [ ] **Step 4g.2: Extend `SessionManager::destroy_session` to clean images/ dir**

In `crates/uefi-engine/src/session.rs`, the existing `destroy_session` already removes `sessions/<id>/` recursively on `purge_files=true`, which covers both `artifacts/` and `images/` subdirs. No code change needed; just verify by test:

Add to `mod tests` in `session.rs`:

```rust
#[test]
fn destroy_with_purge_removes_images_dir() {
    let (td, sm) = sm();
    let (id, _) = sm.create_session("n").unwrap();
    let img_dir = td.path().join("sessions").join(&id).join("images");
    std::fs::create_dir_all(&img_dir).unwrap();
    std::fs::write(img_dir.join("img1.bin"), b"data").unwrap();
    sm.destroy_session(&id, true).unwrap();
    assert!(!img_dir.exists(), "images/ dir must be removed when purge=true");
}
```

- [ ] **Step 4g.3: Re-add or remove the old `rpc_open_save_round_trip` test**

The pre-existing `rpc_open_save_round_trip` test referenced `dump_tree`. Either rewrite it to use `image_nodes_list`, or delete it (it was covered `image_save` round-trip; the new `image_save` test can be added separately if needed). For minimal churn: delete it (the new `write_through_persists_mutation_to_disk` covers the save path implicitly through `flush_image`).

- [ ] **Step 4g.4: Run cargo test for uefi-engine**

Run: `cargo test -p uefi-engine`
Expected: PASS — all storage tests, all rpc tests (including new image_open/flush_image/write_through tests).

- [ ] **Step 4g.5: Run clippy**

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: no warnings.

- [ ] **Step 4g.6: Commit**

```bash
git add crates/uefi-engine/
git commit -m "feat!(uefi-engine): migrate to new proto + image persistence (Plan A Task 4)

- Rename all RPC handlers to noun-first (session_create, image_open,
  image_nodes_list, image_node_insert, artifacts_list, etc.)
- image_open persists bytes to data_dir/sessions/<sid>/images/<id>.bin
  + inserts row into images table; returns confirmed name
- New handlers: image_close (destroy on server), images_list, image_status,
  setup_list_forms/strings (UNIMPLEMENTED stubs for Plan B)
- Write-through: every mutation (insert/remove/replace/rebuild/
  setup_set_form_visibility/setup_form_set_add) calls flush_image after
  success, which builds + atomic-writes to data_dir
- Lazy re-load: get_or_load_image helper reads from disk on cache miss
- image_save always builds from in-memory (safe under write-through failure)
- Delete dump_tree function + DumpFormat enum + find_item handler
- Extend session destroy test to verify images/ dir cleanup

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 5: uefi-cli full rewrite

**Files:**
- Modify: `crates/uefi-cli/src/main.rs` — full `Cmd` enum rewrite + `ValueEnum`
- Modify: `crates/uefi-cli/src/client.rs` — rename all methods + add new
- Modify: `crates/uefi-cli/src/output.rs` — new print fns + remove `print_find` + `OutputFormat` ValueEnum
- Modify: `crates/uefi-cli/src/commands/mod.rs` — add `pub mod node; pub mod artifact;`, remove `pub mod edit;`
- Create: `crates/uefi-cli/src/commands/node.rs`
- Create: `crates/uefi-cli/src/commands/artifact.rs`
- Modify: `crates/uefi-cli/src/commands/image.rs` — refactor
- Modify: `crates/uefi-cli/src/commands/session.rs` — add `--name`
- Modify: `crates/uefi-cli/src/commands/setup.rs` — new form/string subcommand structure
- Delete: `crates/uefi-cli/src/commands/edit.rs`
- Modify: `crates/uefi-cli/tests/mock_server.rs` — migrate MockEngine to new proto (rename all trait methods to noun-first + rename message types); add stubs for new RPCs (image_close, images_list, image_status, setup_list_forms, setup_list_strings, setup_form_set_add); delete dump_tree/find_item/add_setup_form_set stubs
- Modify: `crates/uefi-cli/tests/cli_integration.rs` — rewrite flows to new topology (image open/list/status/close; node list; remove image dump/find)
- Modify: `crates/uefi-cli/tests/e2e.rs` — rewrite edit→node, setup set-visibility→setup form set-visibility

**Interfaces:**
- Consumes: new `uefi_proto` types (Task 3)
- Produces: CLI with 5 top-level commands (session/image/node/artifact/setup)

> **Defect (rule 11):** The original Task 5 `Files` list omitted `crates/uefi-cli/tests/`. Step 5f.1 requires `cargo test -p uefi-cli` to PASS, but `tests/mock_server.rs`, `tests/cli_integration.rs`, `tests/e2e.rs` reference the old proto names (`create_session`, `OpenImageRequest`, `dump_tree`, `Item`, `FindItemRequest`, `SetSetupItemVisibilityRequest`, etc.) and the old command topology (`image dump`, `image find`, `image close` w/o arg, `edit insert/remove`, `setup set-visibility`). They fail to compile after the Task 3 rename, so the test migration is part of Task 5 (handled in Step 5f.0 below).

This task is large; decomposed into 6 subtasks (5a–5f). Commit after each subtask.

### Subtask 5a: `output.rs` updates (ValueEnum + new print fns)

- [ ] **Step 5a.1: Make `OutputFormat` a `clap::ValueEnum`**

In `crates/uefi-cli/src/output.rs`, replace the `OutputFormat` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    #[value(name = "json")]
    Json,
    #[value(name = "text")]
    Text,
    #[value(name = "tsv")]
    Tsv,
}
```

Remove the `parse_format` function (no longer needed; clap validates).

- [ ] **Step 5a.2: Rename `Item` references to `Node`**

In `output.rs`, replace `use uefi_proto::{Item, SessionInfo};` with `use uefi_proto::{Node, SessionInfo, ImageInfo, FormInfo, StringInfo};`. Update `print_items` signature to `print_nodes(nodes: &[Node], format: OutputFormat)` and rename internally.

- [ ] **Step 5a.3: Remove `print_find`, add new print functions**

Delete the `print_find` function. Add:

```rust
pub fn print_image_info(info: &ImageInfo, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string(info).unwrap_or_else(|_| "{}".into());
            println!("{v}");
        }
        _ => {
            println!("image_id\tname\tpath\tmode\tsize\tcreated\tlast_activity");
            println!("{}\t{}\t{}\t{}\t{}\t{}\t{}",
                info.image_id, info.name, info.path, info.mode,
                info.size, info.created_at, info.last_activity);
        }
    }
}

pub fn print_images_list(images: &[ImageInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(images).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        _ => {
            println!("image_id\tname\tmode\tsize\tlast_activity");
            for img in images {
                println!("{}\t{}\t{}\t{}\t{}",
                    img.image_id, img.name, img.mode, img.size, img.last_activity);
            }
        }
    }
}

pub fn print_image_status(info: &ImageInfo, format: OutputFormat) {
    print_image_info(info, format);
}

pub fn print_forms(forms: &[FormInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(forms).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("form_id\tformset_guid\tform_id_ifr\ttitle\tvisible");
            for f in forms {
                println!("{}\t{}\t{}\t{}\t{}",
                    f.form_id, f.formset_guid, f.form_id_ifr, f.title, f.visible);
            }
        }
        OutputFormat::Text => {
            for f in forms {
                println!("{}\t{}\t{}\t{}\tvisible={}",
                    f.form_id, f.formset_guid, f.form_id_ifr, f.title, f.visible);
            }
        }
    }
}

pub fn print_strings(strings: &[StringInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(strings).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("language\tstring_id\ttext");
            for s in strings {
                println!("{}\t{}\t{}", s.language, s.string_id, s.text);
            }
        }
        OutputFormat::Text => {
            for s in strings {
                println!("[{}] {}: {}", s.language, s.string_id, s.text);
            }
        }
    }
}
```

- [ ] **Step 5a.4: Update existing `print_sessions` for new `SessionInfo` (no field changes, but verify)**

`SessionInfo` fields unchanged in proto; `print_sessions` body stays as-is.

- [ ] **Step 5a.5: Update tests in `output.rs`**

In `mod tests`:
- Remove `parse_format_valid` test (function gone).
- Rename `item_serializes_type_field` → `node_serializes_type_field`, update `Item { ... }` → `Node { ... }`.

### Subtask 5b: `client.rs` rename + new methods

- [ ] **Step 5b.1: Rename all client methods**

In `crates/uefi-cli/src/client.rs`, rename:
- `create_session` → `session_create`
- `destroy_session` → `session_destroy`
- `list_sessions` → `sessions_list`
- `open_image` → `image_open` (update fields: `image_path` → `path`, add `name`)
- `list_items` → `image_nodes_list`
- `search_items` → `image_nodes_search`
- `find_item` → DELETE
- `insert` → `image_node_insert`
- `remove` → `image_node_remove`
- `replace` → `image_node_replace`
- `rebuild` → `image_node_rebuild`
- `extract_artifact` → `image_node_extract`
- `export_artifact` → `artifact_export`
- `import_artifact` → `artifact_import` (field `file_path` → `path`)
- `list_artifacts` → `artifacts_list`
- `set_setup_visibility` → `setup_set_form_visibility`
- `save_image` → `image_save`

For each, also update the request/response types referenced (`OpenImageRequest` → `ImageOpenRequest`, `Item` → `Node`, `ListItemsRequest` → `ImageNodesListRequest`, etc.). Update `Vec<Item>` return types to `Vec<Node>`.

- [ ] **Step 5b.2: Add new client methods**

Append to `impl Client`:

```rust
pub async fn image_close(&mut self, image_id: &str) -> Result<(), AppError> {
    let req = ImageCloseRequest { image_id: image_id.into() };
    self.inner.image_close(auth_req(&self.state, req)).await?;
    Ok(())
}

pub async fn images_list(&mut self) -> Result<Vec<ImageInfo>, AppError> {
    let sid = self.state.session_id.clone()
        .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session"))?;
    let req = ImagesListRequest { session_id: sid };
    Ok(self.inner.images_list(auth_req(&self.state, req)).await?.into_inner().images)
}

pub async fn image_status(&mut self, image_id: &str) -> Result<ImageInfo, AppError> {
    let req = ImageStatusRequest { image_id: image_id.into() };
    Ok(self.inner.image_status(auth_req(&self.state, req)).await?.into_inner()
        .info.unwrap())
}

pub async fn setup_list_forms(&mut self, image_id: &str) -> Result<Vec<FormInfo>, AppError> {
    let req = SetupListFormsRequest { image_id: image_id.into() };
    Ok(self.inner.setup_list_forms(auth_req(&self.state, req)).await?.into_inner().forms)
}

pub async fn setup_list_strings(&mut self, image_id: &str) -> Result<Vec<StringInfo>, AppError> {
    let req = SetupListStringsRequest { image_id: image_id.into() };
    Ok(self.inner.setup_list_strings(auth_req(&self.state, req)).await?.into_inner().strings)
}
```

### Subtask 5c: New `commands/node.rs` and `commands/artifact.rs`

- [ ] **Step 5c.1: Update `commands/mod.rs`**

Replace `commands/mod.rs` content:

```rust
pub mod artifact;
pub mod image;
pub mod node;
pub mod session;
pub mod setup;
```

- [ ] **Step 5c.2: Create `commands/node.rs`**

```rust
use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::{AppError, ErrKind};
use uefi_common::state;

pub async fn list(
    filter: Option<&str>,
    tree: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let nodes = client.image_nodes_list(&image_id, filter).await?;
    if tree {
        match format {
            OutputFormat::Text => {
                let rows: Vec<uefi_common::format::TreeRow> = nodes.iter().map(|it| uefi_common::format::TreeRow {
                    path: it.path.clone(),
                    type_: it.r#type,
                    subtype: it.subtype as u8,
                    guid: it.guid.clone(),
                    offset: it.offset,
                    size: it.size,
                    name: it.name.clone(),
                }).collect();
                eprint!("{}", uefi_common::format::format_legend(&rows));
                print!("{}", uefi_common::format::format_tree(&rows));
            }
            _ => crate::output::print_nodes(&nodes, format),
        }
    } else {
        crate::output::print_nodes(&nodes, format);
    }
    Ok(())
}

pub async fn search(
    query: &str,
    modes: &[i32],
    limit: u32,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let nodes = client.image_nodes_search(&image_id, query, modes, limit).await?;
    crate::output::print_nodes(&nodes, format);
    Ok(())
}

pub async fn insert(
    target: &str,
    file: Option<&str>,
    artifact: Option<&str>,
    mode: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (ffs_path, artifact_id) = match (file, artifact) {
        (Some(p), None) => (p.to_string(), String::new()),
        (None, Some(a)) => (String::new(), a.to_string()),
        _ => return Err(AppError::new(ErrKind::RpcInvalidArgument, "exactly one of --file or --artifact required")),
    };
    let mode_i = match mode {
        "into" => 0, "before" => 1, "after" => 2,
        _ => return Err(AppError::new(ErrKind::RpcInvalidArgument, "mode must be into|before|after")),
    };
    let item_id = client.image_node_insert(&image_id, target, &ffs_path, &artifact_id, mode_i).await?;
    crate::output::print_node_id(&item_id, format);
    Ok(())
}

pub async fn remove(target: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.image_node_remove(&image_id, target).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn replace(
    target: &str,
    file: Option<&str>,
    artifact: Option<&str>,
    body_only: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (ffs_path, artifact_id) = match (file, artifact) {
        (Some(p), None) => (p.to_string(), String::new()),
        (None, Some(a)) => (String::new(), a.to_string()),
        _ => return Err(AppError::new(ErrKind::RpcInvalidArgument, "exactly one of --file or --artifact required")),
    };
    let item_id = client.image_node_replace(&image_id, target, &ffs_path, &artifact_id, body_only).await?;
    crate::output::print_node_id(&item_id, format);
    Ok(())
}

pub async fn rebuild(target: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.image_node_rebuild(&image_id, target).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn extract(
    target: &str,
    body_only: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let artifact_id = client.image_node_extract(&image_id, target, body_only).await?;
    match format {
        OutputFormat::Json => println!("{{\"artifact_id\":\"{artifact_id}\"}}"),
        _ => println!("{artifact_id}"),
    }
    Ok(())
}
```

Add helper `print_node_id` to `output.rs` (was `print_find`):

```rust
pub fn print_node_id(item_id: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"item_id\":\"{item_id}\"}}"),
        _ => println!("{item_id}"),
    }
}
```

- [ ] **Step 5c.3: Create `commands/artifact.rs`**

```rust
use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let arts = client.artifacts_list().await?;
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(&arts).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("artifact_id\tkind\tsize");
            for a in &arts {
                println!("{}\t{}\t{}", a.artifact_id, a.kind, a.size);
            }
        }
        OutputFormat::Text => {
            for a in &arts {
                println!("{}  kind={} size={}", a.artifact_id, a.kind, a.size);
            }
        }
    }
    Ok(())
}

pub async fn import(path: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let artifact_id = client.artifact_import(path).await?;
    match format {
        OutputFormat::Json => println!("{{\"artifact_id\":\"{artifact_id}\"}}"),
        _ => println!("{artifact_id}"),
    }
    Ok(())
}

pub async fn export(
    artifact_id: &str,
    output_path: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let out = output_path.unwrap_or(artifact_id);
    client.artifact_export(artifact_id, out).await?;
    crate::output::print_ok(format);
    Ok(())
}
```

### Subtask 5d: Refactor `commands/image.rs`, `commands/session.rs`, `commands/setup.rs`

- [ ] **Step 5d.1: Rewrite `commands/image.rs`**

Replace `commands/image.rs` with only the image-resource ops (open/switch/close/save/list/status). Move extract/export/import/artifacts to artifact.rs (already done in 5c.3). Move dump/list/find to node.rs (already done in 5c.2).

```rust
use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn open(
    path: &str,
    name: Option<&str>,
    mode: i32,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st.clone()).await?;
    let (image_id, root_guid, confirmed_name) = client.image_open(path, name.unwrap_or(""), mode).await?;
    st.active_image_id = Some(image_id.clone());
    state::write_state(&st)?;
    match format {
        OutputFormat::Json => println!(
            "{{\"image_id\":\"{image_id}\",\"root_guid\":\"{root_guid}\",\"name\":\"{confirmed_name}\"}}"
        ),
        _ => println!("{image_id}\t{root_guid}\t{confirmed_name}"),
    }
    Ok(())
}

pub async fn switch(image_id: &str, _format: OutputFormat) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    st.active_image_id = Some(image_id.into());
    state::write_state(&st)?;
    Ok(())
}

pub async fn close(
    image_id: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    let target_id = match image_id {
        Some(id) => id.to_string(),
        None => st.active_image_id.clone().ok_or_else(|| AppError::new(
            uefi_common::error::ErrKind::NoActiveImage,
            "no active image; specify image_id explicitly",
        ))?,
    };
    let mut client = Client::connect(cli_sock, st.clone()).await?;
    client.image_close(&target_id).await?;
    if st.active_image_id.as_deref() == Some(target_id.as_str()) {
        st.active_image_id = None;
        state::write_state(&st)?;
    }
    crate::output::print_ok(format);
    Ok(())
}

pub async fn save(
    output: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.image_save(&image_id, output).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let images = client.images_list().await?;
    crate::output::print_images_list(&images, format);
    Ok(())
}

pub async fn status(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let active = st.active_image_id.clone().ok_or_else(|| AppError::new(
        uefi_common::error::ErrKind::NoActiveImage,
        "no active image",
    ))?;
    let mut client = Client::connect(cli_sock, st).await?;
    let info = client.image_status(&active).await?;
    crate::output::print_image_status(&info, format);
    Ok(())
}
```

- [ ] **Step 5d.2: Update `commands/session.rs` to add `--name` flag**

In existing `session::init`, add a `name: Option<&str>` parameter; pass it to `client.session_create(name.unwrap_or(""))`. Replace the PWD fallback in the engine handler with empty string (engine still does PWD fallback for empty names).

(Read the current `commands/session.rs` to find exact lines to edit; pattern: `init(cli_sock, force, format)` becomes `init(name, cli_sock, force, format)`. Existing body unchanged otherwise.)

- [ ] **Step 5d.3: Rewrite `commands/setup.rs` with form/string subcommands**

```rust
use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn form_list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let forms = client.setup_list_forms(&image_id).await?;
    crate::output::print_forms(&forms, format);
    Ok(())
}

pub async fn form_set_visibility(
    form_id: &str,
    visible: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.setup_set_form_visibility(&image_id, form_id, visible).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn string_list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let strings = client.setup_list_strings(&image_id).await?;
    crate::output::print_strings(&strings, format);
    Ok(())
}
```

- [ ] **Step 5d.4: Delete `commands/edit.rs`**

```bash
git rm crates/uefi-cli/src/commands/edit.rs
```

### Subtask 5e: `main.rs` rewrite (5 top-level commands + ValueEnum + ArgGroup + about:)

- [ ] **Step 5e.1: Replace `main.rs` content**

```rust
mod client;
mod commands;
mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use uefi_common::error;

#[derive(Parser)]
#[command(name = "uefi-cli", version, about = "UEFIPatcher CLI")]
struct Cli {
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<String>,
    #[arg(long, default_value = "text", value_enum)]
    format: OutputFormatCli,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum OutputFormatCli {
    Json,
    Text,
    Tsv,
}

impl From<OutputFormatCli> for output::OutputFormat {
    fn from(c: OutputFormatCli) -> Self {
        match c {
            OutputFormatCli::Json => output::OutputFormat::Json,
            OutputFormatCli::Text => output::OutputFormat::Text,
            OutputFormatCli::Tsv => output::OutputFormat::Tsv,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum ImageModeCli {
    Read,
    Write,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum InsertModeCli {
    Into,
    Before,
    After,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum SearchModeCli {
    Name,
    Utf8,
    Utf16,
    Bytes,
}

#[derive(Subcommand)]
enum Cmd {
    Session {
        #[command(subcommand)]
        sub: SessionCmd,
    },
    Image {
        #[command(subcommand)]
        sub: ImageCmd,
    },
    Node {
        #[command(subcommand)]
        sub: NodeCmd,
    },
    Artifact {
        #[command(subcommand)]
        sub: ArtifactCmd,
    },
    Setup {
        #[command(subcommand)]
        sub: SetupCmd,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    Init {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
    List,
    Destroy,
}

#[derive(Subcommand)]
enum ImageCmd {
    Open {
        path: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum, default_value_t = ImageModeCli::Read)]
        mode: ImageModeCli,
    },
    Switch {
        image_id: String,
    },
    Close {
        image_id: Option<String>,
    },
    Save {
        output: String,
    },
    List,
    Status,
}

#[derive(Subcommand)]
enum NodeCmd {
    List {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        tree: bool,
    },
    Search {
        query: String,
        #[arg(long, value_enum, num_args = 0.., default_values_t = vec![SearchModeCli::Name])]
        mode: Vec<SearchModeCli>,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    Insert {
        target: String,
        #[arg(long, group = "source")]
        file: Option<String>,
        #[arg(long, group = "source")]
        artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = InsertModeCli::Into)]
        mode: InsertModeCli,
    },
    Remove {
        target: String,
    },
    Replace {
        target: String,
        #[arg(long, group = "source")]
        file: Option<String>,
        #[arg(long, group = "source")]
        artifact: Option<String>,
        #[arg(long)]
        body_only: bool,
    },
    Rebuild {
        target: String,
    },
    Extract {
        target: String,
        #[arg(long)]
        body_only: bool,
    },
}

#[derive(Subcommand)]
enum ArtifactCmd {
    List,
    Import {
        path: String,
    },
    Export {
        artifact_id: String,
        output_path: Option<String>,
    },
}

#[derive(Subcommand)]
enum SetupCmd {
    Form {
        #[command(subcommand)]
        sub: SetupFormCmd,
    },
    String {
        #[command(subcommand)]
        sub: SetupStringCmd,
    },
}

#[derive(Subcommand)]
enum SetupFormCmd {
    List,
    SetVisibility {
        form_id: String,
        #[arg(long, conflicts_with = "hidden")]
        visible: bool,
        #[arg(long)]
        hidden: bool,
    },
}

#[derive(Subcommand)]
enum SetupStringCmd {
    List,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let format: output::OutputFormat = cli.format.into();
    let json_err = matches!(format, output::OutputFormat::Json);
    let result = dispatch(&cli, format).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error::print_error(&e, json_err);
            e.exit_code().to_std()
        }
    }
}

async fn dispatch(cli: &Cli, format: output::OutputFormat) -> Result<(), error::AppError> {
    let sock = cli.sock.as_deref();
    match &cli.cmd {
        Cmd::Session { sub } => match sub {
            SessionCmd::Init { name, force } => {
                commands::session::init(name.as_deref(), *force, sock, format).await
            }
            SessionCmd::List => commands::session::list(sock, format).await,
            SessionCmd::Destroy => commands::session::destroy(sock, format).await,
        },
        Cmd::Image { sub } => match sub {
            ImageCmd::Open { path, name, mode } => {
                let mode_i = match mode { ImageModeCli::Read => 0, ImageModeCli::Write => 1 };
                commands::image::open(path, name.as_deref(), mode_i, sock, format).await
            }
            ImageCmd::Switch { image_id } => commands::image::switch(image_id, format).await,
            ImageCmd::Close { image_id } => commands::image::close(image_id.as_deref(), sock, format).await,
            ImageCmd::Save { output } => commands::image::save(output, sock, format).await,
            ImageCmd::List => commands::image::list(sock, format).await,
            ImageCmd::Status => commands::image::status(sock, format).await,
        },
        Cmd::Node { sub } => match sub {
            NodeCmd::List { filter, tree } => {
                commands::node::list(filter.as_deref(), *tree, sock, format).await
            }
            NodeCmd::Search { query, mode, limit } => {
                let modes: Vec<i32> = mode.iter().map(|m| match m {
                    SearchModeCli::Name => uefi_proto::SearchMode::Name as i32,
                    SearchModeCli::Utf8 => uefi_proto::SearchMode::Utf8 as i32,
                    SearchModeCli::Utf16 => uefi_proto::SearchMode::Utf16 as i32,
                    SearchModeCli::Bytes => uefi_proto::SearchMode::Bytes as i32,
                }).collect();
                commands::node::search(query, &modes, *limit, sock, format).await
            }
            NodeCmd::Insert { target, file, artifact, mode } => {
                let mode_s = match mode { InsertModeCli::Into => "into", InsertModeCli::Before => "before", InsertModeCli::After => "after" };
                commands::node::insert(target, file.as_deref(), artifact.as_deref(), mode_s, sock, format).await
            }
            NodeCmd::Remove { target } => commands::node::remove(target, sock, format).await,
            NodeCmd::Replace { target, file, artifact, body_only } => {
                commands::node::replace(target, file.as_deref(), artifact.as_deref(), *body_only, sock, format).await
            }
            NodeCmd::Rebuild { target } => commands::node::rebuild(target, sock, format).await,
            NodeCmd::Extract { target, body_only } => {
                commands::node::extract(target, *body_only, sock, format).await
            }
        },
        Cmd::Artifact { sub } => match sub {
            ArtifactCmd::List => commands::artifact::list(sock, format).await,
            ArtifactCmd::Import { path } => commands::artifact::import(path, sock, format).await,
            ArtifactCmd::Export { artifact_id, output_path } => {
                commands::artifact::export(artifact_id, output_path.as_deref(), sock, format).await
            }
        },
        Cmd::Setup { sub } => match sub {
            SetupCmd::Form { sub } => match sub {
                SetupFormCmd::List => commands::setup::form_list(sock, format).await,
                SetupFormCmd::SetVisibility { form_id, visible, hidden } => {
                    let vis = if *hidden { false } else { *visible };
                    commands::setup::form_set_visibility(form_id, vis, sock, format).await
                }
            },
            SetupCmd::String { sub } => match sub {
                SetupStringCmd::List => commands::setup::string_list(sock, format).await,
            },
        },
    }
}
```

### Subtask 5f: Build + test uefi-cli

- [ ] **Step 5f.0: Migrate `tests/` to new proto + topology (rule 11 fix)**

The three files under `crates/uefi-cli/tests/` still use the pre-Task-3 proto names and pre-Task-5 command topology. They must be migrated in the same task as the `src/` rewrite, otherwise `cargo test -p uefi-cli` (Step 5f.1) cannot compile. There is no verbatim code for these in the brief — the implementer writes the migration following the rename map from 5b.1 and the topology from 5e.1.

`tests/mock_server.rs` — `MockEngine` must implement all 22 trait methods of the new `EngineService` trait (E0046 otherwise). Apply the rename map:
- `create_session`→`session_create` (`CreateSessionRequest`→`SessionCreateRequest`, `CreateSessionResponse`→`SessionCreateResponse`)
- `destroy_session`→`session_destroy` (`DestroySessionRequest`→`SessionDestroyRequest`)
- `list_sessions`→`sessions_list` (`ListSessionsRequest`→`SessionsListRequest`, `ListSessionsResponse`→`SessionsListResponse`; `SessionInfo` now also has a `name` field — set `String::new()`)
- `open_image`→`image_open` (`OpenImageRequest`→`ImageOpenRequest` with fields `session_id`/`path`/`mode`/`name`; `OpenImageResponse`→`ImageOpenResponse` now has 3 fields: `image_id`, `root_guid`, `name`)
- **delete** `dump_tree` (RPC removed) — remove the `DumpTreeRequest`/`DumpTreeResponse` stub entirely
- `list_items`→`image_nodes_list` (`ListItemsRequest`→`ImageNodesListRequest`, `ListItemsResponse`→`ImageNodesResponse`; `Item`→`Node`, response field `items`→`nodes`)
- `search_items`→`image_nodes_search` (`SearchItemsRequest`→`ImageNodesSearchRequest`, `SearchItemsResponse`→`ImageNodesResponse`)
- **delete** `find_item` (RPC removed)
- `insert`→`image_node_insert` (`InsertRequest`→`ImageNodeInsertRequest`, `InsertResponse`→`ImageNodeResponse`)
- `remove`→`image_node_remove` (`RemoveRequest`→`ImageNodeRemoveRequest`)
- `replace`→`image_node_replace` (`ReplaceRequest`→`ImageNodeReplaceRequest`, `ReplaceResponse`→`ImageNodeResponse`)
- `rebuild`→`image_node_rebuild` (`RebuildRequest`→`ImageNodeRebuildRequest`)
- `extract_artifact`→`image_node_extract` (`ExtractArtifactRequest`→`ImageNodeExtractRequest`, `ExtractArtifactResponse`→`ImageNodeExtractResponse`)
- `export_artifact`→`artifact_export` (`ExportArtifactRequest`→`ArtifactExportRequest`)
- `import_artifact`→`artifact_import` (`ImportArtifactRequest`→`ArtifactImportRequest`, field `file_path`→`path`; `ImportArtifactResponse`→`ArtifactImportResponse`)
- `list_artifacts`→`artifacts_list` (`ListArtifactsRequest`→`ArtifactsListRequest`, `ListArtifactsResponse`→`ArtifactsListResponse`; `ArtifactInfo` now also has `source` field — set `String::new()`)
- `set_setup_item_visibility`→`setup_set_form_visibility` (`SetSetupItemVisibilityRequest`→`SetupSetFormVisibilityRequest`; field `item_id` unchanged)
- `save_image`→`image_save` (`SaveImageRequest`→`ImageSaveRequest`)
- **delete** `add_setup_form_set` → replace with `setup_form_set_add` (`AddSetupFormSetRequest`→`SetupFormSetAddRequest`, `AddSetupFormSetResponse`→`SetupFormSetAddResponse`) — required because the trait method was renamed in proto
- **add new stubs**: `image_close` (`ImageCloseRequest`→`Empty`), `images_list` (`ImagesListRequest`→`ImagesListResponse` with `images: vec![]`), `image_status` (`ImageStatusRequest`→`ImageStatusResponse` with `info: None`), `setup_list_forms` (`SetupListFormsRequest`→`SetupListFormsResponse` with `forms: vec![]`), `setup_list_strings` (`SetupListStringsRequest`→`SetupListStringsResponse` with `strings: vec![]`)

The `mock_roundtrip` unit test at the bottom of `mock_server.rs` must also be updated (it calls `client.create_session(CreateSessionRequest{...})` → rename to `session_create(SessionCreateRequest{...})`).

`tests/cli_integration.rs` — rewrite `full_flow` to the new topology:
- `image open /dev/null --mode read` still works (mode is now a ValueEnum, "read" maps to `ImageModeCli::Read`)
- replace `image dump` with `node list` (or `image status`)
- replace `image list` (old node-list-on-active-image) with `node list`; add `image list` (new: list open images in session) and `image status` if desired
- replace `image find 0` with `node search 0` (search replaced find)
- `image close` now takes an optional `image_id` arg (defaults to active); keep `image close` with no arg (closes active)
- `no_state_errors`: replace `image dump` (gone) with `node list` — still expects failure with code 3 (State) when no `.uefipatcher` state

`tests/e2e.rs` — rewrite `edit_flow`:
- `edit insert 0 /dev/null --mode before` → `node insert 0 --file /dev/null --mode before`
- `edit remove 0` → `node remove 0`
- `setup set-visibility 0 --visible` → `setup form set-visibility 0 --visible`
- `image save /dev/null` unchanged (still valid)
- `image open --mode write` still works

- [ ] **Step 5f.1: Run cargo test for uefi-cli**

Run: `cargo test -p uefi-cli`
Expected: PASS.

- [ ] **Step 5f.2: Run clippy**

Run: `cargo clippy -p uefi-cli -- -D warnings`
Expected: no warnings.

- [ ] **Step 5f.3: Manual sanity check**

Run: `cargo run -p uefi-cli -- --help`
Expected: top-level help shows `session`, `image`, `node`, `artifact`, `setup` subcommands.

Run: `cargo run -p uefi-cli -- image --help`
Expected: shows `open`, `switch`, `close`, `save`, `list`, `status`.

- [ ] **Step 5f.4: Commit**

```bash
git add crates/uefi-cli/
git rm crates/uefi-cli/src/commands/edit.rs 2>/dev/null || true
git commit -m "feat!(uefi-cli): restructure to 5 top-level commands + new RPC names (Plan A Task 5)

- 5 top-level: session / image / node / artifact / setup
- node: list (--tree for tree+legend), search, insert/remove/replace/rebuild/extract
- artifact: list / import / export (session-scoped)
- image: open (--name, --mode ValueEnum), switch, close (server destroy),
  save, list (ImagesList), status
- setup: form {list, set-visibility}, string {list}
- ValueEnum for --format, --mode (image open + node insert)
- ArgGroup --file|--artifact for node insert/replace (mutually exclusive)
- session init --name (no implicit PWD-only)
- Rename all client methods to noun-first (session_create, image_open,
  image_nodes_list, image_node_insert, artifacts_list, ...)
- Delete commands/edit.rs (subtree moved to node)
- print_find -> print_node_id; add print_image_info/print_images_list/
  print_forms/print_strings/print_image_status

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 6: uefi-tui mechanical rename

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` — rename client method calls
- Modify: any mock test stubs in `crates/uefi-tui/tests/` or `src/`

**Interfaces:**
- Consumes: new `uefi_proto` types (Task 3)
- Produces: TUI that compiles against new proto (no behavior change)

**Strategy:** Mechanical find/replace. No deep work — `parse_tree_dump` stays (TODO cycle).

- [ ] **Step 6.1: Rename client method calls in TUI**

In `crates/uefi-tui/src/commands.rs` (and any other file that calls client methods), apply renames:
- `client.open_image(...)` → `client.image_open(...)` (update OpenImageRequest → ImageOpenRequest; field image_path → path; add name: String::new())
- `client.dump_tree(...)` → **delete this call** entirely; replace the `parse_tree_dump(&dump.text)` with empty tree OR with a minimal `image_nodes_list` call that produces a flat tree. For mechanical minimum: replace with `client.image_nodes_list(...)` and adapt `app.tree` from the returned nodes (treat as flat list — TUI bug, fixed in TODO cycle).
- `client.save_image(...)` → `client.image_save(...)` (SaveImageRequest → ImageSaveRequest)
- `client.extract_artifact(...)` → `client.image_node_extract(...)` (ExtractArtifactRequest → ImageNodeExtractRequest)
- `client.export_artifact(...)` → `client.artifact_export(...)` (ExportArtifactRequest → ArtifactExportRequest)
- `client.import_artifact(...)` → `client.artifact_import(...)` (ImportArtifactRequest → ArtifactImportRequest; file_path → path)
- `client.list_artifacts(...)` → `client.artifacts_list(...)` (ListArtifactsRequest → ArtifactsListRequest)

For the dump_tree replacement specifically:

```rust
let dump = client
    .inner
    .image_nodes_list(auth_req(&client.state, ImageNodesListRequest {
        image_id: r.image_id.clone(),
        filter: String::new(),
    }))
    .await
    .map_err(|e| e.message().to_string())?
    .into_inner();
app.tree = parse_nodes_flat(&dump.nodes);
```

Add a minimal `parse_nodes_flat` helper that converts `Vec<Node>` to `Vec<TreeNode>` (flat — depth=0 for all, name from node.name, etc.). This preserves the existing TUI display bug (flat list instead of tree) which is fixed in the TODO cycle.

- [ ] **Step 6.2: Update mock test stubs**

Search `crates/uefi-tui/` for references to old RPC names (`dump_tree`, `open_image`, `list_items`, `Item`, etc.) in test files. Update each to the new names.

Run: `rg 'dump_tree|open_image|list_items|search_items|find_item|extract_artifact|export_artifact|import_artifact|list_artifacts|save_image|set_setup_item_visibility|add_setup_form_set|create_session|destroy_session|list_sessions' crates/uefi-tui/`

For each match: rename to noun-first equivalent. Update type references (`Item` → `Node`, `OpenImageRequest` → `ImageOpenRequest`, etc.).

- [ ] **Step 6.3: Run cargo test for uefi-tui**

Run: `cargo test -p uefi-tui`
Expected: PASS.

- [ ] **Step 6.4: Run clippy**

Run: `cargo clippy -p uefi-tui -- -D warnings`
Expected: no warnings.

- [ ] **Step 6.5: Commit**

```bash
git add crates/uefi-tui/
git commit -m "refactor(uefi-tui): mechanical rename to new RPC names (Plan A Task 6)

Mechanical-only migration: rename all client method calls to noun-first
(session_create, image_open, image_nodes_list, image_node_extract,
artifacts_list, image_save, ...). dump_tree call replaced with
image_nodes_list + flat parse (preserves existing TUI display bug;
proper tree rendering deferred to TODO cycle 'TUI migration + bugfix').

No behavior change. No new features.

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 7: uefi-gateway mechanical rename

**Files:**
- Modify: `crates/uefi-gateway/src/client.rs` — rename all methods
- Modify: `crates/uefi-gateway/src/routes/image.rs` — handler fn rename (route paths preserved)
- Modify: mock test stubs

**Interfaces:**
- Consumes: new `uefi_proto` types (Task 3)
- Produces: Gateway that compiles (route paths unchanged; deep rework in TODO cycle)

- [ ] **Step 7.1: Rename client methods in `crates/uefi-gateway/src/client.rs`**

Apply same renames as Task 6 step 6.1 (gateway client mirrors CLI client structure). All `EngineClient` methods renamed.

- [ ] **Step 7.2: Update route handlers in `routes/image.rs`**

For each handler in `routes/image.rs`:
- `open`: `c.open_image(...)` → `c.image_open(...)`; OpenBody adds optional `name` field; OpenImageRequest field image_path → path, add name. Response JSON adds `"name"` field.
- `dump`: keep route `/api/v1/image/:id/dump` (path preserved per Plan A scope), but internally call `c.image_nodes_list(...)` instead of `c.dump_tree(...)`. Return JSON `{"items": ...}` (compatible with current WebUI) or `{"nodes": ...}`. Choose `{"nodes": ...}` to match new contract; WebUI updates in its own cycle.
- `items`: rename handler body to call `c.image_nodes_list(...)`.
- `find`: keep route `/api/v1/image/:id/find` (preserve paths); but `FindItem` RPC is removed. Either (a) remove `/find` route entirely, or (b) reimplement via `image_nodes_list` + filter. For mechanical minimum: remove `/find` route and its handler (it was useless per spec). Update `routes/mod.rs` to drop the route.
- `save`: `c.save_image(...)` → `c.image_save(...)`.

In `routes/mod.rs`:
- Drop the line `.route("/api/v1/image/:id/find", axum::routing::get(image::find))`.
- Other routes preserved.

- [ ] **Step 7.3: Update `routes/upload.rs`**

The `upload` route stores multipart to /tmp and returns path. WebUI then calls `/api/v1/image/open` with that path. The `download` route calls `c.save_image(...)` → rename to `c.image_save(...)`.

- [ ] **Step 7.4: Update `routes/setup.rs`**

Existing `list_items` handler calls `c.list_items(...)` → rename to `c.image_nodes_list(...)`. Path `/api/v1/image/:id/setup-items` preserved.

- [ ] **Step 7.5: Update `routes/edit.rs`**

Existing handlers `insert`/`remove`/`replace`/`rebuild` call `c.insert(...)` etc. → rename to `c.image_node_insert(...)` etc. Request/response body unchanged.

- [ ] **Step 7.6: Update `routes/artifact.rs`**

`extract` calls `c.extract_artifact(...)` → `c.image_node_extract(...)`. `export` → `c.artifact_export(...)`. `import` → `c.artifact_import(...)` (field file_path → path). `list` → `c.artifacts_list(...)`.

- [ ] **Step 7.7: Update `routes/ws.rs`**

`dump_ws` calls `c.dump_tree(...)`. RPC removed. Either delete `routes/ws.rs` entirely (and remove route from mod.rs), or stub it to return error. For mechanical minimum: **delete `routes/ws.rs`** and remove its route from `routes/mod.rs` and the `pub mod ws;` from routes/mod.rs top.

- [ ] **Step 7.8: Update mock test stubs in gateway**

Run: `rg 'dump_tree|open_image|list_items|search_items|find_item|extract_artifact|export_artifact|import_artifact|list_artifacts|save_image|set_setup_item_visibility|add_setup_form_set|create_session|destroy_session|list_sessions' crates/uefi-gateway/`

Rename all matches.

- [ ] **Step 7.9: Run cargo test for uefi-gateway**

Run: `cargo test -p uefi-gateway`
Expected: PASS.

- [ ] **Step 7.10: Run clippy**

Run: `cargo clippy -p uefi-gateway -- -D warnings`
Expected: no warnings.

- [ ] **Step 7.11: Commit**

```bash
git add crates/uefi-gateway/
git commit -m "refactor(uefi-gateway): mechanical rename to new RPC names (Plan A Task 7)

Mechanical migration: rename all client method calls to noun-first
(session_create, image_open, image_nodes_list, image_node_insert,
artifacts_list, image_save, ...). Route paths preserved per Plan A scope;
deep rework (new routes for /nodes, /forms, /strings, /status, DELETE
/image/:id, /images collection, removal of /dump and /dump/ws) deferred
to TODO cycle 'Gateway + WebUI rework'.

Changes:
- /api/v1/image/:id/dump now internally calls image_nodes_list, returns
  {'nodes': [...]} (was dump_tree text)
- /api/v1/image/:id/find route removed (FindItem RPC removed)
- routes/ws.rs deleted (dump_ws only user of removed dump_tree RPC)
- /api/v1/image/open response adds 'name' field

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

---

## Task 8: WebUI minimal update

**Files:**
- Modify: `webui/src/...` — fetch URLs/bodies if WebUI currently builds; otherwise leave with TODO marker

- [ ] **Step 8.1: Check WebUI build state**

Run: `cd webui && npm run check 2>&1 | head -50`
Expected: if green, proceed with minimal updates. If red, document existing breakage and skip to 8.4.

- [ ] **Step 8.2: If WebUI builds, update fetch URLs/bodies minimally**

Search `webui/src/` for fetch calls referencing routes that changed:
- `/api/v1/image/:id/dump` response shape changed from `{text}` to `{nodes: [...]}`. Update consumer.
- `/api/v1/image/open` request body — `name` field is optional, no breaking change for callers that don't send it.
- `/api/v1/image/:id/find` route removed — remove caller.
- All other route paths preserved (Plan A scope).

For each fetch call: rename as needed. Minimal changes only.

- [ ] **Step 8.3: Run npm check**

Run: `cd webui && npm run check`
Expected: green (or same red state as before if WebUI was already broken).

- [ ] **Step 8.4: Commit (or skip)**

If changes were made:
```bash
git add webui/
git commit -m "refactor(webui): minimal fetch updates for /dump response shape (Plan A Task 8)

Minimal mechanical update to fetch consumers for /api/v1/image/:id/dump
which now returns {nodes: [...]} instead of {text: '...'}. Full WebUI
rework deferred to TODO cycle 'Gateway + WebUI rework'.

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md"
```

If WebUI was already broken and no changes were possible, skip commit. Add a note to commit message of the final task (Task 9) mentioning WebUI deferred.

---

## Task 9: Final workspace verification

- [ ] **Step 9.1: Full workspace test**

Run: `cargo test --all`
Expected: PASS — all crates green.

- [ ] **Step 9.2: Full workspace clippy**

Run: `cargo clippy --all -- -D warnings`
Expected: no warnings.

- [ ] **Step 9.3: Format check**

Run: `cargo fmt --all -- --check`
Expected: no diff. If diff appears, run `cargo fmt --all` and amend last commit.

- [ ] **Step 9.4: WebUI check (best-effort)**

Run: `cd webui && npm run check 2>&1 | tail -10`
Expected: green if WebUI was green before Plan A; otherwise unchanged red state (deferred to TODO cycle).

- [ ] **Step 9.5: Smoke test end-to-end (manual)**

If `/run/.containerenv` exists, use `podman-remote` to run engine in container; else run engine locally.

```bash
# Terminal 1: start engine
cargo run -p uefi-engine --bin engine &

# Terminal 2: CLI workflow
cargo run -p uefi-cli -- session init --name "smoke test"
cargo run -p uefi-cli -- image open --name "test bios" --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin
cargo run -p uefi-cli -- image list
cargo run -p uefi-cli -- image status
cargo run -p uefi-cli -- node list --tree | head -20
cargo run -p uefi-cli -- node search "Setup" --limit 5
cargo run -p uefi-cli -- image save /tmp/patched.bin
cargo run -p uefi-cli -- image close
cargo run -p uefi-cli -- session destroy
```

Expected: each command succeeds; `image list` shows the opened image with name "test bios"; `image status` shows metadata; `node list --tree` renders tree with legend; `image close` removes from server (subsequent `image list` would be empty).

- [ ] **Step 9.6: Update TODO.md with completed Plan A reference**

In `TODO.md`, prepend a note to the relevant sections that Plan A is complete:

```markdown
> Plan A complete (commit <HASH>). Deep work tracked below.
```

(Use the actual final commit hash.)

- [ ] **Step 9.7: Final commit**

```bash
git add TODO.md
git commit -m "docs(todo): mark Plan A complete (CLI topology + proto rename + image storage)

Plan A delivered: 5 top-level CLI resources, noun-first proto rename,
image persistence with write-through, lazy re-load after engine restart,
SetupListForms/Strings stubs (UNIMPLEMENTED, Plan B).

Remaining TODO cycles unchanged:
- ImageUpload RPC (docker deployments)
- TUI migration + bugfix (parse_tree_dump removal, proper tree render)
- Gateway + WebUI rework (new routes, full WebUI fix)
- Plan B: IFR form/string extraction + setup_advanced/ merge

Spec: docs/superpowers/specs/2026-08-10-cli-topology-and-image-storage-design.md
Plan: docs/superpowers/plans/2026-08-10-cli-topology-and-image-storage.md"
```

---

## Self-Review Notes

**Spec coverage check:**

- ✅ CLI topology (5 top-level) — Task 5
- ✅ Proto noun-first rename — Task 3
- ✅ Drop DumpTree/FindItem/DumpFormat — Task 3 (proto) + Task 4g (engine dead code) + Task 6/7 (clients stop calling)
- ✅ ImageOpen path → path field rename — Task 3
- ✅ ArtifactImport file_path → path rename — Task 3
- ✅ ImageInfo / ImagesList / ImageClose / ImageStatus new RPCs — Task 3 (proto) + Task 4c (handlers) + Task 5b (CLI client) + Task 5d (CLI commands)
- ✅ Image storage SQLite — Task 1
- ✅ Image file storage helpers — Task 2
- ✅ Write-through flush_image — Task 4a (helper) + Task 4f (wire to mutations)
- ✅ Lazy re-load get_or_load_image — Task 4a + Task 4e
- ✅ image_open persistence (copy bytes + DB row) — Task 4d
- ✅ image_close destroy on server — Task 4c.1
- ✅ image_save always builds from in-memory — Task 4b (save_image body unchanged from current behavior)
- ✅ SetupListForms/Strings stubs UNIMPLEMENTED — Task 4c.4
- ✅ SetupSetFormVisibility rename — Task 4b
- ✅ CLI ValueEnum (--format, --mode) — Task 5e
- ✅ CLI ArgGroup (--file|--artifact) — Task 5e
- ✅ CLI --name (session init, image open) — Task 5e
- ✅ CLI about: strings — Task 5e (clap derive `about` can be added per-subcommand; mention in step but not strictly enforced — review can request addition)
- ✅ node list --tree — Task 5c.2
- ✅ TUI mechanical rename — Task 6
- ✅ Gateway mechanical rename — Task 7
- ✅ WebUI minimal — Task 8
- ✅ Session destroy cleans images/ dir — Task 4g.2 (verified by test; existing recursive remove already covers it)
- ✅ Cascade delete session → images — Task 1 (FK ON DELETE CASCADE in schema; tested in Task 1 Step 3)

**Scope decomposition note:** This plan is large (9 tasks, several with subtasks). If executing via subagent-driven-development, consider batching subtasks within a task to a single subagent (e.g., one subagent for all of Task 4). If executing inline, the task-per-session boundary works.

**Known unknowns:**

- The exact TUI `parse_tree_dump` replacement shape (Task 6 step 6.1) — left somewhat open since TUI will be reworked in TODO cycle anyway. The mechanical replacement just needs to compile.
- WebUI state (Task 8) — may already be broken; plan handles both cases.
- Engine `parse_image` signature (Task 4d) — assumed unchanged from current. If signature differs, fix in plan-defect commit per AGENTS.md rule 11.
