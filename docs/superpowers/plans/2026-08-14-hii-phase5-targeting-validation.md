# HII Targeting-Extension & Validation — Phase 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisites:** Phases 1–4 complete: `hii/` module exists with `HiiError` (8 variants), proto/CLI are `Hii*`, `hii::strings::collect_strings` and `hii::forms::collect_forms` work, `HiiListForms`/`HiiListStrings` handlers are live.

**Goal:** Make `find_item_mut` resolve `Target::Guid`/`Target::GuidSection` (via a new path-resolving `find_item_path`), let `set_item_visibility` accept GUID-targets (closing the `collect_forms` → `hii form set-visibility` round-trip), and validate the whole Plan B pipeline on the real BIOS image plus CLI content-assertions.

**Architecture:** New `find_item_path(root, target) -> Option<Vec<usize>>` in `parser/target.rs` resolves ANY target variant to a tree path (recursive `find_path_by_guid` mirrors `find_by_guid` exactly); `find_item_mut` is reimplemented on top of it (public signature unchanged, Path behavior identical). `set_item_visibility` drops its `Target::Path`-only guard and uses `find_item_path` for the rebuild-path. Validation: two `#[ignore]` real-image tests (HNX99TF: ≥2 formsets, ~English strings, visibility round-trip) and CLI e2e stdout content-assertions against an enriched mock.

**Tech Stack:** Rust (edition 2024), assert_cmd + `predicates` (new dev-dep), tonic mocks.

## Global Constraints

- **TDD order per task:** write failing test → run (fails) → implement → run (passes) → clippy → commit.
- **`find_item` (immutable) is NOT modified** — it works and has real-image coverage; only `find_item_mut` is reimplemented and `find_item_path` added.
- **`find_path_by_guid` must mirror `find_by_guid` semantics exactly:** check `node.guid == Some(g)`, then `ParsingData::Volume.extended_header_guid`, then `ParsingData::GuidedSection.guid`, then children in order (pre-order DFS, first match wins).
- **`find_item_mut` public signature unchanged:** `pub fn find_item_mut<'a>(root: &'a mut FfsNode, target: &Target) -> Result<&'a mut FfsNode, ParserError>`. Existing Path-target behavior identical (same walk, equivalent errors). Only the "only path targets supported for mutable" rejection disappears.
- **`set_item_visibility` semantics unchanged otherwise:** still `HiiError::NotFound` (target unparseable/unresolvable), `HiiError::NotASetupItem` (node is not a Section), still `visible=true` unsuppresses the FIRST suppress-if scope + `mark_rebuild_to_root_by_path`, `visible=false` is a no-op.
- **Real-image tests are `#[ignore]`** with the same reason-string convention as `real_image.rs`; they run only with `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` present (`cargo test -p uefi-engine -- --ignored`).
- **No comments** in code (per AGENTS.md). Per-task verify: `cargo test -p <crate>` + `cargo clippy -p <crate> -- -D warnings` + `cargo fmt -p <crate> -- --check`.
- **Post-rename naming:** proto methods are `hii_list_forms`/`hii_list_strings`/`hii_set_form_visibility`/`hii_form_set_add` (Phase 2); mock methods and CLI noun (`hii form list` etc.) follow.

**Reference — current state (relevant excerpts):**
- `crates/uefi-engine/src/parser/target.rs:87-105` — `find_item_mut` (Path-only, rejects Guid/GuidSection with `"only path targets supported for mutable"`).
- `crates/uefi-engine/src/parser/target.rs:107-127` — `find_by_guid` (immutable, the semantics to mirror).
- `crates/uefi-engine/src/parser/target.rs:129+` — test module with `sample_tree()` (Volume→File(guid `5C60F367-…`, subtype 0x07)→Section(subtype 0x10)) and `use std::str::FromStr;` already in scope via file top (`target.rs:3`).
- `crates/uefi-engine/src/hii/mod.rs` (post-Phase-1) — `set_item_visibility` with the `Target::Path(p) => p.clone(), _ => return Err(HiiError::NotFound)` guard; test module has `mk_node(node_type, body, children)` fixture.
- `crates/uefi-engine/tests/real_image.rs` — `fw_path()`/`load_fw()` helpers; `use uefi_engine::parser::target::{find_item, parse_target};` already imported at line 7.
- `crates/uefi-cli/tests/mock_server.rs` — `hii_list_forms`/`hii_list_strings` stubs return empty vecs (post-Phase-2 names).
- `crates/uefi-cli/tests/e2e.rs` — `edit_flow` drives the binary via assert_cmd against the mock; CLI noun already `hii` (Phase 2).
- `crates/uefi-cli/Cargo.toml` — dev-deps: assert_cmd, tempfile, uuid, tokio-stream (NO `predicates` yet).

---

### Task 1: `find_item_path` + `find_item_mut` Guid/GuidSection support

**Files:**
- Modify: `crates/uefi-engine/src/parser/target.rs`

**Interfaces:**
- Produces: `pub fn find_item_path(root: &FfsNode, target: &Target) -> Option<Vec<usize>>` (resolves any Target variant to a child-index path from `root`; `None` if unresolvable). `find_item_mut` signature unchanged but now accepts all three variants.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `crates/uefi-engine/src/parser/target.rs`:

```rust
    #[test]
    fn find_item_path_resolves_guid_and_guid_section() {
        let tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let p = find_item_path(&tree, &Target::Guid(g)).unwrap();
        assert_eq!(p, vec![0, 0]);
        let p = find_item_path(
            &tree,
            &Target::GuidSection {
                guid: g,
                section_type: 0x10,
                section_index: None,
            },
        )
        .unwrap();
        assert_eq!(p, vec![0, 0, 0]);
    }

    #[test]
    fn find_item_path_returns_none_on_miss() {
        let tree = sample_tree();
        assert!(find_item_path(&tree, &Target::Path(vec![0, 9])).is_none());
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        assert!(
            find_item_path(
                &tree,
                &Target::GuidSection {
                    guid: g,
                    section_type: 0x02,
                    section_index: None,
                },
            )
            .is_none()
        );
        let other = Guid::from_str("00000000-0000-0000-0000-000000000000").unwrap();
        assert!(find_item_path(&tree, &Target::Guid(other)).is_none());
    }

    #[test]
    fn find_item_mut_guid_section_gives_mutable_access() {
        let mut tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let t = Target::GuidSection {
            guid: g,
            section_type: 0x10,
            section_index: None,
        };
        let node = find_item_mut(&mut tree, &t).unwrap();
        node.body.extend_from_slice(b"patched");
        assert!(
            tree.children[0].children[0].children[0]
                .body
                .ends_with(b"patched")
        );
    }

    #[test]
    fn find_item_mut_guid_gives_mutable_access() {
        let mut tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let node = find_item_mut(&mut tree, &Target::Guid(g)).unwrap();
        node.body.push(0xAA);
        assert_eq!(tree.children[0].children[0].body, vec![0xAA]);
    }

    #[test]
    fn find_item_mut_path_still_works() {
        let mut tree = sample_tree();
        let node = find_item_mut(&mut tree, &Target::Path(vec![0, 0, 0])).unwrap();
        node.body.push(1);
        assert_eq!(tree.children[0].children[0].children[0].body, vec![1]);
    }
```

- [ ] **Step 2: Run — verify FAIL**

Run: `cargo test -p uefi-engine parser::target::tests`
Expected: compile FAIL (`cannot find function find_item_path`) — add nothing yet, confirm the tests are the failure.

- [ ] **Step 3: Implement `find_item_path` + rewrite `find_item_mut`**

In `crates/uefi-engine/src/parser/target.rs`, add between `find_item` and `find_item_mut`:

```rust
pub fn find_item_path(root: &FfsNode, target: &Target) -> Option<Vec<usize>> {
    match target {
        Target::Path(indices) => {
            let mut node = root;
            for &i in indices {
                node = node.children.get(i)?;
            }
            Some(indices.clone())
        }
        Target::Guid(g) => find_path_by_guid(root, g, &mut Vec::new()),
        Target::GuidSection {
            guid,
            section_type,
            section_index,
        } => {
            let mut file_path = find_path_by_guid(root, guid, &mut Vec::new())?;
            let file = node_at_path(root, &file_path)?;
            let wanted = *section_index;
            let mut count = 0usize;
            for (i, child) in file.children.iter().enumerate() {
                if child.subtype == *section_type {
                    if wanted.is_none_or(|w| w == count) {
                        file_path.push(i);
                        return Some(file_path);
                    }
                    count += 1;
                }
            }
            None
        }
    }
}

fn node_at_path<'a>(root: &'a FfsNode, path: &[usize]) -> Option<&'a FfsNode> {
    let mut node = root;
    for &i in path {
        node = node.children.get(i)?;
    }
    Some(node)
}

fn find_path_by_guid(node: &FfsNode, g: &Guid, path: &mut Vec<usize>) -> Option<Vec<usize>> {
    if node.guid == Some(*g) {
        return Some(path.clone());
    }
    if let ParsingData::Volume(vd) = &node.parsing_data
        && vd.extended_header_guid == Some(*g)
    {
        return Some(path.clone());
    }
    if let ParsingData::GuidedSection(gs) = &node.parsing_data
        && gs.guid == *g
    {
        return Some(path.clone());
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if let Some(p) = find_path_by_guid(child, g, path) {
            return Some(p);
        }
        path.pop();
    }
    None
}
```

Then REPLACE the whole `find_item_mut` body with:

```rust
pub fn find_item_mut<'a>(
    root: &'a mut FfsNode,
    target: &Target,
) -> Result<&'a mut FfsNode, ParserError> {
    let path = find_item_path(root, target)
        .ok_or_else(|| ParserError::InvalidHeader(format!("target {target:?} not found")))?;
    let mut node = root;
    for &i in &path {
        node = node.children.get_mut(i).ok_or_else(|| {
            ParserError::InvalidHeader(format!("path index {i} not found"))
        })?;
    }
    Ok(node)
}
```

Implementer notes:
- `find_item_path(root, target)` takes an immutable borrow that ends when it returns the owned `Vec<usize>` — the subsequent mutable walk is legal.
- The GuidSection child-scan counter mirrors `find_item`'s arm verbatim (`is_none_or(|w| w == count)`; `count += 1` only reached when `wanted` is `Some`) — this is what makes `collect_forms`'s `<guid>:<subtype>:<idx>` targets resolve to the same node `find_item` picks.
- `find_item_mut`'s Path-target errors are unchanged in substance (a valid path always resolves — `find_item_path` validated it — so the `get_mut` fallback error is defensive only).

- [ ] **Step 4: Run — verify PASS**

```bash
cargo test -p uefi-engine parser::target::tests
cargo test -p uefi-engine
```

Expected: all PASS (5 new + existing; `find_item_by_guid_*` immutable tests untouched).

- [ ] **Step 5: clippy + fmt + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
git add crates/uefi-engine/src/parser/target.rs
git commit -m "feat(uefi-engine): find_item_path + find_item_mut Guid/GuidSection targets"
```

---

### Task 2: `set_item_visibility` accepts GUID-targets

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (post-Phase-1 location; pre-Phase-1 it is `setup/mod.rs`)

**Interfaces:**
- Consumes: `crate::parser::target::find_item_path` (Task 1).
- Produces: `set_item_visibility(image: &mut Image, item_id: &str, visible: bool) -> Result<(), HiiError>` — now accepts Path, `Guid`, and `GuidSection` item_ids (i.e. the `form_id` strings produced by `hii::forms::collect_forms`).

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `crates/uefi-engine/src/hii/mod.rs`, add `use std::str::FromStr;` to the module's imports and append:

```rust
    const FILE_GUID_STR: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn sample_image_with_ifr_guid() -> Image {
        let mut section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        section.subtype = 0x19;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
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
    fn set_item_visibility_guid_section_target_unsuppresses() {
        let mut image = sample_image_with_ifr_guid();
        set_item_visibility(&mut image, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0", true)
            .unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(&section.body[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(section.action, Action::Rebuild);
        assert_eq!(image.root.action, Action::Rebuild);
    }

    #[test]
    fn set_item_visibility_guid_target_rejects_non_section() {
        let mut image = sample_image_with_ifr_guid();
        let err = set_item_visibility(&mut image, FILE_GUID_STR, true).unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }

    #[test]
    fn set_item_visibility_unknown_guid_is_not_found() {
        let mut image = sample_image_with_ifr_guid();
        let err =
            set_item_visibility(&mut image, "00000000-0000-0000-0000-000000000001:0x19:0", true)
                .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }
```

(`HiiError` is in scope via the test module's existing `use super::*;`.)

- [ ] **Step 2: Run — verify FAIL**

Run: `cargo test -p uefi-engine hii::tests::set_item_visibility_guid`
Expected: FAIL — the guid-section test panics with `Err(NotFound)` (the `_ => return Err(HiiError::NotFound)` guard rejects non-Path targets).

- [ ] **Step 3: Implement — drop the Path-only guard**

In `set_item_visibility`, replace:

```rust
    let target = crate::parser::target::parse_target(item_id).map_err(|_| HiiError::NotFound)?;
    let path = match &target {
        Target::Path(p) => p.clone(),
        _ => return Err(HiiError::NotFound),
    };
```

with:

```rust
    let target = crate::parser::target::parse_target(item_id).map_err(|_| HiiError::NotFound)?;
    let path = crate::parser::target::find_item_path(&image.root, &target)
        .ok_or(HiiError::NotFound)?;
```

Nothing else changes: `find_item_mut` (already called below) now resolves the same target; `mark_rebuild_to_root_by_path(&mut image.root, &path)` receives the resolved path for every target kind.

- [ ] **Step 4: Run — verify PASS**

```bash
cargo test -p uefi-engine hii
cargo test -p uefi-engine
```

Expected: all PASS — including the pre-existing path-target test `set_item_visibility_true_unsuppresses_and_cascades` (regression guard).

- [ ] **Step 5: clippy + fmt + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): set_item_visibility accepts GUID-targets"
```

---

### Task 3: Real-image validation tests (HNX99TF)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (append two tests)

**Interfaces:**
- Consumes: `uefi_engine::hii::forms::collect_forms`, `uefi_engine::hii::strings::collect_strings`, `uefi_engine::hii::set_item_visibility`, `uefi_engine::parser::target::{find_item, parse_target}` (already imported at `real_image.rs:7`).

- [ ] **Step 1: Append the real-image tests**

Append to `crates/uefi-engine/tests/real_image.rs`:

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/"]
fn real_image_hii_forms_and_strings() {
    use std::collections::HashSet;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::strings::collect_strings;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    let forms = collect_forms(&img);
    assert!(!forms.is_empty(), "expected forms in real image");
    let formsets: HashSet<&str> = forms.iter().map(|f| f.formset_guid.as_str()).collect();
    assert!(
        formsets.len() >= 2,
        "expected >=2 formsets (Setup + Platform/IntelRCSetup), got {formsets:?}"
    );
    assert!(
        forms.iter().all(|f| f.form_id.contains(':')),
        "every form_id must be a GUID-section target"
    );

    let strings = collect_strings(&img);
    assert!(!strings.is_empty(), "expected strings in real image");
    assert!(
        strings
            .iter()
            .all(|s| s.language.to_lowercase().starts_with("en")),
        "primary language expected ~English (en/en-US/eng), got {:?}",
        strings.first().map(|s| &s.language)
    );

    eprintln!(
        "real_image hii: {} forms across {} formsets {:?}; {} strings (language {:?})",
        forms.len(),
        formsets.len(),
        formsets,
        strings.len(),
        strings.first().map(|s| &s.language)
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/"]
fn real_image_hii_form_visibility_round_trip() {
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::set_item_visibility;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let forms = collect_forms(&img);
    assert!(!forms.is_empty());
    let form_id = forms[0].form_id.clone();

    let t = parse_target(&form_id).expect("form_id parses as target");
    let node = find_item(&img.root, &t).expect("form_id resolves in tree");
    assert_eq!(
        node.node_type,
        FfsType::Section,
        "form target must resolve to a Section"
    );

    set_item_visibility(&mut img, &form_id, true)
        .expect("set_item_visibility on real form target");
}
```

Implementer notes:
- Spec expectation: the image contains two form-packages (Setup + Platform/IntelRCSetup) → `formsets.len() >= 2`. If only one is found, DO NOT relax the assertion — investigate (dump `collect_forms` output, cross-check with UEFITool) and record the finding per AGENTS.md plan-defect rule.
- The round-trip test asserts `Ok(())`, not a body change: most real forms are already unsuppressed (`visible=true` is a no-op then); what is validated is that a `collect_forms`-produced `form_id` parses, resolves to a Section, and passes the mutation gate end-to-end.

- [ ] **Step 2: Run — verify they compile; run ignored tests if firmware present**

```bash
cargo test -p uefi-engine --test real_image
test -f refs/fw/HNX99TF_200525_original_E5C88C6F.bin && \
  cargo test -p uefi-engine --test real_image -- --ignored hii
```

Expected: compile PASS; with the firmware present, both new tests PASS and the eprintln shows the formset GUIDs / string language.

- [ ] **Step 3: clippy + fmt + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): real-image HII forms/strings/visibility validation (ignored)"
```

---

### Task 4: CLI content-assertions (`hii form list` / `hii string list`)

**Files:**
- Modify: `crates/uefi-cli/Cargo.toml` (dev-dep `predicates`)
- Modify: `crates/uefi-cli/tests/mock_server.rs` (enrich `hii_list_forms`/`hii_list_strings`)
- Modify: `crates/uefi-cli/tests/e2e.rs` (new test)

**Interfaces:**
- Consumes: post-Phase-2 mock trait methods `hii_list_forms(Request<HiiListFormsRequest>)` / `hii_list_strings(Request<HiiListStringsRequest>)`; `uefi_proto::{FormInfo, StringInfo}` (fields: `form_id/formset_guid/form_id_ifr/title/visible`, `language/string_id/text`).

- [ ] **Step 1: Add the `predicates` dev-dependency**

In `crates/uefi-cli/Cargo.toml` `[dev-dependencies]`, append:

```toml
predicates = "3"
```

- [ ] **Step 2: Enrich the mock responses**

In `crates/uefi-cli/tests/mock_server.rs`, replace the `hii_list_forms` stub body:

```rust
    async fn hii_list_forms(
        &self,
        _req: Request<HiiListFormsRequest>,
    ) -> Result<Response<HiiListFormsResponse>, Status> {
        Ok(Response::new(HiiListFormsResponse {
            forms: vec![FormInfo {
                form_id: "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0".into(),
                formset_guid: "5C60F367-A505-419A-859E-2A4FF6CA6FE5".into(),
                form_id_ifr: 1,
                title: "Main".into(),
                visible: true,
            }],
        }))
    }
```

and the `hii_list_strings` stub body:

```rust
    async fn hii_list_strings(
        &self,
        _req: Request<HiiListStringsRequest>,
    ) -> Result<Response<HiiListStringsResponse>, Status> {
        Ok(Response::new(HiiListStringsResponse {
            strings: vec![StringInfo {
                language: "eng".into(),
                string_id: 1,
                text: "Hello".into(),
            }],
        }))
    }
```

(`FormInfo`/`StringInfo` arrive via the existing `use uefi_proto::*;`.)

- [ ] **Step 3: Write the e2e content-assertion test**

Append to `crates/uefi-cli/tests/e2e.rs`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn hii_list_output_content() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("e2e-hii.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["hii", "form", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Main"));
    cli(&sock, cwd)
        .args(["hii", "string", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Hello"));
    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
```

- [ ] **Step 4: Run — verify PASS**

```bash
cargo test -p uefi-cli
```

Expected: PASS — `hii_list_output_content` (stdout contains the form title "Main" and string text "Hello" — the soft-note content-assertion from `TODO.md:89`) and the existing `edit_flow` + `mock_roundtrip`.

- [ ] **Step 5: clippy + fmt + commit**

```bash
cargo clippy -p uefi-cli -- -D warnings
cargo fmt -p uefi-cli -- --check
git add crates/uefi-cli/Cargo.toml crates/uefi-cli/tests/mock_server.rs crates/uefi-cli/tests/e2e.rs
git commit -m "test(uefi-cli): hii form/string list stdout content assertions"
```

---

## Self-Review (completed by plan author)

**Spec coverage (Phase 5 = "Targeting-extension + validation"):** ✅ `find_item_mut` Guid/GuidSection arms (Task 1, via `find_item_path` — same resolution semantics, mutable walk); ✅ `set_item_visibility` GUID-target (Task 2); ✅ real-image `#[ignore]` tests — ≥2 formsets (Setup + Platform/IntelRCSetup), non-empty ~English strings, visibility round-trip (Task 3); ✅ CLI content-assertions — `hii form list` stdout contains title (Task 4). Scope check: TUI/Gateway/WebUI client-wiring is explicitly out of scope (spec decision #2) — untouched.

**Placeholder scan:** none — all code blocks complete; every command exact.

**Type consistency:** `find_item_path(&FfsNode, &Target) -> Option<Vec<usize>>` used identically in Tasks 1–2. `find_item_mut` signature unchanged. `HiiError::{NotFound, NotASetupItem}` match spec error-handling section. Mock `FormInfo`/`StringInfo` field sets match `engine.proto:145-166` (and Phase 3's `collect_strings` output types). Mock method names are post-Phase-2 (`hii_list_forms`/`hii_list_strings`) per the plan prerequisite chain. `predicates::str::contains` requires the added dev-dep (Step 1 before Step 3 — ordered).

**Ordering:** Tasks strictly sequential: Task 2 consumes Task 1's `find_item_path`; Tasks 3–4 exercise the full chain. This closes Plan B — all five phases planned (1 merge, 2 rename, 3 strings, 4 forms, 5 targeting+validation).
