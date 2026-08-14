# HII Proto/CLI Rename — Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisite:** Phase 1 complete (`setup/`+`setup_advanced/` already merged into `hii/` inside `uefi-engine`; `crate::hii::` paths exist).

**Goal:** Rename the gRPC contract `Setup*`→`Hii*` (RPC methods + messages) and the CLI noun `setup`→`hii`, propagating to every Rust caller so the whole workspace compiles. Also relocate gateway `routes/setup.rs`→`routes/hii.rs`. WebUI is untouched (it talks HTTP, not gRPC; its fetch paths contain no `setup` token).

**Architecture:** Atomic mechanical rename in two tasks. Task 1 is the gRPC/proto ripple: once `engine.proto` changes, every gRPC client/server/mock breaks, so proto + engine + cli + gateway-client + all mocks land in ONE commit (only way to keep `cargo check --workspace` green between commits). Task 2 is the gateway REST-route module rename (non-breaking, separate commit).

**Tech Stack:** Rust (edition 2024), tonic (codegen), clap, axum, git mv.

## Global Constraints

- **CRITICAL — rename exact identifiers, NOT bare prefixes.** The tokens `Setup`/`setup` appear as **data** (UEFI section names `"Setup"`, search queries `"setup"`, test fixtures) in ~10 files and MUST NOT change. Only the 8 code identifiers below (and the CLI clap variants / module paths) are renamed. Before any edit, the executor MUST read the per-file rules — do NOT blanket-replace `Setup`/`setup`.
- **The 8 safe code identifiers** (replace these, and only these, as tokens):
  - PascalCase (proto RPC + message names): `SetupListForms`, `SetupSetFormVisibility`, `SetupListStrings`, `SetupFormSetAdd`
  - snake_case (generated trait/client method names): `setup_list_forms`, `setup_set_form_visibility`, `setup_list_strings`, `setup_form_set_add`
- **`FormInfo`, `StringInfo` are NOT renamed** (no Setup/Hii prefix).
- **`build.rs` (uefi-proto) is NOT modified** — its `message_attribute` calls reference `engine.FormInfo`/`engine.StringInfo`, which are unchanged.
- **Do NOT touch** `name == "Setup"` / `"setup"` search-query / `setup_env()` / `setup_gateway()` test-helper / `encode_utf16le_null("Setup")` occurrences — those are data/fixtures, not code.
- **Per-task verify:** `cargo test --all` AND `cargo clippy --all -- -D warnings` AND `cargo fmt --all -- --check`.
- **No comments** in code (per AGENTS.md).
- **One commit per task.**

**Codegen fact:** tonic generates the gRPC trait/client method name as snake_case of the RPC name. So proto `rpc SetupListForms` → method `setup_list_forms`; after rename `rpc HiiListForms` → method `hii_list_forms`. Confirmed against `crates/uefi-proto/build.rs`.

**Authoritative rename table (apply per file as indicated):**

| Old identifier | New identifier |
|---|---|
| `SetupListForms` | `HiiListForms` |
| `SetupSetFormVisibility` | `HiiSetFormVisibility` |
| `SetupListStrings` | `HiiListStrings` |
| `SetupFormSetAdd` | `HiiFormSetAdd` |
| `setup_list_forms` | `hii_list_forms` |
| `setup_set_form_visibility` | `hii_set_form_visibility` |
| `setup_list_strings` | `hii_list_strings` |
| `setup_form_set_add` | `hii_form_set_add` |

CLI-only extras (apply ONLY in `uefi-cli/src/main.rs`): the clap enum variants `Setup`/`SetupCmd`/`SetupFormCmd`/`SetupStringCmd` → `Hii`/`HiiCmd`/`HiiFormCmd`/`HiiStringCmd`; and the module path `commands::setup::` → `commands::hii::`.

---

### Task 1: Proto rename + all Rust callers (atomic)

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (lines 27-30, 144, 152, 154, 160, 166, 168, 173)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (trait method impls + message types + 2 unimplemented strings)
- Modify: `crates/uefi-cli/src/client.rs` (3 methods + 3 message types)
- Modify: `crates/uefi-cli/src/main.rs` (clap variants + dispatch + module path)
- Modify: `crates/uefi-cli/src/commands/setup.rs` (3 client calls) — file itself renamed in Step
- Modify: `crates/uefi-cli/src/commands/mod.rs` (`pub mod setup`→`pub mod hii`)
- Modify: `crates/uefi-cli/tests/e2e.rs:44` (CLI noun arg)
- Modify: `crates/uefi-cli/tests/mock_server.rs` (4 trait stubs)
- Modify: `crates/uefi-gateway/src/client.rs` (1 method + message type)
- Modify: `crates/uefi-gateway/src/routes/setup.rs` (1 client call — file renamed in Task 2)
- Modify: `crates/uefi-gateway/tests/mock_server.rs` (4 trait stubs)
- Modify: `crates/uefi-tui/tests/mock_server.rs` (4 trait stubs ONLY — leave line 146 `name: "Setup"` untouched)

**Interfaces:** Unchanged behavior; only names change. gRPC service now exposes `HiiListForms`/`HiiSetFormVisibility`/`HiiListStrings`/`HiiFormSetAdd`. CLI noun is `hii`.

- [ ] **Step 1: Rename in `engine.proto`**

In `crates/uefi-proto/proto/engine.proto`, apply the 4 PascalCase replacements from the table (`replaceAll` each): `SetupListForms`→`HiiListForms`, `SetupSetFormVisibility`→`HiiSetFormVisibility`, `SetupListStrings`→`HiiListStrings`, `SetupFormSetAdd`→`HiiFormSetAdd`. This covers all 4 `rpc` lines and all 7 `message` declarations (e.g. `message SetupListFormsRequest` → `message HiiListFormsRequest`). Do NOT touch `FormInfo`/`StringInfo`.

Sanity check: `rg -n Setup crates/uefi-proto/proto/engine.proto` must return nothing.

- [ ] **Step 2: Rename in engine `server.rs`**

In `crates/uefi-engine/src/rpc/server.rs`, apply all 8 table replacements (`replaceAll` each). This changes:
- trait method impls `async fn setup_list_forms` → `hii_list_forms` (and the other 3),
- message types in signatures (`SetupListFormsRequest`→`HiiListFormsRequest`, etc.),
- the `.inner.setup_form_set_add`-style calls (none in server — it's the impl side),
- the two unimplemented strings `"SetupListForms not implemented (Plan B)"` → `"HiiListForms not implemented (Plan B)"` and `"SetupListStrings..."` → `"HiiListStrings..."`.

Sanity check: `rg -n 'setup_list_forms|setup_set_form_visibility|setup_list_strings|setup_form_set_add|SetupListForms|SetupSetFormVisibility|SetupListStrings|SetupFormSetAdd' crates/uefi-engine/src/rpc/server.rs` must return nothing.

- [ ] **Step 3: Rename in cli `client.rs`**

In `crates/uefi-cli/src/client.rs`, apply all 8 table replacements. This changes `pub async fn setup_list_forms`→`hii_list_forms`, the `self.inner.setup_list_forms(...)` calls, and the `SetupListFormsRequest`/`SetupListStringsRequest`/`SetupSetFormVisibilityRequest` message literals.

Sanity check: `rg -n 'setup_list_forms|setup_list_strings|setup_set_form_visibility|SetupListForms|SetupListStrings|SetupSetFormVisibility' crates/uefi-cli/src/client.rs` must return nothing.

- [ ] **Step 4: Rename cli command module `setup.rs` → `hii.rs` and update its calls**

Run:
```bash
git mv crates/uefi-cli/src/commands/setup.rs crates/uefi-cli/src/commands/hii.rs
```
In `crates/uefi-cli/src/commands/hii.rs`, apply the 3 snake_case replacements that appear there: `setup_list_forms`→`hii_list_forms`, `setup_set_form_visibility`→`hii_set_form_visibility`, `setup_list_strings`→`hii_list_strings` (`client.setup_list_forms`→`client.hii_list_forms`, etc.).

In `crates/uefi-cli/src/commands/mod.rs`, change `pub mod setup;` → `pub mod hii;`.

- [ ] **Step 5: Rename in cli `main.rs`**

In `crates/uefi-cli/src/main.rs`:
(a) Replace the clap enum variants: `replaceAll` `Setup`→`Hii`. This turns `Cmd::Setup`→`Cmd::Hii`, `SetupCmd`→`HiiCmd`, `SetupFormCmd`→`HiiFormCmd`, `SetupStringCmd`→`HiiStringCmd`. (Safe: main.rs has no `"Setup"` string literal.)
(b) Replace the module path: `replaceAll` `commands::setup::`→`commands::hii::` (3 dispatch call sites).

Sanity check: `rg -n 'Setup|commands::setup' crates/uefi-cli/src/main.rs` must return nothing.

- [ ] **Step 6: Update cli e2e test**

In `crates/uefi-cli/tests/e2e.rs:44`, change the CLI noun arg:
```rust
        .args(["hii", "form", "set-visibility", "0", "--visible"])
```
(Only the first element `setup`→`hii`; `form`/`set-visibility` are unchanged.)

- [ ] **Step 7: Rename in the 3 mock_server.rs files**

In each of `crates/uefi-cli/tests/mock_server.rs`, `crates/uefi-gateway/tests/mock_server.rs`, `crates/uefi-tui/tests/mock_server.rs`: apply all 8 table replacements (`replaceAll` each). This renames the 4 trait stubs (`async fn setup_list_forms`→`hii_list_forms`, etc.) and their message types.

> **TUI mock caution:** `crates/uefi-tui/tests/mock_server.rs:146` has `name: "Setup".into()` — that is mock DATA (a UEFI section literally named "Setup"), NOT code. The 8 table replacements target multi-word identifiers (`SetupListForms` etc.) and snake_case methods (`setup_list_forms`), which do NOT match the bare string `"Setup"`, so `replaceAll` of the 8 identifiers leaves line 146 untouched. After editing, verify: `rg -n '"Setup"' crates/uefi-tui/tests/mock_server.rs` must STILL show line 146.

Sanity check (all 3 mocks): `rg -n 'setup_list_forms|setup_set_form_visibility|setup_list_strings|setup_form_set_add|SetupListForms|SetupSetFormVisibility|SetupListStrings|SetupFormSetAdd' crates/uefi-cli/tests/mock_server.rs crates/uefi-gateway/tests/mock_server.rs crates/uefi-tui/tests/mock_server.rs` must return nothing.

- [ ] **Step 8: Rename in gateway `client.rs`**

In `crates/uefi-gateway/src/client.rs`, apply the replacements that appear there: `setup_set_form_visibility`→`hii_set_form_visibility` (method name + the `.inner.setup_set_form_visibility(...)` call) and `SetupSetFormVisibilityRequest`→`HiiSetFormVisibilityRequest`.

- [ ] **Step 9: Rename the one call in gateway `routes/setup.rs`**

In `crates/uefi-gateway/src/routes/setup.rs:24`, `c.setup_set_form_visibility(...)` → `c.hii_set_form_visibility(...)`. (The file itself stays `setup.rs` for now — renamed in Task 2.)

- [ ] **Step 10: Verify the whole workspace compiles + tests + clippy + fmt**

Run (each must succeed):
```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```
Expected: all tests pass; the renamed mock stubs satisfy the regenerated `EngineService` trait; the cli e2e test passes with the `hii` noun. No warnings.

If any test fails on a missed identifier, search for stragglers: `rg -n 'setup_list_forms|setup_set_form_visibility|setup_list_strings|setup_form_set_add|SetupListForms|SetupSetFormVisibility|SetupListStrings|SetupFormSetAdd' crates/` — must return nothing.

- [ ] **Step 11: Commit**

```bash
git add -A && git commit -m "refactor: rename Setup* gRPC contract + CLI noun to Hii* (Phase 2/Task 1)"
```

---

### Task 2: Gateway REST-route module rename `setup.rs` → `hii.rs`

**Files:**
- Rename: `crates/uefi-gateway/src/routes/setup.rs` → `crates/uefi-gateway/src/routes/hii.rs`
- Modify: `crates/uefi-gateway/src/routes/mod.rs` (`pub mod setup`→`pub mod hii`; 2 handler refs)

**Interfaces:** REST route PATHS (`/api/v1/image/:id/set-visibility`, `/api/v1/image/:id/setup-items`) are UNCHANGED — they are the public REST surface and contain no gRPC-derived name (webUI calls `/set-visibility` and `/items`). Only the internal Rust module location moves.

- [ ] **Step 1: `git mv` the route module**

Run:
```bash
git mv crates/uefi-gateway/src/routes/setup.rs crates/uefi-gateway/src/routes/hii.rs
```

- [ ] **Step 2: Update `routes/mod.rs`**

In `crates/uefi-gateway/src/routes/mod.rs`:
(a) Line 5: `pub mod setup;` → `pub mod hii;`
(b) Line 59: `setup::set_visibility` → `hii::set_visibility`
(c) Line 63: `setup::list_items` → `hii::list_items`

(The route path strings `"/api/v1/image/:id/set-visibility"` and `"/api/v1/image/:id/setup-items"` on lines 58-63 are UNCHANGED.)

- [ ] **Step 3: Verify**

Run:
```bash
cargo test -p uefi-gateway
cargo clippy -p uefi-gateway -- -D warnings
cargo fmt -p uefi-gateway -- --check
```
Expected: pass, no warnings.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "refactor(uefi-gateway): rename routes/setup.rs -> hii.rs (Phase 2/Task 2)"
```

---

## Self-Review (completed by plan author)

**Spec coverage (Phase 2 scope = "Proto/CLI rename Setup*→Hii*"):**
- ✅ Proto RPC + messages renamed — Task 1 Step 1.
- ✅ CLI noun `setup`→`hii` (`hii form list` / `hii form set-visibility` / `hii string list`) — Task 1 Steps 4-6.
- ✅ Engine handlers renamed — Task 1 Step 2.
- ✅ cli/tui/gateway gRPC clients renamed — Task 1 Steps 3, 7, 8.
- ✅ All mock_server.rs stubs renamed — Task 1 Step 7.
- ✅ Gateway route module relocated — Task 2.
- ✅ WebUI: confirmed NO gRPC-derived token to rename (fetch paths are `/items`, `/set-visibility`, `/add-formset` — none contain `setup`). Cosmetic webui route-dir rename (`/routes/setup/`) deferred to WebUI cycle per spec scope.

**Placeholder scan:** none. Every step gives exact identifier replacements or exact commands. The `replaceAll` operations are pinned to specific multi-word tokens (not bare prefixes), with per-file sanity-check `rg` commands.

**Type consistency:** `Hii*` names derived mechanically from `Setup*`; generated trait/client methods `hii_*` are the snake_case tonic form of `Hii*` RPC names. `FormInfo`/`StringInfo` unchanged (verified: build.rs attributes still valid). CLI dispatch variants `Hii`/`HiiCmd`/`HiiFormCmd`/`HiiStringCmd` are internally consistent (main.rs only).

**Safety audit (data-vs-code):** the 8 renamed identifiers are multi-word (`SetupListForms`, `setup_list_forms`, …); the bare `"Setup"`/`"setup"` DATA tokens (in `real_image.rs`, `search.rs`, `format.rs`, `image.rs`, `app.rs`, tui mock:146) never match them. Verified the one CLI-noun data token (`e2e.rs:44` arg `"setup"`) IS intentionally renamed (Step 6) because it IS the CLI noun, not UEFI data. `setup_env()`/`setup_gateway()` helpers are untouched (unrelated test fixtures).

**Dependency order:** Phase 1 → Phase 2. Phases 3/4 (the actual readers) depend on both.
