# HII Phase 6 — PE-Resource Extraction (read-only) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Prerequisites:** Phases 1–5 complete: `hii/` module with readers (`collect_forms`/`collect_strings` work on bare `0x19` bodies), `find_item`/`find_item_path`/`find_item_mut` resolve `GuidSection` targets over direct file children, `set_item_visibility` mutates bare sections. Two real-image tests in `tests/real_image.rs` are RED (`real_image_hii_forms_and_strings`, `real_image_hii_form_visibility_round_trip`). Spec corrections committed as `d60dc82` (fixture attribution, third mutation gate, first-package semantics).

**Goal:** Make `collect_forms`/`collect_strings` see real-world HII stored as `L"HII"` PE resources and bare FORM packages in PE bodies (acceptance: the two HNX99TF `#[ignore]` tests go green without assert changes beyond the spec-§6 visibility amendment), with honest refusal for mutations behind the compression barrier.

**Architecture:** Two new read-only modules in `crates/uefi-engine/src/hii/`: `package_list.rs` (list splitter) and `pe_resource.rs` (`object`-based `.rsrc` walk + bare-Package scan with deep validation). The tree, builder, proto and CLI are untouched (lazy extraction at read time — spec approach 1). The forms/strings walkers are restructured to a per-file DFS that numbers sections through compression wrappers, mirrored exactly in `parser/target.rs`. `set_item_visibility` gains three gates (Write mode, compression barrier, bare-only targets).

**Tech Stack:** Rust (edition 2024), `object` 0.39 workspace dep (features `read_core, pe`), `r-efi` HII constants, existing `tracing` logging.

**Spec:** `docs/superpowers/specs/2026-08-14-hii-pe-resource-extraction-design.md` (as amended by `d60dc82`).

## Global Constraints

- **TDD order per task:** write failing test → run it (fails) → implement → run (passes) → commit. Module-first rule: `pub mod <name>;` goes into `hii/mod.rs` in the SAME step the module file is created, BEFORE `cargo test`.
- **No comments in code** (AGENTS.md); error/log message strings are fine.
- **Per-task verify:** `cargo test -p uefi-engine` + `cargo clippy -p uefi-engine -- -D warnings` + `cargo fmt -p uefi-engine -- --check`.
- **Extraction failures are not errors:** no `.rsrc`, non-PE, missing `'HII'` → empty return + `tracing::debug`; malformed package list → `None` + `tracing::warn` (spec §7).
- **Counter-mirror discipline (spec §4.6):** the section numbering in `hii/forms.rs` and `parser/target.rs` must stay identical — pre-order DFS over Section children, per-subtype counter, descend ONLY into Section children with subtype `0x01` (COMPRESSION) or `0x02` (GUID_DEFINED). Never descend into `0x17` FV-image sections. On wrapper-free images the numbering is byte-identical to phases 3–5 (all existing synthetic tests must stay green unamended, except the two amendments explicitly ordered by this plan).
- **Real-image tests are `#[ignore]`**, external file HNX99TF at `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, run via `UEFIPATCHER_TEST_FW=... cargo test -p uefi-engine --test real_image -- --ignored`.
- **Lenovo bytes never committed**; committed fixtures are extracted from the open-license edk2-rk3588 image only.

**Reference — verified live facts (probe 2026-08-14, `hack/hii_probe.py`):**

- 'HII' resource blobs are package lists: `16 B list GUID + u32 total + packages (u24 len + u8 type), terminator PACKAGE_END=0xDF`. `total` is a hint only (AMI sometimes `total == len-1`).
- rk3588 session image `/var/home/dsevosty/.local/share/uefipatcher/sessions/7a126462-3ca8-42c6-8835-9ccaec3a90b7/images/9aa679cd-cfa4-4f25-89c8-ae55fb0717d3.bin` has exactly ONE decompressible LZMA blob (16 056 464 B); inside it:
  - string resource blob `@0x598374`, 6 876 B — list `A487A478-51EF-48AA-8794-7BEE2A0562F1`, one STRING package 6 852 B (`HdrSize=52`, `StringInfoOffset=52`, language `en-US` at +46, first SIBT `0x14` UCS-2);
  - bare FORM package `@0x251350`, 1 424 B — formset `642237C7-35D4-472D-8365-12E0CCF27A22`, 21 forms, deep-validates with exact opcode consumption.
- HNX99TF: 6 lists / 4 formsets; primary STRING language `en-US`, **secondary language `x-UEFI-AMI`** (→ `collect_strings` must keep first-package-only semantics); every HII PE32 sits inside an LZMA-guided wrapper; the first HII-bearing file in tree order is the Setup file `899407D7-99FE-43D8-9A21-79EC328CAC21`.
- `object` 0.39.1 API (verified against upstream tag v0.39.1): `PeFile64::parse/PeFile32::parse` (inherent), `file.data()`, `file.section_table()`, `file.data_directories().resource_directory(data, &sections) -> Result<Option<ResourceDirectory>>`, `ResourceDirectory::root() -> Result<ResourceDirectoryTable>` with `entries: &[ImageResourceDirectoryEntry]`, `entry.name_or_id() -> ResourceNameOrId` (`.name() -> Option<ResourceName>`, `ResourceName::to_string_lossy(rsrc)`), `entry.data(rsrc) -> Result<ResourceDirectoryEntryData>` (`Table(ResourceDirectoryTable)` / `Data(&ImageResourceDataEntry)`), `ImageResourceDataEntry.offset_to_data/size: U32<LittleEndian>` (RVA + size, resolve via `SectionTable::pe_file_range_at(rva) -> Option<(file_off, avail)>`).

**Reference — current code:**

- `crates/uefi-engine/src/hii/ifr.rs` — `parse_form_package` walks `body.len()` (no declared-length bound, Task 3); test helpers `opcode/form_set/form/end/package` (copy them where needed).
- `crates/uefi-engine/src/hii/strings.rs` — `parse_string_package` (correct, `LANGUAGE_OFFSET=46`); `collect_strings` + `walk_for_string_package` (first package only); test helper `make_pkg(language, sibt_bytes)` builds the real header layout — reuse in Task 5.
- `crates/uefi-engine/src/hii/forms.rs` — `collect_forms` + `walk` with per-parent counters; test `LANGUAGE_OFFSET: usize = 12` fixture is the WRONG layout (Task 5 replaces it); `FILE_GUID = "899407d7-92a6-4174-968f-6f0b47f86a23"`, `FORMSET_GUID = "5C60F367-A505-419A-859E-2A4FF6CA6FE5"`.
- `crates/uefi-engine/src/parser/target.rs:63-84` — `find_item` GuidSection arm (direct children only); `:97-117` — `find_item_path` GuidSection arm (direct children only).
- `crates/uefi-engine/src/hii/mod.rs` — `HiiError` (10 variants), `set_item_visibility`.
- `crates/uefi-engine/src/ffs.rs:5-14` — `EFI_SECTION_COMPRESSION=0x01`, `EFI_SECTION_GUID_DEFINED=0x02`, `EFI_SECTION_PE32=0x10`, `EFI_SECTION_RAW=0x19`.
- `crates/uefi-engine/Cargo.toml` — workspace `object = { version = "0.39", default-features = false, features = ["read_core", "pe"] }` declared at root `Cargo.toml:22`; not yet a dependency of any crate.
- `crates/uefi-engine/tests/real_image.rs:616-682` — the two HII tests (Task 7 amends the visibility one, Task 8 cosmetics).
- `hack/hii_probe.py` — importable as a module (`LZMA_GUIDS`, `u16`, `guid_str`); top-level loop is a no-op when `sys.argv[1:]` is empty.

---

### Task 0: Recon — extract rk3588 fixtures, add `object` dependency

**Files:**
- Create: `crates/uefi-engine/tests/fixtures/hii_rk3588_string_res.bin` (6 876 B)
- Create: `crates/uefi-engine/tests/fixtures/hii_rk3588_bare_form.bin` (1 424 B)
- Modify: `crates/uefi-engine/Cargo.toml`
- Modify: `Cargo.lock` (regenerated by cargo)

**Interfaces:**
- Produces: fixture files referenced by Tasks 1–5 tests via `include_bytes!`; `object` available to `uefi-engine` for Task 2.

- [ ] **Step 1: Extract the two fixtures from the rk3588 session image**

Run from repo root (script reuses `hack/hii_probe.py` helpers; asserts make it fail loudly if the image or offsets drift):

```bash
python3 - <<'EOF'
import sys, lzma
sys.path.insert(0, 'hack')
import hii_probe as hp

IMG = '/var/home/dsevosty/.local/share/uefipatcher/sessions/7a126462-3ca8-42c6-8835-9ccaec3a90b7/images/9aa679cd-cfa4-4f25-89c8-ae55fb0717d3.bin'
data = open(IMG, 'rb').read()
best = b''
for g in hp.LZMA_GUIDS:
    i = data.find(g)
    while i >= 0:
        doff = hp.u16(data, i + 16)
        try:
            out = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(data[i - 4 + doff:])
        except Exception:
            out = b''
        if len(out) > len(best):
            best = out
        i = data.find(g, i + 1)
assert len(best) == 16_056_464, f'unexpected decompressed size {len(best)}'

string_res = best[0x598374:0x598374 + 6876]
assert hp.guid_str(string_res[:16]) == 'A487A478-51EF-48AA-8794-7BEE2A0562F1'
assert string_res[23] == 0x04
open('crates/uefi-engine/tests/fixtures/hii_rk3588_string_res.bin', 'wb').write(string_res)

bare = best[0x251350:0x251350 + 1424]
assert bare[3] == 0x02 and bare[4] == 0x0E
assert hp.guid_str(bare[6:22]) == '642237C7-35D4-472D-8365-12E0CCF27A22'
open('crates/uefi-engine/tests/fixtures/hii_rk3588_bare_form.bin', 'wb').write(bare)
print('fixtures written: 6876 + 1424 bytes')
EOF
```

Expected: `fixtures written: 6876 + 1424 bytes`.

- [ ] **Step 2: Add `object` to uefi-engine dependencies**

In `crates/uefi-engine/Cargo.toml`, after the `binrw.workspace = true` line add:

```toml
object.workspace = true
```

- [ ] **Step 3: Verify the dependency resolves**

Run: `cargo check -p uefi-engine`
Expected: compiles; `Cargo.lock` gains the `object` entry. Then run `cargo test -p uefi-engine` — all green (no code changed).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/fixtures/hii_rk3588_string_res.bin \
        crates/uefi-engine/tests/fixtures/hii_rk3588_bare_form.bin \
        crates/uefi-engine/Cargo.toml Cargo.lock
git commit -m "test(uefi-engine): add rk3588 HII fixtures (string resource blob + bare form package), pull object dep"
```

---

### Task 1: `hii/package_list.rs` — package-list splitter

**Files:**
- Create: `crates/uefi-engine/src/hii/package_list.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (module declaration — same step)

**Interfaces:**
- Produces (used by Tasks 2, 4, 5):

```rust
pub struct HiiPackage<'a> { pub kind: u8, pub bytes: &'a [u8] }        // bytes = whole package incl. u24+type header
pub struct HiiPackageList<'a> { pub guid: Guid, pub packages: Vec<HiiPackage<'a>> }
pub fn parse_package_list(bytes: &[u8]) -> Option<HiiPackageList<'_>>
```

- [ ] **Step 1: Create the module with failing tests + declare the module**

Create `crates/uefi-engine/src/hii/package_list.rs` containing ONLY the test module:

```rust
#[cfg(test)]
use r_efi::hii::{PACKAGE_FORMS, PACKAGE_STRINGS};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Guid;
    use std::str::FromStr;

    const LIST_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";

    fn pkg(kind: u8, payload: &[u8]) -> Vec<u8> {
        let len = 4 + payload.len();
        let mut b = vec![(len & 0xFF) as u8, ((len >> 8) & 0xFF) as u8, ((len >> 16) & 0xFF) as u8, kind];
        b.extend_from_slice(payload);
        b
    }

    fn list(guid: &Guid, total: u32, pkgs: &[&[u8]]) -> Vec<u8> {
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&total.to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, PACKAGE_END]);
        b
    }

    #[test]
    fn parses_form_and_string_packages() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = pkg(PACKAGE_STRINGS, &[0x00; 8]);
        let bytes = list(&g, 0, &[&form, &string]);
        let parsed = parse_package_list(&bytes).unwrap();
        assert_eq!(parsed.guid, g);
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].kind, PACKAGE_FORMS);
        assert_eq!(parsed.packages[0].bytes, &form[..]);
        assert_eq!(parsed.packages[1].kind, PACKAGE_STRINGS);
    }

    #[test]
    fn empty_list_with_only_end_terminator_is_valid() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let bytes = list(&g, 20, &[]);
        let parsed = parse_package_list(&bytes).unwrap();
        assert!(parsed.packages.is_empty());
    }

    #[test]
    fn total_is_a_hint_and_not_validated() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xBB]);
        let total = (20 + form.len() + 4) as u32 - 1;
        let bytes = list(&g, total, &[&form]);
        assert!(parse_package_list(&bytes).is_some());
    }

    #[test]
    fn truncated_mid_package_returns_none() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22]);
        let mut bytes = list(&g, 0, &[&form]);
        bytes.truncate(bytes.len() - 10);
        assert!(parse_package_list(&bytes).is_none());
    }

    #[test]
    fn package_length_below_header_returns_none() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let mut bytes = g.to_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0x02, 0x00, 0x00, PACKAGE_FORMS]);
        assert!(parse_package_list(&bytes).is_none());
    }

    #[test]
    fn missing_end_terminator_returns_none() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let mut bytes = g.to_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xDD]));
        assert!(parse_package_list(&bytes).is_none());
    }

    #[test]
    fn short_input_returns_none() {
        assert!(parse_package_list(&[0u8; 19]).is_none());
    }

    #[test]
    fn parses_real_rk3588_string_resource() {
        let bytes = include_bytes!("../../tests/fixtures/hii_rk3588_string_res.bin");
        let parsed = parse_package_list(bytes).unwrap();
        assert_eq!(
            parsed.guid,
            Guid::from_str("A487A478-51EF-48AA-8794-7BEE2A0562F1").unwrap()
        );
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].kind, PACKAGE_STRINGS);
        let len = parsed.packages[0].bytes[0] as usize
            | (parsed.packages[0].bytes[1] as usize) << 8
            | (parsed.packages[0].bytes[2] as usize) << 16;
        assert_eq!(len, 6852);
    }
}
```

And in `crates/uefi-engine/src/hii/mod.rs` add `pub mod package_list;` to the module list (same step — module-first rule).

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p uefi-engine package_list`
Expected: compile error (`parse_package_list` etc. not defined).

- [ ] **Step 3: Implement**

Add above the test module in `package_list.rs`:

```rust
use crate::types::Guid;
use r_efi::hii::PACKAGE_END;

pub struct HiiPackage<'a> {
    pub kind: u8,
    pub bytes: &'a [u8],
}

pub struct HiiPackageList<'a> {
    pub guid: Guid,
    pub packages: Vec<HiiPackage<'a>>,
}

pub fn parse_package_list(bytes: &[u8]) -> Option<HiiPackageList<'_>> {
    if bytes.len() < 20 {
        tracing::debug!("HII package list shorter than 20-byte header");
        return None;
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes[0..16]);
    let guid = Guid::from_bytes(arr);
    let mut packages = Vec::new();
    let mut pos = 20usize;
    while pos + 4 <= bytes.len() {
        let plen =
            bytes[pos] as usize | (bytes[pos + 1] as usize) << 8 | (bytes[pos + 2] as usize) << 16;
        let kind = bytes[pos + 3];
        if kind == PACKAGE_END {
            return Some(HiiPackageList { guid, packages });
        }
        if plen < 4 || pos + plen > bytes.len() {
            tracing::warn!(pos, plen, kind, "malformed HII package; dropping whole list");
            return None;
        }
        packages.push(HiiPackage {
            kind,
            bytes: &bytes[pos..pos + plen],
        });
        pos += plen;
    }
    tracing::warn!("HII package list without END terminator; dropping whole list");
    None
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p uefi-engine package_list`
Expected: 8 passed.

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/hii/package_list.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii package-list splitter (parse_package_list)"
```

---

### Task 2: `hii/pe_resource.rs` — `.rsrc` `L"HII"` extraction via `object`

**Files:**
- Create: `crates/uefi-engine/src/hii/pe_resource.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (module declaration — same step)

**Interfaces:**
- Consumes: `object` dep (Task 0).
- Produces (used by Tasks 4, 5):

```rust
pub fn hii_resource_blobs(pe: &[u8]) -> Vec<&[u8]>            // leaf data slices of 'HII'-typed resources
pub fn hii_resource_ranges(pe: &[u8]) -> Vec<(usize, usize)>  // same leaves as (file_offset, len) within pe
```

- [ ] **Step 1: Create the module with failing tests + declare the module**

Create `crates/uefi-engine/src/hii/pe_resource.rs` containing ONLY the test module. The synthetic PE builder writes a minimal PE32+ with one `.rsrc` section, a type directory entry named `"HII"`, and the given blob as leaf data:

```rust
#[cfg(test)]
fn rsrc_dir_header(named_entries: u16, id_entries: u16) -> [u8; 16] {
    let mut header = [0u8; 16];
    header[12..14].copy_from_slice(&named_entries.to_le_bytes());
    header[14..16].copy_from_slice(&id_entries.to_le_bytes());
    header
}

#[cfg(test)]
pub(crate) fn synth_hii_pe(type_name: &str, blob: &[u8]) -> Vec<u8> {
    let rsrc_rva: u32 = 0x1000;
    let mut rsrc = Vec::new();
    rsrc.extend_from_slice(&rsrc_dir_header(1, 0));
    rsrc.extend_from_slice(&0x8000_0048u32.to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0018u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 1));
    rsrc.extend_from_slice(&1u32.to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0030u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 1));
    rsrc.extend_from_slice(&0x409u32.to_le_bytes());
    rsrc.extend_from_slice(&0x80u32.to_le_bytes());
    rsrc.extend_from_slice(&(type_name.len() as u16).to_le_bytes());
    for u in type_name.encode_utf16() {
        rsrc.extend_from_slice(&u.to_le_bytes());
    }
    while rsrc.len() < 0x80 {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(&(rsrc_rva + 0x90).to_le_bytes());
    rsrc.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    rsrc.extend_from_slice(&0u32.to_le_bytes());
    rsrc.extend_from_slice(&0u32.to_le_bytes());
    while rsrc.len() < 0x90 {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(blob);

    let mut pe = vec![0u8; 0x170];
    pe[0] = b'M';
    pe[1] = b'Z';
    pe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    pe[0x40..0x44].copy_from_slice(b"PE\0\0");
    pe[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
    pe[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
    pe[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
    pe[0x58..0x5a].copy_from_slice(&0x20bu16.to_le_bytes());
    pe[0xc4..0xc8].copy_from_slice(&16u32.to_le_bytes());
    pe[0xd8..0xdc].copy_from_slice(&rsrc_rva.to_le_bytes());
    pe[0xdc..0xe0].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x148..0x14f].copy_from_slice(b".rsrc\0\0");
    pe[0x150..0x154].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x154..0x158].copy_from_slice(&rsrc_rva.to_le_bytes());
    pe[0x158..0x15c].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x15c..0x160].copy_from_slice(&0x170u32.to_le_bytes());
    pe[0x16c..0x170].copy_from_slice(&0x4000_0040u32.to_le_bytes());
    pe.extend_from_slice(&rsrc);
    pe
}

#[cfg(test)]
mod tests {
    use super::*;

    const RK3588_STRING_RES: &[u8] =
        include_bytes!("../../tests/fixtures/hii_rk3588_string_res.bin");

    #[test]
    fn extracts_hii_blob_from_synthetic_pe() {
        let pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let blobs = hii_resource_blobs(&pe);
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0], RK3588_STRING_RES);
    }

    #[test]
    fn ranges_point_at_blob_bytes() {
        let pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let ranges = hii_resource_ranges(&pe);
        assert_eq!(ranges.len(), 1);
        let (off, len) = ranges[0];
        assert_eq!(&pe[off..off + len], RK3588_STRING_RES);
    }

    #[test]
    fn non_pe_input_returns_empty() {
        assert!(hii_resource_blobs(&[]).is_empty());
        assert!(hii_resource_blobs(b"MZnotape").is_empty());
        assert!(hii_resource_blobs(&[0u8; 64]).is_empty());
    }

    #[test]
    fn pe_without_hii_type_returns_empty() {
        let pe = synth_hii_pe("REGISTRY", RK3588_STRING_RES);
        assert!(hii_resource_blobs(&pe).is_empty());
        assert!(hii_resource_ranges(&pe).is_empty());
    }

    #[test]
    fn pe_without_resource_directory_returns_empty() {
        let mut pe = synth_hii_pe("HII", RK3588_STRING_RES);
        pe[0xd8..0xdc].copy_from_slice(&0u32.to_le_bytes());
        assert!(hii_resource_blobs(&pe).is_empty());
    }
}
```

And add `pub mod pe_resource;` to `crates/uefi-engine/src/hii/mod.rs` (same step).

Note the layout the builder encodes (needed if a test fails): DOS header 0x40; PE sig @0x40; COFF @0x44 (machine 0x8664, 1 section, optsz 240); PE32+ optional header @0x58 (magic 0x20b, NumberOfRvaAndSizes=16 @0xc4, data dir[2]=RESOURCE @0xd8); section table @0x148 (`.rsrc`, vsize=vaddr=raw size of `.rsrc`, vaddr 0x1000, raw ptr 0x170, chars 0x40000040); `.rsrc`: root dir @0 with one named entry (name string @0x48 → `type_name`, ≤20 chars) → table @0x18 with one id entry → table @0x30 with one id entry → data entry @0x80 pointing at blob @0x90.

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p uefi-engine pe_resource`
Expected: compile error (`hii_resource_blobs`/`hii_resource_ranges` not defined).

- [ ] **Step 3: Implement**

Add above the test code in `pe_resource.rs`:

```rust
use object::endian::LittleEndian;
use object::read::pe::{ImageNtHeaders, PeFile, PeFile32, PeFile64, ResourceDirectoryEntryData};

pub fn hii_resource_blobs(pe: &[u8]) -> Vec<&[u8]> {
    hii_resource_ranges(pe)
        .into_iter()
        .filter_map(|(off, len)| pe.get(off..off + len))
        .collect()
}

pub fn hii_resource_ranges(pe: &[u8]) -> Vec<(usize, usize)> {
    if let Ok(file) = PeFile64::parse(pe) {
        return hii_entries(&file);
    }
    if let Ok(file) = PeFile32::parse(pe) {
        return hii_entries(&file);
    }
    tracing::debug!("not a PE image; no HII resources");
    Vec::new()
}

fn hii_entries<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>) -> Vec<(usize, usize)> {
    let sections = file.section_table();
    let Ok(Some(rsrc)) = file.data_directories().resource_directory(file.data(), &sections)
    else {
        tracing::debug!("no .rsrc directory; no HII resources");
        return Vec::new();
    };
    let Ok(root) = rsrc.root() else {
        tracing::debug!("invalid .rsrc root; no HII resources");
        return Vec::new();
    };
    let mut out = Vec::new();
    for type_entry in root.entries {
        let Some(name) = type_entry.name_or_id().name() else {
            continue;
        };
        let Ok(type_name) = name.to_string_lossy(rsrc) else {
            continue;
        };
        if type_name != "HII" {
            continue;
        }
        let Ok(ResourceDirectoryEntryData::Table(name_table)) = type_entry.data(rsrc) else {
            continue;
        };
        for name_entry in name_table.entries {
            let Ok(ResourceDirectoryEntryData::Table(lang_table)) = name_entry.data(rsrc) else {
                continue;
            };
            for lang_entry in lang_table.entries {
                let Ok(ResourceDirectoryEntryData::Data(d)) = lang_entry.data(rsrc) else {
                    continue;
                };
                push_leaf(file, d.offset_to_data.get(LittleEndian), d.size.get(LittleEndian), &mut out);
            }
        }
    }
    out
}

fn push_leaf<Pe: ImageNtHeaders>(
    file: &PeFile<'_, Pe>,
    rva: u32,
    size: u32,
    out: &mut Vec<(usize, usize)>,
) {
    let Some((off, avail)) = file.section_table().pe_file_range_at(rva) else {
        tracing::debug!(rva, "resource RVA outside sections; skipped");
        return;
    };
    let len = (size as usize).min(avail as usize);
    out.push((off as usize, len));
}
```

If the `object` 0.39.1 API differs in practice (spec §10 risk 2), fall back to a raw walk of `object::pe::ImageResourceDirectory*` — resolve via `SectionTable::pe_data_at` — keeping the public signatures and tests identical; a deviation beyond this requires a separate plan-fix commit (AGENTS.md rule 11).

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p uefi-engine pe_resource`
Expected: 5 passed.

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/hii/pe_resource.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): PE .rsrc 'HII' resource extraction via object"
```

---

### Task 3: IFR walker — declared-length bound

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs:74-127` (`parse_form_package`)

**Interfaces:**
- Produces: unchanged signature `pub fn parse_form_package(body: &[u8]) -> Option<FormSetInfo>`; now consumes opcodes only within the declared u24 package length, ignores tail bytes past it, returns `None` when the walk breaks inside the declared range. Makes the bare channel (Task 4) safe (spec §4.4, closes the deferred phase-4 TODO).

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `ifr.rs`:

```rust
    #[test]
    fn parse_ignores_tail_beyond_declared_length() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        let declared = pkg.len();
        pkg.extend(vec![0xAA; 16]);
        let fs = parse_form_package(&pkg).unwrap();
        assert_eq!(fs.guid, g);
        assert_eq!(fs.forms.len(), 1);
        assert_eq!(declared, pkg.len() - 16);
    }

    #[test]
    fn parse_returns_none_when_declared_length_cuts_mid_opcode() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        let shrinked = (pkg.len() - 5) as u32;
        pkg[0] = (shrinked & 0xFF) as u8;
        pkg[1] = ((shrinked >> 8) & 0xFF) as u8;
        pkg[2] = ((shrinked >> 16) & 0xFF) as u8;
        assert!(parse_form_package(&pkg).is_none());
    }
```

- [ ] **Step 2: Run tests, verify the new one fails**

Run: `cargo test -p uefi-engine ifr`
Expected: both new tests FAIL — `parse_ignores_tail_beyond_declared_length` (current walker reads into the `0xAA` tail and returns `None`) and `parse_returns_none_when_declared_length_cuts_mid_opcode` (current walker ignores the declared length, walks to body end and returns `Some`); all pre-existing ifr tests PASS.

**Why shrink by 5 (not 3):** with `pkg.len() = 37`, a `- 3` shrink puts plen=34, cutting on byte 2 of the trailing `end` opcode — the loop-entry guard `i + 2 <= end` exits cleanly and the walker returns `Some`, so the inner `i + length > end` None-path is unreachable. With `- 5`, plen=32 and the `form` opcode at offset 27 (length 6) straddles the bound (27+6=33 > 32), firing the inner check → `None` — the None-path this test exists to cover.

- [ ] **Step 3: Implement**

In `parse_form_package`, after the `is_form_package` check add the bound, and use it in the loop (three changed lines):

```rust
    let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
    let end = plen.min(body.len());
```

then replace `while i + 2 <= body.len() {` with `while i + 2 <= end {`, and the bound check `if length < 2 || i + length > body.len() {` with `if length < 2 || i + length > end {`. Nothing else changes (all slice reads stay `< end <= body.len()`).

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p uefi-engine ifr`
Expected: all pass (7 form-package tests incl. the pre-existing `parse_returns_none_on_truncation`).

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/hii/ifr.rs
git commit -m "fix(uefi-engine): bound ifr walker to declared package length"
```

---

### Task 4: bare FORM-package channel (scan + deep validation + dedup)

**Files:**
- Modify: `crates/uefi-engine/src/hii/pe_resource.rs` (add `bare_form_packages`)

**Interfaces:**
- Consumes: `parse_form_package` with the length bound (Task 3), `hii_resource_ranges` (Task 2).
- Produces (used by Task 5):

```rust
pub fn bare_form_packages<'a>(pe: &'a [u8], exclude: &[(usize, usize)]) -> Vec<&'a [u8]>
```

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `pe_resource.rs`:

```rust
    const RK3588_BARE_FORM: &[u8] =
        include_bytes!("../../tests/fixtures/hii_rk3588_bare_form.bin");

    #[test]
    fn bare_scan_finds_valid_form_package_in_pe_body() {
        let mut body = vec![0x11u8; 64];
        body.extend_from_slice(RK3588_BARE_FORM);
        body.extend_from_slice(vec![0x22; 32]);
        let found = bare_form_packages(&body, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], RK3588_BARE_FORM);
    }

    #[test]
    fn bare_scan_rejects_shallow_false_positive() {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&[27u8, 0, 0, 0x02, 0x0E]);
        body.extend_from_slice(vec![0u8; 22]);
        assert!(bare_form_packages(&body, &[]).is_empty());
    }

    #[test]
    fn bare_scan_drops_candidates_covered_by_resource_ranges() {
        let pe = synth_hii_pe("HII", RK3588_BARE_FORM);
        let exclude = hii_resource_ranges(&pe);
        assert!(!exclude.is_empty());
        let found = bare_form_packages(&pe, &exclude);
        assert!(
            !found.iter().any(|p| *p == RK3588_BARE_FORM),
            "resource-covered copy must be deduplicated"
        );
    }

    #[test]
    fn bare_scan_skips_past_accepted_or_covered_pattern() {
        let mut body = vec![0u8; 8];
        body.extend_from_slice(RK3588_BARE_FORM);
        body.extend_from_slice(vec![0x33; 4]);
        assert_eq!(bare_form_packages(&body, &[(0, body.len())]).len(), 0);
        assert_eq!(bare_form_packages(&body, &[]).len(), 1);
    }
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p uefi-engine pe_resource`
Expected: compile error (`bare_form_packages` not defined).

- [ ] **Step 3: Implement**

Add to `pe_resource.rs` (implementation section):

```rust
use r_efi::hii::{IFR_FORM_SET_OP, PACKAGE_FORMS};

use crate::hii::ifr::parse_form_package;

pub fn bare_form_packages<'a>(pe: &'a [u8], exclude: &[(usize, usize)]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 5 <= pe.len() {
        let plen =
            pe[pos] as usize | (pe[pos + 1] as usize) << 8 | (pe[pos + 2] as usize) << 16;
        if pe[pos + 3] == PACKAGE_FORMS
            && pe[pos + 4] == IFR_FORM_SET_OP
            && plen >= 24
            && pos + plen <= pe.len()
        {
            let covered = exclude.iter().any(|&(o, l)| o <= pos && pos < o + l);
            if !covered && parse_form_package(&pe[pos..pos + plen]).is_some() {
                out.push(&pe[pos..pos + plen]);
            }
            pos += plen;
        } else {
            pos += 1;
        }
    }
    out
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p uefi-engine pe_resource`
Expected: all pass (5 from Task 2 + 4 new).

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/hii/pe_resource.rs
git commit -m "feat(uefi-engine): bare FORM-package scan with deep validation and range dedup"
```

---

### Task 5: Walk restructure — resource/bare channels wired, per-file titles, real-layout fixtures

**Files:**
- Modify: `crates/uefi-engine/src/hii/forms.rs` (full restructure of `collect_forms`/`walk`, fixture alignment)
- Modify: `crates/uefi-engine/src/hii/strings.rs` (`collect_strings` resource channel + bare-path declared-length sanity)

**Interfaces:**
- Consumes: `parse_package_list` (Task 1), `hii_resource_blobs`/`hii_resource_ranges`/`bare_form_packages` (Tasks 2/4), `synth_hii_pe` test helper (Task 2).
- Produces: `collect_forms(image) -> Vec<FormInfo>` and `collect_strings(image) -> Vec<StringInfo>` with UNCHANGED signatures and proto shapes; `form_id` targets now also point at PE32 sections (`{file_guid}:0x10:{idx}`). Section numbering = pre-order DFS per file through `0x01`/`0x02` wrappers (spec §4.6) — must match Task 6's mirror.

- [ ] **Step 1: Write the failing tests**

In `forms.rs` tests: replace the wrong-layout `string_pkg()` builder (language at offset 12) with the real-layout one (copy of `make_pkg` semantics from `strings.rs`, language at +46):

```rust
    fn string_pkg() -> Vec<u8> {
        let lang = "eng";
        let hdr_size: u32 = (46 + lang.len() + 1) as u32;
        let mut b = vec![0u8; 3];
        b.push(0x04);
        b.extend_from_slice(&hdr_size.to_le_bytes());
        b.extend_from_slice(&hdr_size.to_le_bytes());
        while b.len() < 44 {
            b.push(0);
        }
        b.extend_from_slice(&0u16.to_le_bytes());
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
```

(delete the `LANGUAGE_OFFSET` const — nothing else uses it), and append the new tests:

```rust
    fn hii_list(form: &[u8], string: &[u8]) -> Vec<u8> {
        let g = Guid::from_str(FILE_GUID).unwrap();
        let mut b = g.to_bytes().to_vec();
        let total = 20 + form.len() + string.len() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(form);
        b.extend_from_slice(string);
        b.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        b
    }

    #[test]
    fn collect_forms_titles_are_scoped_to_file() {
        let mk_file = |guid: &str, text: &[u8]| {
            let str_sec = mk_node(None, FfsType::Section, 0x19, string_pkg_with(text), vec![]);
            let form_sec = mk_node(None, FfsType::Section, 0x19, form_pkg(1), vec![]);
            mk_node(
                Some(Guid::from_str(guid).unwrap()),
                FfsType::File,
                0x07,
                vec![],
                vec![str_sec, form_sec],
            )
        };
        let f1 = mk_file(FILE_GUID, b"MainA");
        let f2 = mk_file("899407d7-92a6-4174-968f-6f0b47f86a99", b"MainB");
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![f1, f2]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let forms = collect_forms(&image);
        assert_eq!(forms.len(), 4);
        let main_a = forms.iter().find(|f| f.title == "MainA").unwrap();
        let main_b = forms.iter().find(|f| f.title == "MainB").unwrap();
        assert!(main_a.form_id.starts_with(FILE_GUID));
        assert!(main_b.form_id.starts_with("899407d7-92a6-4174-968f-6f0b47f86a99"));
    }

    fn string_pkg_with(text: &[u8]) -> Vec<u8> {
        let lang = "eng";
        let hdr_size: u32 = (46 + lang.len() + 1) as u32;
        let mut b = vec![0u8; 3];
        b.push(0x04);
        b.extend_from_slice(&hdr_size.to_le_bytes());
        b.extend_from_slice(&hdr_size.to_le_bytes());
        while b.len() < 44 {
            b.push(0);
        }
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(lang.as_bytes());
        b.push(0);
        b.push(SIBT_STRING_SCSU);
        b.extend_from_slice(text);
        b.push(0);
        b.push(SIBT_END);
        let len = b.len() as u32;
        b[0] = (len & 0xFF) as u8;
        b[1] = ((len >> 8) & 0xFF) as u8;
        b[2] = ((len >> 16) & 0xFF) as u8;
        b
    }

    #[test]
    fn collect_forms_sees_forms_inside_pe_resources() {
        let list = hii_list(&form_pkg(1), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let pe_sec = mk_node(None, FfsType::Section, 0x10, pe, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![pe_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let forms = collect_forms(&image);
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x10:0"));
        assert_eq!(forms[0].title, "Main");
        assert_eq!(forms[0].formset_guid, FORMSET_GUID);
    }

    #[test]
    fn collect_forms_sees_bare_form_package_in_pe_body() {
        let mut body = vec![0x44u8; 16];
        body.extend_from_slice(include_bytes!(
            "../../tests/fixtures/hii_rk3588_bare_form.bin"
        ));
        let pe_sec = mk_node(None, FfsType::Section, 0x10, body, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![pe_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let forms = collect_forms(&image);
        assert!(!forms.is_empty());
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x10:0"));
        assert_eq!(
            forms[0].formset_guid,
            "642237C7-35D4-472D-8365-12E0CCF27A22"
        );
    }

    #[test]
    fn collect_forms_numbers_sections_through_wrappers() {
        let list = hii_list(&form_pkg(1), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let inner = mk_node(None, FfsType::Section, 0x10, pe, vec![]);
        let wrapper = mk_node(None, FfsType::Section, 0x02, vec![], vec![inner]);
        let direct = mk_node(None, FfsType::Section, 0x10, vec![0x55; 8], vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![wrapper, direct],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let forms = collect_forms(&image);
        assert!(!forms.is_empty());
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x10:0"));
    }
```

In `strings.rs` tests append:

```rust
    #[test]
    fn collect_strings_from_pe_resources() {
        let blob = include_bytes!("../../tests/fixtures/hii_rk3588_string_res.bin");
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", blob);
        let mut section = mk_node(FfsType::Section, pe, vec![]);
        section.subtype = crate::ffs::EFI_SECTION_PE32;
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let out = collect_strings(&image);
        assert!(!out.is_empty());
        assert_eq!(out[0].language, "en-US");
    }

    #[test]
    fn collect_strings_ignores_bare_body_with_bogus_declared_length() {
        let mut image = mk_image(vec![0x09, 0x00, 0x00, 0x04]);
        image.root.children[0].children[0].children[0].subtype = crate::ffs::EFI_SECTION_RAW;
        assert!(collect_strings(&image).is_empty());
    }
```

- [ ] **Step 2: Run tests, verify new ones fail**

Run: `cargo test -p uefi-engine hii::`
Expected: the 6 new tests FAIL (resource/bare channels absent; per-file titles absent); pre-existing forms/strings tests PASS except those that now reference `string_pkg_with`/helpers not yet defined (compile errors count as failing).

- [ ] **Step 3: Implement `forms.rs` restructure**

Replace the whole implementation section of `forms.rs` (keep the test module) with:

```rust
use std::collections::HashMap;

use r_efi::hii::{PACKAGE_FORMS, PACKAGE_STRINGS};
use uefi_proto::FormInfo;

use crate::ffs::{EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW};
use crate::hii::ifr::{parse_form_package, FormSetInfo};
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::{bare_form_packages, hii_resource_blobs, hii_resource_ranges};
use crate::hii::strings::{declared_len_sane, parse_string_package};
use crate::types::{FfsNode, FfsType, Guid, Image, guid_to_upper_string};

pub fn collect_forms(image: &Image) -> Vec<FormInfo> {
    let mut out = Vec::new();
    walk_files(&image.root, &mut out);
    out
}

fn walk_files(node: &FfsNode, out: &mut Vec<FormInfo>) {
    for child in &node.children {
        if child.node_type == FfsType::File {
            collect_file_forms(child, out);
        }
        walk_files(child, out);
    }
}

fn collect_file_forms(file: &FfsNode, out: &mut Vec<FormInfo>) {
    let Some(fg) = file.guid else {
        return;
    };
    let mut titles: HashMap<u16, String> = HashMap::new();
    let mut found: Vec<(String, FormSetInfo)> = Vec::new();
    let mut counters: HashMap<u8, usize> = HashMap::new();
    walk_sections(file, fg, &mut titles, &mut found, &mut counters);
    for (target, fs) in found {
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
}

fn walk_sections(
    node: &FfsNode,
    fg: Guid,
    titles: &mut HashMap<u16, String>,
    found: &mut Vec<(String, FormSetInfo)>,
    counters: &mut HashMap<u8, usize>,
) {
    for child in &node.children {
        if child.node_type != FfsType::Section {
            continue;
        }
        let idx = *counters.entry(child.subtype).or_insert(0);
        counters.insert(child.subtype, idx + 1);
        let target = format!("{}:{:#04x}:{}", fg, child.subtype, idx);
        if child.subtype == EFI_SECTION_RAW {
            if declared_len_sane(&child.body)
                && let Some(pkg) = parse_string_package(&child.body)
            {
                for (sid, text) in pkg.strings {
                    titles.entry(sid).or_insert(text);
                }
            } else if let Some(fs) = parse_form_package(&child.body) {
                found.push((target, fs));
            }
        } else if child.subtype == EFI_SECTION_PE32 {
            let ranges = hii_resource_ranges(&child.body);
            for blob in hii_resource_blobs(&child.body) {
                let Some(list) = parse_package_list(blob) else {
                    continue;
                };
                for pkg in &list.packages {
                    match pkg.kind {
                        PACKAGE_FORMS => {
                            if let Some(fs) = parse_form_package(pkg.bytes) {
                                found.push((target.clone(), fs));
                            }
                        }
                        PACKAGE_STRINGS => {
                            if let Some(sp) = parse_string_package(pkg.bytes) {
                                for (sid, text) in sp.strings {
                                    titles.entry(sid).or_insert(text);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            for pkg in bare_form_packages(&child.body, &ranges) {
                if let Some(fs) = parse_form_package(pkg) {
                    found.push((target.clone(), fs));
                }
            }
        }
        if child.subtype == EFI_SECTION_COMPRESSION || child.subtype == EFI_SECTION_GUID_DEFINED {
            walk_sections(child, fg, titles, found, counters);
        }
    }
}
```

Update the test-module imports if the compiler asks (e.g. `use r_efi::hii::PACKAGE_END;` for `hii_list`).

- [ ] **Step 4: Implement `strings.rs` channel**

In `strings.rs`: add imports

```rust
use crate::ffs::EFI_SECTION_PE32;
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::hii_resource_blobs;
use r_efi::hii::PACKAGE_STRINGS;
```

replace `walk_for_string_package` with:

```rust
fn walk_for_string_package(node: &FfsNode, out: &mut Vec<StringInfo>) {
    if node.node_type == FfsType::Section {
        if declared_len_sane(&node.body)
            && is_string_package(&node.body)
            && let Some(pkg) = parse_string_package(&node.body)
        {
            push_strings(pkg, out);
            return;
        }
        if node.subtype == EFI_SECTION_PE32
            && let Some(pkg) = first_resource_string_package(&node.body)
        {
            push_strings(pkg, out);
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

fn push_strings(pkg: ParsedStringPackage, out: &mut Vec<StringInfo>) {
    for (sid, text) in pkg.strings {
        out.push(StringInfo {
            language: pkg.language.clone(),
            string_id: sid as u32,
            text,
        });
    }
}

fn first_resource_string_package(pe: &[u8]) -> Option<ParsedStringPackage> {
    for blob in hii_resource_blobs(pe) {
        let Some(list) = parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == PACKAGE_STRINGS
                && let Some(sp) = parse_string_package(pkg.bytes)
            {
                return Some(sp);
            }
        }
    }
    None
}

pub(crate) fn declared_len_sane(body: &[u8]) -> bool {
    body.len() >= 4
        && (body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16) <= body.len()
}
```

First-package-only semantics preserved on purpose: HNX99TF secondary packages are language `x-UEFI-AMI` and aggregation would break the `en*` real-image assert (spec §4.5 as amended). `declared_len_sane` is a hardening of the bare path so garbage section bodies with `body[3]==0x04` cannot shadow real packages on live images.

- [ ] **Step 5: Run tests, verify they pass**

Run: `cargo test -p uefi-engine hii::`
Expected: all pass, INCLUDING the pre-existing `collect_forms_resolves_titles_and_visibility` (`:0x19:1` numbering unchanged on wrapper-free images) and `collect_strings_returns_first_package_strings`.

- [ ] **Step 6: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/hii/forms.rs crates/uefi-engine/src/hii/strings.rs
git commit -m "feat(uefi-engine): collect forms/strings from PE resources (per-file titles, wrapper DFS numbering, real-layout fixtures)"
```

---

### Task 6: `find_item`/`find_item_path` — DFS mirror through wrappers

**Files:**
- Modify: `crates/uefi-engine/src/parser/target.rs:63-84` (`find_item` GuidSection arm), `:97-117` (`find_item_path` GuidSection arm)

**Interfaces:**
- Consumes: nothing new.
- Produces: unchanged public signatures; `GuidSection` targets now resolve sections nested inside `0x01`/`0x02` wrapper sections, numbered pre-order per subtype exactly like `collect_forms` (spec §5; semantics change from direct-children-only is documented there).

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `target.rs`:

```rust
    fn wrapped_tree() -> FfsNode {
        let inner = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x10,
            offset: 0,
            header: vec![],
            body: vec![0xAA],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let wrapper = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x02,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![inner],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let direct = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x10,
            offset: 0,
            header: vec![],
            body: vec![0xBB],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let file = FfsNode {
            guid: Some(Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap()),
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![wrapper, direct],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        FfsNode {
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
        }
    }

    fn wrapped_target(index: Option<usize>) -> Target {
        Target::GuidSection {
            guid: Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap(),
            section_type: 0x10,
            section_index: index,
        }
    }

    #[test]
    fn find_item_resolves_sections_inside_guided_wrapper() {
        let tree = wrapped_tree();
        let inner = find_item(&tree, &wrapped_target(Some(0))).unwrap();
        assert_eq!(inner.body, vec![0xAA]);
        let direct = find_item(&tree, &wrapped_target(Some(1))).unwrap();
        assert_eq!(direct.body, vec![0xBB]);
        let first = find_item(&tree, &wrapped_target(None)).unwrap();
        assert_eq!(first.body, vec![0xAA]);
    }

    #[test]
    fn find_item_path_resolves_sections_inside_guided_wrapper() {
        let tree = wrapped_tree();
        assert_eq!(
            find_item_path(&tree, &wrapped_target(Some(0))).unwrap(),
            vec![0, 0, 0, 0]
        );
        assert_eq!(
            find_item_path(&tree, &wrapped_target(Some(1))).unwrap(),
            vec![0, 0, 1]
        );
        assert!(find_item_path(&tree, &wrapped_target(Some(2))).is_none());
    }

    #[test]
    fn find_item_mut_reaches_wrapped_section() {
        let mut tree = wrapped_tree();
        let node = find_item_mut(&mut tree, &wrapped_target(Some(0))).unwrap();
        node.body.push(1);
        assert_eq!(tree.children[0].children[0].children[0].body, vec![0xAA, 1]);
    }
```

And in `forms.rs` tests append the mirror-equivalence round-trip:

```rust
    #[test]
    fn collect_forms_wrapped_target_round_trips() {
        let list = hii_list(&form_pkg(1), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let inner = mk_node(None, FfsType::Section, 0x10, pe, vec![]);
        let wrapper = mk_node(None, FfsType::Section, 0x02, vec![], vec![inner]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![wrapper],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let forms = collect_forms(&image);
        let t = crate::parser::target::parse_target(&forms[0].form_id).unwrap();
        let node = crate::parser::target::find_item(&image.root, &t).unwrap();
        assert_eq!(node.subtype, 0x10);
        assert_eq!(node.node_type, FfsType::Section);
    }
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p uefi-engine target::`
Expected: the three new target tests FAIL (wrapper sections invisible to the current direct-children scan); `collect_forms_wrapped_target_round_trips` FAILS (`find_item` cannot resolve `:0x10:0` inside the wrapper).

- [ ] **Step 3: Implement**

In `target.rs` add the shared DFS (place it next to `node_at_path`):

```rust
fn find_guid_section_dfs<'a>(
    node: &'a FfsNode,
    section_type: u8,
    wanted: Option<usize>,
    count: &mut usize,
    path: &mut Vec<usize>,
) -> Option<(&'a FfsNode, Vec<usize>)> {
    for (i, child) in node.children.iter().enumerate() {
        if child.node_type != FfsType::Section {
            continue;
        }
        let hit = child.subtype == section_type && wanted.is_none_or(|w| w == *count);
        if child.subtype == section_type {
            *count += 1;
        }
        if hit {
            path.push(i);
            return Some((child, path.clone()));
        }
        if child.subtype == crate::ffs::EFI_SECTION_COMPRESSION
            || child.subtype == crate::ffs::EFI_SECTION_GUID_DEFINED
        {
            path.push(i);
            if let Some(found) = find_guid_section_dfs(child, section_type, wanted, count, path) {
                return Some(found);
            }
            path.pop();
        }
    }
    None
}
```

Replace the GuidSection arm of `find_item` with:

```rust
        Target::GuidSection {
            guid,
            section_type,
            section_index,
        } => {
            let file = find_by_guid(root, guid)
                .ok_or_else(|| ParserError::InvalidHeader(format!("guid {guid} not found")))?;
            let mut count = 0usize;
            find_guid_section_dfs(file, *section_type, *section_index, &mut count, &mut Vec::new())
                .map(|(node, _)| node)
                .ok_or_else(|| ParserError::InvalidHeader(format!(
                    "section type {section_type:#x} not found in {guid}"
                )))
        }
```

and the GuidSection arm of `find_item_path` with:

```rust
        Target::GuidSection {
            guid,
            section_type,
            section_index,
        } => {
            let mut file_path = find_path_by_guid(root, guid, &mut Vec::new())?;
            let file = node_at_path(root, &file_path)?;
            let mut count = 0usize;
            let (_, rel) = find_guid_section_dfs(
                file,
                *section_type,
                *section_index,
                &mut count,
                &mut Vec::new(),
            )?;
            file_path.extend(rel);
            Some(file_path)
        }
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p uefi-engine target:: && cargo test -p uefi-engine hii::`
Expected: all pass — pre-existing target tests included (direct-child numbering is the degenerate case of the DFS).

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/parser/target.rs crates/uefi-engine/src/hii/forms.rs
git commit -m "feat(uefi-engine): resolve GuidSection targets through compression wrappers (DFS mirror)"
```

---

### Task 7: `set_item_visibility` gates + test amendments

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`HiiError` + `set_item_visibility` + tests)
- Modify: `crates/uefi-engine/tests/real_image.rs:659-682` (visibility test expectation — spec §6 amendment)

**Interfaces:**
- Consumes: `find_item_path` incl. wrapper resolution (Task 6).
- Produces: two new `HiiError` variants — `NotWritable`, `MutationBehindCompression` — and the bare-only mutability gate (`subtype 0x19 || is_form_package(body)`, else `NotASetupItem`) (spec §6 as amended by `d60dc82`).

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `hii/mod.rs` (helpers `mk_node`/`sample_image_with_ifr*` already exist there):

```rust
    #[test]
    fn set_item_visibility_refuses_read_only_mode() {
        let mut image = sample_image_with_ifr_guid();
        image.mode = ImageMode::Read;
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotWritable));
    }

    #[test]
    fn set_item_visibility_refuses_mutation_behind_compression() {
        let mut inner = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        inner.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = 0x02;
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::MutationBehindCompression));
    }

    #[test]
    fn set_item_visibility_refuses_uncompressed_pe32_target() {
        let mut section = mk_node(FfsType::Section, vec![b'M', b'Z', 0, 0], vec![]);
        section.subtype = 0x10;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }
```

Amend the existing fixture `sample_image_with_ifr` (bare `0/0/0` path test): the section gets subtype `0x19` (spec §6 amendment — mutability is bare-section-only):

```rust
        let mut section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        section.subtype = 0x19;
```

(replacing the current `let section = mk_node(...)` binding).

And amend the real-image test in `tests/real_image.rs` (`real_image_hii_form_visibility_round_trip`, spec §6 — resolution assertions stay, the mutation now expects the honest refusal):

```rust
    let err = set_item_visibility(&mut img, &form_id, true)
        .expect_err("HNX99TF HII lives inside LZMA-guided wrappers; mutation must be refused until the recompression phase (spec §6)");
    assert!(matches!(err, uefi_engine::hii::HiiError::MutationBehindCompression));
```

(replacing the current `set_item_visibility(&mut img, &form_id, true).expect(...)` line).

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p uefi-engine hii::`
Expected: the three new tests FAIL (variants don't exist — compile error); `set_item_visibility_true_unsuppresses_and_cascades` FAILS too until the `sample_image_with_ifr` amendment compiles in (it passes as soon as subtype is 0x19 and the bare gate lands).

- [ ] **Step 3: Implement**

In `hii/mod.rs` add to `HiiError`:

```rust
    #[error("image is not writable (open in Write mode first)")]
    NotWritable,
    #[error("target is behind a compressed/guided section; mutation requires recompression (planned next phase)")]
    MutationBehindCompression,
```

and extend `set_item_visibility` (imports first: `use crate::ffs::{EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_RAW};`):

```rust
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let target = crate::parser::target::parse_target(item_id).map_err(|_| HiiError::NotFound)?;
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let mut ancestor = &image.root;
    for &i in &path[..path.len() - 1] {
        ancestor = &ancestor.children[i];
        if ancestor.node_type == FfsType::Section
            && (ancestor.subtype == EFI_SECTION_COMPRESSION
                || ancestor.subtype == EFI_SECTION_GUID_DEFINED)
        {
            return Err(HiiError::MutationBehindCompression);
        }
    }
    let mut changed = false;
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section
            || (node.subtype != EFI_SECTION_RAW && !ifr::is_form_package(&node.body))
        {
            return Err(HiiError::NotASetupItem);
        }
        if visible && let Some(scope) = ifr::find_suppress_if_scopes(&node.body).into_iter().next()
        {
            let mut body = node.body.clone();
            ifr::unsuppress(&mut body, &scope);
            node.body = body;
            changed = true;
        }
    }
    if changed {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(changed, "set_item_visibility done");
    Ok(())
}
```

(`path` is never empty — it always contains at least the target index — so `path[..path.len() - 1]` is safe.)

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p uefi-engine hii::`
Expected: all pass, including the pre-existing `set_item_visibility_guid_section_target_unsuppresses` (0x19 direct child — no gate fires) and `set_item_visibility_guid_target_rejects_non_section`.

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`
(The real-image visibility test itself runs in Task 9 — it needs the external file.)

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/tests/real_image.rs
git commit -m "feat(uefi-engine): set_item_visibility gates (Write-only, compression barrier, bare-only targets)"
```

---

### Task 8: `real_image.rs` cosmetics

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs`

**Interfaces:** none — test hygiene only (spec §3 item 8).

- [ ] **Step 1: Apply the two cosmetic changes**

1. Unify the ignore reason: the three trailing tests use `#[ignore = "requires external real BIOS image under refs/fw/"]`; change them to the long form used by the rest of the file: `#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]` (tests `real_image_full_flash_round_trip`, `real_image_full_flash_repatch_stability`, `real_image_hii_forms_and_strings`, `real_image_hii_form_visibility_round_trip` — grep to catch all).

2. Language assert message — replace

```rust
        "primary language expected ~English (en/en-US/eng), got {:?}",
        strings.first().map(|s| &s.language)
```

with

```rust
        "expected primary language ~English (en/en-US/eng), got {:?}",
        strings.first().map(|s| s.language.as_str())
```

- [ ] **Step 2: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "chore(uefi-engine): real_image test cosmetics (ignore strings, language assert message)"
```

---

### Task 9: Final verification — real-image acceptance + CLI e2e

**Files:** none modified (verification only; any failure found here is fixed under the systematic-debugging skill BEFORE moving on, with a plan-fix commit first if it reveals a plan/spec defect).

- [ ] **Step 1: Full local suite**

Run: `cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check`
Expected: all green.

- [ ] **Step 2: Real-image acceptance on HNX99TF**

```bash
UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin \
  cargo test -p uefi-engine --test real_image -- --ignored
```

Expected: ALL real-image tests pass. Specifically:
- `real_image_hii_forms_and_strings` green **without assert changes** — expect 4+ formsets (`7B59104A…` Setup, `EC87D643…` Platform, `80E1202E…`, `932D37B0…`), strings language `en-US`;
- `real_image_hii_form_visibility_round_trip` green with the amended expectation (`MutationBehindCompression`).

Contingency (documented, spec §10 risk 1): if `forms[0]` resolves to an UNCOMPRESSED PE32 (bare-channel hit in a raw-stored PE before the Setup file in tree order), the visibility test fails with `NotASetupItem` instead. Evidence first: print `forms[0].form_id`/`formset_guid`, verify with `hack/hii_probe.py refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, then amend the expectation to the actually-demonstrated refusal kind via a plan-fix commit — never weaken to "any error".

- [ ] **Step 3: Cross-check on rk3588 (EDK2 world)**

```bash
UEFIPATCHER_TEST_FW=/var/home/dsevosty/.local/share/uefipatcher/sessions/7a126462-3ca8-42c6-8835-9ccaec3a90b7/images/9aa679cd-cfa4-4f25-89c8-ae55fb0717d3.bin \
  cargo test -p uefi-engine --test real_image real_image_hii_forms_and_strings -- --ignored
```

Expected: passes — forms arrive via the bare channel (14+ formsets incl. `642237C7…`), strings via the resource channel (`en-US`). Other real-image tests are NOT expected to pass on this image (different flash layout) — run only the HII one. OVMF is not suitable for this check (no string resources — spec §8).

- [ ] **Step 4: CLI e2e unchanged**

Run: `cargo test -p uefi-cli`
Expected: green (proto/CLI untouched this phase; `hii form list`-style flows now surface real forms against the mock).

- [ ] **Step 5: Report**

Report per-task results, the two acceptance outputs (HNX99TF + rk3588 cross-check), and any contingencies hit. No commit (nothing changed).
