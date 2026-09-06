# HII String-Reader — Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisites:** Phase 1 (hii/ module exists) + Phase 2 (proto is `Hii*`) complete.

**Goal:** Implement `parse_string_package` (pure SIBT-walker + language extraction) and `collect_strings(image)`, and wire the `HiiListStrings` RPC handler to return real data instead of `UNIMPLEMENTED`.

**Note on ordering:** The spec lists "String-reader" as Фаза 4 and "IFR-reader" as Фаза 3, but states they may be done in either order. This plan does strings FIRST because `collect_forms` (Phase 4) resolves form titles via the StringId→text map produced here. Implementation order: merge → rename → **strings** → forms → targeting.

**Architecture:** Pure parser `parse_string_package(&[u8]) -> Option<ParsedStringPackage>` (TDD on synthetic bytes, no `Image`), plus a thin read-only tree-walk driver `collect_strings(&Image) -> Vec<StringInfo>` that takes the FIRST string-package (primary language). The writer (`string_pack.rs`) is untouched; this adds the mirror reader in a new `strings.rs`. Engine already depends on `uefi-proto`, so `collect_strings` returns proto `StringInfo` directly (matches how `collect_forms` will return `FormInfo`).

**Tech Stack:** Rust (edition 2024), `r_efi::hii::PACKAGE_STRINGS`, `uefi_proto::StringInfo`.

## Global Constraints

- **Module-first rule:** add `pub mod strings;` to `hii/mod.rs` in the SAME step as creating `strings.rs`, BEFORE `cargo test`.
- **TDD order per task:** write failing test → run (fails) → implement → run (passes) → clippy → commit.
- **Best-effort reader:** malformed SIBT blocks stop the walk (`break`) + `tracing::warn!`; never panic. Strings before the break are returned.
- **Primary-language only:** `collect_strings` returns the first string-package found (pre-order DFS); stop after it.
- **No comments** in code. Per-task: `cargo test -p uefi-engine` + `cargo clippy -p uefi-engine -- -D warnings`.
- **Language header layout (real edk2 `EFI_HII_STRING_PACKAGE_HDR`, verified against `refs/IFRExtractor-RS/src/uefi_parser.rs` `hii_string_package`):** Header(4, type byte 0x04) + HdrSize u32(@4) + StringInfoOffset u32(@8) + `CHAR16 LanguageWindow[16]` (32 bytes @12, zeroed in practice) + LanguageName u16(@44) + Language[] ASCII NUL-terminated starting @46, running until StringInfoOffset. `language` = ASCII from offset 46 to first NUL; `LANGUAGE_OFFSET = 46`. Packages whose header is smaller than 46 naturally fall back to `""` (the spec permits `language = ""` for nonstandard layouts). No separate legacy path.
- **STRINGS_* truncation break (review fix):** in every `SIBT_STRINGS_*` inner loop, check `p >= body.len()` before decoding each string; if the data ran out while `count` strings remain, emit `tracing::warn!` and break out of the SIBT walk (labeled `break 'outer`). Never push phantom empty entries; strings decoded before the truncation are still returned; well-formed packages behave identically.

**Reference:** existing writer `crates/uefi-engine/src/setup_advanced→hii/string_pack.rs` (`scan_sibt`, `skip_scsu`, `skip_ucs2`, `is_string_package`). The reader mirrors the SIBT cases but captures text.

---

### Task 1: `parse_string_package` pure parser

**Files:**
- Create: `crates/uefi-engine/src/hii/strings.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (add `pub mod strings;`)

**Interfaces:**
- Produces: `pub struct ParsedStringPackage { pub language: String, pub strings: Vec<(u16, String)> }`; `pub fn parse_string_package(body: &[u8]) -> Option<ParsedStringPackage>`.

- [ ] **Step 1: Create `strings.rs` with the failing test + module declaration**

Add to `crates/uefi-engine/src/hii/mod.rs` (top, with the other `pub mod` lines): `pub mod strings;`

Create `crates/uefi-engine/src/hii/strings.rs` with ONLY the test module + a stub that returns `None`:

```rust
use crate::hii::string_pack::is_string_package;

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

const HEADER_LEN: usize = 4;
const STRING_INFO_OFFSET_POS: usize = 8;
const LANGUAGE_OFFSET: usize = 46;

pub struct ParsedStringPackage {
    pub language: String,
    pub strings: Vec<(u16, String)>,
}

pub fn parse_string_package(_body: &[u8]) -> Option<ParsedStringPackage> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pkg(language: &str, sibt_bytes: &[u8]) -> Vec<u8> {
        let hdr_size: u32 = (46 + language.len() + 1) as u32;
        let info_off = hdr_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(0x04);
        buf.extend_from_slice(&hdr_size.to_le_bytes());
        buf.extend_from_slice(&info_off.to_le_bytes());
        while buf.len() < 44 {
            buf.push(0);
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(language.as_bytes());
        buf.push(0);
        while buf.len() < info_off as usize {
            buf.push(0);
        }
        buf.extend_from_slice(sibt_bytes);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    #[test]
    fn parse_scsu_strings_and_language() {
        let sibt = [
            SIBT_STRING_SCSU, b'H', b'i', 0,
            SIBT_STRING_SCSU, b'B', b'y', b'e', 0,
            SIBT_END,
        ];
        let pkg_body = make_pkg("en-US", &sibt);
        let parsed = parse_string_package(&pkg_body).expect("some package");
        assert_eq!(parsed.language, "en-US");
        assert_eq!(parsed.strings.len(), 2);
        assert_eq!(parsed.strings[0], (1, "Hi".to_string()));
        assert_eq!(parsed.strings[1], (2, "Bye".to_string()));
    }
}
```

- [ ] **Step 2: Run test — verify it FAILS**

Run: `cargo test -p uefi-engine hii::strings::tests::parse_scsu_strings_and_language`
Expected: FAIL (`panic: some package` — stub returns `None`).

- [ ] **Step 3: Implement `parse_string_package` + helpers**

Replace the stub body and add helpers so the full file is:

```rust
use std::collections::HashMap;

use crate::hii::string_pack::is_string_package;

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

const HEADER_LEN: usize = 4;
const STRING_INFO_OFFSET_POS: usize = 8;
const LANGUAGE_OFFSET: usize = 46;

pub struct ParsedStringPackage {
    pub language: String,
    pub strings: Vec<(u16, String)>,
}

pub fn parse_string_package(body: &[u8]) -> Option<ParsedStringPackage> {
    if !is_string_package(body) {
        return None;
    }
    let info_off = read_u32(body, STRING_INFO_OFFSET_POS)
        .filter(|&o| o as usize >= HEADER_LEN && o as usize <= body.len())
        .unwrap_or(HEADER_LEN as u32) as usize;
    let language = read_language(body, LANGUAGE_OFFSET, info_off);
    let mut strings: Vec<(u16, String)> = Vec::new();
    let mut by_id: HashMap<u16, String> = HashMap::new();
    let mut next_id: u16 = 1;
    let mut pos = info_off;
    while pos < body.len() {
        match body[pos] {
            SIBT_END => break,
            SIBT_STRING_SCSU => {
                let (text, p) = read_scsu(body, pos + 1);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRING_SCSU_FONT => {
                let (text, p) = read_scsu(body, pos + 2);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRINGS_SCSU => {
                let (count, mut p) = read_u16(body, pos + 1);
                for _ in 0..count {
                    let (text, np) = read_scsu(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_STRINGS_SCSU_FONT => {
                let (count, mut p) = read_u16(body, pos + 2);
                for _ in 0..count {
                    let (text, np) = read_scsu(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_STRING_UCS2 => {
                let (text, p) = read_ucs2(body, pos + 1);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRING_UCS2_FONT => {
                let (text, p) = read_ucs2(body, pos + 2);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRINGS_UCS2 => {
                let (count, mut p) = read_u16(body, pos + 1);
                for _ in 0..count {
                    let (text, np) = read_ucs2(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_STRINGS_UCS2_FONT => {
                let (count, mut p) = read_u16(body, pos + 2);
                for _ in 0..count {
                    let (text, np) = read_ucs2(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_DUPLICATE => {
                let (ref_id, _) = read_u16(body, pos + 1);
                let text = by_id.get(&ref_id).cloned().unwrap_or_default();
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos += 1 + 2;
            }
            SIBT_SKIP2 => {
                let (count, p) = read_u16(body, pos + 1);
                next_id = next_id.wrapping_add(count);
                pos = p;
            }
            SIBT_SKIP1 => {
                let count = body.get(pos + 1).copied().unwrap_or(0);
                next_id = next_id.wrapping_add(count as u16);
                pos += 2;
            }
            other => {
                tracing::warn!(opcode = other, "unknown SIBT opcode; stopping string parse");
                break;
            }
        }
    }
    Some(ParsedStringPackage { language, strings })
}

fn push(
    strings: &mut Vec<(u16, String)>,
    by_id: &mut HashMap<u16, String>,
    next_id: &mut u16,
    text: String,
) {
    by_id.insert(*next_id, text.clone());
    strings.push((*next_id, text));
    *next_id = next_id.wrapping_add(1);
}

fn read_u32(body: &[u8], pos: usize) -> Option<u32> {
    if pos + 4 > body.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        body[pos],
        body[pos + 1],
        body[pos + 2],
        body[pos + 3],
    ]))
}

fn read_u16(body: &[u8], pos: usize) -> (u16, usize) {
    if pos + 2 > body.len() {
        return (0, body.len());
    }
    (u16::from_le_bytes([body[pos], body[pos + 1]]), pos + 2)
}

fn read_language(body: &[u8], start: usize, end: usize) -> String {
    let stop = end.min(body.len());
    let mut s = String::new();
    let mut i = start;
    while i < stop && body[i] != 0 {
        s.push(body[i] as char);
        i += 1;
    }
    s
}

fn read_scsu(body: &[u8], start: usize) -> (String, usize) {
    let start = start.min(body.len());
    let mut i = start;
    while i < body.len() && body[i] != 0 {
        i += 1;
    }
    let text = String::from_utf8_lossy(&body[start..i]).into_owned();
    (text, if i < body.len() { i + 1 } else { body.len() })
}

fn read_ucs2(body: &[u8], start: usize) -> (String, usize) {
    let start = start.min(body.len());
    let mut i = start;
    while i + 1 < body.len() && !(body[i] == 0 && body[i + 1] == 0) {
        i += 2;
    }
    let units: Vec<u16> = body[start..i]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units);
    (text, if i + 1 < body.len() { i + 2 } else { body.len() })
}
```

(Keep the existing `#[cfg(test)] mod tests` from Step 1 at the bottom.)

- [ ] **Step 4: Run test — verify it PASSES**

Run: `cargo test -p uefi-engine hii::strings`
Expected: PASS (`parse_scsu_strings_and_language`).

- [ ] **Step 5: Add edge-case tests (TDD: write, run, confirm pass)**

Append to the `tests` module in `strings.rs`:

```rust
    #[test]
    fn parse_returns_none_for_non_string_package() {
        assert!(parse_string_package(&[0x00, 0x00, 0x00, 0x02]).is_none());
    }

    #[test]
    fn parse_ucs2_string() {
        let mut sibt = vec![SIBT_STRING_UCS2];
        for u in "Hi".encode_utf16() {
            sibt.extend_from_slice(&u.to_le_bytes());
        }
        sibt.push(0);
        sibt.push(0);
        sibt.push(SIBT_END);
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings[0], (1, "Hi".to_string()));
    }

    #[test]
    fn parse_skip_blocks_advance_ids_without_text() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_SKIP2, 0x02, 0x00,
            SIBT_STRING_SCSU, b'B', 0,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 2);
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (4, "B".to_string()));
    }

    #[test]
    fn parse_duplicate_copies_prior_text() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_DUPLICATE, 0x01, 0x00,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (2, "A".to_string()));
    }

    #[test]
    fn parse_truncated_language_falls_back_empty() {
        let mut buf = vec![0u8; 12];
        buf[3] = 0x04;
        buf.extend_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        buf.push(SIBT_END);
        let parsed = parse_string_package(&buf).unwrap();
        assert!(parsed.language.is_empty());
    }

    #[test]
    fn parse_truncated_font_opcode_does_not_panic() {
        let pkg = make_pkg("en", &[SIBT_STRING_SCSU_FONT]);
        let parsed = parse_string_package(&pkg).unwrap();
        assert!(parsed.strings.len() <= 1);
        if let Some((_, text)) = parsed.strings.first() {
            assert!(text.is_empty());
        }
    }
```

Run: `cargo test -p uefi-engine hii::strings`
Expected: all PASS. (The truncated-language test: StringInfoOffset = HEADER_LEN = 4, which is ≤ LANGUAGE_OFFSET=46, so `read_language` scans an empty range → `""`. `info_off` clamps to `HEADER_LEN`.)

- [ ] **Step 6: clippy + fmt + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
git add -A && git commit -m "feat(uefi-engine): hii::strings parse_string_package (SIBT reader + language)"
```

---

### Task 2: `collect_strings` driver + `HiiListStrings` RPC handler

**Files:**
- Modify: `crates/uefi-engine/src/hii/strings.rs` (add `collect_strings` + walk helper)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_list_strings` handler)

**Interfaces:**
- Produces: `pub fn collect_strings(image: &Image) -> Vec<uefi_proto::StringInfo>` (first string-package, primary language).

- [ ] **Step 1: Add the failing driver test**

Add to `strings.rs` test module (needs `Image`, `FfsNode`, etc.):

```rust
    use crate::types::{Action, FfsType, Image, ImageMode, FfsNode, ParsingData};

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None, node_type, subtype: 0, offset: 0, header: vec![],
            body, tail: vec![], children, action: Action::NoAction,
            parsing_data: ParsingData::None, fixed: false, compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn mk_image(pkg_body: Vec<u8>) -> Image {
        let section = mk_node(FfsType::Section, pkg_body, vec![]);
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read }
    }

    #[test]
    fn collect_strings_returns_first_package_strings() {
        let sibt = [SIBT_STRING_SCSU, b'X', 0, SIBT_END];
        let pkg = make_pkg("eng", &sibt);
        let image = mk_image(pkg);
        let out = collect_strings(&image);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].language, "eng");
        assert_eq!(out[0].string_id, 1);
        assert_eq!(out[0].text, "X");
    }

    #[test]
    fn collect_strings_empty_when_no_package() {
        let image = mk_image(vec![0x00, 0x00, 0x00, 0x02]);
        assert!(collect_strings(&image).is_empty());
    }
```

- [ ] **Step 2: Run — verify FAIL (collect_strings undefined)**

Run: `cargo test -p uefi-engine hii::strings::tests::collect_strings`
Expected: FAIL (`cannot find function collect_strings`).

- [ ] **Step 3: Implement `collect_strings` + walk helper**

Add to `strings.rs` (above the `#[cfg(test)]` block). Also add these imports at the top of the file (with the existing `use` lines):

```rust
use crate::types::{FfsNode, FfsType, Image};
use uefi_proto::StringInfo;
```

```rust
pub fn collect_strings(image: &Image) -> Vec<StringInfo> {
    let mut out: Vec<StringInfo> = Vec::new();
    walk_for_string_package(&image.root, &mut out);
    out
}

fn walk_for_string_package(node: &FfsNode, out: &mut Vec<StringInfo>) {
    if node.node_type == FfsType::Section && is_string_package(&node.body) {
        if let Some(pkg) = parse_string_package(&node.body) {
            for (sid, text) in pkg.strings {
                out.push(StringInfo {
                    language: pkg.language.clone(),
                    string_id: sid as u32,
                    text,
                });
            }
            return;
        }
    }
    for child in &node.children {
        walk_for_string_package(child, out);
        if !out.is_empty() {
            return;
        }
    }
}
```

The first `return` (after pushing one package's strings) enforces primary-language-only; the `if !out.is_empty()` check after recursing into children stops the DFS once any package has been collected.

- [ ] **Step 4: Run — verify PASS**

Run: `cargo test -p uefi-engine hii::strings`
Expected: all PASS.

- [ ] **Step 5: Wire the `HiiListStrings` RPC handler**

In `crates/uefi-engine/src/rpc/server.rs`, replace the `hii_list_strings` handler (currently returning `UNIMPLEMENTED`) with:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn hii_list_strings(
        &self,
        req: Request<HiiListStringsRequest>,
    ) -> RpcResult<HiiListStringsResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let strings = crate::hii::strings::collect_strings(&img);
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(HiiListStringsResponse { strings }))
    }
```

- [ ] **Step 6: Verify build + tests + clippy + fmt**

```bash
cargo test -p uefi-engine
cargo clippy -p uefi-engine -- -D warnings
cargo fmt -p uefi-engine -- --check
```
Expected: pass; `hii_list_strings` no longer returns UNIMPLEMENTED.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(uefi-engine): collect_strings + wire HiiListStrings RPC handler"
```

---

## Self-Review (completed by plan author)

**Spec coverage (string-reader scope):** ✅ `parse_string_package` (SIBT walker + language @offset46), ✅ `collect_strings` (primary-language, first package), ✅ `HiiListStrings` handler oживлен, ✅ pure-parser unit tests (synthetic), ✅ best-effort error handling (`tracing::warn` + break).

**Placeholder scan:** none. All code inlined and final.

**Type consistency:** `ParsedStringPackage.strings: Vec<(u16, String)>`; `collect_strings` maps to `StringInfo { language: String, string_id: u32, text: String }` (`u16`→`u32` widening matches proto field). `read_u16`/`read_u32`/`read_scsu`/`read_ucs2` signatures used consistently. SIBT constants mirror `string_pack.rs` (intentional reader/writer duplication of stable UEFI values).

**Ordering:** strings before forms (this plan) so Phase 4's `collect_forms` can call `collect_strings` for title resolution. Confirmed `uefi-engine` depends on `uefi-proto` (server.rs already imports it), so `StringInfo` is reachable from the hii module.
