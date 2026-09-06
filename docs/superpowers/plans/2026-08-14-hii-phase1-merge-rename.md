# HII Module Merge & Rename — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge `setup/` + `setup_advanced/` into a single `hii/` module inside `uefi-engine`, and unify `SetupError` + `SetupAdvancedError` into one `HiiError` enum. Pure engine-internal refactor — no proto/CLI API changes (those are Phase 2).

**Architecture:** Mechanical file-move refactor in two tasks. Task 1 moves `setup/`→`hii/` and renames `SetupError`→`HiiError` (3 variants); the crate compiles with `hii` + `setup_advanced` coexisting. Task 2 moves `setup_advanced/`→`hii/`, grows `HiiError` to 8 variants, and relocates `add_setup_formset` into `hii/formset_add.rs`. Each task leaves `cargo test -p uefi-engine` green and `clippy -D warnings` clean.

**Tech Stack:** Rust (edition 2024), `thiserror`, `git mv` (preserves file history), r-efi.

## Global Constraints

- **Module-first rule:** update `lib.rs` `pub mod` declarations in the SAME step as the move, BEFORE running `cargo test` (avoids false 0-of-0 test runs).
- **Refactor = existing tests must pass unchanged.** No new tests in Phase 1; moved test modules travel with their files.
- **Per-task verify:** `cargo test -p uefi-engine` AND `cargo clippy -p uefi-engine -- -D warnings` AND `cargo fmt -p uefi-engine -- --check`.
- **No proto/CLI changes** in Phase 1: RPC method names stay `Setup*`, generated server trait methods stay `setup_*`, CLI noun stays `setup`. Only `crate::setup[_Advanced]::` paths and the error type names change inside the engine.
- **Use `git mv`** for files whose content is largely preserved (history-friendly). Edit the moved files in place afterward.
- **No comments** in code (per AGENTS.md).
- **One commit per task** with the message shown.

**Reference — current state (baseline):**
- `crates/uefi-engine/src/lib.rs:9-10` → `pub mod setup;` / `pub mod setup_advanced;`
- `crates/uefi-engine/src/setup/{mod.rs,ifr.rs}` — `SetupError` (3 variants), `set_item_visibility`, suppress-if logic.
- `crates/uefi-engine/src/setup_advanced/{mod.rs,string_pack.rs,schema.rs,ami_patcher.rs,ffs_assembler.rs,ifr_builder.rs}` — `SetupAdvancedError` (5 variants), `add_setup_formset` + helpers.
- `crates/uefi-engine/src/rpc/server.rs` — the ONLY internal caller: `:639` (`crate::setup::set_item_visibility`), `:684` (`crate::setup_advanced::schema::parse_schema`), `:700` (`crate::setup_advanced::add_setup_formset`), `:702-716` (`crate::setup_advanced::SetupAdvancedError::*` match).

**Target layout after Phase 1:**
```
crates/uefi-engine/src/hii/
  mod.rs           — pub mod decls + HiiError (8 variants) + set_item_visibility
  ifr.rs           — suppress-if (moved from setup/, unchanged)
  formset_add.rs   — add_setup_formset + helpers (moved from setup_advanced/mod.rs)
  string_pack.rs   — moved from setup_advanced/ (writer)
  schema.rs        — moved from setup_advanced/
  ami_patcher.rs   — moved from setup_advanced/
  ffs_assembler.rs — moved from setup_advanced/
  ifr_builder.rs   — moved from setup_advanced/ (no error refs — unchanged)
```

---

### Task 1: Move `setup/` → `hii/`, rename `SetupError` → `HiiError`

**Files:**
- Create: `crates/uefi-engine/src/hii/mod.rs` (moved from `setup/mod.rs`, `SetupError`→`HiiError`)
- Create: `crates/uefi-engine/src/hii/ifr.rs` (moved from `setup/ifr.rs`, unchanged)
- Modify: `crates/uefi-engine/src/lib.rs:9` — `pub mod setup;` → `pub mod hii;`
- Modify: `crates/uefi-engine/src/rpc/server.rs:639` — `crate::setup::` → `crate::hii::`
- Delete: `crates/uefi-engine/src/setup/` (both files)

**Interfaces:**
- Consumes: `crate::ops`, `crate::types::*`, `crate::parser::target::{parse_target, find_item_mut}` (unchanged paths).
- Produces: `crate::hii::HiiError` (variants: `NotFound`, `NotASetupItem`, `InvalidIfr`), `crate::hii::set_item_visibility`, `crate::hii::ifr::{SuppressScope, find_suppress_if_scopes, unsuppress}`. `setup_advanced/` is untouched and still defines its own `SetupAdvancedError` (coexists).

- [ ] **Step 1: Create `hii/` dir and `git mv` the two setup files**

Run:
```bash
mkdir -p crates/uefi-engine/src/hii
git mv crates/uefi-engine/src/setup/ifr.rs crates/uefi-engine/src/hii/ifr.rs
git mv crates/uefi-engine/src/setup/mod.rs crates/uefi-engine/src/hii/mod.rs
```
`crates/uefi-engine/src/setup/` is now empty (git removes the dir automatically).

- [ ] **Step 2: Rename `SetupError` → `HiiError` in `hii/mod.rs`**

In `crates/uefi-engine/src/hii/mod.rs`, replace every occurrence of `SetupError` with `HiiError` (5 occurrences: enum name, 3 `#[error]`-bearing variant references in signatures/returns, the `Result<(), SetupError>` return type — use `replaceAll`). After the edit, the enum block must read:

```rust
#[derive(Debug, Error)]
pub enum HiiError {
    #[error("not found")]
    NotFound,
    #[error("not a setup item")]
    NotASetupItem,
    #[error("invalid IFR")]
    InvalidIfr,
}
```

And the function signature:

```rust
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), HiiError> {
```

Confirm every `SetupError::Foo` in the body became `HiiError::Foo` (e.g. `return Err(HiiError::NotASetupItem);`).

- [ ] **Step 3: Update `lib.rs` module declaration**

In `crates/uefi-engine/src/lib.rs`, change line 9:

```rust
pub mod hii;
```

(Line 10 `pub mod setup_advanced;` stays unchanged for Task 1.)

- [ ] **Step 4: Update the one server.rs caller**

In `crates/uefi-engine/src/rpc/server.rs`, change line 639:

```rust
            crate::hii::set_item_visibility(img_slot, &r.item_id, r.visible)
```

(The `.map_err(|e| Status::internal(e.to_string()))` on line 640 already works for `HiiError` — no change needed there.)

- [ ] **Step 5: Verify compile + tests + clippy + fmt**

Run (each must succeed):
```bash
cargo test -p uefi-engine
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
```
Expected: all tests pass (the moved `unsuppress`/`set_item_visibility` tests now run under `hii::`), no warnings, fmt clean.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor(uefi-engine): move setup/ -> hii/, rename SetupError -> HiiError (Phase 1/Task 1)"
```

---

### Task 2: Move `setup_advanced/` → `hii/`, merge `HiiError`, add `formset_add.rs`

**Files:**
- Create: `crates/uefi-engine/src/hii/string_pack.rs` (moved, `SetupAdvancedError`→`HiiError`)
- Create: `crates/uefi-engine/src/hii/schema.rs` (moved, `SetupAdvancedError`→`HiiError`)
- Create: `crates/uefi-engine/src/hii/ami_patcher.rs` (moved, `SetupAdvancedError`→`HiiError`)
- Create: `crates/uefi-engine/src/hii/ffs_assembler.rs` (moved, `SetupAdvancedError`→`HiiError`)
- Create: `crates/uefi-engine/src/hii/ifr_builder.rs` (moved, unchanged — no error refs)
- Create: `crates/uefi-engine/src/hii/formset_add.rs` (moved from `setup_advanced/mod.rs`, re-parented imports, `SetupAdvancedError`→`HiiError`, enum definition removed)
- Modify: `crates/uefi-engine/src/hii/mod.rs` — add `pub mod` decls + 5 new `HiiError` variants
- Modify: `crates/uefi-engine/src/lib.rs:10` — delete `pub mod setup_advanced;`
- Modify: `crates/uefi-engine/src/rpc/server.rs:684,700-716` — `crate::setup_advanced::` → `crate::hii::`, `SetupAdvancedError` → `HiiError`, add catch-all arm
- Delete: `crates/uefi-engine/src/setup_advanced/` (all 6 files)

**Interfaces:**
- Consumes: `crate::ops`, `crate::types::*`, `crate::ffs::*`, `r_efi::hii::*`, `crate::parser::file::parse_file` (in ffs_assembler tests).
- Produces: `crate::hii::HiiError` (now 8 variants), `crate::hii::formset_add::{add_setup_formset, AddSetupResult}`, `crate::hii::schema::{parse_schema, FormSetSchema, ...}`, `crate::hii::string_pack::{add_strings, is_string_package, PACKAGE_STRINGS}`, `crate::hii::ami_patcher::{patch_ami, QuestionAmiRecord, AMI_DEFAULT_ACCESS_LEVEL}`, `crate::hii::ffs_assembler::assemble_ffs`, `crate::hii::ifr_builder::IfrBuilder`.

- [ ] **Step 1: `git mv` the five unchanged-content submodules**

Run:
```bash
git mv crates/uefi-engine/src/setup_advanced/ifr_builder.rs crates/uefi-engine/src/hii/ifr_builder.rs
git mv crates/uefi-engine/src/setup_advanced/string_pack.rs   crates/uefi-engine/src/hii/string_pack.rs
git mv crates/uefi-engine/src/setup_advanced/schema.rs        crates/uefi-engine/src/hii/schema.rs
git mv crates/uefi-engine/src/setup_advanced/ami_patcher.rs   crates/uefi-engine/src/hii/ami_patcher.rs
git mv crates/uefi-engine/src/setup_advanced/ffs_assembler.rs crates/uefi-engine/src/hii/ffs_assembler.rs
```
(`ifr_builder.rs` has no error references — leave its content untouched.)

- [ ] **Step 2: `git mv` `setup_advanced/mod.rs` → `hii/formset_add.rs`**

Run:
```bash
git mv crates/uefi-engine/src/setup_advanced/mod.rs crates/uefi-engine/src/hii/formset_add.rs
```
`crates/uefi-engine/src/setup_advanced/` is now empty.

- [ ] **Step 3: Rename `SetupAdvancedError` → `HiiError` in the four error-referencing submodules**

In each of `hii/string_pack.rs`, `hii/schema.rs`, `hii/ami_patcher.rs`, `hii/ffs_assembler.rs`: replace every occurrence of `SetupAdvancedError` with `HiiError` (`replaceAll`). This covers the `use super::SetupAdvancedError;` import line (becomes `use super::HiiError;` — note `super` is now `hii`, which is correct), all return types, and test assertions like `Err(SetupAdvancedError::StringPackageNotFound)` → `Err(HiiError::StringPackageNotFound)`.

Verify after editing: no token `SetupAdvancedError` remains in those four files (`rg -n SetupAdvancedError crates/uefi-engine/src/hii/` returns nothing in them).

- [ ] **Step 4: Reparent `formset_add.rs` imports and error type**

`hii/formset_add.rs` was `setup_advanced/mod.rs`, so it is now a SIBLING of the submodules it formerly owned as children. The in-body calls use bare `QuestionAmiRecord` / `Vec<QuestionAmiRecord>`, `ami_patcher::patch_ami`, `ami_patcher::AMI_DEFAULT_ACCESS_LEVEL`, `ffs_assembler::assemble_ffs`, `schema::...`, `string_pack::...`, and `ifr_builder::*` constants/`IfrBuilder`. Four edits:

(a) Delete the `pub enum SetupAdvancedError { ... }` block entirely (the whole `#[derive(Debug, Error)] pub enum SetupAdvancedError { ... }` with its 5 variants, ~lines 16-27 of the original) — these variants are added to `HiiError` in Step 6. Also delete the now-unused `use thiserror::Error;` import line (the derive lives in `mod.rs` now).

(b) Replace every remaining `SetupAdvancedError` token with `HiiError` (`replaceAll`). After this, all `Result<..., SetupAdvancedError>` become `Result<..., HiiError>` and `SetupAdvancedError::InvalidSchema` → `HiiError::InvalidSchema`, etc.

(c) Replace the two child-import lines:
```rust
use ami_patcher::QuestionAmiRecord;
use ifr_builder::*;
```
with the sibling-aware import block (note `self` brings the `ami_patcher` module into scope so the in-body `ami_patcher::patch_ami`/`ami_patcher::AMI_DEFAULT_ACCESS_LEVEL` calls resolve, while `QuestionAmiRecord` stays bare):
```rust
use super::ami_patcher::{self, QuestionAmiRecord};
use super::ffs_assembler;
use super::ifr_builder::*;
use super::schema;
use super::string_pack;
use super::HiiError;
```

(d) Confirm `use crate::ops;` and `use crate::types::*;` (top of file) are unchanged, and `use std::collections::HashMap;` is unchanged. No in-body call needs further editing — every `ami_patcher::...`, `ffs_assembler::...`, `schema::...`, `string_pack::...`, bare `QuestionAmiRecord`, and `ifr_builder` constant now resolves through the block in (c).

- [ ] **Step 5: Verify the four submodules compile-check individually (fast feedback)**

Run:
```bash
cargo check -p uefi-engine 2>&1 | head -40
```
Expected: errors only from (i) missing `HiiError` variants in `mod.rs` (not yet added — Step 6) and (ii) `lib.rs` still pointing at `setup_advanced` (Step 7). No `unresolved import` errors for the moved submodules themselves. If you see `unresolved import super::SetupAdvancedError`, re-check Step 3 missed a file.

- [ ] **Step 6: Grow `HiiError` to 8 variants and add `pub mod` declarations in `hii/mod.rs`**

In `crates/uefi-engine/src/hii/mod.rs`:

(a) Add `pub mod` declarations for all moved submodules at the very top (after any existing `pub mod ifr;`):
```rust
pub mod ami_patcher;
pub mod ffs_assembler;
pub mod formset_add;
pub mod ifr;
pub mod ifr_builder;
pub mod schema;
pub mod string_pack;
```

(b) Extend the `HiiError` enum with the 5 variants absorbed from `SetupAdvancedError`. Final enum:
```rust
#[derive(Debug, Error)]
pub enum HiiError {
    #[error("not found")]
    NotFound,
    #[error("not a setup item")]
    NotASetupItem,
    #[error("invalid IFR")]
    InvalidIfr,
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
(The three new tuple/unit variants each have a live use site in `formset_add.rs`/`string_pack.rs`/`ami_patcher.rs`/`schema.rs`/`ffs_assembler.rs`, so no dead-code warning.)

- [ ] **Step 7: Remove `setup_advanced` from `lib.rs`**

In `crates/uefi-engine/src/lib.rs`, delete line 10 (`pub mod setup_advanced;`). The `pub mod hii;` from Task 1 remains. Final relevant lines:
```rust
pub mod hii;
```

- [ ] **Step 8: Update `server.rs` form_set_add handler**

In `crates/uefi-engine/src/rpc/server.rs`:

(a) Line 684 — `crate::setup_advanced::schema::parse_schema` → `crate::hii::schema::parse_schema`.

(b) Line 700 — `crate::setup_advanced::add_setup_formset` → `crate::hii::formset_add::add_setup_formset`.

(c) The `.map_err(|e| match e { ... })?` block (lines 701-717) — replace the whole match. The new arms use `crate::hii::HiiError::*` and add a catch-all for the 3 variants `add_setup_formset` never returns (so the match is exhaustive). Replace with:
```rust
                .map_err(|e| match e {
                    crate::hii::HiiError::InvalidSchema(s) => {
                        Status::invalid_argument(s)
                    }
                    crate::hii::HiiError::StringPackageNotFound => {
                        Status::not_found("string package not found")
                    }
                    crate::hii::HiiError::AmiFilesNotFound => {
                        Status::not_found("AMI setupdataBin/amitseSct not found")
                    }
                    _ => Status::internal(e.to_string()),
                })?
```

- [ ] **Step 9: Verify compile + full test suite + clippy + fmt**

Run (each must succeed):
```bash
cargo test -p uefi-engine
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
```
Expected: all tests pass, including the moved `string_pack`, `schema`, `ami_patcher`, `ffs_assembler`, `ifr_builder` test modules (now under `hii::`). No `dead_code` warnings (every `HiiError` variant is used). No warnings.

If clippy flags the `_` catch-all arm, replace it with explicit arms:
```rust
                    crate::hii::HiiError::IfrBuildError(s) => Status::internal(s),
                    crate::hii::HiiError::FfsAssemblyError(s) => Status::internal(s),
                    crate::hii::HiiError::NotFound
                    | crate::hii::HiiError::NotASetupItem
                    | crate::hii::HiiError::InvalidIfr => Status::internal(e.to_string()),
```
(merge `IfrBuildError`/`FfsAssemblyError` into the catch-all only if clippy's `match_same_arms` fires).

- [ ] **Step 10: Whole-workspace smoke check (no other crate should break)**

Run:
```bash
cargo check --workspace
```
Expected: SUCCESS. No other crate references `crate::setup[_Advanced]` (they go through proto/`crate::hii` is engine-only). If any crate fails, it means an engine-internal path leaked — re-check Steps 4/8 covered all server.rs references.

- [ ] **Step 11: Commit**

```bash
git add -A && git commit -m "refactor(uefi-engine): merge setup_advanced/ into hii/, unify HiiError (Phase 1/Task 2)"
```

---

## Self-Review (completed by plan author)

**Spec coverage (Phase 1 scope = "Merge + module rename"):**
- ✅ `setup/` + `setup_advanced/` → `hii/` flat hierarchy — Task 1 (setup/) + Task 2 (setup_advanced/).
- ✅ `HiiError` unified enum (8 variants) — Task 1 (3) + Task 2 (5).
- ✅ `lib.rs` module decl update — Task 1 Step 3 + Task 2 Step 7.
- ✅ "Mechanical refactor, всё компилируется" — every step keeps the crate compiling; per-task `cargo test` + `clippy` gates.
- Note: proto/CLI rename is Phase 2 (next plan-document), per spec §"Реализация: конвенция по фазам".

**Placeholder scan:** none. Every step has exact commands or exact code. Task 2 Step 4 gives a single deterministic import block (`use super::ami_patcher::{self, QuestionAmiRecord}; ...`) verified against the in-body call sites.

**Type consistency:** `HiiError` variant names are identical across Task 1 (definition) and Task 2 (extensions): `NotFound`, `NotASetupItem`, `InvalidIfr`, `InvalidSchema(String)`, `StringPackageNotFound`, `AmiFilesNotFound`, `IfrBuildError(String)`, `FfsAssemblyError(String)`. `add_setup_formset`, `AddSetupResult`, `parse_schema` signatures unchanged (only the error type name changes). `set_item_visibility` signature unchanged.

**Dependency order:** Task 1 before Task 2 (Task 2 grows the enum Task 1 created). Both before Phase 2.
