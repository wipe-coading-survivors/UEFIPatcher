# LZMA Recompression (Phase 7) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Мутации узлов внутри GUIDed plain-LZMA секций материализуются в выходных байтах (recompress-ветка билдера); прочие dirty-обёртки дают честную ошибку; гейт HII сужается до непересжимаемых обёрток.

**Architecture:** Изолированный примитив `compress_lzma` (публичный lzma-rs 0.3.0 компрессор, формат EDK2 known-size + round-trip self-check) → `subtree_dirty`-детект → recompress-ветка в `build_section` (сериализация детей, дословный префикс GUID/DataOffset/Attributes, `set_section_size`). Порядок фаз: **фаза 6 реализована раньше** (решение пользователя), поэтому Task 7 сужает уже реализованный гейт `set_item_visibility` и меняет ожидание real-image теста.

**Tech Stack:** Rust (edition 2024), lzma-rs 0.3.0 (уже в workspace — новых зависимостей НЕТ), thiserror, tracing.

**Spec:** `docs/superpowers/specs/2026-08-14-lzma-recompression-design.md`

## Global Constraints

- Никаких комментариев в коде (кроме ссылок на референс `file:line`).
- Module-first rule: `pub mod compress;` в `lib.rs` в ТОМ ЖЕ шаге, что создание файла, ДО запуска `cargo test`.
- TDD порядок: тесты → красный прогон → реализация → зелёный прогон → commit.
- После каждой задачи: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`.
- Real-image тесты — `#[ignore]` с длинной причиной: `#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]`; запуск: `UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin cargo test -p uefi-engine --test real_image -- --ignored`.
- Ветка `fix/cycle6-reimplent`; байты Lenovo в git не попадают (фикстуры уже в `tests/fixtures/`, синтетика в коде).
- Этот план исполняется ТОЛЬКО после завершения фазы 6 (Task 7 опирается на её код: гейты `set_item_visibility`, helpers `mk_node`/`FILE_GUID_STR` в тестах `hii/mod.rs`, ожидание `MutationBehindCompression` в `real_image.rs`).

## Reference — verified live facts

- **lzma-rs 0.3.0** (Cargo.lock, локальная копия registry): `pub fn lzma_compress_with_options<R: io::BufRead, W: io::Write>(input: &mut R, output: &mut W, options: &compress::Options) -> io::Result<()>` (`src/lib.rs:70`); опции `lzma_rs::compress::Options { unpacked_size: UnpackedSize::WriteToHeader(Some(u64)) }`. Заголовок вывода: `[0x5D, 0x00, 0x00, 0x80, 0x00, <size u64 LE>]` — props lc=3/lp=0/pb=2, dict 0x800000, известный size, без EOS. Энкодер literal-only; реальные блобы 0.346–0.426; round-trip валидирован lzma-rs и liblzma (probe, коммит `000b6bd`).
- **HNX99TF** (16 MB): FV0 @0x800000 free=0; FV1 @0x890000..0xd60000 (5.06 MB, 216 files, **free=2 033 KiB**, все LZMA-секции props 0x5D dict 0x1000000); FV2 @0xda0000 free=0. Первый guided-узел с UI-ребёнком при DFS по дереву лежит в FV1 (у FV0 единственный файл — RAW без секций).
- **Fixture** `tests/fixtures/lzma_guided_section.bin` (186 B, guided LZMA, DataOffset=0x18) + `.decompressed.bin` (210 B): распакованный буфер = ОДНА секция RAW 0x19 (header 4 B + 206 B x86-кода). Из `src/` путь: `../../../tests/fixtures/…`, из `src/builder/`: `../../../../tests/fixtures/…`.
- **Guided-layout ноды**: `header` = 4/8 B section-header; `body` = GUID(16) + DataOffset u16 (секционно-относительный, стандартно 0x18) + Attributes u16 + LZMA-payload; payload = `body[data_offset-4..]` (`parser/section.rs:81-84`).
- **Каскад ops**: `mark_rebuild_to_root_by_path` (`ops.rs:111-125`) помечает всех предков Rebuild — guided-узел всегда dirty при мутации потомка.
- **Пост-фаза-6 гейт** (`set_item_visibility`, план фазы 6 Task 7): mode-гейт → `find_item_path` → предок 0x01/0x02 → `HiiError::MutationBehindCompression` → `find_item_mut` → bare-гейт (`subtype 0x19 || is_form_package`) → `NotASetupItem` → unsuppress.

## Reference — current code

- `crates/uefi-engine/src/builder/mod.rs:105-135` — `build_section` + `is_compressed_or_guided`; `:137-183` — set_ffs_size/set_section_size/recompute_ffs_checksums; `:1` — `use self::align::{align4, align8, pad_to};`.
- `crates/uefi-engine/src/ops.rs:101-109` — `rebuild`; `:111-125` — `mark_rebuild_to_root_by_path`.
- `crates/uefi-engine/src/rpc/server.rs:31-66` — `flush_image` (инлайн `map_err(|e| Status::internal(e.to_string()))` на `:38`).
- `crates/uefi-engine/src/lib.rs` — модули по алфавиту: builder, decompress, ffs, hii, logging, ops, parser, rpc, session, storage, types.
- `crates/uefi-engine/src/ffs.rs:22-44` — GUID-функции (lzma/lzma_hp/lzma_ms/lzmaf86/tiano) и `is_lzma_guid`; тесты с `:150`.
- `crates/uefi-engine/tests/real_image.rs` — helpers `fw_path`/`load_fw`/`scan_ffs_files`; file-level `use uefi_engine::types::{Action, FfsNode, FfsType, ParsingData};` (`:9`).

---

### Task 1: `compress.rs` — примитив `compress_lzma`

**Files:**
- Create: `crates/uefi-engine/src/compress.rs`
- Modify: `crates/uefi-engine/src/lib.rs` (добавить `pub mod compress;`)

**Interfaces:**
- Consumes: `lzma_rs::lzma_compress_with_options`, `lzma_rs::lzma_decompress` (крейт уже в зависимостях).
- Produces: `pub enum CompressError { EmptyInput, CompressFailed, RoundTripFailed }`; `pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError>` — выход = полный alone-стрим (13-байтный заголовок + поток), декодируемый `decompress::decompress(out, 2)`.

- [ ] **Step 1: Создать файл с тестами + заглушкой, объявить модуль**

Создать `crates/uefi-engine/src/compress.rs`:

```rust
use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("empty input")]
    EmptyInput,
    #[error("lzma compression failed")]
    CompressFailed,
    #[error("lzma round-trip check failed")]
    RoundTripFailed,
}

pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    let _ = input;
    Err(CompressError::CompressFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECOMPRESSED: &[u8] =
        include_bytes!("../../../tests/fixtures/lzma_guided_section.decompressed.bin");

    #[test]
    fn compress_lzma_empty_input_errors() {
        assert!(matches!(compress_lzma(&[]), Err(CompressError::EmptyInput)));
    }

    #[test]
    fn compress_lzma_writes_edk2_alone_header() {
        let out = compress_lzma(&[0x00; 64]).unwrap();
        assert!(out.len() > 13);
        assert_eq!(out[0], 0x5D);
        assert_eq!(&out[1..5], &[0x00, 0x00, 0x80, 0x00]);
        assert_eq!(&out[5..13], &64u64.to_le_bytes());
    }

    #[test]
    fn compress_lzma_round_trips_real_blob() {
        let out = compress_lzma(DECOMPRESSED).unwrap();
        let mut decoded = Vec::new();
        lzma_rs::lzma_decompress(&mut Cursor::new(&out), &mut decoded).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    #[test]
    fn compress_lzma_output_decodable_by_engine() {
        let out = compress_lzma(DECOMPRESSED).unwrap();
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }
}
```

В `crates/uefi-engine/src/lib.rs` добавить строку `pub mod compress;` между `pub mod builder;` и `pub mod decompress;`.

- [ ] **Step 2: Красный прогон**

Run: `cargo test -p uefi-engine compress::`
Expected: FAIL — `compress_lzma_empty_input_errors` (получен `CompressFailed`), `compress_lzma_writes_edk2_alone_header`, `compress_lzma_round_trips_real_blob`, `compress_lzma_output_decodable_by_engine` (unwrap на `Err(CompressFailed)`).

- [ ] **Step 3: Реализация**

Заменить заглушку `compress_lzma` на:

```rust
pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }
    let options = lzma_rs::compress::Options {
        unpacked_size: lzma_rs::compress::UnpackedSize::WriteToHeader(Some(input.len() as u64)),
    };
    let mut out = Vec::new();
    lzma_rs::lzma_compress_with_options(&mut Cursor::new(input), &mut out, &options)
        .map_err(|_| CompressError::CompressFailed)?;
    let mut round_trip = Vec::new();
    lzma_rs::lzma_decompress(&mut Cursor::new(&out), &mut round_trip)
        .map_err(|_| CompressError::RoundTripFailed)?;
    if round_trip.as_slice() != input {
        tracing::warn!(size = input.len(), "lzma round-trip mismatch");
        return Err(CompressError::RoundTripFailed);
    }
    Ok(out)
}
```

- [ ] **Step 4: Зелёный прогон**

Run: `cargo test -p uefi-engine compress::`
Expected: PASS (4 теста).

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/compress.rs crates/uefi-engine/src/lib.rs
git commit -m "feat(uefi-engine): compress_lzma primitive (EDK2 alone header, known size, round-trip self-check)"
```

---

### Task 2: `subtree_dirty` + `is_recompressable_lzma_guid`

**Files:**
- Modify: `crates/uefi-engine/src/builder/mod.rs` (новая приватная fn + тесты)
- Modify: `crates/uefi-engine/src/ffs.rs` (новая pub fn + тест)

**Interfaces:**
- Consumes: `Action`, `FfsNode` (types), GUID-функции ffs.rs.
- Produces: `fn subtree_dirty(node: &FfsNode) -> bool` (приватная в builder); `pub fn is_recompressable_lzma_guid(g: &Guid) -> bool` (ffs.rs; lzma/lzma_hp/lzma_ms, БЕЗ f86).

- [ ] **Step 1: Тесты + заглушки**

В `builder/mod.rs`, в конец модуля `tests` (после `rebuild_file_with_checksum_attribute_uses_body_checksum`):

```rust
    fn leaf_section(action: Action) -> FfsNode {
        FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![0xAA; 8],
            tail: vec![],
            children: vec![],
            action,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    #[test]
    fn subtree_dirty_false_for_clean_tree() {
        assert!(!subtree_dirty(&leaf_section(Action::NoAction)));
    }

    #[test]
    fn subtree_dirty_true_for_dirty_descendant() {
        let mut parent = leaf_section(Action::NoAction);
        parent.children = vec![leaf_section(Action::Replace)];
        assert!(subtree_dirty(&parent));
    }

    #[test]
    fn subtree_dirty_true_for_own_action() {
        assert!(subtree_dirty(&leaf_section(Action::Rebuild)));
    }
```

В `builder/mod.rs` рядом с `is_compressed_or_guided` (`:133`) добавить заглушку:

```rust
fn subtree_dirty(node: &FfsNode) -> bool {
    let _ = node;
    false
}
```

В `ffs.rs`, в модуль `tests`, добавить:

```rust
    #[test]
    fn is_recompressable_lzma_guid_excludes_f86_and_tiano() {
        assert!(is_recompressable_lzma_guid(&lzma_guid()));
        assert!(is_recompressable_lzma_guid(&lzma_hp_guid()));
        assert!(is_recompressable_lzma_guid(&lzma_ms_guid()));
        assert!(!is_recompressable_lzma_guid(&lzmaf86_guid()));
        assert!(!is_recompressable_lzma_guid(&tiano_guid()));
    }
```

и рядом с `is_lzma_guid` (`:38`) заглушку:

```rust
pub fn is_recompressable_lzma_guid(g: &Guid) -> bool {
    let _ = g;
    false
}
```

- [ ] **Step 2: Красный прогон**

Run: `cargo test -p uefi-engine subtree_dirty && cargo test -p uefi-engine is_recompressable`
Expected: FAIL — `subtree_dirty_true_for_dirty_descendant`, `subtree_dirty_true_for_own_action`, `is_recompressable_lzma_guid_excludes_f86_and_tiano` (заглушки возвращают false).

- [ ] **Step 3: Реализация**

Заменить заглушку в `builder/mod.rs`:

```rust
fn subtree_dirty(node: &FfsNode) -> bool {
    node.action != Action::NoAction || node.children.iter().any(subtree_dirty)
}
```

Заменить заглушку в `ffs.rs`:

```rust
pub fn is_recompressable_lzma_guid(g: &Guid) -> bool {
    let b = g.to_bytes();
    b == lzma_guid().to_bytes()
        || b == lzma_hp_guid().to_bytes()
        || b == lzma_ms_guid().to_bytes()
}
```

- [ ] **Step 4: Зелёный прогон**

Run: `cargo test -p uefi-engine subtree_dirty && cargo test -p uefi-engine is_recompressable`
Expected: PASS.

- [ ] **Step 5: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/builder/mod.rs crates/uefi-engine/src/ffs.rs
git commit -m "feat(uefi-engine): subtree_dirty detector + is_recompressable_lzma_guid predicate (no F86)"
```

---

### Task 3: `build_section` recompress-ветка + `BuilderError`

**Files:**
- Modify: `crates/uefi-engine/src/builder/mod.rs` (`BuilderError`, `build_section`, новые fn + тесты)

**Interfaces:**
- Consumes: `compress_lzma`/`CompressError` (Task 1), `subtree_dirty`/`leaf_section` (Task 2), `is_recompressable_lzma_guid` (Task 2), `set_section_size`, `align4`, `pad_to`, `parse_section`.
- Produces: `BuilderError::Compression(#[from] CompressError)`; `BuilderError::RecompressionUnsupported`; recompress-семантика `build_section` (спека §4.4, матрица §6).

- [ ] **Step 1: Варианты ошибок + failing-тесты**

В `builder/mod.rs` заменить enum `BuilderError` на:

```rust
#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("size mismatch: content exceeds container capacity")]
    SizeMismatch,
    #[error("checksum failed")]
    ChecksumFailed,
    #[error(transparent)]
    Compression(#[from] crate::compress::CompressError),
    #[error("compressed/guided section cannot be rebuilt: unsupported algorithm (Tiano, LZMAF86, standard compression, unknown GUID) or no decompressed children")]
    RecompressionUnsupported,
}
```

и добавить импорт после `use crate::ffs::*;`:

```rust
use crate::compress::compress_lzma;
```

В модуль `tests` добавить (после тестов Task 2):

```rust
    fn guided_node(guid: Guid, action: Action) -> FfsNode {
        FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_GUID_DEFINED,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![],
            tail: vec![],
            children: vec![leaf_section(Action::NoAction)],
            action,
            parsing_data: ParsingData::GuidedSection(GuidedSectionParsingData {
                guid,
                dictionary_size: 0,
            }),
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    #[test]
    fn guided_lzma_clean_tree_builds_verbatim() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let node = crate::parser::section::parse_section(section, 0).unwrap();
        assert!(!node.children.is_empty());
        let mut out = Vec::new();
        build_section(&node, &mut out).unwrap();
        assert_eq!(out.as_slice(), &section[..]);
    }

    #[test]
    fn guided_lzma_dirty_child_recompresses_and_materializes() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let mut node = crate::parser::section::parse_section(section, 0).unwrap();
        let prefix_before = node.body[..20].to_vec();
        let child = &mut node.children[0];
        child.action = Action::Replace;
        child.children.clear();
        child.body = vec![0x42; 24];
        node.action = Action::Rebuild;
        let mut out = Vec::new();
        build_section(&node, &mut out).unwrap();
        assert_ne!(out.as_slice(), &section[..]);
        assert_eq!(section_size(&out) as usize, out.len());
        let rebuilt = crate::parser::section::parse_section(&out, 0).unwrap();
        assert_eq!(&rebuilt.body[..20], &prefix_before[..]);
        assert_eq!(rebuilt.children[0].body, vec![0x42; 24]);
    }

    #[test]
    fn guided_lzma_noaction_with_dirty_descendant_recompresses() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let mut node = crate::parser::section::parse_section(section, 0).unwrap();
        node.children[0].action = Action::Replace;
        let mut out = Vec::new();
        build_section(&node, &mut out).unwrap();
        assert_ne!(out.as_slice(), &section[..]);
        assert!(crate::parser::section::parse_section(&out, 0).is_ok());
    }

    #[test]
    fn guided_lzma_remove_only_child_errors_empty_input() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let mut node = crate::parser::section::parse_section(section, 0).unwrap();
        node.children[0].action = Action::Remove;
        node.action = Action::Rebuild;
        let mut out = Vec::new();
        let err = build_section(&node, &mut out).unwrap_err();
        assert!(matches!(
            err,
            BuilderError::Compression(crate::compress::CompressError::EmptyInput)
        ));
    }

    #[test]
    fn guided_lzma_replace_body_only_errors_without_children() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let mut node = crate::parser::section::parse_section(section, 0).unwrap();
        node.action = Action::Replace;
        node.children.clear();
        let mut out = Vec::new();
        let err = build_section(&node, &mut out).unwrap_err();
        assert!(matches!(err, BuilderError::RecompressionUnsupported));
    }

    #[test]
    fn dirty_tiano_guided_errors_recompression_unsupported() {
        let node = guided_node(tiano_guid(), Action::Rebuild);
        let mut out = Vec::new();
        let err = build_section(&node, &mut out).unwrap_err();
        assert!(matches!(err, BuilderError::RecompressionUnsupported));
    }

    #[test]
    fn dirty_compression_section_errors_recompression_unsupported() {
        let mut node = leaf_section(Action::Rebuild);
        node.subtype = EFI_SECTION_COMPRESSION;
        node.children = vec![leaf_section(Action::NoAction)];
        let mut out = Vec::new();
        let err = build_section(&node, &mut out).unwrap_err();
        assert!(matches!(err, BuilderError::RecompressionUnsupported));
    }
```

- [ ] **Step 2: Красный прогон**

Run: `cargo test -p uefi-engine builder::`
Expected: FAIL — `guided_lzma_dirty_child_recompresses_and_materializes`, `guided_lzma_noaction_with_dirty_descendant_recompresses` (сегодняшний код эммитит verbatim → `out == section`), `guided_lzma_remove_only_child_errors_empty_input`, `guided_lzma_replace_body_only_errors_without_children`, `dirty_tiano_guided_…`, `dirty_compression_section_…` (сегодня `build_section` возвращает `Ok`). `guided_lzma_clean_tree_builds_verbatim` PASS уже сейчас.

- [ ] **Step 3: Реализация**

Заменить в `builder/mod.rs` функцию `build_section` целиком на:

```rust
fn build_section(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.action == Action::Remove {
        return Ok(());
    }
    if is_compressed_or_guided(node) {
        if node.action == Action::NoAction && !subtree_dirty(node) {
            out.extend_from_slice(&node.header);
            out.extend_from_slice(&node.body);
            out.extend_from_slice(&node.tail);
            return Ok(());
        }
        if node.subtype == EFI_SECTION_GUID_DEFINED
            && recompressable_guid(node)
            && !node.children.is_empty()
        {
            return build_recompressed_guided(node, out);
        }
        return Err(BuilderError::RecompressionUnsupported);
    }
    if node.action == Action::NoAction {
        out.extend_from_slice(&node.header);
        out.extend_from_slice(&node.body);
        out.extend_from_slice(&node.tail);
        return Ok(());
    }
    let mut body = if node.children.is_empty() {
        node.body.clone()
    } else {
        Vec::new()
    };
    for child in &node.children {
        build_node(child, &mut body)?;
        let target = align4(body.len());
        pad_to(&mut body, target, 0x00);
    }
    let mut header = node.header.clone();
    let total = header.len() + body.len();
    set_section_size(&mut header, total);
    out.extend_from_slice(&header);
    out.extend_from_slice(&body);
    Ok(())
}

fn recompressable_guid(node: &FfsNode) -> bool {
    matches!(
        &node.parsing_data,
        ParsingData::GuidedSection(d) if is_recompressable_lzma_guid(&d.guid)
    )
}

fn build_recompressed_guided(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    let mut children = Vec::new();
    for child in &node.children {
        build_node(child, &mut children)?;
        let target = align4(children.len());
        pad_to(&mut children, target, 0x00);
    }
    let stream = compress_lzma(&children)?;
    if node.body.len() < 18 {
        return Err(BuilderError::RecompressionUnsupported);
    }
    let data_offset = u16::from_le_bytes([node.body[16], node.body[17]]) as usize;
    let prefix_len = data_offset.wrapping_sub(4);
    if !(20..=node.body.len()).contains(&prefix_len) {
        return Err(BuilderError::RecompressionUnsupported);
    }
    let mut header = node.header.clone();
    let total = header.len() + prefix_len + stream.len();
    set_section_size(&mut header, total);
    out.extend_from_slice(&header);
    out.extend_from_slice(&node.body[..prefix_len]);
    out.extend_from_slice(&stream);
    Ok(())
}
```

- [ ] **Step 4: Зелёный прогон**

Run: `cargo test -p uefi-engine`
Expected: PASS — все тесты крейта, включая прежние (`round_trip_volume`, `build_section_skips_removed`, `build_section_emits_rebuilt_when_not_removed` — RAW-секции идут по generic-пути).

- [ ] **Step 5: Verify + commit**

Run: `cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/builder/mod.rs
git commit -m "feat(uefi-engine): build_section recompress branch for guided LZMA + honest RecompressionUnsupported elsewhere"
```

---

### Task 4: `ops::rebuild` не перетирает `Action::Remove`

**Files:**
- Modify: `crates/uefi-engine/src/ops.rs:101-109` (`rebuild` + тест)

**Interfaces:**
- Consumes: существующие тест-хелперы `make_simple_image`/`make_ffs_file`, `insert`/`remove`/`rebuild`.
- Produces: guard-семантика rebuild (no-op на Remove-узле) — спека §4.5.

- [ ] **Step 1: Failing-тест**

В `ops.rs`, модуль `tests`, добавить:

```rust
    #[test]
    fn rebuild_after_remove_keeps_remove() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let ffs = make_ffs_file();
        let t = parse_target("0").unwrap();
        insert(&mut img.root, &t, &ffs, InsertMode::Into).unwrap();
        let file_target = parse_target("0/0").unwrap();
        remove(&mut img.root, &file_target).unwrap();
        rebuild(&mut img.root, &file_target).unwrap();
        assert_eq!(
            img.root.children[0].children[0].action,
            Action::Remove,
            "rebuild must not resurrect a removed node"
        );
    }
```

- [ ] **Step 2: Красный прогон**

Run: `cargo test -p uefi-engine rebuild_after_remove`
Expected: FAIL — action стал `Rebuild`.

- [ ] **Step 3: Реализация**

В `ops.rs` в `rebuild` после строки `let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;` добавить:

```rust
    if node.action == Action::Remove {
        return Ok(());
    }
```

- [ ] **Step 4: Зелёный прогон**

Run: `cargo test -p uefi-engine ops::`
Expected: PASS — включая прежние `rebuild_marks_node_and_cascade` (узел не Remove — guard не срабатывает).

- [ ] **Step 5: Verify + commit**

Run: `cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/ops.rs
git commit -m "fix(uefi-engine): ops::rebuild no longer overrides Action::Remove (issue II-ter)"
```

---

### Task 5: `flush_image` маппинг `RecompressionUnsupported` → `failed_precondition`

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (helper + `flush_image:38` + тест)

**Interfaces:**
- Consumes: `BuilderError` (Task 3).
- Produces: `fn builder_error_status(e: crate::builder::BuilderError) -> Status` — RPC-статус `FailedPrecondition` для барьера, `Internal` для остальных (спека §4.6, §7).

- [ ] **Step 1: Failing-тест**

В `server.rs`, модуль `tests` (после `setup`), добавить:

```rust
    #[test]
    fn builder_error_status_maps_recompression_to_failed_precondition() {
        let st = builder_error_status(crate::builder::BuilderError::RecompressionUnsupported);
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert!(st.message().contains("cannot be rebuilt"));
        let st = builder_error_status(crate::builder::BuilderError::SizeMismatch);
        assert_eq!(st.code(), tonic::Code::Internal);
    }
```

- [ ] **Step 2: Красный прогон**

Run: `cargo test -p uefi-engine builder_error_status`
Expected: FAIL компиляцией — `builder_error_status` не определена.

- [ ] **Step 3: Реализация**

В `server.rs` перед `impl EngineServer` добавить:

```rust
fn builder_error_status(e: crate::builder::BuilderError) -> Status {
    match e {
        crate::builder::BuilderError::RecompressionUnsupported => {
            Status::failed_precondition(e.to_string())
        }
        _ => Status::internal(e.to_string()),
    }
}
```

В `flush_image` заменить строку

```rust
            let bytes =
                crate::builder::build_image(img).map_err(|e| Status::internal(e.to_string()))?;
```

на

```rust
            let bytes = crate::builder::build_image(img).map_err(builder_error_status)?;
```

- [ ] **Step 4: Зелёный прогон**

Run: `cargo test -p uefi-engine rpc::`
Expected: PASS.

- [ ] **Step 5: Verify + commit**

Run: `cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): map RecompressionUnsupported to failed_precondition in flush_image"
```

---

### Task 6: Real-image acceptance — remove UI внутри LZMA

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (2 helper + тест)

**Interfaces:**
- Consumes: recompress-ветка (Task 3), `ops::remove`, `parse_image`/`build_image`, `is_recompressable_lzma_guid`.
- Produces: acceptance-тест issue IV (спека §8): мутация внутри LZMA материализуется, длина сохранена, вне FV1 байт-идентично.

- [ ] **Step 1: Добавить helpers и тест**

В `tests/real_image.rs` после `scan_ffs_files` добавить:

```rust
fn find_guided_ui_target(
    node: &FfsNode,
    path: &mut Vec<usize>,
    out: &mut Option<(Vec<usize>, String)>,
) {
    if out.is_some() {
        return;
    }
    if node.node_type == FfsType::Section
        && node.subtype == EFI_SECTION_GUID_DEFINED
        && matches!(
            &node.parsing_data,
            ParsingData::GuidedSection(d)
                if uefi_engine::ffs::is_recompressable_lzma_guid(&d.guid)
        )
    {
        for (i, child) in node.children.iter().enumerate() {
            if child.node_type == FfsType::Section
                && child.subtype == uefi_engine::ffs::EFI_SECTION_UI
            {
                let units: Vec<u16> = child
                    .body
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let name = String::from_utf16_lossy(&units)
                    .trim_end_matches('\0')
                    .to_string();
                let mut ui_path = path.clone();
                ui_path.push(i);
                *out = Some((ui_path, name));
                return;
            }
        }
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        find_guided_ui_target(child, path, out);
        path.pop();
        if out.is_some() {
            return;
        }
    }
}

fn collect_ui_names(node: &FfsNode, out: &mut Vec<String>) {
    if node.node_type == FfsType::Section && node.subtype == uefi_engine::ffs::EFI_SECTION_UI {
        let units: Vec<u16> = node
            .body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        out.push(
            String::from_utf16_lossy(&units)
                .trim_end_matches('\0')
                .to_string(),
        );
    }
    for child in &node.children {
        collect_ui_names(child, out);
    }
}
```

В конец файла добавить:

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_recompress_remove_ui_inside_lzma() {
    use uefi_engine::builder::build_image;
    use uefi_engine::ops::remove;
    use uefi_engine::types::{ImageMode, Target};

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let mut found = None;
    let mut path = vec![];
    find_guided_ui_target(&img.root, &mut path, &mut found);
    let (ui_path, ui_name) = found.expect("guided LZMA section with a UI child");
    assert!(!ui_name.is_empty());

    let mut before_names = vec![];
    collect_ui_names(&img.root, &mut before_names);
    let occurrences_before = before_names.iter().filter(|n| **n == ui_name).count();
    assert!(occurrences_before >= 1);

    remove(&mut img.root, &Target::Path(ui_path)).unwrap();

    let built = build_image(&img).expect("build_image after remove inside LZMA");
    assert_eq!(
        built.len(),
        data.len(),
        "total flash length must be preserved"
    );
    assert_eq!(
        &built[..0x890000],
        &data[..0x890000],
        "bytes before FV1 must be untouched"
    );
    assert_eq!(
        &built[0xd60000..],
        &data[0xd60000..],
        "bytes after FV1 must be untouched"
    );

    let re_img = parse_image(&built, ImageMode::Read, "img1", "s1").expect("re-parse");
    let mut after_names = vec![];
    collect_ui_names(&re_img.root, &mut after_names);
    let occurrences_after = after_names.iter().filter(|n| **n == ui_name).count();
    assert_eq!(
        occurrences_after,
        occurrences_before - 1,
        "removed UI section '{ui_name}' must be materialized as absent"
    );
}
```

(Границы 0x890000/0xd60000 — FV1 из probe-верификации `000b6bd`; первый guided-узел с UI-ребёнком при DFS лежит в FV1, т.к. единственный файл FV0 — RAW без секций.)

- [ ] **Step 2: Прогон (verification — задачи 1–3 уже дали поведение)**

Run: `UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin cargo test -p uefi-engine --test real_image real_image_recompress_remove_ui_inside_lzma -- --ignored`
Expected: PASS. Если FAIL — не ослаблять ассерты: разбирать (systematic-debugging), причина скорее всего в dispatch-порядке `build_section`.

- [ ] **Step 3: Регрессии**

Run: `UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin cargo test -p uefi-engine --test real_image -- --ignored`
Expected: PASS все, ОБЯЗАТЕЛЬНО включая `real_image_full_flash_round_trip` и `real_image_full_flash_repatch_stability` (clean-дерево не пересжимается никогда) и пост-фазовые HII-тесты (с их текущими ожиданиями; visibility-тест ещё ждёт `MutationBehindCompression` — flip в Task 7).

- [ ] **Step 4: Verify + commit**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check`

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): real-image acceptance — remove UI inside LZMA materializes via recompression"
```

---

### Task 7: Сужение HII-гейта + тест-флипы + docs

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (гейт + Display `MutationBehindCompression` + тест)
- Modify: `crates/uefi-engine/tests/real_image.rs` (visibility-тест → `NotASetupItem`)
- Modify: `docs/superpowers/specs/2026-08-14-hii-pe-resource-extraction-design.md` (§6-поправка)
- Modify: `TODO.md` (закрытие пунктов)

**Interfaces:**
- Consumes: пост-фазовые гейты `set_item_visibility` (план фазы 6 Task 7), `is_recompressable_lzma_guid` (Task 2), тест-хелперы `mk_node`/`FILE_GUID_STR` из `hii/mod.rs`.
- Produces: end-state гейта (спека §5): `MutationBehindCompression` только для непересжимаемых обёрток; HNX99TF-ожидание `NotASetupItem`.

- [ ] **Step 1: Failing-тест (hii)**

В `hii/mod.rs`, модуль `tests`, добавить:

```rust
    #[test]
    fn set_item_visibility_allows_lzma_backed_wrapper() {
        let mut inner = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        inner.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = 0x02;
        wrapper.parsing_data = crate::types::ParsingData::GuidedSection(
            crate::types::GuidedSectionParsingData {
                guid: crate::ffs::lzma_guid(),
                dictionary_size: 0x0080_0000,
            },
        );
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
        let body_before = image.root.children[0].children[0].children[0].children[0].body.clone();
        set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .expect("lzma-backed wrapper is recompressable; mutation must be allowed");
        let body_after = image.root.children[0].children[0].children[0].children[0].body;
        assert_ne!(body_before, body_after, "unsuppress must patch the form body");
    }
```

- [ ] **Step 2: Красный прогон**

Run: `cargo test -p uefi-engine set_item_visibility_allows`
Expected: FAIL — текущий гейт возвращает `MutationBehindCompression` для любого предка 0x02.

- [ ] **Step 3: Реализация**

В `hii/mod.rs` в `set_item_visibility` заменить блок предка-скана на:

```rust
    let mut ancestor = &image.root;
    for &i in &path[..path.len() - 1] {
        ancestor = &ancestor.children[i];
        if ancestor.node_type == FfsType::Section
            && (ancestor.subtype == EFI_SECTION_COMPRESSION
                || ancestor.subtype == EFI_SECTION_GUID_DEFINED)
            && !matches!(
                &ancestor.parsing_data,
                crate::types::ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid)
            )
        {
            return Err(HiiError::MutationBehindCompression);
        }
    }
```

Display-текст варианта `MutationBehindCompression` заменить на:

```rust
    #[error("target is behind a compressed/guided section that cannot be recompressed (Tiano, LZMAF86, standard compression, unknown GUID)")]
    MutationBehindCompression,
```

- [ ] **Step 4: Зелёный прогон + флип real-image теста**

Run: `cargo test -p uefi-engine hii::`
Expected: PASS все, включая пост-фазовый `set_item_visibility_refuses_mutation_behind_compression` (wrapper без `parsing_data` → `ParsingData::None` → гейт по-прежнему срабатывает).

В `tests/real_image.rs` в `real_image_hii_form_visibility_round_trip` заменить блок вызова на:

```rust
    let err = set_item_visibility(&mut img, &form_id, true).expect_err(
        "HNX99TF HII targets are PE32 sections behind LZMA; gate 2 passes (recompression available), gate 3 must refuse",
    );
    assert!(matches!(err, uefi_engine::hii::HiiError::NotASetupItem));
```

Run: `UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin cargo test -p uefi-engine --test real_image real_image_hii_form_visibility_round_trip -- --ignored`
Expected: PASS.

- [ ] **Step 5: Verify + commit (код)**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt -p uefi-engine -- --check && UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin cargo test -p uefi-engine --test real_image -- --ignored`

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/tests/real_image.rs
git commit -m "feat(uefi-engine): narrow HII compression gate to non-recompressable wrappers (LZMA mutations allowed)"
```

- [ ] **Step 6: Docs-коммит**

В `docs/superpowers/specs/2026-08-14-hii-pe-resource-extraction-design.md`, раздел §6, сразу после абзаца о втором гейте (тексте про `MutationBehindCompression`) добавить:

```markdown
> **Поправка фазы 7** (рекомпрессия, спека
> `2026-08-14-lzma-recompression-design.md` §5, реализована после фазы 6):
> второй гейт сужен — `MutationBehindCompression` возвращается только для
> обёрток, не являющихся GUIDed plain-LZMA (`is_recompressable_lzma_guid`);
> за LZMA-обёрткой мутация разрешена (билдер пересожмёт). Ожидание
> real-image теста `real_image_hii_form_visibility_round_trip` изменено на
> `NotASetupItem` (целевой узел — PE32, третий гейт).
```

В `TODO.md`:

1. Пункт «Спека: рекомпрессия LZMA/GUID_DEFINED в билдере» (раздел issue IV) — отметить `[x]` и дописать: «закрыто: спека `2026-08-14-lzma-recompression-design.md` + план `2026-08-14-lzma-recompression.md`».
2. Пункт «Минимум: BuilderError на мутации внутри compression barrier» — `[x]`, дописать: «закрыто фазой рекомпрессии: `BuilderError::RecompressionUnsupported` (Tiano/F86/COMPRESSION/unknown), LZMA — recompress».
3. Пункт «Минимум: `ops::rebuild` не перетирает `Action::Remove`» — `[x]`, дописать: «закрыто фазой рекомпрессии (no-op guard)».
4. Пункт «ENGINE: гейт `Action::Remove` в `ops::rebuild` и `ops::replace`» (issue II-ter) — переформулировать: `[x]` для rebuild (no-op guard, фаза рекомпрессии); для replace семантика «воскрешает узел» задокументирована в спеке рекомпрессии §6 — опциональный явный отказ остаётся отдельным пунктом `[ ]`.
5. Пункт «`ops::rebuild` перетирает `Action::Remove`» (issue IV, «Связанные баги») — `[x]`, дописать: «закрыто вместе с issue II-ter (фаза рекомпрессии)».

```bash
git add docs/superpowers/specs/2026-08-14-hii-pe-resource-extraction-design.md TODO.md
git commit -m "docs: phase-6 spec gate amendment + TODO close-out (issue IV full+minimum, II-ter rebuild guard)"
```

---

### Task 8: Финальная верификация

- [ ] **Step 1: Полный прогон**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
UEFIPATCHER_TEST_FW=refs/fw/HNX99TF_200525_original_E5C88C6F.bin cargo test -p uefi-engine --test real_image -- --ignored
cargo test -p uefi-cli
```

Expected: всё зелёное; в real_image-наборе `real_image_recompress_remove_ui_inside_lzma` PASS, `real_image_hii_form_visibility_round_trip` PASS с `NotASetupItem`.

- [ ] **Step 2: Сверка спеки**

Пройти по `2026-08-14-lzma-recompression-design.md` §2 (цели 1–6), §4.1–4.6, §5, §8 — каждая строка отражена в задачах 1–7. Расхождения — чинить кодом/тестами, не спекой (спека утверждена).
