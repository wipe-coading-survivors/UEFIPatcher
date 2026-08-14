# HII IFR-Reader & Forms — Phase 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisites:** Phase 1 (`hii/` module exists), Phase 2 (proto is `Hii*`), Phase 3 (`hii::strings::collect_strings` exists — used here for title resolution) complete.

**Goal:** Implement `is_form_package` + `parse_form_package` (pure IFR opcode-walker in `hii/ifr.rs`) and `collect_forms(image)` (tree-walk driver in new `hii/forms.rs`), and wire the `HiiListForms` RPC handler to return real data instead of `UNIMPLEMENTED`.

**Architecture:** Pure parser `parse_form_package(&[u8]) -> Option<FormSetInfo>` (TDD on synthetic bytes, no `Image`), appended to the existing `hii/ifr.rs` next to the suppress-if helpers. A thin read-only driver `collect_forms(&Image) -> Vec<FormInfo>` walks the FFS tree, parses every form-package section, builds the GUID-target `form_id` (`<file_guid>:<subtype>:<idx>`, mirrors `parser::target::find_item` GuidSection semantics), and resolves titles via a StringId→text map from `hii::strings::collect_strings`.

**Tech Stack:** Rust (edition 2024), `r_efi::hii` (opcodes + `FormId`/`StringId`), `uefi_proto::FormInfo`.

## Global Constraints

- **Module-first rule:** add `pub mod forms;` to `hii/mod.rs` in the SAME step as creating `forms.rs`, BEFORE `cargo test`.
- **TDD order per task:** write failing test → run (fails) → implement → run (passes) → clippy → commit.
- **Best-effort reader:** `collect_forms` is infallible/read-only — malformed packages are skipped (`parse_form_package` returns `None`); never panic. `parse_form_package` returns `None` on truncation or missing `IFR_FORM_SET_OP`.
- **No proto/CLI changes** in Phase 4: `HiiListFormsRequest/Response`/`FormInfo` already exist (Phase 2 left the handler stub returning `UNIMPLEMENTED`); only the handler body changes.
- **No comments** in code (per AGENTS.md). Per-task verify: `cargo test -p uefi-engine` + `cargo clippy -p uefi-engine -- -D warnings` + `cargo fmt -p uefi-engine -- --check`.
- **Opcode constants from `r_efi::hii` only** (do not redefine): `IFR_FORM_SET_OP=0x0E`, `IFR_FORM_OP=0x01`, `IFR_SUPPRESS_IF_OP=0x0A`, `IFR_END_OP=0x29`, `PACKAGE_FORMS=0x02`.
- **IFR opcode encoding** (UEFI IFR spec, `r_efi::hii::IfrOpHeader`): `[op_code: u8][length_and_scope: u8]` where `length = length_and_scope & 0x7F` (FULL opcode length incl. the 2-byte header, min 2) and `scope = length_and_scope & 0x80 != 0`.
- **`IfrFormSet` payload layout** (after the 2-byte op header): `guid[16]` `form_set_title: u16 LE` `help: u16 LE` `flags: u8` `class_guid[N][16]` — total fixed part 23 bytes; class_guids are skipped (walker advances by `length`), per spec "число class_guid выводится из length".
- **`IfrForm` payload layout**: `form_id: u16 LE` `form_title: u16 LE` (6 bytes total incl. header).
- **form_id target format:** `<file_guid>:<subtype:#04x>:<idx>` where `idx` counts same-subtype siblings BEFORE this child within the same parent node (exact mirror of `parser::target::find_item`'s GuidSection counter — makes the target round-trip through `parse_target`).

**Reference — current state (after Phases 1–3):**
- `crates/uefi-engine/src/hii/ifr.rs` — `SuppressScope`, `find_suppress_if_scopes`, `unsuppress` (no test module yet; keep unchanged).
- `crates/uefi-engine/src/hii/strings.rs` — `pub fn collect_strings(image: &Image) -> Vec<uefi_proto::StringInfo>` (StringInfo: `language: String, string_id: u32, text: String`).
- `crates/uefi-engine/src/rpc/server.rs` — `hii_list_forms` handler stub returns `Err(Status::unimplemented(...))`.
- `crates/uefi-proto` `FormInfo`: `form_id: String, formset_guid: String, form_id_ifr: u32, title: String, visible: bool` (reachable as `uefi_proto::FormInfo`).
- `crates/uefi-engine/src/parser/file.rs:9` — `guid_from_bytes(&[u8]) -> Option<Guid>` (mixed-endian `Guid::from_bytes`) — reuse for the FormSet GUID.

---

### Task 1: `is_form_package` + `parse_form_package` pure IFR walker

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (append structs + fns + test module)

**Interfaces:**
- Produces: `pub struct FormSetInfo { pub guid: Guid, pub title: StringId, pub forms: Vec<RawForm> }`; `pub struct RawForm { pub form_id: FormId, pub title: StringId, pub suppressed: bool }`; `pub fn is_form_package(body: &[u8]) -> bool`; `pub fn parse_form_package(body: &[u8]) -> Option<FormSetInfo>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/uefi-engine/src/hii/ifr.rs` (file currently has no test module):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use r_efi::hii::{PACKAGE_STRINGS};
    use std::str::FromStr;

    const FORMSET_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn form_set(guid: &Guid, title: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&guid.to_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16, title: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        opcode(IFR_FORM_OP, true, &p)
    }

    fn end() -> Vec<u8> {
        vec![IFR_END_OP, 0x02]
    }

    fn package(ifr: &[u8]) -> Vec<u8> {
        let total = 4 + ifr.len();
        let mut b = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            PACKAGE_FORMS,
        ];
        b.extend_from_slice(ifr);
        b
    }

    #[test]
    fn is_form_package_detects_form_packages() {
        let pkg = package(&form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 1));
        assert!(is_form_package(&pkg));
        assert!(!is_form_package(&[0x00, 0x00, 0x00, PACKAGE_FORMS]));
        assert!(!is_form_package(&[
            0x17, 0x00, 0x00, PACKAGE_STRINGS, IFR_FORM_SET_OP
        ]));
    }

    #[test]
    fn parse_extracts_formset_and_forms() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let fs = parse_form_package(&package(&ifr)).unwrap();
        assert_eq!(fs.guid, g);
        assert_eq!(fs.title, 7);
        assert_eq!(fs.forms.len(), 1);
        assert_eq!(fs.forms[0].form_id, 1);
        assert_eq!(fs.forms[0].title, 10);
        assert!(!fs.forms[0].suppressed);
    }

    #[test]
    fn parse_marks_suppressed_form() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(2, 20));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let fs = parse_form_package(&package(&ifr)).unwrap();
        assert_eq!(fs.forms.len(), 2);
        assert!(!fs.forms[0].suppressed);
        assert!(fs.forms[1].suppressed);
    }

    #[test]
    fn parse_returns_none_on_truncation() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        pkg.truncate(pkg.len() - 7);
        assert!(parse_form_package(&pkg).is_none());
    }

    #[test]
    fn parse_returns_none_for_non_form_package() {
        assert!(parse_form_package(&[
            0x00, 0x00, 0x00, PACKAGE_STRINGS, IFR_FORM_SET_OP
        ])
        .is_none());
    }
}
```

- [ ] **Step 2: Run — verify FAIL**

Run: `cargo test -p uefi-engine hii::ifr`
Expected: compile FAIL (`cannot find function is_form_package` / `parse_form_package`, `cannot find type FormSetInfo`).

- [ ] **Step 3: Implement the walker**

Add these imports at the very top of `crates/uefi-engine/src/hii/ifr.rs`:

```rust
use r_efi::hii::{
    FormId, IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP, PACKAGE_FORMS,
    StringId,
};

use crate::types::Guid;
```

Add above the `#[cfg(test)]` block:

```rust
#[derive(Debug, Clone)]
pub struct FormSetInfo {
    pub guid: Guid,
    pub title: StringId,
    pub forms: Vec<RawForm>,
}

#[derive(Debug, Clone)]
pub struct RawForm {
    pub form_id: FormId,
    pub title: StringId,
    pub suppressed: bool,
}

pub fn is_form_package(body: &[u8]) -> bool {
    body.len() >= 5 && body[3] == PACKAGE_FORMS && body[4] == IFR_FORM_SET_OP
}

pub fn parse_form_package(body: &[u8]) -> Option<FormSetInfo> {
    if !is_form_package(body) {
        return None;
    }
    let mut guid = None;
    let mut title: StringId = 0;
    let mut forms = Vec::new();
    let mut scope_stack: Vec<u8> = Vec::new();
    let mut i = 4;
    while i + 2 <= body.len() {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > body.len() {
            return None;
        }
        match op_code {
            IFR_FORM_SET_OP => {
                if length < 23 {
                    return None;
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&body[i + 2..i + 18]);
                guid = Some(Guid::from_bytes(arr));
                title = u16::from_le_bytes([body[i + 18], body[i + 19]]);
            }
            IFR_FORM_OP => {
                if length < 6 {
                    return None;
                }
                forms.push(RawForm {
                    form_id: u16::from_le_bytes([body[i + 2], body[i + 3]]),
                    title: u16::from_le_bytes([body[i + 4], body[i + 5]]),
                    suppressed: scope_stack.contains(&IFR_SUPPRESS_IF_OP),
                });
            }
            IFR_END_OP => {
                scope_stack.pop();
            }
            _ => {
                tracing::trace!(op_code, offset = i, "unknown ifr opcode skipped");
            }
        }
        if length_and_scope & 0x80 != 0 {
            scope_stack.push(op_code);
        }
        i += length;
    }
    guid.map(|g| FormSetInfo {
        guid: g,
        title,
        forms,
    })
}
```

Notes for the implementer:
- Walker starts at offset 4 (after `PackageHeader` = `length[3] + type[1]`).
- `IfrFormSet` fixed part is 23 bytes (2 header + 16 guid + 2 title + 2 help + 1 flags); class_guids after it are skipped implicitly by advancing `i += length`.
- Scope-stack: opcodes with the scope bit push their `op_code`; `IFR_END_OP` pops. A form is `suppressed` iff `IFR_SUPPRESS_IF_OP` is on the stack at the moment the `IfrForm` opcode is read (lexical scoping).
- Stray `END` (pop on empty stack) is a no-op (`Vec::pop` returns `None`).
- The final `guid.map(...)` makes the "no `IFR_FORM_SET_OP` found" case `None` (unreachable in practice because `is_form_package` gates on `body[4] == IFR_FORM_SET_OP`, but keeps the contract total).
- The truncation test cuts 7 bytes off a 37-byte package → offset 30 lands INSIDE the `IfrForm` opcode (which spans 27..33): the 2-byte header is readable but `i + length (33) > body.len() (30)` → `None`. Cutting only the trailing END bytes would exit the loop cleanly and return `Some` — do not "simplify" the test to a smaller cut.

- [ ] **Step 4: Run — verify PASS**

Run: `cargo test -p uefi-engine hii::ifr`
Expected: all PASS (5 new tests; existing engine tests untouched).

- [ ] **Step 5: clippy + fmt + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
git add crates/uefi-engine/src/hii/ifr.rs
git commit -m "feat(uefi-engine): hii::ifr parse_form_package (IFR opcode walker)"
```

---

### Task 2: `collect_forms` tree-walk driver

**Files:**
- Create: `crates/uefi-engine/src/hii/forms.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (add `pub mod forms;`)

**Interfaces:**
- Consumes: `crate::hii::ifr::{parse_form_package, FormSetInfo, RawForm}` (Task 1); `crate::hii::strings::collect_strings(&Image) -> Vec<uefi_proto::StringInfo>` (Phase 3).
- Produces: `pub fn collect_forms(image: &Image) -> Vec<uefi_proto::FormInfo>`.

- [ ] **Step 1: Add module declaration + create `forms.rs` with failing tests**

Add to `crates/uefi-engine/src/hii/mod.rs` (top, with the other `pub mod` lines): `pub mod forms;`

Create `crates/uefi-engine/src/hii/forms.rs` with the test module + a stub:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsType, ImageMode, ParsingData, Target};
    use r_efi::hii::{
        IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP, PACKAGE_FORMS,
    };
    use std::str::FromStr;

    const SIBT_STRING_SCSU: u8 = 0x10;
    const SIBT_END: u8 = 0x00;
    const LANGUAGE_OFFSET: usize = 12;
    const FILE_GUID: &str = "899407d7-92a6-4174-968f-6f0b47f86a23";
    const FORMSET_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn form_set(guid: &Guid, title: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&guid.to_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16, title: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        opcode(IFR_FORM_OP, true, &p)
    }

    fn end() -> Vec<u8> {
        vec![IFR_END_OP, 0x02]
    }

    fn package(ifr: &[u8]) -> Vec<u8> {
        let total = 4 + ifr.len();
        let mut b = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            PACKAGE_FORMS,
        ];
        b.extend_from_slice(ifr);
        b
    }

    fn string_pkg() -> Vec<u8> {
        let lang = "eng";
        let info_off = LANGUAGE_OFFSET + lang.len() + 1;
        let mut b = vec![0u8; 3];
        b.push(0x04);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(info_off as u32).to_le_bytes());
        while b.len() < LANGUAGE_OFFSET {
            b.push(0);
        }
        b.extend_from_slice(lang.as_bytes());
        b.push(0);
        b.extend_from_slice(&[
            SIBT_STRING_SCSU, b'M', b'a', b'i', b'n', 0,
            SIBT_STRING_SCSU, b'H', b'i', b'd', b'd', b'e', b'n', 0,
            SIBT_END,
        ]);
        let len = b.len() as u32;
        b[0] = (len & 0xFF) as u8;
        b[1] = ((len >> 8) & 0xFF) as u8;
        b[2] = ((len >> 16) & 0xFF) as u8;
        b
    }

    fn form_pkg(form_title_id: u16) -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 1);
        ifr.extend(form(1, form_title_id));
        ifr.extend(end());
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(2, 2));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        package(&ifr)
    }

    fn mk_node(
        guid: Option<Guid>,
        node_type: FfsType,
        subtype: u8,
        body: Vec<u8>,
        children: Vec<FfsNode>,
    ) -> FfsNode {
        FfsNode {
            guid,
            node_type,
            subtype,
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

    fn mk_image(form_body: Vec<u8>) -> Image {
        let str_sec = mk_node(None, FfsType::Section, 0x19, string_pkg(), vec![]);
        let form_sec = mk_node(None, FfsType::Section, 0x19, form_body, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![str_sec, form_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        }
    }

    #[test]
    fn collect_forms_resolves_titles_and_visibility() {
        let image = mk_image(form_pkg(1));
        let forms = collect_forms(&image);
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x19:1"));
        assert_eq!(forms[0].formset_guid, FORMSET_GUID);
        assert_eq!(forms[0].form_id_ifr, 1);
        assert_eq!(forms[0].title, "Main");
        assert!(forms[0].visible);
        assert_eq!(forms[1].form_id, format!("{FILE_GUID}:0x19:1"));
        assert_eq!(forms[1].form_id_ifr, 2);
        assert_eq!(forms[1].title, "Hidden");
        assert!(!forms[1].visible);
    }

    #[test]
    fn collect_forms_target_round_trips() {
        let image = mk_image(form_pkg(1));
        let forms = collect_forms(&image);
        let t = crate::parser::target::parse_target(&forms[0].form_id).unwrap();
        match t {
            Target::GuidSection {
                section_type,
                section_index,
                ..
            } => {
                assert_eq!(section_type, 0x19);
                assert_eq!(section_index, Some(1));
            }
            other => panic!("expected GuidSection, got {other:?}"),
        }
    }

    #[test]
    fn collect_forms_title_fallback_empty() {
        let image = mk_image(form_pkg(99));
        let forms = collect_forms(&image);
        assert_eq!(forms[0].title, "");
    }

    #[test]
    fn collect_forms_empty_image() {
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        assert!(collect_forms(&image).is_empty());
    }
}
```

And a stub above it so the file compiles:

```rust
pub fn collect_forms(_image: &Image) -> Vec<FormInfo> {
    Vec::new()
}
```

(plus the `use` lines from Step 3 so the test module's `use super::*` resolves `Image`, `FormInfo`, `Guid`).

- [ ] **Step 2: Run — verify FAIL**

Run: `cargo test -p uefi-engine hii::forms`
Expected: FAIL (`collect_forms_resolves_titles_and_visibility` panics on `assert_eq!(forms.len(), 2)` — stub returns empty vec; other tests likewise).

- [ ] **Step 3: Implement `collect_forms` + walk helper**

The full non-test content of `crates/uefi-engine/src/hii/forms.rs`:

```rust
use std::collections::HashMap;

use uefi_proto::FormInfo;

use crate::hii::ifr::parse_form_package;
use crate::hii::strings::collect_strings;
use crate::types::{guid_to_upper_string, FfsNode, FfsType, Guid, Image};

pub fn collect_forms(image: &Image) -> Vec<FormInfo> {
    let mut titles: HashMap<u16, String> = HashMap::new();
    for s in collect_strings(image) {
        titles.entry(s.string_id as u16).or_insert(s.text);
    }
    let mut out = Vec::new();
    walk(&image.root, None, &titles, &mut out);
    out
}

fn walk(
    node: &FfsNode,
    file_guid: Option<Guid>,
    titles: &HashMap<u16, String>,
    out: &mut Vec<FormInfo>,
) {
    let file_guid = if node.node_type == FfsType::File {
        node.guid.or(file_guid)
    } else {
        file_guid
    };
    let mut counters: HashMap<u8, usize> = HashMap::new();
    for child in &node.children {
        let idx = *counters.entry(child.subtype).or_insert(0);
        if child.node_type == FfsType::Section
            && let Some(fs) = parse_form_package(&child.body)
            && let Some(fg) = file_guid
        {
            let target = format!("{}:{:#04x}:{}", fg, child.subtype, idx);
            for raw in &fs.forms {
                out.push(FormInfo {
                    form_id: target.clone(),
                    formset_guid: guid_to_upper_string(&fs.guid),
                    form_id_ifr: raw.form_id as u32,
                    title: titles.get(&raw.title).cloned().unwrap_or_default(),
                    visible: !raw.suppressed,
                });
            }
        }
        counters.insert(child.subtype, idx + 1);
        walk(child, file_guid, titles, out);
    }
}
```

Notes for the implementer:
- `parse_form_package` internally gates on `is_form_package`, so no separate detection call is needed here.
- The per-parent `counters` map (keyed by subtype) mirrors `parser::target::find_item`'s GuidSection counter exactly: index = number of same-subtype siblings preceding this child under the same parent. For sections that are DIRECT children of their File (the normal case), the produced `form_id` round-trips through `parse_target` → `find_item`/`find_item_mut` (the latter lands in Phase 5).
- `node.guid.or(file_guid)` propagates the enclosing File's GUID down through nested nodes (compressed sections etc.), so form packages inside decompressed children still get a GUID-target — with the caveat that `Target::GuidSection` only resolves direct file children; a form section nested under a non-File parent is listed but its target may not resolve for mutation in Phase 5 (acceptable v1: real Setup/Platform form sections are direct children).
- All forms of one FormSet share the same `form_id` (section-level targeting, per spec "Семантика видимости v1").
- Title fallback: StringId not in the primary-language map → `""` (spec "Title resolution global-map").
- `FILE_GUID` const in tests is LOWERCASE on purpose: `format!("{}...", fg)` renders `Guid` via uguid's lowercase `Display`, so the produced `form_id` contains a lowercase GUID (`parse_target` accepts lowercase — see its `parse_lowercase_guid_target` test). `FORMSET_GUID` stays uppercase because `formset_guid` goes through `guid_to_upper_string`.

- [ ] **Step 4: Run — verify PASS**

Run: `cargo test -p uefi-engine hii::forms`
Expected: all 4 PASS. Also run `cargo test -p uefi-engine` (no regressions).

- [ ] **Step 5: clippy + fmt + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
git add crates/uefi-engine/src/hii/forms.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii::forms collect_forms (tree-walk + title resolution)"
```

---

### Task 3: Wire the `HiiListForms` RPC handler

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_list_forms` handler — the stub currently at ~line 648 returning `UNIMPLEMENTED`)

**Interfaces:**
- Consumes: `crate::hii::forms::collect_forms(&Image) -> Vec<uefi_proto::FormInfo>` (Task 2). `HiiListFormsRequest`/`HiiListFormsResponse` are already imported in server.rs (the stub's signature uses them).

- [ ] **Step 1: Replace the stub handler**

Replace the `hii_list_forms` stub:

```rust
    #[tracing::instrument(skip(self, _req), err)]
    async fn hii_list_forms(
        &self,
        _req: Request<HiiListFormsRequest>,
    ) -> RpcResult<HiiListFormsResponse> {
        Err(Status::unimplemented(
            "HiiListForms not implemented (Plan B)",
        ))
    }
```

with (mirror of the Phase 3 `hii_list_strings` handler):

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn hii_list_forms(
        &self,
        req: Request<HiiListFormsRequest>,
    ) -> RpcResult<HiiListFormsResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let forms = crate::hii::forms::collect_forms(&img);
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(HiiListFormsResponse { forms }))
    }
```

- [ ] **Step 2: Verify build + tests + clippy + fmt**

```bash
cargo test -p uefi-engine
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
```

Expected: all pass; `hii_list_forms` no longer returns `UNIMPLEMENTED`. Read-only handler — no `flush_image` (matches `hii_list_strings`).

- [ ] **Step 3: Commit**

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): wire HiiListForms RPC handler"
```

---

## Self-Review (completed by plan author)

**Spec coverage (IFR-reader scope):** ✅ `parse_form_package` opcode walker (scope-stack, FORM_SET/FORM extraction, unknown-opcode skip + trace, `None` on truncation/missing FORM_SET); ✅ `is_form_package` (`body[3]==PACKAGE_FORMS && body[4]==IFR_FORM_SET_OP`); ✅ `collect_forms` (tree-walk, GUID-target `<guid>:<subtype>:<idx>`, formset_guid upper, form_id_ifr u16→u32, title via StringId→text map from `collect_strings`, `visible = !suppressed`); ✅ `HiiListForms` handler оживлён; ✅ pure-parser unit tests (synthetic IFR incl. suppressed form); ✅ synthetic-Image driver tests; ✅ target round-trip test. Real-image `#[ignore]` tests + CLI content-assertions + `find_item_mut` GuidSection arms → Phase 5 (per spec decomposition).

**Placeholder scan:** none — all code blocks complete and final.

**Type consistency:** `FormSetInfo { guid: Guid, title: StringId(u16), forms: Vec<RawForm> }`; `RawForm { form_id: FormId(u16), title: StringId(u16), suppressed: bool }` — matches spec signatures. `collect_forms` → `uefi_proto::FormInfo { form_id: String, formset_guid: String, form_id_ifr: u32, title: String, visible: bool }`; u16→u32 widening for `form_id_ifr`, u32→u16 narrowing for the titles-map key (`StringInfo.string_id` was widened from the same u16 in Phase 3). `guid_to_upper_string` reused from `crate::types`. `Guid::from_bytes`/`to_bytes` round-trip verified against `parser/file.rs` usage. Test fixtures (`opcode`/`form_set`/`form`/`package`) duplicated between ifr.rs and forms.rs test modules — intentional (test-local builders, keep prod modules decoupled).
