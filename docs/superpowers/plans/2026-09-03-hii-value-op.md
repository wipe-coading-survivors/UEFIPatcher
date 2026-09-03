# Мини-цикл «value-op» (v1 карта вопросов + v5 StdDefaults-флип) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Экспозиция карты «вопрос → значение» (RPC/CLI `hii question info`) и операция `set-value` — согласованный флип байта дефолта в NVAR-сторах «StdDefaults» (обе копии), аппаратно-валидированный класс E14.

**Architecture:** Два новых движковых модуля: `hii/values.rs` (грамматик-осознающий walker varstore-объявлений и вопросов form-пакета — тот же стиль, что `hii/gates.rs`) и `hii/nvar.rs` (ридер NVAR-стора: внешняя запись `StdDefaults`, вложенные записи переменных). Интеграция в `hii/mod.rs` зеркалит `gates_list`/`unlock`; proto — `HiiQuestionInfo`/`HiiSetValue`; CLI — `hii question info|set-value`.

**Tech Stack:** Rust workspace (edition 2024), r-efi 7.0 IFR-константы/смещения, tonic/prost RPC, существующий LZMA-recompress-путь билдера.

**Spec:** `docs/superpowers/specs/2026-09-03-hii-value-op-design.md`

## Global Constraints

- Не писать самописный byte-offset парсинг там, где есть binrw/референсы — здесь IFR/NVAR читаются линейными walker'ами по образцу `gates.rs` (декларативные структуры r-efi неполные для byte-level, смещения — из r-efi-структур, спека §3).
- Опкоды IFR — ТОЛЬКО константы r-efi (`IFR_VARSTORE_OP=0x24`, `IFR_VARSTORE_EFI_OP=0x26`, `IFR_DEFAULT_OP=0x5B`, `IFR_NUMERIC_OP=0x07`, `IFR_CHECKBOX_OP=0x06`, `IFR_ONE_OF_OP=0x05`, `IFR_ONE_OF_OPTION_OP=0x09`).
- Комментариев в коде нет (кроме ссылок на референс `file:line`).
- После каждой задачи: `cargo test --all`, `cargo clippy --all -- -D warnings`, `cargo fmt --all -- --check`.
- Один коммит на задачу; сообщения — из плана.
- Правило Module-first: `pub mod values;`/`pub mod nvar;` в `hii/mod.rs` в том же шаге, что создание файла.
- Дефекты плана — отдельный docs-коммит ДО реализации (AGENTS.md п.11).
- Формат NVAR — UEFITool-референс `../refs/UEFITool-ai-fork/common/nvram.h` (`NVAR_ENTRY_HEADER`) + `ksy/ami_nvar.ksy`; координаты живого образа — спека §3.1.

---

### Task 1: `hii/values.rs` — varstore-карта и карта вопроса

**Files:**
- Create: `crates/uefi-engine/src/hii/values.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (добавить `pub mod values;`)

**Interfaces:**
- Produces:
  - `pub struct VarStoreMap { pub id: u16, pub guid: Option<Guid>, pub size: u16, pub name: String }`
  - `pub enum QuestionKind { OneOf, CheckBox, Numeric, Other }`
  - `pub struct OptionEntry { pub string_id: u16, pub flags: u8, pub value: u64 }`
  - `pub struct DefaultEntry { pub default_id: u16, pub type_: u8, pub value: u64 }`
  - `pub struct QuestionMap { pub question_id: u16, pub kind: QuestionKind, pub var_store_id: u16, pub var_offset: u16, pub width: u8, pub varstore: Option<VarStoreMap>, pub options: Vec<OptionEntry>, pub defaults: Vec<DefaultEntry>, pub min: u64, pub max: u64, pub step: u64 }`
  - `pub fn varstore_map(pkg: &[u8]) -> Vec<VarStoreMap>`
  - `pub fn find_question(pkg: &[u8], form_id: u16, question_id: u16) -> Option<QuestionMap>`

- [ ] **Step 1: создать модуль с тестами + объявить**

`crates/uefi-engine/src/hii/values.rs` — каркас с тестами (фикстуры в стиле
`gates.rs` tests); в `hii/mod.rs` после `pub mod string_pack;` добавить
`pub mod values;` (по алфавиту: после `pub mod strings;`).

Тесты (все в `mod tests`, хелперы `opcode/package/form_set/form/end` —
скопировать из `gates.rs` tests, НЕ импортировать):

```rust
fn varstore_buffer(id: u16, size: u16, name: &str) -> Vec<u8> {
    let g = Guid::from_str(VARSTORE_GUID_STR).unwrap();
    let mut p = Vec::new();
    p.extend_from_slice(&g.to_bytes());
    p.extend_from_slice(&id.to_le_bytes());
    p.extend_from_slice(&size.to_le_bytes());
    p.extend_from_slice(name.as_bytes());
    p.push(0);
    opcode(IFR_VARSTORE_OP, false, &p)
}

fn varstore_efi(id: u16, size: u16, name_ucs2: &str) -> Vec<u8> {
    let g = Guid::from_str(VARSTORE_GUID_STR).unwrap();
    let mut p = Vec::new();
    p.extend_from_slice(&id.to_le_bytes());
    p.extend_from_slice(&g.to_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&size.to_le_bytes());
    for u in name_ucs2.encode_utf16().chain(std::iter::once(0)) {
        p.extend_from_slice(&u.to_le_bytes());
    }
    opcode(IFR_VARSTORE_EFI_OP, false, &p)
}

fn one_of_4g() -> Vec<u8> {
    // prompt 0x01A3, help 0x01A4, qid 0x3B, varstore 1, var_offset 0x3A,
    // qflags 0x10 + ONE_OF flags 0x10 (байты 13..17 из живого образа)
    let mut p = Vec::new();
    p.extend_from_slice(&0x01A3u16.to_le_bytes());
    p.extend_from_slice(&0x01A4u16.to_le_bytes());
    p.extend_from_slice(&0x003Bu16.to_le_bytes());
    p.extend_from_slice(&1u16.to_le_bytes());
    p.extend_from_slice(&0x003Au16.to_le_bytes());
    p.push(0x10); // question flags @+12
    p.push(0x10); // one-of flags @+13
    p.extend_from_slice(&[0x00, 0x01, 0x00]); // data @+14..17
    opcode(IFR_ONE_OF_OP, true, &p)
}

fn option(string_id: u16, flags: u8, value_bytes: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&string_id.to_le_bytes());
    p.push(flags);
    p.push(0x00);
    p.extend_from_slice(value_bytes);
    opcode(IFR_ONE_OF_OPTION_OP, false, &p)
}
```

Тест-случаи:
1. `varstore_map_reads_buffer_and_efi_declarations` — пакет
   [FORM_SET, varstore_buffer(1, 0x72, "Setup"),
   varstore_efi(2, 4, "EfVar"), END] → map.len()==2; map[0]:
   id 1, size 0x72, name "Setup", guid Some(VARSTORE_GUID); map[1]:
   id 2, size 4, name "EfVar".
2. `varstore_map_empty_package_is_empty`.
3. `find_question_4g_like_fixture` — пакет [FORM_SET, varstore_buffer,
   FORM(10029), ONE_OF 0x3B, OPTION(str 4, 0x30, 0), OPTION(str 3,
   0x00, 1), END, END, END] → Some: qid 0x3B, kind OneOf, var_store_id
   1, var_offset 0x3A, width 1, varstore Some(name "Setup", size 0x72),
   options.len()==2, options[0].value 0 + flags 0x30, options[1].value 1
   + flags 0x00, defaults пусто.
4. `find_question_numeric_min_max_step` — NUMERIC qid 0x55 width-flags
   IFR_NUMERIC_SIZE_1, min/max/step байты 5/9/2 → kind Numeric, width 1,
   min 5, max 9, step 2.
5. `find_question_checkbox_width_one` — CHECKBOX → width 1, kind
   CheckBox.
6. `find_question_ignores_other_form_and_unknown_qid` — None.
7. `find_question_reads_default_opcodes_in_scope` — ONE_OF + DEFAULT
   (default_id 0, type 1, value 0) в скоупе → defaults.len()==1.
8. `find_question_two_byte_option_values` — опции со значениями 0x0102
   и 0x0001 (2-байтовые) → width 2.
9. `find_question_varstore_unknown_is_none_field` — без объявления →
   varstore None, но вопрос найден.
10. `varstore_map_stops_on_truncated_package` — обрезанный пакет не
    паникует.

- [ ] **Step 2: прогнать тесты — должны падать (функции не реализованы)**

Run: `cargo test -p uefi-engine values::`
Expected: FAIL (компилируется, тесты падают на unwrap/None/len)

- [ ] **Step 3: реализация**

```rust
use super::ifr::is_form_package;
use crate::types::Guid;
use r_efi::hii::{
    IFR_CHECKBOX_OP, IFR_DEFAULT_OP, IFR_END_OP, IFR_FORM_OP, IFR_NUMERIC_OP, IFR_NUMERIC_SIZE,
    IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP, IFR_VARSTORE_EFI_OP, IFR_VARSTORE_OP,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarStoreMap {
    pub id: u16,
    pub guid: Option<Guid>,
    pub size: u16,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionKind {
    OneOf,
    CheckBox,
    Numeric,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionEntry {
    pub string_id: u16,
    pub flags: u8,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultEntry {
    pub default_id: u16,
    pub type_: u8,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionMap {
    pub question_id: u16,
    pub kind: QuestionKind,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub width: u8,
    pub varstore: Option<VarStoreMap>,
    pub options: Vec<OptionEntry>,
    pub defaults: Vec<DefaultEntry>,
    pub min: u64,
    pub max: u64,
    pub step: u64,
}

fn package_bounds(body: &[u8]) -> (usize, usize) {
    if is_form_package(body) {
        let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
        (4, plen.min(body.len()))
    } else {
        (0, body.len())
    }
}
```

`is_varstore_op(op)` = `IFR_VARSTORE_OP | IFR_VARSTORE_EFI_OP`;
`is_question_op(op)` — множество как `gates.rs::is_question_op`
(ONE_OF/CHECKBOX/NUMERIC/PASSWORD/ORDERED_LIST/STRING/DATE/TIME/ACTION).

`varstore_map(pkg)`: линейный обход от `package_bounds`; для
`IFR_VARSTORE_OP` (len ≥ 22): id@+18, size@+20, guid `Guid::from_bytes`
из +2..18, имя ASCII до NUL от +22 (обрезано len);
`IFR_VARSTORE_EFI_OP` (len ≥ 26): id@+2, guid из +4..20, size@+24, имя
UCS2 до 0x0000 от +26. Обрыв/мусор — остановка обхода.

`find_question(pkg, form_id, qid)`: обход как `gates.rs::find_gates`
(стек фреймов, scope-bit, statement-множество из gates, линейный обход
выражений внутри гейтов — переиспользовать логику зеркально); при
statement-опкоде: FORM фиксирует текущую форму; вопрос с
`qid@+6 == qid && current_form == form_id` → собрать:

- kind по опкоду (ONE_OF→OneOf, CHECKBOX→CheckBox, NUMERIC→Numeric,
  прочее→Other);
- header: var_store_id@+8, var_offset@+10;
- width/kind-поля: CHECKBOX→1; NUMERIC (len≥14)→`1 << (pkg[i+13] &
  IFR_NUMERIC_SIZE)`, min/max/step LE с +14 по width байт (обрезано
  len); ONE_OF→собрать опции, width = max(1, байтовая ширина max
  значения, уточнение DEFAULT-типом скоупа: type 1..=4 → 1<<(type-1));
  Other→0;
- опции/DEFAULT: линейно от конца вопроса до END скоупа
  (`IFR_ONE_OF_OPTION_OP` len≥7: string_id@+2, flags@+4, value LE из
  +6..min(len,6+8); `IFR_DEFAULT_OP` len≥6: default_id@+2, type@+4,
  value LE из +5..min(len,5+8); прочие опкоды — пропуск по длине);
- varstore: `varstore_map(pkg).into_iter().find(|v| v.id ==
  var_store_id)` (var_store_id 0 → None).

- [ ] **Step 4: тесты зелёные**

Run: `cargo test -p uefi-engine values::`
Expected: PASS

- [ ] **Step 5: гейты + коммит**

```bash
cargo clippy -p uefi-engine --all-targets -- -D warnings
cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/values.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii/values.rs question/varstore map walker"
```

---

### Task 2: `hii/nvar.rs` — ридер NVAR-стора

**Files:**
- Create: `crates/uefi-engine/src/hii/nvar.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (добавить `pub mod nvar;`)

**Interfaces:**
- Produces:
  - `pub const ATTR_VALID: u8 = 0x80; pub const ATTR_DATA_ONLY: u8 = 0x08; pub const ATTR_LOCAL_GUID: u8 = 0x04; pub const ATTR_ASCII_NAME: u8 = 0x02;`
  - `pub struct NvarRecord { pub offset: usize, pub size: usize, pub attributes: u8, pub guid_index: Option<u8>, pub name: Option<String>, pub data_offset: usize, pub data_len: usize }`
  - `pub fn parse_entry(buf: &[u8], off: usize) -> Option<NvarRecord>`
  - `pub fn walk(buf: &[u8]) -> Vec<NvarRecord>`
  - `pub fn std_defaults_data(buf: &[u8]) -> Option<(usize, &[u8])>` — (offset данных первой записи, срез)
  - `pub fn find_varstore_record(body: &[u8], name: &str, data_len: usize) -> Option<(usize, &[u8])>` — смещение данных записи name с data_len внутри СТОРА (body)

- [ ] **Step 1: тесты + объявление модуля**

`nvar.rs` с `mod tests`; в `hii/mod.rs` добавить `pub mod nvar;` (по
алфавиту после `pub mod mod…` — между `mod ifr_builder;` и `mod
pe_resource;` … фактически: `pub mod ifr_builder; pub mod nvar; pub mod
package_list;`).

Фикстура-конструктор (геометрия живого образа, спека §3.1):

```rust
fn entry(name: Option<&str>, data: &[u8], attributes: u8, guid_index: Option<u8>) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend_from_slice(b"NVAR");
    let mut body = Vec::new();
    if let Some(gi) = guid_index {
        body.push(gi);
    }
    if let Some(n) = name {
        body.extend_from_slice(n.as_bytes());
        body.push(0);
    }
    body.extend_from_slice(data);
    let size = 10 + body.len();
    e.extend_from_slice(&(size as u16).to_le_bytes());
    e.extend_from_slice(&[0xFF, 0xFF, 0xFF]); // next
    e.push(attributes);
    e.extend_from_slice(&body);
    e
}

fn store_fixture() -> Vec<u8> {
    let mut inner = Vec::new();
    inner.extend_from_slice(&entry(Some("Setup"), &[0u8; 114], 0x82, Some(0)));
    inner.extend_from_slice(&entry(Some("Timeout"), &[0, 0], 0x83, Some(1)));
    inner.extend_from_slice(&entry(Some("Setup"), &[0x11; 6], 0x82, Some(4))); // вторая Setup
    let mut store = entry(Some("StdDefaults"), &inner, 0x82, Some(0));
    store.extend_from_slice(&[0xFF; 8]); // свободный хвост
    store
}
```

Тест-случаи:
1. `parse_entry_reads_std_defaults_geometry` — parse_entry(store,0):
   name "StdDefaults", size == store.len()-8, data_offset 23,
   data_len == inner.len().
2. `parse_entry_rejects_bad_signature_and_bounds` — None на мусоре,
   на обрезанной записи, на size < 10.
3. `walk_is_sequential_by_size` — 3 внутренних записи walk(inner):
   имена/длины.
4. `std_defaults_data_returns_inner` — Some((23, inner.as_slice())).
5. `std_defaults_data_rejects_non_std_defaults_store` — стор с первой
   записью «Setup» → None.
6. `find_varstore_record_matches_name_and_len` —
   find_varstore_record(&store, "Setup", 114) → offset 23+17 (заголовок
   внутренней записи 10 + guid_index 1 + имя 6), срез == &[0;114].
7. `find_varstore_record_ignores_same_name_wrong_len` —
   find_varstore_record(&store, "Setup", 6) → матчит ВТОРУЮ запись
   (offset второй), find_varstore_record(&store, "Setup", 7) → None.
8. `find_varstore_record_missing_is_none`.
9. `parse_entry_data_only_has_no_name` — attr DATA_ONLY: name None,
   данные с data_offset == 10 (+ guid_index отсутствует).
10. `parse_entry_local_guid_skips_16_bytes` — attr LOCAL_GUID|ASCII:
    guid_index None, имя читается после 16 байт GUID.
11. `parse_entry_malformed_geometries_return_none` (паник-регрессии
    fix-раунда): (а) VALID, size=10, буфер кончается ровно на записи —
    чтение guid_index за границей; (б) LOCAL_GUID (attr 0x86) с
    size < 26; (в) UCS2-имя (VALID без ASCII-бита) без двойного-NUL
    терминатора — все → None, без паники.
12. `parse_entry_ucs2_name_reads_name` — happy-path UCS2-имя: attr
    VALID (без 0x02), имя читается корректно, data_offset после него.
13. `walk_stops_at_ff_tail` — walk(&store) даёт ровно 1 запись
    (StdDefaults), хвост 0xFF останавливает обход.

- [ ] **Step 2: тесты падают**

Run: `cargo test -p uefi-engine nvar::`
Expected: FAIL

- [ ] **Step 3: реализация**

```rust
const SIG: [u8; 4] = *b"NVAR";
pub const ATTR_VALID: u8 = 0x80;
pub const ATTR_DATA_ONLY: u8 = 0x08;
pub const ATTR_LOCAL_GUID: u8 = 0x04;
pub const ATTR_ASCII_NAME: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvarRecord {
    pub offset: usize,
    pub size: usize,
    pub attributes: u8,
    pub guid_index: Option<u8>,
    pub name: Option<String>,
    pub data_offset: usize,
    pub data_len: usize,
}

pub fn parse_entry(buf: &[u8], off: usize) -> Option<NvarRecord> {
    if buf.get(off..off + 4)? != &SIG[..] {
        return None;
    }
    let size = u16::from_le_bytes([*buf.get(off + 4)?, *buf.get(off + 5)?]) as usize;
    if size < 10 || off + size > buf.len() {
        return None;
    }
    let attributes = buf[off + 9];
    let entry_end = off + size;
    let mut p = off + 10;
    let mut guid_index = None;
    if attributes & ATTR_VALID != 0 && attributes & ATTR_DATA_ONLY == 0 {
        if attributes & ATTR_LOCAL_GUID != 0 {
            p += 16;
        } else {
            guid_index = Some(*buf.get(p)?);
            p += 1;
        }
        if p > entry_end {
            return None;
        }
    }
    let mut name = None;
    if attributes & ATTR_VALID != 0 && attributes & ATTR_DATA_ONLY == 0 {
        if attributes & ATTR_ASCII_NAME != 0 {
            let end = buf[p..entry_end].iter().position(|&b| b == 0)? + p;
            name = Some(String::from_utf8_lossy(&buf[p..end]).into_owned());
            p = end + 1;
        } else {
            let mut end = p;
            while end + 1 < entry_end && !(buf[end] == 0 && buf[end + 1] == 0) {
                end += 2;
            }
            if end + 1 >= entry_end {
                return None;
            }
            let units: Vec<u16> = buf[p..end]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            name = Some(String::from_utf16_lossy(&units));
            p = end + 2;
        }
        if p > entry_end {
            return None;
        }
    }
    Some(NvarRecord {
        offset: off,
        size,
        attributes,
        guid_index,
        name,
        data_offset: p,
        data_len: entry_end - p,
    })
}

pub fn walk(buf: &[u8]) -> Vec<NvarRecord> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(r) = parse_entry(buf, off) {
        off += r.size;
        out.push(r);
    }
    out
}

pub fn std_defaults_data(buf: &[u8]) -> Option<(usize, &[u8])> {
    let first = parse_entry(buf, 0)?;
    if first.name.as_deref() != Some("StdDefaults") {
        return None;
    }
    Some((first.data_offset, buf.get(first.data_offset..first.data_offset + first.data_len)?))
}

pub fn find_varstore_record(body: &[u8], name: &str, data_len: usize) -> Option<(usize, &[u8])> {
    let (inner_off, inner) = std_defaults_data(body)?;
    for r in walk(inner) {
        if r.name.as_deref() == Some(name) && r.data_len == data_len {
            return Some((
                inner_off + r.data_offset,
                &inner[r.data_offset..r.data_offset + r.data_len],
            ));
        }
    }
    None
}
```

- [ ] **Step 4: тесты зелёные**

Run: `cargo test -p uefi-engine nvar::`
Expected: PASS

- [ ] **Step 5: гейты + коммит**

```bash
cargo clippy -p uefi-engine --all-targets -- -D warnings
cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/nvar.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii/nvar.rs NVAR StdDefaults store reader"
```

---

### Task 3: proto-сообщения value-op (без service-методов)

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (только message-блоки)
- Modify: `crates/uefi-proto/build.rs` (serde-атрибуты)

**Interfaces:**
- Consumes: —
- Produces: типы `uefi_proto::{VarStoreInfo, OptionEntry, DefaultEntry,
  QuestionInfo}` (serde::Serialize) для Task 4 (движок) и Task 6 (CLI).
  Service-методы `HiiQuestionInfo`/`HiiSetValue` — Task 5 (иначе
  uefi-engine не соберётся: tonic требует реализацию всех методов
  server-trait).

- [ ] **Step 1: messages + build.rs**

В `engine.proto` после `HiiUnlockResponse` (блок — из спеки §4.4):

```proto
message VarStoreInfo { uint32 id = 1; string guid = 2; uint32 size = 3; string name = 4; }
message OptionEntry  { uint32 string_id = 1; uint64 value = 2; uint32 flags = 3; }
message DefaultEntry { uint32 default_id = 1; uint32 type = 2; uint64 value = 3; }
message QuestionInfo {
  uint32 form_id = 1;
  uint32 question_id = 2;
  string kind = 3;
  uint32 var_store_id = 4;
  VarStoreInfo varstore = 5;
  uint32 var_offset = 6;
  uint32 width = 7;
  uint64 min = 8;
  uint64 max = 9;
  uint64 step = 10;
  repeated OptionEntry options = 11;
  repeated DefaultEntry defaults = 12;
}
message HiiQuestionInfoRequest  { string image_id = 1; string item_id = 2; }
message HiiQuestionInfoResponse { QuestionInfo question = 1; }
message HiiSetValueRequest      { string image_id = 1; string item_id = 2; uint64 value = 3; }
message HiiSetValueResponse     { QuestionInfo question = 1; repeated string applied_flips = 2; repeated string stores = 3; }
```

`build.rs` — четыре строки:

```rust
.message_attribute("engine.VarStoreInfo", "#[derive(serde::Serialize)]")
.message_attribute("engine.OptionEntry", "#[derive(serde::Serialize)]")
.message_attribute("engine.DefaultEntry", "#[derive(serde::Serialize)]")
.message_attribute("engine.QuestionInfo", "#[derive(serde::Serialize)]")
```

- [ ] **Step 2: сборка + тесты proto-зависимых крейтов**

```bash
cargo test -p uefi-proto && cargo build -p uefi-engine
cargo clippy -p uefi-proto -p uefi-engine -- -D warnings
```

- [ ] **Step 3: коммит**

```bash
git add crates/uefi-proto
git commit -m "feat(uefi-proto): value-op messages (QuestionInfo + request/response)"
```

---

### Task 4: `hii/mod.rs` — question_info + set_value

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (новые функции + ошибка + тесты)

**Interfaces:**
- Consumes: Task 1 `values::*`, Task 2 `nvar::*`, Task 3 proto-типы
  `uefi_proto::{QuestionInfo, VarStoreInfo, OptionEntry, DefaultEntry}`,
  существующие `parse_item_id`, `form_package_ranges`,
  `ops::mark_rebuild_to_root_by_path`.
- Produces:
  - `HiiError::ValueOpUnsupported(String)`
  - `pub fn question_info(image: &Image, item_id: &str) -> Result<uefi_proto::QuestionInfo, HiiError>`
  - `pub struct ValueOutcome { pub question: uefi_proto::QuestionInfo, pub applied: Vec<String>, pub stores: Vec<String> }`
  - `pub fn set_value(image: &mut Image, item_id: &str, value: u64) -> Result<ValueOutcome, HiiError>`
  - `fn question_info_proto(form_id: u16, map: &values::QuestionMap) -> uefi_proto::QuestionInfo`

- [ ] **Step 1: тесты в `hii/mod.rs` tests-модуле**

Расширить тестовый арсенал (переиспользовать существующие `g_*`-хелперы
и `vendor_image_with`), добавить:

```rust
fn value_forms_pkg() -> Vec<u8> {
    // FORM_SET + varstore_buffer(1,0x72,"Setup") + FORM 10029 +
    // ONE_OF qid 0x3B (varstore 1, var_offset 0x3A) + OPTION(4,0x30,0)
    // + OPTION(3,0x00,1) + END-ы
}

fn nvar_store_body() -> Vec<u8> {
    // nvar-фикстура Task 2 (StdDefaults → Setup[114]) с байтом 0
    // на var_offset 0x3A
}

fn image_with_nvar_stores() -> Image {
    // Volume[0]: File(guid NVAR_FV0_GUID, subtype RAW, body=nvar_store_body(), children=[])
    // Volume[1]: File(guid NVAR_FV2_GUID) → Section(GUIDED LZMA, parsing_data lzma_guid())
    //   → child Section(RAW subtype 0x19, body=nvar_store_body())
    //   (LZMA-секция собрать через compress_lzma_fit билдера НЕ нужно:
    //    парсеру достаточно parsing_data GuidedSection(lzma_guid());
    //    для unit-теста мутация идёт по дереву, recompress проверяется
    //    real-image тестом; children у guided-секции задать явно)
    // + Volume[0] содержит также File с value_forms_pkg в RAW 0x19-секции
    //    (target для item_id)
}
```

Тест-случаи:
1. `question_info_reports_4g_like_question` — Read-режим; item
   `<guid>:0x19:0#10029:0x3B`: varstore name "Setup" size 0x72,
   var_offset 0x3A, width 1, kind "one_of", options values [0,1],
   flags [0x30, 0x00].
2. `question_info_requires_question_discriminator` — item без qid →
   NotFound; несуществующий qid → NotFound; не-секция → NotASetupItem.
3. `set_value_flips_both_stores_atomically` — Write; set_value(item,1):
   applied.len()==2; байты store[0x28+0x3A] == 1 в ОБОИХ телах; длины
   тел неизменны; action==Rebuild у обоих носителей и их предков.
4. `set_value_refuses_read_only` — NotWritable.
5. `set_value_refuses_value_not_in_options` — value 7 →
   ValueOpUnsupported, байты не тронуты.
6. `set_value_refuses_unknown_varstore` — вопрос без объявления →
   ValueOpUnsupported.
7. `set_value_refuses_when_no_stores` — образ без NVAR-сторов →
   ValueOpUnsupported, мутаций нет.
8. `set_value_refuses_store_behind_non_recompressable` — RAW-секция
   стора внутри GUIDED c чужим GUID → MutationBehindCompression.
9. `set_value_repeated_is_noop_report` — повторный set_value(1):
   applied пуст (from==to — байты уже 1); байты не меняются; узлы не
   помечаются Rebuild (no-op, симметрично отказу повторного unlock в
   1711de9 — фантомные флипы не репортятся). Частичный случай: часть
   копий уже с значением — применяются только реальные изменения.
10. `set_value_idempotent_bytes` — set_value(1); build_image не
    требуется (unit-уровень) — только инвариант длины тел.

- [ ] **Step 2: тесты падают**

Run: `cargo test -p uefi-engine hii::tests`
Expected: FAIL (функции/ошибка не существуют)

- [ ] **Step 3: реализация**

В `HiiError` добавить arm:

```rust
#[error("value operation not supported: {0}")]
ValueOpUnsupported(String),
```

```rust
fn question_kind_str(kind: values::QuestionKind) -> &'static str {
    match kind {
        values::QuestionKind::OneOf => "one_of",
        values::QuestionKind::CheckBox => "checkbox",
        values::QuestionKind::Numeric => "numeric",
        values::QuestionKind::Other => "other",
    }
}

fn question_info_proto(form_id: u16, map: &values::QuestionMap) -> uefi_proto::QuestionInfo {
    uefi_proto::QuestionInfo {
        form_id: form_id as u32,
        question_id: map.question_id as u32,
        kind: question_kind_str(map.kind).to_string(),
        var_store_id: map.var_store_id as u32,
        varstore: map.varstore.as_ref().map(|v| uefi_proto::VarStoreInfo {
            id: v.id as u32,
            guid: v.guid.map(|g| crate::guid_to_upper_string(&g)).unwrap_or_default(),
            size: v.size as u32,
            name: v.name.clone(),
        }),
        var_offset: map.var_offset as u32,
        width: map.width as u32,
        min: map.min,
        max: map.max,
        step: map.step,
        options: map.options.iter().map(|o| uefi_proto::OptionEntry {
            string_id: o.string_id as u32,
            value: o.value,
            flags: o.flags as u32,
        }).collect(),
        defaults: map.defaults.iter().map(|d| uefi_proto::DefaultEntry {
            default_id: d.default_id as u32,
            r#type: d.type_ as u32,
            value: d.value,
        }).collect(),
    }
}

pub fn question_info(image: &Image, item_id: &str) -> Result<uefi_proto::QuestionInfo, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let Some(question_id) = question_id else {
        return Err(HiiError::NotFound);
    };
    let node = crate::parser::target::find_item(&image.root, &target)
        .map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    for (start, len) in form_package_ranges(node) {
        if let Some(map) = values::find_question(&node.body[start..start + len], form_id, question_id) {
            return Ok(question_info_proto(form_id, &map));
        }
    }
    Err(HiiError::NotFound)
}
```

`set_value` (полный скелет):

```rust
pub struct ValueOutcome {
    pub question: uefi_proto::QuestionInfo,
    pub applied: Vec<String>,
    pub stores: Vec<String>,
}

struct StoreHit {
    path: Vec<usize>,
    desc: String,   // "file <guid> (raw body)" | "section <guid>/… raw body"
    body_offset: usize, // offset данных записи внутри тела узла
}

fn collect_std_defaults_hits(node: &FfsNode, path: &mut Vec<usize>, barrier: bool,
    name: &str, data_len: usize, out: &mut Vec<StoreHit>) -> Result<(), HiiError> {
    let is_store_body = |n: &FfsNode| {
        matches!(n.node_type, FfsType::File | FfsType::Section)
            && (n.node_type != FfsType::File || n.children.is_empty())
            && nvar::is_std_defaults(&n.body)   // = std_defaults_data(body).is_some() c name-чеком
    };
    if barrier {
        // проверяем только когда тело — стор: отказ; иначе спускаемся глубже (barrier сохраняется)
    }
    if is_store_body(node) {
        if barrier {
            return Err(HiiError::MutationBehindCompression);
        }
        if let Some((off, _)) = nvar::find_varstore_record(&node.body, name, data_len) {
            out.push(StoreHit { path: path.clone(), desc: store_desc(node), body_offset: off });
        } else {
            return Err(HiiError::ValueOpUnsupported(format!(
                "StdDefaults store {} has no record {:?} of {} bytes", store_desc(node), name, data_len)));
        }
        return Ok(());
    }
    let child_barrier = barrier || (node.node_type == FfsType::Section
        && (node.subtype == EFI_SECTION_COMPRESSION || node.subtype == EFI_SECTION_GUID_DEFINED)
        && !matches!(&node.parsing_data, crate::types::ParsingData::GuidedSection(d)
            if crate::ffs::is_recompressable_lzma_guid(&d.guid)));
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        collect_std_defaults_hits(child, path, child_barrier, name, data_len, out)?;
        path.pop();
    }
    Ok(())
}
```

`is_std_defaults` в nvar.rs: `pub fn is_std_defaults(body: &[u8]) -> bool
{ std_defaults_data(body).is_some() }` (добавить в Task 2 — если
забыли, добавить здесь отдельным шагом).

`set_value`:

```rust
#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id, value), err)]
pub fn set_value(image: &mut Image, item_id: &str, value: u64) -> Result<ValueOutcome, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let Some(question_id) = question_id else { return Err(HiiError::NotFound) };
    let map = {
        let node = crate::parser::target::find_item(&image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section { return Err(HiiError::NotASetupItem); }
        let mut found = None;
        for (start, len) in form_package_ranges(node) {
            if let Some(m) = values::find_question(&node.body[start..start + len], form_id, question_id) {
                found = Some(m);
                break;
            }
        }
        found.ok_or(HiiError::NotFound)?
    };
    let info = question_info_proto(form_id, &map);
    let width = validate_set_value(&map, value)?;
    let varstore = map.varstore.as_ref()
        .ok_or_else(|| HiiError::ValueOpUnsupported("question varstore is not declared in the form package".into()))?;
    if varstore.name.is_empty() {
        return Err(HiiError::ValueOpUnsupported("varstore has no name; cannot match a StdDefaults record".into()));
    }
    if map.var_offset as usize + width as usize > varstore.size as usize {
        return Err(HiiError::ValueOpUnsupported(format!(
            "var_offset {:#x} + width {} exceeds varstore size {:#x}", map.var_offset, width, varstore.size)));
    }
    let mut hits = Vec::new();
    let mut path = Vec::new();
    collect_std_defaults_hits(&image.root, &mut path, false, &varstore.name, varstore.size as usize, &mut hits)?;
    if hits.is_empty() {
        return Err(HiiError::ValueOpUnsupported(
            "no NVAR StdDefaults stores found in the image".into()));
    }
    // plan
    struct Plan { hit_index: usize, from: Vec<u8>, to: Vec<u8> }
    let mut plans = Vec::new();
    for (i, hit) in hits.iter().enumerate() {
        let node = node_at(&image.root, &hit.path);
        let off = hit.body_offset + map.var_offset as usize;
        let from = node.body.get(off..off + width as usize)
            .ok_or_else(|| HiiError::ValueOpUnsupported("record data out of bounds".into()))?.to_vec();
        let mut to = value.to_le_bytes().to_vec();
        to.truncate(width as usize);
        plans.push(Plan { hit_index: i, from, to });
    }
    let mut applied = Vec::new();
    for plan in &plans {
        let hit = &hits[plan.hit_index];
        {
            let node = node_at_mut(&mut image.root, &hit.path);
            node.body[hit.body_offset + map.var_offset as usize
                ..hit.body_offset + map.var_offset as usize + width as usize]
                .copy_from_slice(&plan.to);
        }
        ops::mark_rebuild_to_root_by_path(&mut image.root, &hit.path);
        applied.push(format!("{} store+{:#x}: {} -> {}", hit.desc,
            hit.body_offset + map.var_offset as usize,
            hex(&plan.from), hex(&plan.to)));
    }
    tracing::debug!(stores = hits.len(), "set_value done");
    Ok(ValueOutcome {
        question: info,
        applied,
        stores: hits.iter().map(|h| h.desc.clone()).collect(),
    })
}
```

`validate_set_value(map, value) -> Result<u8, HiiError>`: kind Other →
отказ; width 0/`>8` → отказ; `value >= (1 << (8*width))` → отказ
«does not fit»; CheckBox: value>1 → отказ; OneOf: нет опции со
значением → отказ со списком допустимых; Numeric: вне [min,max] →
отказ. `node_at`/`node_at_mut` — приватные хелперы по path;
`hex(&[u8])` — как `flip_text` в mod.rs (переиспользовать замыкание).

Семантика повторного set_value тем же значением: from==to у копии —
флип копии пропускается (не мутация, не Rebuild, не строка в
applied); если все копии no-op — applied пуст, образ не трогается.
Симметрично отказу повторного unlock (1711de9) — фантомные флипы не
репортятся (тест 9).

- [ ] **Step 4: тесты зелёные**

Run: `cargo test -p uefi-engine hii::tests`
Expected: PASS (включая все прежние)

- [ ] **Step 5: гейты + коммит**

```bash
cargo clippy -p uefi-engine --all-targets -- -D warnings
cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii question_info + set_value via StdDefaults"
```

---

### Task 5: RPC service + handlers + mock-заглушки

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (rpc-строки в service)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (2 handler'а + arm маппинга)
- Modify: `crates/uefi-cli/tests/mock_server.rs`, `crates/uefi-tui/tests/mock_server.rs`, `crates/uefi-gateway/tests/mock_server.rs`

**Interfaces:**
- Consumes: Task 3 proto-сообщения, Task 4 `hii::question_info`/
  `hii::set_value`/`ValueOutcome`.
- Produces: RPC `HiiQuestionInfo`/`HiiSetValue` в service;
  `hii_error_status` arm `ValueOpUnsupported → failed_precondition`.

- [ ] **Step 1: rpc-строки в service**

В service после `rpc HiiUnlock…`:

```proto
  rpc HiiQuestionInfo(HiiQuestionInfoRequest) returns (HiiQuestionInfoResponse);
  rpc HiiSetValue(HiiSetValueRequest)       returns (HiiSetValueResponse);
```

(messages уже добавлены Task 3; build.rs — тоже.)

- [ ] **Step 2: серверные handler'ы**

В `rpc/server.rs` после `hii_unlock` (зеркало gates-хендлеров):

```rust
#[tracing::instrument(skip(self, req), err)]
async fn hii_question_info(
    &self,
    req: Request<HiiQuestionInfoRequest>,
) -> RpcResult<HiiQuestionInfoResponse> {
    let r = req.into_inner();
    let img = self.get_or_load_image(&r.image_id).await?;
    let question = crate::hii::question_info(&img, &r.item_id).map_err(hii_error_status)?;
    let _ = self.sm.touch(&img.session_id);
    tracing::info!(image_id = %r.image_id, item_id = %r.item_id, "hii question info");
    Ok(Response::new(HiiQuestionInfoResponse {
        question: Some(question),
    }))
}

#[tracing::instrument(skip(self, req), err)]
async fn hii_set_value(&self, req: Request<HiiSetValueRequest>) -> RpcResult<HiiSetValueResponse> {
    let r = req.into_inner();
    let img = self.get_or_load_image(&r.image_id).await?;
    let outcome = {
        let mut images = self.images.lock().await;
        let img_slot = images.get_mut(&r.image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        crate::hii::set_value(img_slot, &r.item_id, r.value).map_err(hii_error_status)?
    };
    self.flush_image(&r.image_id).await?;
    let _ = self.sm.touch(&img.session_id);
    tracing::info!(image_id = %r.image_id, item_id = %r.item_id, value = r.value, flips = outcome.applied.len(), "hii set value");
    Ok(Response::new(HiiSetValueResponse {
        question: Some(outcome.question),
        applied_flips: outcome.applied,
        stores: outcome.stores,
    }))
}
```

В `hii_error_status` arm failed_precondition добавить
`| crate::hii::HiiError::ValueOpUnsupported(_)`.

- [ ] **Step 3: mock-заглушки (3 крейта)**

В каждый tests/mock_server.rs (по образцу `hii_gates_list`) —

cli-мок (содержательные данные для e2e):

```rust
async fn hii_question_info(&self, _req: Request<HiiQuestionInfoRequest>)
    -> Result<Response<HiiQuestionInfoResponse>, Status> {
    Ok(Response::new(HiiQuestionInfoResponse {
        question: Some(QuestionInfo {
            form_id: 10029,
            question_id: 0x3B,
            kind: "one_of".into(),
            var_store_id: 1,
            varstore: Some(VarStoreInfo {
                id: 1, guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 0x72, name: "Setup".into(),
            }),
            var_offset: 0x3A, width: 1,
            min: 0, max: 0, step: 0,
            options: vec![
                OptionEntry { string_id: 4, value: 0, flags: 0x30 },
                OptionEntry { string_id: 3, value: 1, flags: 0x00 },
            ],
            defaults: vec![],
        }),
    }))
}
async fn hii_set_value(&self, _req: Request<HiiSetValueRequest>)
    -> Result<Response<HiiSetValueResponse>, Status> {
    Ok(Response::new(HiiSetValueResponse {
        question: Some(QuestionInfo { /* same as above */ ..Default::default() }),
        applied_flips: vec!["file …raw body store+0x62: 00 -> 01".into()],
        stores: vec!["mock store".into()],
    }))
}
```

tui/gateway-моки — минимальные заглушки
(`Ok(Response::new(HiiQuestionInfoResponse::default()))` /
`HiiSetValueResponse::default()`).

- [ ] **Step 4: сборка + тесты всех крейтов**

```bash
cargo test -p uefi-proto -p uefi-engine -p uefi-cli -p uefi-tui -p uefi-gateway
cargo clippy --all -- -D warnings
```

Expected: PASS

- [ ] **Step 5: коммит**

```bash
git add crates/uefi-proto crates/uefi-engine/src/rpc/server.rs \
        crates/uefi-cli/tests/mock_server.rs crates/uefi-tui/tests/mock_server.rs \
        crates/uefi-gateway/tests/mock_server.rs
git commit -m "feat(uefi-proto,uefi-engine): HiiQuestionInfo/HiiSetValue RPC"
```

---

### Task 6: CLI

**Files:**
- Modify: `crates/uefi-cli/src/client.rs` (2 метода)
- Modify: `crates/uefi-cli/src/commands/hii.rs` (2 команды)
- Modify: `crates/uefi-cli/src/output.rs` (2 принтера)
- Modify: `crates/uefi-cli/src/main.rs` (enum + dispatch)
- Test: `crates/uefi-cli/tests/cli_integration.rs`, `crates/uefi-cli/tests/e2e.rs`

**Interfaces:**
- Consumes: Task 5 rpc-методы/типы (client) и Task 3 proto-типы.
- Produces:
  - `Client::hii_question_info(&self, image_id: &str, item_id: &str) -> Result<QuestionInfo, AppError>`
  - `Client::hii_set_value(&self, image_id: &str, item_id: &str, value: u64) -> Result<(QuestionInfo, Vec<String>, Vec<String>), AppError>`
  - `output::print_question_info(&QuestionInfo, OutputFormat)`; `output::print_set_value(&QuestionInfo, &[String], &[String], OutputFormat)`
  - CLI `hii question info <item_id>` / `hii question set-value <item_id> <value>`

- [ ] **Step 1: client.rs** (по образцу `hii_gates_list`, строки рядом):

```rust
pub async fn hii_question_info(
    &mut self,
    image_id: &str,
    item_id: &str,
) -> Result<QuestionInfo, AppError> {
    let req = HiiQuestionInfoRequest {
        image_id: image_id.into(),
        item_id: item_id.into(),
    };
    let resp = self
        .inner
        .hii_question_info(auth_req(&self.state, req))
        .await?
        .into_inner();
    resp.question.ok_or_else(|| AppError::new(ErrKind::RpcNotFound, "empty question info"))
}

pub async fn hii_set_value(
    &mut self,
    image_id: &str,
    item_id: &str,
    value: u64,
) -> Result<(QuestionInfo, Vec<String>, Vec<String>), AppError> {
    let req = HiiSetValueRequest {
        image_id: image_id.into(),
        item_id: item_id.into(),
        value,
    };
    let resp = self.inner.hii_set_value(auth_req(&self.state, req)).await?.into_inner();
    let q = resp.question.ok_or_else(|| AppError::new(ErrKind::RpcNotFound, "empty question info"))?;
    Ok((q, resp.applied_flips, resp.stores))
}
```

- [ ] **Step 2: output.rs** — `print_question_info` (JSON: serde
  QuestionInfo; TSV: строка скаляров `form_id\tquestion_id\tkind\t
  var_store_id\tvarstore\tvar_offset\twidth\tmin\tmax\tstep` + строка
  на опцию `option\tstring_id\tvalue\tflags`; text: блок «question
  …#form:qid», varstore-строка, «width …, offset …», опции
  `value = … (string …, flags …)`, defaults) и `print_set_value`
  (JSON-объект {question, applied, stores}; TSV applied-строки; text —
  applied по строкам + summary «stores: N»). Пустые опции — «no
  options».

- [ ] **Step 3: commands/hii.rs**:

```rust
pub async fn question_info(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let q = client.hii_question_info(&image_id, item_id).await?;
    crate::output::print_question_info(&q, format);
    Ok(())
}

pub async fn question_set_value(
    item_id: &str,
    value: u64,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (q, applied, stores) = client.hii_set_value(&image_id, item_id, value).await?;
    crate::output::print_set_value(&q, &applied, &stores, format);
    Ok(())
}
```

- [ ] **Step 4: main.rs** — варианты:

```rust
enum HiiQuestionCmd {
    Gates { item_id: String },
    Unlock { item_id: String },
    Info { item_id: String },
    SetValue { item_id: String, value: String },
}
```

`value` парсить свободно (как qid): `0x`-hex или dec u64 — хелпер
`parse_u64_loose` (рядом с dispatch или в commands::hii). Dispatch
вызывает `commands::hii::question_info` / `question_set_value`.
help-строки: `info: show question value map (varstore/offset/width/options)`,
`set-value: seed a default value via NVAR StdDefaults stores`.

- [ ] **Step 5: интеграция/e2e тесты**

`cli_integration.rs`/`e2e.rs` — по образцу gates-кейсов: запуск с
mock-сервером, `hii question info <item>` → exit 0, stdout содержит
`"Setup"`, `0x3a`/`58`, `one_of`, `value = 1` (value=1 опция); `hii
question set-value <item> 1` → stdout содержит `00 -> 01`. Плюс кейс
`0x1`-hex значения.

- [ ] **Step 6: прогоны + коммит**

```bash
cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-cli
git commit -m "feat(uefi-cli): hii question info/set-value commands"
```

---

### Task 7: real-image приёмка (info + set-value ≡ E14)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs`

**Interfaces:**
- Consumes: `uefi_engine::hii::{question_info, set_value}`,
  существующие хелперы `load_fw`, `module_pe32_node_path`,
  `file_extent`; `lzma`-декомпрессия через
  `uefi_engine::decompress::decompress`.

- [ ] **Step 1: тест `real_image_hii_question_info_4g`**

```rust
const PCI_SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21"; // уже есть

#[ignore]
#[test]
fn real_image_hii_question_info_4g() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let item = format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029:0x3B");
    let q = uefi_engine::hii::question_info(&img, &item).expect("question_info");
    assert_eq!(q.form_id, 10029);
    assert_eq!(q.question_id, 0x3B);
    assert_eq!(q.kind, "one_of");
    assert_eq!(q.var_store_id, 1);
    let vs = q.varstore.expect("varstore declared");
    assert_eq!(vs.id, 1);
    assert_eq!(vs.size, 0x72);
    assert_eq!(vs.name, "Setup");
    assert_eq!(vs.guid, "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9");
    assert_eq!(q.var_offset, 0x3A);
    assert_eq!(q.width, 1);
    assert_eq!(q.options.len(), 2);
    assert_eq!(q.options[0].value, 0);
    assert_eq!(q.options[0].flags, 0x30);
    assert_eq!(q.options[1].value, 1);
    assert_eq!(q.options[1].flags, 0x00);
    assert!(q.defaults.is_empty());
}
```

- [ ] **Step 2: тест `real_image_hii_set_value_matches_e14`**

Координаты (спека §3.1): FV0-файл CEF5B9A3 — store@0x800060, данные
'Setup'@store+0x28, 4G-байт store+0x28+0x3A = **0x8000C2**; FV2-файл
9221315B@0xa77d28, LZMA-секция @0xa77d40 (data_offset@+20),
декомпрессат: RAW-секция (4 байта заголовок) → store@dec+4 → байт
@dec+0x66. Флеш-дифф эталона E14 против оригинала: 1 байт (FV0) +
слот секции 9221315B.

```rust
#[ignore]
#[test]
fn real_image_hii_set_value_matches_e14() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let item = format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029:0x3B");
    let outcome = uefi_engine::hii::set_value(&mut img, &item, 1).expect("set_value");
    assert_eq!(outcome.stores.len(), 2, "FV0 raw copy + FV2 LZMA copy");
    assert_eq!(outcome.applied.len(), 2);
    let built = uefi_engine::builder::build_image(&img).expect("build_image");

    assert_eq!(built.len(), data.len(), "total flash length preserved");
    assert_eq!(built[0x8000C2], 1, "FV0 StdDefaults 4G byte 0 -> 1 (E14)");
    // дифф-регионы: ровно [0x8000C2] + экстент LZMA-секции файла 9221315B
    let diff: Vec<usize> = (0..data.len()).filter(|&i| data[i] != built[i]).collect();
    let sec_extent = 0xa77d40..0xa77d40 + 0x366;
    assert!(diff.iter().all(|&i| i == 0x8000C2 || sec_extent.contains(&i)),
        "flash diff must stay inside the two StdDefaults store slots");

    // декомпрессат FV2-копии: ровно 1 байт @0x66 (0->1), побайтово E14-класс
    let lzma_diff = decompressed_diff(&data, &built, 0xa77d40);
    assert_eq!(lzma_diff, vec![(0x66, 0, 1)]);

    // re-parse: карта видит новое значение байта; повторный set_value(1) — план-мутации нет
    let rebuilt = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let q = uefi_engine::hii::question_info(&rebuilt, &item).expect("info after set");
    assert_eq!(q.var_offset, 0x3A);
    // round-trip идемпотентность
    let built2 = uefi_engine::builder::build_image(&rebuilt).expect("build2");
    assert_eq!(built2.len(), built.len());
}
```

Хелперы: `decompressed_diff(orig, built, sec_off)` — читает
section-header из orig (data_offset u16@+20, size u24@0),
`uefi_engine::decompress::decompress(payload, lzma-type)` обеих версий,
возвращает Vec<(off, old, new)>; payload — как в
`guided_lzma_payload_has_no_padding_after_last_child`
(`&body[data_offset - 4..]`); тип — `uefi_common::pi::CompressionType::Lzma`.

Если реальный дифф покажет иные базисы/офсеты — это план-дефект
(AGENTS.md п.11): сначала docs-коммит с уточнением по пробнику
`refs/amibcp/probes/build-e14.py`, затем правка теста.

- [ ] **Step 3: прогон real-image**

```bash
cargo test -p uefi-engine --test real_image -- --ignored
```

Expected: 22/22 PASS (20 прежних + 2 новых)

- [ ] **Step 4: коммит**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): real-image question map + set-value matches E14"
```

---

### Task 8: полные прогоны + актуализация TODO.md

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: финальные гейты**

```bash
cargo test --all
cargo test -p uefi-engine --test real_image -- --ignored
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

Expected: всё зелёное.

- [ ] **Step 2: TODO.md**

В секции «Мини-цикл „value-op“»: v1 → `[x]` (коммиты Task 1/4/5/6),
v5 → `[x]` (Task 2/4/7); заголовок секции — «завершён (2026-09-03)»;
v2/v3 — пометить явно «отложено (E13: инертны для посева; остаются
для Load-Defaults-семантики)», оставить `[ ]`. Новая секция
«Находки ревью мини-цикла value-op» с minors, найденными при
реализации (обязательно внести: gates.rs width-байт qflags@+12 vs
numeric-flags@+13; option-тексты question info; всё, что всплывёт).

- [ ] **Step 3: коммит**

```bash
git add TODO.md
git commit -m "docs(todo): value-op mini-cycle complete"
```

---

## Self-Review

- Спека §2 цели v1/v5 → Tasks 1–6; §4.4/4.5 → Tasks 3/5/6; §6 real-image
  → Task 7; §8 компоненты → все файлы перечислены. Не-цели (v2/v3,
  option-тексты, TUI/WebUI) задач не имеют — сознательно.
- Placeholder-скан: код-блоки всех шагов содержат конкретику;
  «same as above» встречается один раз в mock-коде Task 5 Step 3
  (осознанно — полный дубль QuestionInfo приведён в том же блоке выше).
- Типы: `values::QuestionMap`/`VarStoreMap` (Task 1) потреблены в
  Task 4 (`find_question`, `question_info_proto`); `nvar::
  find_varstore_record` (Task 2) потреблён в Task 4
  (`collect_std_defaults_hits`); proto-имена Task 3 совпадают с
  usage в Task 4/6; `ValueOutcome` поля `{question, applied, stores}`
  согласованы между Task 4 и Task 5 handler'ом.
