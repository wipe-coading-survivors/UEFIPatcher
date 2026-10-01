# tse-unhide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Пользовательская операция снятия hide-маркера AMITSE (зануление stride-записи в TSE PE) + $SPF/stride-инвентарь как recon, engine→RPC→CLI→TUI.

**Architecture:** Новый модуль `hii/tse.rs` (скан stride-блоков P2 + операция `tse_unhide` с root-фильтром дискриминации + `tse_report`); читатели formset-blob/var-catalog добавляются в `hii/spf.rs` по пинн-слотам хедера (Task 2, ДО unhide — union known_formsets требует scan_formsets); поверхности — RPC (proto+server), CLI-группа `tse`, TUI `:tse`/`:tse-unhide`. Приёмка — байт-паритет с артефактом v3 на 450x.

**Tech Stack:** Rust workspace (edition 2024), uguid (`Guid::from_bytes`/`to_bytes`, Eq+Hash+Copy), tonic RPC, clap CLI, существующие паттерны uefi-engine.

**Спека:** `docs/superpowers/specs/2026-10-02-tse-unhide-design.md` (включая правку контракта дискриминации 2026-10-02: root-фильтр + `--block-offset`).

## Global Constraints

- Module-first rule: `pub mod tse;` в `hii/mod.rs` В ТОМ ЖЕ ШАГЕ, что создание файла модуля, ДО запуска тестов.
- TDD порядок (AGENTS.md п.5): тесты (+заглушки `todo!()`) → падающий `cargo test` → реализация → зелень → коммит. Каждый шаг — отдельное действие.
- Без narration-комментариев; короткие rustdoc-контракты `///` на pub-функциях со ссылкой на спеку.
- Чек-лист HII (AGENTS.md): bounds checked; целочисленные сужения — try-конверсия с диагностикой (на recon-полях proto допустима сатурация `unwrap_or(u32::MAX)` — задокументировано здесь); мутатор — snapshot-rollback; порядок ошибок по спеке §2.2 (NotFound модуль → NotWritable → NotFound запись/InvalidSchema).
- `HiiError::NotFound` — **unit-вариант** (`HiiError::NotFound`, без payload; сервер обогащает сообщение ctx'ом через `hii_error_status_ctx`). Инвентарь блоков в отказах — через `tracing::warn!`, не через текст NotFound.
- Гейты после каждой задачи: `cargo test -p <crate>` и `cargo clippy -p <crate> --all-targets -- -D warnings`; финальный гейт цикла — `cargo test --all && cargo clippy --all --all-targets -- -D warnings && cargo fmt --all -- --check`.
- Один коммит на задачу; сообщения — из плана.
- Референс-образы (в тестах — только относительные пути из CARGO_MANIFEST_DIR):
  - сток 450x: `../../../refs/fw/450x.bin`
  - артефакт v3: `../../../refs/amibcp/450x-intelrcsetup-tse-unhide-v3.bin`
  - универсальность: `../../../refs/amibcp/{C275D4I3.20,mz32-ar0-RBU.rom,226D2IL3.30}`, `../../../refs/fw/X10DRH1_816.bin`
- Пинн-данные 450x: блоки PE — фаза-1 `@0x1a80` (5× 7B59104A: 10001,10002,10009,10010,10012), hide `@0x1b40` ({EC87D643,1},{7B59104A,10000}), header-запись `@0x1ba0` ({9204ECBE,fid=0} — НЕ матчится сканом), бар `@0x1bc0` (9 записей, вкл. {EC87D643,1} и некорневые 10001/2049). Root-карта IFR: EC87D643→1, 7B59104A→10000, 01239999→2049, 932D37B0→1, 80E1202E→1.
- $SPF слоты (оффсеты от magic `$SPF`): +0x2c→pages table; сама таблица: **count u32 @+0x60 (SPF_PAGE_COUNT_OFFSET), указатели u32 @+0x64+4·i (SPF_PAGE_TABLE_OFFSET)**; +0x34→var catalog; +0x40→formsets blob. Formset-запись: GUID(16)+u32@+0x10, stride 0x14, оффсеты от начала блоба. Var-запись: len 0x7C, имя UCS-2 @+0x10..+0x60, attrs u32 @+0x60, u32 @+0x64, size u32 @+0x78. Page-запись: fsIdx u16 @+0x08, formId u16 @+0x0A, header 0x20.
- Ключевые существующие API: `crate::ops::mark_rebuild_to_root_by_path(root: &mut FfsNode, path: &[usize])` (ops.rs:137, pub); `crate::hii::with_rollback` (mod.rs, pub(crate)); `crate::hii::{node_at, node_at_mut}`; `crate::hii::HiiError`; `crate::hii::forms::collect_forms(&Image) -> Vec<uefi_proto::FormInfo>` (formset_guid String UPPER, form_id_ifr u32); `hii::spf::container_start`; `crate::ffs::EFI_SECTION_PE32`; `uguid::Guid::try_parse`/`from_bytes([u8;16])`/`to_bytes()`; `crate::guid_to_upper_string(&Guid)`. Секции subtype 0x19 (RAW) парсятся как form-пакеты (forms.rs:103-113).
- CLI-конвенции: `cli_sock: Option<&str>` (commands/nvar.rs:9), формат — глобальный `--format` → `output::OutputFormat` (main.rs:15-16), parse-тесты через `Cli::try_parse_from` (main.rs:907).
- TUI: `execute_command` кладёт команду в `parts[0]` → для top-level `:tse-unhide GUID FID` аргументы с `parts.get(1)/get(2)` (ср. `:open` :658; `:hii …` использует get(2), т.к. get(1) — подкоманда).

---

### Task 1: `hii/tse.rs` — скан stride-блоков

**Files:**
- Create: `crates/uefi-engine/src/hii/tse.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (добавить `pub mod tse;` после `pub mod string_pack;`)

**Interfaces:**
- Produces: `pub const AMITSE_GUID_STR / STRIDE_ENTRY_SIZE`; `pub struct StrideEntry { pub guid: Guid, pub form_id: u64 }`; `pub struct StrideBlock { pub pe_offset: usize, pub entries: Vec<StrideEntry> }`; `pub fn scan_stride_blocks(pe: &[u8], known_formsets: &HashSet<Guid>) -> Vec<StrideBlock>`; (crate) `fn stride_series_at(pe: &[u8], off: usize) -> Option<Vec<StrideEntry>>`.

- [ ] **Step 1: Создать модуль с тестами и заглушками + объявить `pub mod tse;`**

`crates/uefi-engine/src/hii/tse.rs` — тесты и структуры; тела скан-функций — заглушки `todo!()`:

```rust
//! AMITSE setup browser: stride-таблицы `{GUID, u64 form-id, u64 0}`
//! в PE-теле (P2 analyzer-patterns) и операция снятия hide-маркера.
//! Спека 2026-10-02-tse-unhide-design.md.

use std::collections::HashSet;

use uguid::Guid;

pub const AMITSE_GUID_STR: &str = "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A";
pub const STRIDE_ENTRY_SIZE: usize = 0x20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrideEntry {
    pub guid: Guid,
    pub form_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrideBlock {
    pub pe_offset: usize,
    pub entries: Vec<StrideEntry>,
}

fn stride_series_at(_pe: &[u8], _off: usize) -> Option<Vec<StrideEntry>> {
    todo!()
}

/// Серии ≥2 записей `{GUID, u64 form-id, u64 0}` шагом 0x20 с нулевым
/// терминатором; блок валиден, если ВСЕ его GUID-ы — формсеты образа
/// (known_formsets). Скан по границам 0x20 от нуля; блоки могут
/// перекрываться (хвост бара = динамика) — это отражается в инвентаре.
/// Спека tse-unhide §2.1.
pub fn scan_stride_blocks(_pe: &[u8], _known_formsets: &HashSet<Guid>) -> Vec<StrideBlock> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(guid: &str, fid: u64) -> Vec<u8> {
        let g = Guid::try_parse(guid).unwrap();
        let mut b = g.to_bytes().to_vec();
        b.extend_from_slice(&fid.to_le_bytes());
        b.extend_from_slice(&[0u8; 8]);
        b
    }

    fn term() -> Vec<u8> {
        vec![0u8; 0x20]
    }

    fn guid_set(items: &[&str]) -> HashSet<Guid> {
        items.iter().map(|s| Guid::try_parse(s).unwrap()).collect()
    }

    const A: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";
    const B: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";

    #[test]
    fn scan_finds_block_with_terminator() {
        let mut pe = vec![0x11u8; 0x40];
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 10000));
        pe.extend(term());
        pe.extend(vec![0x22u8; 0x10]);
        let blocks = scan_stride_blocks(&pe, &guid_set(&[A, B]));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].pe_offset, 0x40);
        assert_eq!(blocks[0].entries.len(), 2);
        assert_eq!(blocks[0].entries[0].form_id, 1);
    }

    #[test]
    fn scan_rejects_foreign_guid_block() {
        let mut pe = Vec::new();
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 2));
        pe.extend(term());
        assert!(scan_stride_blocks(&pe, &guid_set(&[A])).is_empty());
    }

    #[test]
    fn scan_requires_two_entries_and_terminator() {
        let mut one = entry(A, 1);
        one.extend(term());
        assert!(scan_stride_blocks(&one, &guid_set(&[A])).is_empty());
        let mut no_term = Vec::new();
        no_term.extend(entry(A, 1));
        no_term.extend(entry(A, 2));
        assert!(scan_stride_blocks(&no_term, &guid_set(&[A])).is_empty());
        let mut bad_tail = Vec::new();
        bad_tail.extend(entry(A, 1));
        bad_tail.extend(entry(A, 2));
        let mut last = bad_tail.split_off(0x20);
        last[24] = 1;
        bad_tail.extend(last);
        bad_tail.extend(term());
        assert!(scan_stride_blocks(&bad_tail, &guid_set(&[A])).is_empty());
        let mut big_fid = Vec::new();
        big_fid.extend(entry(A, 0x10000));
        big_fid.extend(entry(A, 2));
        big_fid.extend(term());
        assert!(scan_stride_blocks(&big_fid, &guid_set(&[A])).is_empty());
    }

    #[test]
    fn scan_reports_overlapping_tail_block() {
        let mut pe = Vec::new();
        pe.extend(entry(B, 10001));
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 2));
        pe.extend(term());
        let blocks = scan_stride_blocks(&pe, &guid_set(&[A, B]));
        assert_eq!(blocks.len(), 2, "бар с 0 и хвост-блок с 0x20");
        assert_eq!(blocks[0].pe_offset, 0);
        assert_eq!(blocks[1].pe_offset, 0x20);
    }
}
```

В `hii/mod.rs` после `pub mod string_pack;` добавить `pub mod tse;`.

- [ ] **Step 2: Запустить тесты — RED**

Run: `cargo test -p uefi-engine --lib tse:: 2>&1 | tail -5`
Expected: 4 теста FAILED/panicked (`not yet implemented` — todo!()). Если «0 passed; 0 filtered» — модуль не объявлен (module-first нарушен), вернуться.

- [ ] **Step 3: Реализация (GREEN)** — заменить `todo!()`-тела:

```rust
fn stride_series_at(pe: &[u8], off: usize) -> Option<Vec<StrideEntry>> {
    let mut entries = Vec::new();
    let mut p = off;
    while p + STRIDE_ENTRY_SIZE <= pe.len() {
        let rec = &pe[p..p + STRIDE_ENTRY_SIZE];
        if rec.iter().all(|&b| b == 0) {
            return Some(entries);
        }
        let form_id = u64::from_le_bytes(rec[16..24].try_into().unwrap());
        let tail = u64::from_le_bytes(rec[24..32].try_into().unwrap());
        if tail != 0 || form_id == 0 || form_id >= 0x10000 {
            return None;
        }
        entries.push(StrideEntry {
            guid: Guid::from_bytes(rec[..16].try_into().unwrap()),
            form_id,
        });
        p += STRIDE_ENTRY_SIZE;
    }
    None
}

pub fn scan_stride_blocks(pe: &[u8], known_formsets: &HashSet<Guid>) -> Vec<StrideBlock> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + STRIDE_ENTRY_SIZE <= pe.len() {
        if let Some(entries) = stride_series_at(pe, off)
            && entries.len() >= 2
            && entries.iter().all(|e| known_formsets.contains(&e.guid))
        {
            out.push(StrideBlock {
                pe_offset: off,
                entries,
            });
        }
        off += STRIDE_ENTRY_SIZE;
    }
    out
}
```

Run: `cargo test -p uefi-engine --lib tse::` — Expected: `4 passed`.

- [ ] **Step 4: Гейты + коммит**

```bash
cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/tse.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): tse — скан stride-блоков AMITSE PE (tse-unhide §2.1)"
```

---

### Task 2: `spf.rs` — читатели formset-blob, каталога переменных, pages

Выполняется ДО `tse_unhide`: union `known_formsets` (спека §2.1) требует `scan_formsets`.

**Files:**
- Modify: `crates/uefi-engine/src/hii/spf.rs`

**Interfaces:**
- Consumes: `container_start` (существует).
- Produces: `pub const SPF_SLOT_PAGES: usize = 0x2c; pub const SPF_SLOT_VAR_CATALOG: usize = 0x34; pub const SPF_SLOT_FORMSETS: usize = 0x40; pub const SPF_FORMSET_ENTRY_SIZE: usize = 0x14; pub const SPF_VAR_ENTRY_SIZE: usize = 0x7C;`; `pub struct SpfFormsetEntry { pub guid: Guid, pub raw_u32: u32 }`; `pub struct SpfVarEntry { pub guid: Guid, pub name: String, pub attrs: u32, pub size: u32 }`; `pub fn header_slot(body: &[u8], slot: usize) -> Option<u32>`; `pub fn scan_formsets(body: &[u8]) -> Vec<SpfFormsetEntry>`; `pub fn scan_var_catalog(body: &[u8]) -> Vec<SpfVarEntry>`; `pub fn pages_fs_counts(body: &[u8]) -> Vec<(u16, u32)>`; `pub fn pages_count(body: &[u8]) -> u32`.

- [ ] **Step 1: Тесты + заглушки (RED)** — в mod tests spf.rs добавить (хелперы `put_u32/put_u16/write_page` — локальные; `Guid` импортировать, если ещё нет):

```rust
    fn put_u32(b: &mut [u8], off: usize, v: u32) {
        b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn put_u16(b: &mut [u8], off: usize, v: u16) {
        b[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn write_page(b: &mut [u8], off: usize, fs_idx: u16, form_id: u16) {
        put_u16(b, off + 0x08, fs_idx);
        put_u16(b, off + 0x0A, form_id);
    }

    fn spf_fixture() -> Vec<u8> {
        let m = SPF_CONTAINER_BASE; // 0x10 — magic в теле секции
        let mut b = vec![0u8; m + 0x500];
        b[m..m + 4].copy_from_slice(b"$SPF");
        put_u32(&mut b, m + SPF_SLOT_FORMSETS, 0x200);
        put_u32(&mut b, m + SPF_SLOT_VAR_CATALOG, 0x300);
        put_u32(&mut b, m + SPF_SLOT_PAGES, 0x60);
        put_u32(&mut b, m + SPF_PAGE_COUNT_OFFSET, 2);
        put_u32(&mut b, m + SPF_PAGE_TABLE_OFFSET, 0x100);
        put_u32(&mut b, m + SPF_PAGE_TABLE_OFFSET + 4, 0x120);
        write_page(&mut b, m + 0x100, 0, 1);
        write_page(&mut b, m + 0x120, 1, 2);
        put_u32(&mut b, m + 0x200, 2);
        put_u32(&mut b, m + 0x204, 0x20);
        put_u32(&mut b, m + 0x208, 0x34);
        let g1 = Guid::try_parse("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        b[m + 0x220..m + 0x230].copy_from_slice(&g1.to_bytes());
        put_u32(&mut b, m + 0x230, 7);
        let gv = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        b[m + 0x310..m + 0x320].copy_from_slice(&gv.to_bytes());
        for (i, ch) in "Setup".encode_utf16().enumerate() {
            put_u16(&mut b, m + 0x310 + 0x10 + i * 2, ch);
        }
        put_u32(&mut b, m + 0x310 + 0x60, 3);
        put_u32(&mut b, m + 0x310 + 0x78, 148);
        b
    }
```

(в spf.rs тестах используется тип Guid из `uguid` — сверить фактический импорт в файле; если в тестах уже есть свой способ — применять его.)

```rust
    #[test]
    fn spf_formsets_and_var_catalog_read() {
        let b = spf_fixture();
        assert_eq!(pages_count(&b), 2);
        let fs = scan_formsets(&b);
        assert_eq!(fs.len(), 2);
        assert_eq!(fs[0].raw_u32, 7);
        let vars = scan_var_catalog(&b);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "Setup");
        assert_eq!(vars[0].attrs, 3);
        assert_eq!(vars[0].size, 148);
        assert_eq!(pages_fs_counts(&b), vec![(0, 1), (1, 1)]);
    }

    #[test]
    fn spf_readers_fail_soft_on_garbage() {
        assert!(scan_formsets(b"garbage").is_empty());
        assert!(scan_var_catalog(b"garbage").is_empty());
        assert!(pages_fs_counts(b"garbage").is_empty());
        assert_eq!(pages_count(b"garbage"), 0);
        assert_eq!(header_slot(b"garbage", SPF_SLOT_FORMSETS), None);
    }
```

Заглушки production-функций — `todo!()` на все Produces-сигнатуры.

Run: `cargo test -p uefi-engine --lib spf::` — Expected: 2 FAILED (`not yet implemented`).

- [ ] **Step 2: Реализация (GREEN)**

```rust
pub const SPF_SLOT_PAGES: usize = 0x2c;
pub const SPF_SLOT_VAR_CATALOG: usize = 0x34;
pub const SPF_SLOT_FORMSETS: usize = 0x40;
pub const SPF_FORMSET_ENTRY_SIZE: usize = 0x14;
pub const SPF_VAR_ENTRY_SIZE: usize = 0x7C;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpfFormsetEntry {
    pub guid: Guid,
    pub raw_u32: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpfVarEntry {
    pub guid: Guid,
    pub name: String,
    pub attrs: u32,
    pub size: u32,
}

/// u32-слот хедера $SPF (оффсет от magic). Пинн по 450x: +0x2c таблица
/// страниц, +0x34 каталог переменных, +0x40 блоб формсетов
/// (спека tse-unhide §2.3).
pub fn header_slot(body: &[u8], slot: usize) -> Option<u32> {
    let m = container_start(body)?;
    let p = m.checked_add(slot)?;
    let b = body.get(p..p + 4)?;
    Some(u32::from_le_bytes(b.try_into().unwrap()))
}

/// Блоб формсетов: {u32 count, u32 offs[count]} → записи
/// {GUID(16), u32@+0x10}, stride 0x14, оффсеты от начала блоба.
/// u32-поле семантически неясно (450x: 0,1,2,1,121,64,20) — raw.
/// Мусор/переполнения → пустой список (fail-soft, recon-канал).
pub fn scan_formsets(body: &[u8]) -> Vec<SpfFormsetEntry> {
    read_indexed_blob(body, SPF_SLOT_FORMSETS, SPF_FORMSET_ENTRY_SIZE, |rec| {
        Some(SpfFormsetEntry {
            guid: Guid::from_bytes(rec[..16].try_into().unwrap()),
            raw_u32: u32::from_le_bytes(rec[16..20].try_into().unwrap()),
        })
    })
}

/// Каталог переменных: записи 0x7C — GUID, имя UCS-2 @+0x10..+0x60,
/// attrs @+0x60, size @+0x78 (P3 analyzer-patterns, спека §2.3).
pub fn scan_var_catalog(body: &[u8]) -> Vec<SpfVarEntry> {
    read_indexed_blob(body, SPF_SLOT_VAR_CATALOG, SPF_VAR_ENTRY_SIZE, |rec| {
        let units: Vec<u16> = rec[0x10..0x60]
            .chunks_exact(2)
            .take_while(|c| c != &[0, 0])
            .map(|c| u16::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Some(SpfVarEntry {
            guid: Guid::from_bytes(rec[..16].try_into().unwrap()),
            name: String::from_utf16_lossy(&units),
            attrs: u32::from_le_bytes(rec[0x60..0x64].try_into().unwrap()),
            size: u32::from_le_bytes(rec[0x78..0x7C].try_into().unwrap()),
        })
    })
}

/// Число страниц: u32 @magic+SPF_PAGE_COUNT_OFFSET (0x60). Fail-soft 0.
pub fn pages_count(body: &[u8]) -> u32 {
    let Some(m) = container_start(body) else { return 0 };
    body.get(m + SPF_PAGE_COUNT_OFFSET..m + SPF_PAGE_COUNT_OFFSET + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .unwrap_or(0)
}

/// Число страниц по fsIdx: указатели таблицы @magic+0x64+4·i, у каждой
/// записи fsIdx u16 @+0x08 (запись SPF_PAGE_HEADER_SIZE). Fail-soft [].
pub fn pages_fs_counts(body: &[u8]) -> Vec<(u16, u32)> {
    let Some(m) = container_start(body) else { return Vec::new() };
    let count = pages_count(body) as usize;
    let mut counts: std::collections::BTreeMap<u16, u32> = Default::default();
    for i in 0..count {
        let tp = m + SPF_PAGE_TABLE_OFFSET + 4 * i;
        let Some(relb) = body.get(tp..tp + 4) else { break };
        let rel = u32::from_le_bytes(relb.try_into().unwrap()) as usize;
        let Some(page) = body.get(m + rel..m + rel + SPF_PAGE_HEADER_SIZE) else { break };
        let fs = u16::from_le_bytes(page[0x08..0x0A].try_into().unwrap());
        *counts.entry(fs).or_default() += 1;
    }
    counts.into_iter().collect()
}

fn read_indexed_blob<T>(
    body: &[u8],
    slot: usize,
    entry_size: usize,
    parse: impl Fn(&[u8]) -> Option<T>,
) -> Vec<T> {
    let Some(m) = container_start(body) else { return Vec::new() };
    let Some(blob) = header_slot(body, slot) else { return Vec::new() };
    let Some(base) = m.checked_add(blob as usize) else { return Vec::new() };
    let Some(h) = body.get(base..base + 4) else { return Vec::new() };
    let count = u32::from_le_bytes(h.try_into().unwrap()) as usize;
    let mut out = Vec::new();
    for i in 0..count {
        let Some(op) = body.get(base + 4 + i * 4..base + 8 + i * 4) else { break };
        let rel = u32::from_le_bytes(op.try_into().unwrap()) as usize;
        let Some(rec) = body.get(base + rel..base + rel + entry_size) else { continue };
        if let Some(v) = parse(rec) {
            out.push(v);
        }
    }
    out
}
```

(Проверить импорт `uguid::Guid` в spf.rs — добавить при отсутствии.)

Run: `cargo test -p uefi-engine --lib spf::` — Expected: PASS (новые 2 + существующие).

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/spf.rs
git commit -m "feat(engine): spf — читатели formset-blob, каталога переменных, pages (tse-unhide §2.3)"
```

---

### Task 3: `tse_unhide` — root-фильтр, guard, мутация

**Files:**
- Modify: `crates/uefi-engine/src/hii/tse.rs`

**Interfaces:**
- Consumes: `scan_stride_blocks` (Task 1), `spf::scan_formsets` (Task 2), `collect_forms`, `with_rollback`, `node_at`/`node_at_mut`, `mark_rebuild_to_root_by_path`.
- Produces: `pub struct UnhideOutcome { pub pe_offset: usize, pub entry_pe_offset: usize, pub formset_guid: Guid, pub form_id: u16 }`; `pub fn tse_unhide(image: &mut Image, formset_guid: &Guid, form_id: u16, block_pe_offset: Option<usize>) -> Result<UnhideOutcome, HiiError>`; (crate) `fn formset_roots(image: &Image) -> HashMap<Guid, u16>`; (crate) `fn known_formsets(image: &Image) -> HashSet<Guid>` (union IFR + $SPF blob); (crate) `fn amitse_file(image) -> Result<(Vec<usize>, &FfsNode), HiiError>`; (crate) `fn pe32_rel(file: &FfsNode) -> Option<Vec<usize>>`; (crate) `fn spf_section_body(file: &FfsNode) -> Option<&[u8]>`.

- [ ] **Step 1: Тесты + заглушки (RED)** — добавить в `mod tests` tse.rs:

```rust
    use crate::hii::HiiError;
    use crate::types::{Action, FfsNode, FfsType, Image, ImageMode};

    const PE_GUID: &str = "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A";

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: crate::types::ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    /// PE-тело: [pad 0x40][hide: {A#1, B#10000}][term]
    /// [бар: {B#10001, A#1, B#2049}][term]. Скан даёт 3 блока
    /// (hide @0x40, бар @0xA0, перекрывающийся хвост @0xC0).
    fn tse_pe_body() -> Vec<u8> {
        let mut pe = vec![0x11u8; 0x40];
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 10000));
        pe.extend(term());
        pe.extend(entry(B, 10001));
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 2049));
        pe.extend(term());
        pe
    }

    fn g_opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn g_form(id: u16) -> Vec<u8> {
        g_opcode(r_efi::hii::IFR_FORM_OP, true, &[id.to_le_bytes(), 7u16.to_le_bytes()].concat())
    }

    fn g_end() -> Vec<u8> {
        vec![r_efi::hii::IFR_END_OP, 0x02]
    }

    /// Форм-пакет формсета `guid` с формами `forms`; закрывающий
    /// FORM_SET END обязателен (иначе formset_spans None — прецедент
    /// f7f6500). По образцу forms_pkg в hii/mod.rs:3552.
    fn forms_pkg_for(guid: &str, forms: &[u16]) -> Vec<u8> {
        let g = Guid::try_parse(guid).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&7u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0);
        let mut ifr = g_opcode(r_efi::hii::IFR_FORM_SET_OP, true, &p);
        for &f in forms {
            ifr.extend(g_form(f));
            ifr.extend(g_end());
        }
        ifr.extend(g_end());
        let total = 4 + ifr.len();
        let mut pkg = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend_from_slice(&ifr);
        pkg
    }

    /// Образ: Volume > File(B1DA0ADF) > [PE32 leaf, Section 0x19
    /// (forms_pkg_for(A, &[1,5])), Section 0x19 (forms_pkg_for(B,
    /// &[10000,10001,2049]))]. Root-карта: A→1, B→10000.
    fn root_image(pe: Vec<u8>, mode: ImageMode) -> Image {
        let pe32 = {
            let mut n = mk_node(FfsType::Section, pe, vec![]);
            n.subtype = crate::ffs::EFI_SECTION_PE32;
            n
        };
        let fs_a = {
            let mut n = mk_node(FfsType::Section, forms_pkg_for(A, &[1, 5]), vec![]);
            n.subtype = 0x19;
            n
        };
        let fs_b = {
            let mut n = mk_node(FfsType::Section, forms_pkg_for(B, &[10000, 10001, 2049]), vec![]);
            n.subtype = 0x19;
            n
        };
        let file = {
            let mut n = mk_node(FfsType::File, vec![], vec![pe32, fs_a, fs_b]);
            n.guid = Some(Guid::try_parse(PE_GUID).unwrap());
            n
        };
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root: mk_node(FfsType::Image, vec![], vec![mk_node(FfsType::Volume, vec![], vec![file])]),
            mode,
        }
    }

    #[test]
    fn formset_roots_from_forms_packages() {
        let img = root_image(tse_pe_body(), ImageMode::Read);
        let roots = formset_roots(&img);
        assert_eq!(roots.get(&Guid::try_parse(A).unwrap()), Some(&1));
        assert_eq!(roots.get(&Guid::try_parse(B).unwrap()), Some(&10000));
    }

    #[test]
    fn tse_unhide_zeroes_hide_entry_and_marks_rebuild() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let out = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap();
        assert_eq!(out.pe_offset, 0x40, "hide-блок (бар отфильтрован root-правилом)");
        assert_eq!(out.entry_pe_offset, 0x40);
        let file = &img.root.children[0].children[0];
        let pe32 = &file.children[0];
        assert!(pe32.body[0x40..0x60].iter().all(|&b| b == 0), "запись занулена");
        assert_eq!(pe32.action, Action::Rebuild, "leaf помечен");
        assert_eq!(file.action, Action::Rebuild, "цепочка до файла");
    }

    #[test]
    fn tse_unhide_read_mode_refused() {
        let mut img = root_image(tse_pe_body(), ImageMode::Read);
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap_err();
        assert!(matches!(err, HiiError::NotWritable));
    }

    #[test]
    fn tse_unhide_no_amitse_not_found() {
        let mut img = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root: mk_node(FfsType::Image, vec![], vec![]),
            mode: ImageMode::Write,
        };
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn tse_unhide_visible_tab_not_found() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let err = tse_unhide(&mut img, &Guid::try_parse(B).unwrap(), 10001, None).unwrap_err();
        assert!(matches!(err, HiiError::NotFound), "{err:?}");
    }

    #[test]
    fn tse_unhide_explicit_offset_bypasses_candidates() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let out = tse_unhide(&mut img, &Guid::try_parse(B).unwrap(), 10000, Some(0x60)).unwrap();
        assert_eq!(out.entry_pe_offset, 0x60);
        assert_eq!(out.pe_offset, 0x60, "экспертный режим: pe_offset == entry_pe_offset");
    }

    #[test]
    fn tse_unhide_guard_mismatch_refused() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let err = tse_unhide(&mut img, &Guid::try_parse(B).unwrap(), 10000, Some(0x80)).unwrap_err();
        assert!(matches!(err, HiiError::InvalidSchema(_)), "{err:?}");
        let file = &img.root.children[0].children[0];
        assert_eq!(file.children[0].action, Action::NoAction, "rollback: мутаций нет");
    }

    #[test]
    fn tse_unhide_ambiguous_root_blocks_refused() {
        let mut pe = tse_pe_body();
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 10000));
        pe.extend(term());
        let mut img = root_image(pe, ImageMode::Write);
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap_err();
        assert!(matches!(err, HiiError::InvalidSchema(ref m) if m.contains("ambiguous")), "{err:?}");
    }
```

Заглушки production-функций (`formset_roots/known_formsets/amitse_file/pe32_rel/spf_section_body/tse_unhide/UnhideOutcome`) — `todo!()`.

Run: `cargo test -p uefi-engine --lib tse::` — Expected: 8 FAILED (`not yet implemented`); 4 из Task 1 остаются зелёными.

- [ ] **Step 2: Реализация (GREEN)**

```rust
use std::collections::HashMap;

use crate::hii::HiiError;
use crate::types::{FfsNode, FfsType, Image, ImageMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnhideOutcome {
    pub pe_offset: usize,
    pub entry_pe_offset: usize,
    pub formset_guid: Guid,
    pub form_id: u16,
}

/// Корневая форма формсета = минимальный form_id по IFR-формам
/// формсета (collect_forms). Спека tse-unhide §2.2 п.3.
fn formset_roots(image: &Image) -> HashMap<Guid, u16> {
    let mut roots: HashMap<Guid, u16> = HashMap::new();
    for f in crate::hii::forms::collect_forms(image) {
        let Ok(g) = Guid::try_parse(&f.formset_guid) else { continue };
        let Ok(fid) = u16::try_from(f.form_id_ifr) else { continue };
        roots.entry(g).and_modify(|r| *r = (*r).min(fid)).or_insert(fid);
    }
    roots
}

/// Формсеты образа для GUID-фильтра скана: union IFR collect_forms +
/// $SPF formset-blob (спека §2.1).
fn known_formsets(image: &Image) -> HashSet<Guid> {
    let mut s: HashSet<Guid> = formset_roots(image).into_keys().collect();
    if let Ok((_, file)) = amitse_file(image)
        && let Some(body) = spf_section_body(file)
    {
        s.extend(crate::hii::spf::scan_formsets(body).into_iter().map(|f| f.guid));
    }
    s
}

fn find_file<'a>(
    node: &'a FfsNode,
    guid: &Guid,
    path: &mut Vec<usize>,
) -> Option<(Vec<usize>, &'a FfsNode)> {
    if node.node_type == FfsType::File && node.guid.as_ref() == Some(guid) {
        return Some((path.clone(), node));
    }
    for (i, c) in node.children.iter().enumerate() {
        path.push(i);
        if let Some(found) = find_file(c, guid, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

/// Путь (относительно файла) к первому PE32-листу.
fn pe32_rel(file: &FfsNode) -> Option<Vec<usize>> {
    fn walk(node: &FfsNode, path: &mut Vec<usize>) -> Option<Vec<usize>> {
        if node.node_type == FfsType::Section
            && node.subtype == crate::ffs::EFI_SECTION_PE32
            && node.children.is_empty()
        {
            return Some(path.clone());
        }
        for (i, c) in node.children.iter().enumerate() {
            path.push(i);
            if let Some(p) = walk(c, path) {
                return Some(p);
            }
            path.pop();
        }
        None
    }
    walk(file, &mut Vec::new())
}

fn amitse_file(image: &Image) -> Result<(Vec<usize>, &FfsNode), HiiError> {
    let g = Guid::try_parse(AMITSE_GUID_STR)
        .map_err(|e| HiiError::InvalidItemId(e.to_string()))?;
    find_file(&image.root, &g, &mut Vec::new()).ok_or(HiiError::NotFound)
}

fn walk_spf<'a>(node: &'a FfsNode, path: &mut Vec<usize>) -> Option<(&'a [u8], Vec<usize>)> {
    if node.node_type == FfsType::Section
        && node.children.is_empty()
        && crate::hii::spf::container_start(&node.body).is_some()
    {
        return Some((&node.body, path.clone()));
    }
    for (i, c) in node.children.iter().enumerate() {
        path.push(i);
        if let Some(found) = walk_spf(c, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

/// Тело секции $SPF в файле AMITSE (детект по magic; спека §2.3).
fn spf_section_body(file: &FfsNode) -> Option<&[u8]> {
    walk_spf(file, &mut Vec::new()).map(|(b, _)| b)
}

fn entry_bytes_match(pe: &[u8], off: usize, guid: &Guid, form_id: u16) -> bool {
    let Some(rec) = pe.get(off..off + STRIDE_ENTRY_SIZE) else {
        return false;
    };
    rec[..16] == guid.to_bytes()
        && rec[16..24] == u64::from(form_id).to_le_bytes()
        && rec[24..32] == [0u8; 8]
}

/// Зануление hide-записи `{formset_guid, form_id}` в stride-таблице TSE
/// PE. Кандидат = блок содержит запись И все записи блока — корневые
/// формы (дискриминация hide/бар; спека tse-unhide §2.2, правка
/// 2026-10-02). `block_pe_offset` — экспертный обход выбора блока,
/// byte-guard остаётся; в экспертном режиме `pe_offset == entry_pe_offset`.
/// Ошибки: NotFound (модуль/запись) → NotWritable →
/// InvalidSchema (ambiguous/guard). Мутация + Rebuild по цепочке предков,
/// snapshot-rollback при Err.
pub fn tse_unhide(
    image: &mut Image,
    formset_guid: &Guid,
    form_id: u16,
    block_pe_offset: Option<usize>,
) -> Result<UnhideOutcome, HiiError> {
    crate::hii::with_rollback(image, |image| {
        let (file_path, file) = amitse_file(image)?;
        let rel = pe32_rel(file).ok_or(HiiError::NotFound)?;
        let path: Vec<usize> = [file_path, rel].concat();
        if image.mode != ImageMode::Write {
            return Err(HiiError::NotWritable);
        }
        let (block_off, entry_pe_offset) = match block_pe_offset {
            Some(off) => {
                let pe = &crate::hii::node_at(&image.root, &path).body;
                if !entry_bytes_match(pe, off, formset_guid, form_id) {
                    return Err(HiiError::InvalidSchema(format!(
                        "guard mismatch at pe+{off:#x}"
                    )));
                }
                (off, off)
            }
            None => {
                let roots = formset_roots(image);
                let known = known_formsets(image);
                let pe = &crate::hii::node_at(&image.root, &path).body;
                let blocks = scan_stride_blocks(pe, &known);
                let mut cands: Vec<(usize, usize)> = Vec::new();
                for b in &blocks {
                    let all_roots = b.entries.iter().all(|e| {
                        roots.get(&e.guid).is_some_and(|r| u64::from(*r) == e.form_id)
                    });
                    if !all_roots {
                        continue;
                    }
                    if let Some(pos) = b.entries.iter().position(|e| {
                        e.guid == *formset_guid && e.form_id == u64::from(form_id)
                    }) {
                        cands.push((b.pe_offset, b.pe_offset + STRIDE_ENTRY_SIZE * pos));
                    }
                }
                tracing::warn!(blocks = blocks.len(), candidates = cands.len(), "tse unhide scan");
                match cands.as_slice() {
                    [] => return Err(HiiError::NotFound),
                    [single] => *single,
                    many => {
                        return Err(HiiError::InvalidSchema(format!(
                            "ambiguous: entry in {} hide-candidate blocks at {:?}",
                            many.len(),
                            many.iter().map(|(b, _)| format!("{b:#x}")).collect::<Vec<_>>()
                        )))
                    }
                }
            }
        };
        let node = crate::hii::node_at_mut(&mut image.root, &path);
        node.body[entry_pe_offset..entry_pe_offset + STRIDE_ENTRY_SIZE].fill(0);
        crate::ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
        Ok(UnhideOutcome {
            pe_offset: block_off,
            entry_pe_offset,
            formset_guid: *formset_guid,
            form_id,
        })
    })
}
```

(Заимствования: в `None`-ветке иммутабельные заёмы `roots`/`pe` заканчиваются до `node_at_mut` — NLL. `tracing` в engine уже используется — сверить импорт трейта `tracing::warn` в tse.rs: макрос доступен через полный путь.)

Run: `cargo test -p uefi-engine --lib tse::` — Expected: PASS (4 + 8 = 12).

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/tse.rs
git commit -m "feat(engine): tse_unhide — root-фильтр дискриминации, byte-guard, rollback (tse-unhide §2.2)"
```

---

### Task 4: `tse_report`

**Files:**
- Modify: `crates/uefi-engine/src/hii/tse.rs`

**Interfaces:**
- Consumes: Tasks 1-3 API (`known_formsets`, `amitse_file`, `pe32_rel`, `spf_section_body`, spf-читатели).
- Produces: `pub struct SpfSummary { pub page_count: u32, pub pages_per_formset: Vec<(u16, u32)>, pub formsets: Vec<SpfFormsetEntry>, pub vars: Vec<SpfVarEntry>, pub string_controls: usize }`; `pub struct TseReport { pub blocks: Vec<StrideBlock>, pub spf: Option<SpfSummary>, pub pe_len: usize }`; `pub fn tse_report(image: &Image) -> Result<TseReport, HiiError>`.

- [ ] **Step 1: Тесты + заглушки (RED)** — в mod tests tse.rs добавить:

```rust
    /// Минимальное $SPF-тело секции: [subtype-guid 16 б (значение не
    /// важно — детект по magic)][фикстура]. Слоты/данные — по образцу
    /// spf_fixture из Task 2, но локально (модули тестов разные).
    fn spf_section() -> FfsNode {
        let m = 0x10;
        let mut b = vec![0u8; m + 0x500];
        b[m..m + 4].copy_from_slice(b"$SPF");
        put32(&mut b, m + 0x40, 0x200); // SPF_SLOT_FORMSETS
        put32(&mut b, m + 0x34, 0x300); // SPF_SLOT_VAR_CATALOG
        put32(&mut b, m + 0x2c, 0x60);  // SPF_SLOT_PAGES
        put32(&mut b, m + 0x60, 1);     // pages count
        put32(&mut b, m + 0x64, 0x100);
        put16(&mut b, m + 0x100 + 0x08, 0);
        put32(&mut b, m + 0x200, 1);    // formsets count
        put32(&mut b, m + 0x204, 0x20);
        let g = Guid::try_parse(A).unwrap();
        b[m + 0x220..m + 0x230].copy_from_slice(&g.to_bytes());
        put32(&mut b, m + 0x230, 7);
        put32(&mut b, m + 0x300, 1);    // vars count
        put32(&mut b, m + 0x304, 0x10);
        let gv = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        b[m + 0x310..m + 0x320].copy_from_slice(&gv.to_bytes());
        for (i, ch) in "Setup".encode_utf16().enumerate() {
            put16(&mut b, m + 0x310 + 0x10 + i * 2, ch);
        }
        put32(&mut b, m + 0x310 + 0x60, 3);
        put32(&mut b, m + 0x310 + 0x78, 148);
        mk_node(FfsType::Section, b, vec![])
    }

    fn put32(b: &mut [u8], off: usize, v: u32) {
        b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn put16(b: &mut [u8], off: usize, v: u16) {
        b[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn root_image_with_spf(pe: Vec<u8>, mode: ImageMode) -> Image {
        let mut img = root_image(pe, mode);
        let vol = &mut img.root.children[0];
        let file = &mut vol.children[0];
        file.children.push(spf_section());
        img
    }

    #[test]
    fn tse_report_collects_blocks_and_spf() {
        let img = root_image_with_spf(tse_pe_body(), ImageMode::Read);
        let rep = tse_report(&img).unwrap();
        assert_eq!(rep.blocks.len(), 3, "hide @0x40 + бар @0xA0 + хвост @0xC0");
        assert_eq!(rep.blocks[0].pe_offset, 0x40);
        let s = rep.spf.as_ref().expect("spf summary");
        assert_eq!(s.page_count, 1);
        assert_eq!(s.formsets.len(), 1);
        assert_eq!(s.formsets[0].raw_u32, 7);
        assert_eq!(s.vars.len(), 1);
        assert_eq!(s.vars[0].name, "Setup");
    }

    #[test]
    fn tse_report_without_spf_section() {
        let img = root_image(tse_pe_body(), ImageMode::Read);
        let rep = tse_report(&img).unwrap();
        assert!(rep.spf.is_none());
        assert_eq!(rep.blocks.len(), 3);
    }
```

Заглушки `SpfSummary/TseReport/tse_report` — `todo!()`.

Run: `cargo test -p uefi-engine --lib tse::report` — Expected: 2 FAILED.

- [ ] **Step 2: Реализация (GREEN)**

```rust
use crate::hii::spf::{self, SpfFormsetEntry, SpfVarEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpfSummary {
    pub page_count: u32,
    pub pages_per_formset: Vec<(u16, u32)>,
    pub formsets: Vec<SpfFormsetEntry>,
    pub vars: Vec<SpfVarEntry>,
    pub string_controls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TseReport {
    pub blocks: Vec<StrideBlock>,
    pub spf: Option<SpfSummary>,
    pub pe_len: usize,
}

/// Recon-инвентарь AMITSE: stride-блоки PE (root-фильтр НЕ применяется —
/// инвентарь полный, блоки могут перекрываться) + $SPF-сводка, если у
/// файла есть секция с magic. NotFound — только если файла/PE AMITSE
/// нет. Спека tse-unhide §2.3.
pub fn tse_report(image: &Image) -> Result<TseReport, HiiError> {
    let (file_path, file) = amitse_file(image)?;
    let rel = pe32_rel(file).ok_or(HiiError::NotFound)?;
    let path: Vec<usize> = [file_path, rel].concat();
    let pe = crate::hii::node_at(&image.root, &path).body.clone();
    let known = known_formsets(image);
    let blocks = scan_stride_blocks(&pe, &known);
    let spf = spf_section_body(file).map(|b| SpfSummary {
        page_count: spf::pages_count(b),
        pages_per_formset: spf::pages_fs_counts(b),
        formsets: spf::scan_formsets(b),
        vars: spf::scan_var_catalog(b),
        string_controls: spf::scan_string_controls(b).len(),
    });
    Ok(TseReport {
        blocks,
        spf,
        pe_len: pe.len(),
    })
}
```

Run: `cargo test -p uefi-engine --lib tse::` — Expected: PASS (12 + 2 = 14).

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/tse.rs
git commit -m "feat(engine): tse_report — инвентарь stride-блоков + SPF-сводка (tse-unhide §2.3)"
```

---

### Task 5: proto + RPC-хендлеры + стабы моков + server-тесты

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-engine/src/rpc/server.rs`
- Modify: `crates/uefi-cli/tests/mock_server.rs`
- Modify: `crates/uefi-tui/tests/mock_server.rs`

**Interfaces:**
- Consumes: `tse_report`, `tse_unhide` (Tasks 3-4).
- Produces: RPC `TseReport/TseUnhide`; proto-типы `TseStrideEntry/TseStrideBlock/TseSpfFormset/TseSpfVar/TseSpfSummary/TseReportRequest/TseReportResponse/TseUnhideRequest/TseUnhideResponse`; server-хелпер `fn spf_summary_proto(s: &SpfSummary) -> TseSpfSummary`.

- [ ] **Step 1: proto — rpc-строки в сервис-блок (рядом с существующими rpc), сообщения перед `message Empty {}`**

```proto
rpc TseReport(TseReportRequest) returns (TseReportResponse);
rpc TseUnhide(TseUnhideRequest) returns (TseUnhideResponse);

message TseStrideEntry { string formset_guid = 1; uint64 form_id = 2; }
message TseStrideBlock { uint32 pe_offset = 1; repeated TseStrideEntry entries = 2; }
message TseSpfFormset { string guid = 1; uint32 raw_u32 = 2; uint32 pages = 3; }
message TseSpfVar { string guid = 1; string name = 2; uint32 attrs = 3; uint32 size = 4; }
message TseSpfSummary {
  uint32 page_count = 1;
  repeated TseSpfFormset formsets = 2;
  repeated TseSpfVar vars = 3;
  uint32 string_controls = 4;
}
message TseReportRequest  { string image_id = 1; }
message TseReportResponse {
  repeated TseStrideBlock blocks = 1;
  optional TseSpfSummary spf = 2;
  uint32 pe_len = 3;
}
message TseUnhideRequest {
  string image_id = 1;
  string formset_guid = 2;
  uint32 form_id = 3;
  optional uint32 block_pe_offset = 4;
}
message TseUnhideResponse { uint32 pe_offset = 1; uint32 entry_pe_offset = 2; }
```

Run: `cargo build -p uefi-proto` — Expected: OK.

- [ ] **Step 2: server.rs — чистый хелпер маппинга + хендлеры**

Хелпер (маппинг formsets↔pages_per_formset по индексу fsIdx — вынесен для прямого теста):

```rust
fn spf_summary_proto(s: &crate::hii::tse::SpfSummary) -> TseSpfSummary {
    TseSpfSummary {
        page_count: s.page_count,
        formsets: s
            .formsets
            .iter()
            .enumerate()
            .map(|(idx, f)| TseSpfFormset {
                guid: crate::guid_to_upper_string(&f.guid),
                raw_u32: f.raw_u32,
                pages: s
                    .pages_per_formset
                    .iter()
                    .find(|(i, _)| usize::from(*i) == idx)
                    .map(|(_, c)| *c)
                    .unwrap_or(0),
            })
            .collect(),
        vars: s
            .vars
            .iter()
            .map(|v| TseSpfVar {
                guid: crate::guid_to_upper_string(&v.guid),
                name: v.name.clone(),
                attrs: v.attrs,
                size: v.size,
            })
            .collect(),
        string_controls: u32::try_from(s.string_controls).unwrap_or(u32::MAX),
    }
}
```

(Сатурация `unwrap_or(u32::MAX)` на recon-полях — осознанное решение, размеры PE/счётчики реально < u32; для `block_pe_offset` входного — try-конверсия с invalid_argument ниже.)

Хендлеры (по образцу hii_get_value :1169 / hii_set_value :1140; `images: HashMap<String, Image>`, слот = `&mut Image`):

```rust
    async fn tse_report(
        &self,
        req: Request<TseReportRequest>,
    ) -> RpcResult<TseReportResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let rep = crate::hii::tse::tse_report(&img)
            .map_err(|e| hii_error_status_ctx(e, &r.image_id))?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, blocks = rep.blocks.len(), "tse report");
        Ok(Response::new(TseReportResponse {
            blocks: rep
                .blocks
                .into_iter()
                .map(|b| TseStrideBlock {
                    pe_offset: u32::try_from(b.pe_offset).unwrap_or(u32::MAX),
                    entries: b
                        .entries
                        .into_iter()
                        .map(|e| TseStrideEntry {
                            formset_guid: crate::guid_to_upper_string(&e.guid),
                            form_id: e.form_id,
                        })
                        .collect(),
                })
                .collect(),
            spf: rep.spf.as_ref().map(spf_summary_proto),
            pe_len: u32::try_from(rep.pe_len).unwrap_or(u32::MAX),
        }))
    }

    async fn tse_unhide(
        &self,
        req: Request<TseUnhideRequest>,
    ) -> RpcResult<TseUnhideResponse> {
        let r = req.into_inner();
        let guid = uguid::Guid::try_parse(&r.formset_guid).map_err(|e| {
            Status::invalid_argument(format!("invalid formset guid: {e}"))
        })?;
        let fid = u16::try_from(r.form_id)
            .map_err(|_| Status::invalid_argument("form_id exceeds u16"))?;
        let off = r.block_pe_offset.map(usize::from);
        self.ensure_image_loaded(&r.image_id).await?;
        let (outcome, session_id) = {
            let mut images = self.images.lock().await;
            let img = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let session_id = img.session_id.clone();
            let outcome = crate::hii::tse::tse_unhide(img, &guid, fid, off)
                .map_err(|e| hii_error_status_ctx(e, &r.formset_guid))?;
            (outcome, session_id)
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&session_id);
        tracing::info!(image_id = %r.image_id, guid = %r.formset_guid, form = fid, "tse unhide");
        Ok(Response::new(TseUnhideResponse {
            pe_offset: u32::try_from(outcome.pe_offset).unwrap_or(u32::MAX),
            entry_pe_offset: u32::try_from(outcome.entry_pe_offset).unwrap_or(u32::MAX),
        }))
    }
```

- [ ] **Step 3: server-тесты (в mod tests server.rs, по образцу чистых статус-хелперов :1600+)**

```rust
    #[test]
    fn tse_spf_summary_proto_maps_pages_by_fsidx() {
        let g = uguid::Guid::try_parse("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        let s = crate::hii::tse::SpfSummary {
            page_count: 214,
            pages_per_formset: vec![(1, 5), (2, 161)],
            formsets: vec![
                crate::hii::spf::SpfFormsetEntry { guid: uguid::Guid::from_bytes([0; 16]), raw_u32: 0 },
                crate::hii::spf::SpfFormsetEntry { guid: g, raw_u32: 2 },
                crate::hii::spf::SpfFormsetEntry { guid: uguid::Guid::from_bytes([0; 16]), raw_u32: 9 },
            ],
            vars: vec![],
            string_controls: 4,
        };
        let p = spf_summary_proto(&s);
        assert_eq!(p.formsets[0].pages, 0, "fsIdx 0 отсутствует в карте");
        assert_eq!(p.formsets[1].pages, 5, "fsIdx 1");
        assert_eq!(p.formsets[2].pages, 161, "fsIdx 2");
        assert_eq!(p.page_count, 214);
        assert_eq!(p.string_controls, 4);
    }

    #[test]
    fn tse_unhide_bad_guid_maps_invalid_argument() {
        let st = uguid::Guid::try_parse("not-a-guid").map_err(|e| {
            Status::invalid_argument(format!("invalid formset guid: {e}"))
        }).unwrap_err();
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        assert!(st.message().contains("invalid formset guid"));
    }
```

(`uguid::Guid::from_bytes([0;16])` — нулевой GUID, конструктор константный.)

- [ ] **Step 4: стабы моков (обязательны — иначе trait неполон и workspace не компилируется)**

`crates/uefi-cli/tests/mock_server.rs` (и зеркально `uefi-tui/tests/mock_server.rs`):

```rust
    async fn tse_report(
        &self,
        _req: Request<TseReportRequest>,
    ) -> Result<Response<TseReportResponse>, Status> {
        Ok(Response::new(TseReportResponse {
            blocks: vec![TseStrideBlock {
                pe_offset: 0x1b40,
                entries: vec![
                    TseStrideEntry { formset_guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(), form_id: 1 },
                    TseStrideEntry { formset_guid: "7B59104A-C00D-4158-87FF-F04D6396A915".into(), form_id: 10000 },
                ],
            }],
            spf: Some(TseSpfSummary {
                page_count: 214,
                formsets: vec![TseSpfFormset { guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(), raw_u32: 2, pages: 161 }],
                vars: vec![TseSpfVar { guid: "899407D7-99FE-43D8-9A21-79EC328CAC21".into(), name: "Setup".into(), attrs: 3, size: 148 }],
                string_controls: 4,
            }),
            pe_len: 0x48860,
        }))
    }
    async fn tse_unhide(
        &self,
        req: Request<TseUnhideRequest>,
    ) -> Result<Response<TseUnhideResponse>, Status> {
        let r = req.into_inner();
        if r.formset_guid == "00000000-0000-0000-0000-000000000000" {
            return Err(Status::not_found("no AMITSE setup browser module"));
        }
        Ok(Response::new(TseUnhideResponse {
            pe_offset: 0x1b40,
            entry_pe_offset: 0x1b40,
        }))
    }
```

(Нулевой GUID — контракт мока для варианта «нет AMITSE» из спеки §4.)

- [ ] **Step 5: Гейты + коммит**

```bash
cargo test --all && cargo clippy --all --all-targets -- -D warnings
git add crates/uefi-proto/proto/engine.proto crates/uefi-engine/src/rpc/server.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-tui/tests/mock_server.rs
git commit -m "feat(rpc): TseReport/TseUnhide — proto, хендлеры, server-тесты, стабы моков (tse-unhide §3.1)"
```

---

### Task 6: CLI — группа `tse`

**Files:**
- Create: `crates/uefi-cli/src/commands/tse.rs`
- Modify: `crates/uefi-cli/src/commands/mod.rs` (`pub mod tse;`)
- Modify: `crates/uefi-cli/src/main.rs` (группа `Tse`)
- Modify: `crates/uefi-cli/src/output.rs` (printers)
- Test: `crates/uefi-cli/src/main.rs` (parse-тесты), `crates/uefi-cli/tests/cli_integration.rs`

**Interfaces:**
- Consumes: RPC-типы Task 5; конвенции `cli_sock: Option<&str>` (nvar.rs:9), глобальный `--format` → `output::OutputFormat` (main.rs:15-16, диспатч передаёт как в `Cmd::Nvar`).
- Produces: `pub async fn report(image_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> anyhow::Result<()>`; `pub async fn unhide(image_id: &str, guid: &str, form_id: u16, block_offset: Option<usize>, cli_sock: Option<&str>) -> anyhow::Result<()>`.

- [ ] **Step 1: Parse-тесты (RED)** — в tests main.rs по образцу `parse_nvar_list_and_set_args` (:906):

```rust
    #[test]
    fn parse_tse_report_and_unhide_args() {
        let cli = Cli::try_parse_from(["uefi-cli", "tse", "report", "img-1"]).unwrap();
        assert!(matches!(cli.command, Cmd::Tse { sub: TseCmd::Report { image_id, .. } } if image_id == "img-1"));
        let cli = Cli::try_parse_from([
            "uefi-cli", "tse", "unhide", "img-1",
            "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9", "1", "--block-offset", "0x1b40",
        ])
        .unwrap();
        assert!(matches!(cli.command, Cmd::Tse { sub: TseCmd::Unhide { image_id, formset_guid, form_id, block_offset, .. } }
            if image_id == "img-1" && formset_guid == "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9" && form_id == 1
                && block_offset.as_deref() == Some("0x1b40")));
    }
```

Run: `cargo test -p uefi-cli parse_tse` — Expected: FAIL (нет Cmd::Tse).

- [ ] **Step 2: main.rs — группа + диспатч**

```rust
    #[command(about = "AMITSE setup browser: stride-tables recon and unhide")]
    Tse {
        #[command(subcommand)]
        sub: TseCmd,
    },
```

```rust
#[derive(Subcommand)]
enum TseCmd {
    #[command(about = "stride-blocks and $SPF inventory of the AMITSE module")]
    Report { image_id: String },
    #[command(about = "zero a hide-table entry {formset-guid, form-id} in the TSE PE")]
    Unhide {
        image_id: String,
        formset_guid: String,
        form_id: u16,
        #[arg(long)]
        block_offset: Option<String>,
    },
}
```

Диспатч (по образцу `Cmd::Nvar` :579; формат/сокет — из окружения матч-руки как у соседей):

```rust
        Cmd::Tse { sub } => match sub {
            TseCmd::Report { image_id } => {
                commands::tse::report(&image_id, sock.as_deref(), format).await
            }
            TseCmd::Unhide { image_id, formset_guid, form_id, block_offset } => {
                let off = match block_offset.as_deref() {
                    None => None,
                    Some(s) => Some(
                        usize::from_str_radix(s.trim_start_matches("0x"), 16)
                            .map_err(|e| anyhow::anyhow!("invalid --block-offset: {e}"))?,
                    ),
                };
                commands::tse::unhide(&image_id, &formset_guid, form_id, off, sock.as_deref()).await
            }
        },
```

- [ ] **Step 3: commands/tse.rs + printers (по образцу commands/nvar.rs; подключение клиента к сокету — скопировать каркас из nvar::list; `format: OutputFormat` — как в nvar::list)**

`report`: `TseReportRequest { image_id }` → response → печать по OutputFormat:
- Text: `stride blocks: N` + на блок `@0x1b40 (2 entries):` + строки `  GUID #form`; `$SPF: pages=214, formsets=M, vars=K, string-controls=C`; формсеты `  GUID raw=2 pages=161`; переменные `  GUID name attrs=#x size`.
- TSV: заголовок `type\toff\tformset_guid\tform_id\textra` + строки блоков/записей; секции `spf_formset\t…`, `spf_var\t…`.
- Json: `serde_json::json!({blocks: […], spf: {…}})` — по образцу существующих json-принтеров output.rs.

`unhide`: try-конверсия off → u32 (`u32::try_from(off).map_err(|_| anyhow!("--block-offset exceeds u32"))?`) → `TseUnhideRequest { image_id, formset_guid, form_id: u32::from(form_id), block_pe_offset: Some(v) }` → печать `unhidden {guid}#{form}: block @ {pe_offset:#x}, entry @ {entry_pe_offset:#x}`.

- [ ] **Step 4: Интеграционный тест с mock (cli_integration.rs)**

```rust
    #[tokio::test]
    async fn tse_report_and_unhide_roundtrip() {
        // start_mock + сессия/активный образ по образцу соседних тестов
        // tse report → stdout содержит "0x1b40" и "214"
        // tse unhide <valid-guid> 1 → stdout содержит "0x1b40"
        // tse unhide 00000000-0000-0000-0000-000000000000 1 → exit failure, "no AMITSE"
        // tse unhide not-a-guid 1 → exit failure, "invalid formset guid"
    }
```

Run: `cargo test -p uefi-cli` — Expected: PASS.

- [ ] **Step 5: Гейты + коммит**

```bash
cargo test -p uefi-cli && cargo clippy -p uefi-cli --all-targets -- -D warnings
git add crates/uefi-cli/src/commands/tse.rs crates/uefi-cli/src/commands/mod.rs crates/uefi-cli/src/main.rs crates/uefi-cli/src/output.rs crates/uefi-cli/tests/cli_integration.rs
git commit -m "feat(cli): tse report/unhide — группа команд, printers, тесты (tse-unhide §3.2)"
```

---

### Task 7: TUI — `:tse` / `:tse-unhide`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs`
- Modify: `crates/uefi-tui/tests/tui_integration.rs`

**Interfaces:**
- Consumes: RPC Task 5, стабы Task 5.
- Produces: команды `:tse` и `:tse-unhide <formset-guid> <form-id> [--block-offset <hex>]`.

- [ ] **Step 1: Интеграционный тест (RED)**

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn tse_commands_report_and_unhide() {
        // стандартный mock-сетап (по образцу varstores_cache_invalidated…)
        execute_command(&mut app, "open /dev/null", &mut client).await.unwrap();
        execute_command(&mut app, "tse", &mut client).await.unwrap();
        assert!(app.status_msg.contains("1 stride blocks"));
        execute_command(&mut app, "tse-unhide EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 1", &mut client)
            .await.unwrap();
        assert!(app.status_msg.contains("0x1b40"));
        assert!(execute_command(&mut app, "tse-unhide bad", &mut client).await.is_err());
    }
```

Run: `cargo test -p uefi-tui tse_commands` — Expected: FAIL.

- [ ] **Step 2: Реализация в commands.rs** (по образцу `:artifacts` :882 и `:hii set-value` :1102; топ-level команда → аргументы с `parts.get(1)`/`parts.get(2)`; активный образ — паттерн `app.active_image_id.clone().or_else(|| client.state.active_image_id.clone()).ok_or("no active image")?`):

```rust
        "tse" => {
            let iid = app
                .active_image_id
                .clone()
                .or_else(|| client.state.active_image_id.clone())
                .ok_or("no active image")?;
            let r = client
                .inner
                .tse_report(auth_req(&client.state, TseReportRequest { image_id: iid }))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            let spf = r.spf.as_ref()
                .map(|s| format!("spf: {} pages, {} formsets, {} vars", s.page_count, s.formsets.len(), s.vars.len()))
                .unwrap_or_else(|| "spf: none".into());
            app.status_msg = format!("{} stride blocks, {spf}", r.blocks.len());
            Ok(app.status_msg.clone())
        }
        "tse-unhide" => {
            let usage = "usage: :tse-unhide FORMSET-GUID FORM-ID [--block-offset HEX]";
            let iid = app
                .active_image_id
                .clone()
                .or_else(|| client.state.active_image_id.clone())
                .ok_or("no active image")?;
            let guid = parts.get(1).ok_or(usage)?.to_string();
            let fid: u32 = parts.get(2).ok_or(usage)?.parse().map_err(|_| usage)?;
            let off = parts
                .iter()
                .position(|p| *p == "--block-offset")
                .and_then(|i| parts.get(i + 1))
                .map(|s| usize::from_str_radix(s.trim_start_matches("0x"), 16))
                .transpose()
                .map_err(|_| usage)?;
            let off_u32 = off
                .map(u32::try_from)
                .transpose()
                .map_err(|_| "block offset exceeds u32".to_string())?;
            let r = client
                .inner
                .tse_unhide(auth_req(&client.state, TseUnhideRequest {
                    image_id: iid,
                    formset_guid: guid.clone(),
                    form_id: fid,
                    block_pe_offset: off_u32,
                }))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("unhidden {} #{} @ {:#x}", guid, fid, r.entry_pe_offset);
            Ok(app.status_msg.clone())
        }
```

Hint-бар/help-экран не трогаем (команды префиксные, как `:artifacts`).

Run: `cargo test -p uefi-tui` — Expected: PASS.

- [ ] **Step 3: Гейты + коммит**

```bash
cargo test -p uefi-tui && cargo clippy -p uefi-tui --all-targets -- -D warnings
git add crates/uefi-tui/src/commands.rs crates/uefi-tui/tests/tui_integration.rs
git commit -m "feat(tui): :tse/:tse-unhide — инвентарь и операция (tse-unhide §3.3)"
```

---

### Task 8: real-image тесты — байт-паритет v3 + универсальность

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs`

**Interfaces:**
- Consumes: `tse_report`, `tse_unhide`, `build_image`, хелперы `load_fw`/`parse_image`.

- [ ] **Step 1: Тесты (#[ignore], presence-gated; хелпер путей назвать `refs_fw` — `fw_path` уже занят в real_image.rs:11)**

```rust
fn refs_fw(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../refs/fw")
        .join(name)
}

#[ignore = "requires refs/fw/450x.bin and refs/amibcp/450x-intelrcsetup-tse-unhide-v3.bin"]
#[test]
fn real_450x_tse_unhide_byte_parity_v3() {
    let stock = refs_fw("450x.bin");
    let artifact = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../refs/amibcp/450x-intelrcsetup-tse-unhide-v3.bin");
    if !stock.exists() || !artifact.exists() {
        eprintln!("skip: no 450x fixtures");
        return;
    }
    let data = std::fs::read(&stock).unwrap();
    let mut img = parse_image(&data, ImageMode::Write, "tse", "s").unwrap();
    let g = Guid::try_parse("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
    let out = uefi_engine::hii::tse::tse_unhide(&mut img, &g, 1, None).unwrap();
    assert_eq!(out.pe_offset, 0x1b40, "hide-блок найден авто-режимом");
    assert_eq!(out.entry_pe_offset, 0x1b40);
    let built = uefi_engine::builder::build_image(&img).unwrap();
    let want = std::fs::read(&artifact).unwrap();
    assert_eq!(built.len(), want.len());
    assert_eq!(built, want, "байт-паритет с артефактом v3 (sha256 9d5f6b55…)");
}

#[ignore = "requires refs/fw/HNX99TF_200525_original_E5C88C6F.bin"]
#[test]
fn real_hnx_tse_report_runs() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "tse", "s").unwrap();
    match uefi_engine::hii::tse::tse_report(&img) {
        Ok(rep) => eprintln!("hnx: {} blocks, spf={}", rep.blocks.len(), rep.spf.is_some()),
        Err(e) => panic!("report failed: {e}"),
    }
}

#[ignore = "requires neighbor images (refs/amibcp, refs/fw)"]
#[test]
fn real_neighbors_tse_report_inventory() {
    let amibcp = |n: &str| {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../refs/amibcp")
            .join(n)
    };
    let cases = [
        ("asrock C275D4I3.20", amibcp("C275D4I3.20")),
        ("mz32", amibcp("mz32-ar0-RBU.rom")),
        ("226D2IL3.30", amibcp("226D2IL3.30")),
        ("X10DRH1", refs_fw("X10DRH1_816.bin")),
    ];
    for (name, p) in cases {
        if !p.exists() {
            eprintln!("skip {name}: {}", p.display());
            continue;
        }
        let data = std::fs::read(&p).unwrap();
        let img = parse_image(&data, ImageMode::Read, "tse", "s").unwrap();
        match uefi_engine::hii::tse::tse_report(&img) {
            Ok(rep) => eprintln!(
                "{name}: blocks={} spf={} pages={:?}",
                rep.blocks.len(),
                rep.spf.is_some(),
                rep.spf.as_ref().map(|s| s.page_count)
            ),
            Err(e) => eprintln!("{name}: {e}"),
        }
    }
}
```

Run: `cargo test -p uefi-engine --test real_image -- --ignored real_450x_tse real_hnx_tse real_neighbors --nocapture 2>&1 | tail -20` (с наличными образами) — Expected: parity PASS (главный гейт приёмки); inventory печатает факты для отчёта цикла. Если parity падает — стоп, разбираться (не ослаблять ассерт).

- [ ] **Step 2: Гейты + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(engine): real-image tse — байт-паритет v3, HNX, универсальность (tse-unhide §5)"
```

---

### Task 9: финальные гейты, TODO, отчёт

- [ ] **Step 1: Финальный гейт**

```bash
cargo test --all && cargo clippy --all --all-targets -- -D warnings && cargo fmt --all -- --check
```

- [ ] **Step 2: TODO.md — закрыть запись мини-цикла (поиск по «TSE unhide», отметить [x] + строка «Закрыто циклом tse-unhide (2026-10-02): …»).**

- [ ] **Step 3: Отчёт цикла + analyzer-patterns**

Если на образах-соседях найдены новые подтверждённые шаблоноподобные факты (отличные слоты, иные GUID AMITSE) — предложить владельцу записи в `docs/analyzer-patterns.md` списком. Инвентарь соседей — в вердикт спеки.

- [ ] **Step 4: Коммит**

```bash
git add TODO.md docs/superpowers/specs/2026-10-02-tse-unhide-design.md docs/analyzer-patterns.md
git commit -m "docs: закрытие TODO tse-unhide + вердикт спеки (инвентарь соседей)"
```

- [ ] **Step 5: Финальное ревью plan-reviewer (Kimi K3, режим final-review) — блокирующий гейт закрытия цикла (AGENTS.md).**

---

## Self-Review (после правок по внешнему ревью 2026-10-02)

1. **Spec coverage:** §2.1 → Task 1 (+union в Task 3 `known_formsets`); §2.2 → Task 3; §2.3 → Tasks 2-4; §3.1 → Task 5; §3.2 → Task 6; §3.3 → Task 7; §3.4 — не делаем (по дизайну); §4 → Task 5 Step 4 (в т.ч. вариант «нет AMITSE» нулевым GUID); §5 → Tasks 1-8 (паритет — Task 8); §6 → Task 9; §7 — не делаем.
2. **Placeholders:** `todo!` — только как RED-заглушки Step 1 каждой задачи (замещаются реализацией в GREEN-шаге); TBD отсутствуют.
3. **Type consistency:** `StrideEntry/StrideBlock/UnhideOutcome/TseReport/SpfSummary` стабильны между задачами; RPC-имена совпадают в proto/моках/CLI/TUI; CLI-конвенции (`cli_sock: Option<&str>`, `OutputFormat`, `Cli::try_parse_from`) сверены с кодом крейта.
4. **Правки внешнего ревью:** блокеры 1-10 (пути amitse, заимствования, unit-NotFound, scope-бит, импорт HiiError, слоты pages, 3 блока, parts-индексы, refs_fw, parse-тест), мажоры 11-14 (RED-фазы, swap Task 2/3 для union, server-тесты, мёртвый код), миноры 15-22 — внесены.
