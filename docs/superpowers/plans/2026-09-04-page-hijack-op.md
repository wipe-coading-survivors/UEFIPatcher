# page-hijack-op Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Новая op `hii form hijack` — in-place захват существующей страницы TSE (E18): same-length правки IFR (string-id заголовка/вопросов) + fs/opt в 72-байтовых записях $SPF, атомарный precheck.

**Architecture:** Три слоя поверх существующей техники: (1) `hii/spf.rs` — сканер 72-байтовых записей-вопросов контейнера $SPF по байтовой сигнатуре (разведка спеки §2.2); (2) `hii/form_hijack.rs` — локаторы формы/вопросов поверх `values::walk_statements` + same-length рерайты u16 string-id + оркестрация precheck→мутация; (3) RPC+CLI зеркало form_add. Рост образа — только от appended-строк (машина E16); $SPF и form-пакет не меняют длину.

**Tech Stack:** Rust workspace (edition 2024), tonic/gRPC (uefi-proto), существующие hii-модули движка.

**Spec:** `docs/superpowers/specs/2026-09-04-page-hijack-op-design.md` — план аргументируется от спеки; исполнители читают оба документа.

## Global Constraints

- Module-first rule: сразу после создания файла модуля добавить `pub mod <name>;` в `crates/uefi-engine/src/hii/mod.rs` В ТОМ ЖЕ ШАГЕ, до `cargo test`.
- TDD порядок: тест → падение → реализация → проходит → commit.
- Один коммит на задачу; сообщения из плана.
- Без комментариев в коде (кроме ссылок на референс `file:line`).
- После каждой задачи: `cargo test -p uefi-engine` и `cargo clippy -p uefi-engine -- -D warnings` (для CLI-задачи — `-p uefi-cli -p uefi-proto`).
- Формат смещений в записях $SPF — только из спеки §2.2 (size 72, qid u32@0, ifr u32@28, fs@52, opt@53, сигнатура f8ff×7@36, хвост 0100 0100@44). НЕ использовать legacy-константы `ami_patcher::AMI_*` (108-байтовый формат — известная ошибка, спека §2.4).
- Тестовые данные: реальный образ `../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin` только в `#[ignore]`-тестах `real_image.rs`.

---

### Task 1: $SPF-сканер записей-вопросов (`hii/spf.rs`)

**Files:**
- Create: `crates/uefi-engine/src/hii/spf.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (добавить `pub mod spf;`)

**Interfaces:**
- Consumes: ничего (чистый модуль).
- Produces: `spf::SPF_RECORD_SIZE`, `spf::SpfQuestionRecord { offset, question_id, ifr_offset, failsafe, optimal }`, `spf::scan_question_records(&[u8]) -> Vec<SpfQuestionRecord>`, `spf::write_record_defaults(&mut [u8], usize, u8, u8)`.

- [ ] **Step 1: Создать модуль с тестами (module-first)**

`crates/uefi-engine/src/hii/spf.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn question_record(qid: u16, ifr: u32, fs: u8, opt: u8) -> Vec<u8> {
        let mut r = vec![0u8; SPF_RECORD_SIZE];
        r[0..4].copy_from_slice(&(qid as u32).to_le_bytes());
        r[8..10].copy_from_slice(&6u16.to_le_bytes());
        r[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
        r[16] = 0x09;
        r[20..24].copy_from_slice(&0x1A4u32.to_le_bytes());
        r[24..28].copy_from_slice(&0x10066u32.to_le_bytes());
        r[28..32].copy_from_slice(&ifr.to_le_bytes());
        r[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        r[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
        r[48..52].copy_from_slice(&0x1A3u32.to_le_bytes());
        r[52] = fs;
        r[53] = opt;
        r
    }

    #[test]
    fn scan_finds_question_records_and_fields() {
        let mut body = vec![0xABu8; 64];
        body.extend_from_slice(&question_record(0x3B, 0x0DD3, 0, 0));
        body.extend_from_slice(&question_record(0x36, 0x0CA8, 2, 3));
        body.extend_from_slice(&question_record(0x3C, 0x0DFE, 255, 255));
        body.extend_from_slice(&[0xFFu8; 100]);
        let recs = scan_question_records(&body);
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].offset, 64);
        assert_eq!(recs[0].question_id, 0x3B);
        assert_eq!(recs[0].ifr_offset, 0x0DD3);
        assert_eq!(recs[0].failsafe, 0);
        assert_eq!(recs[0].optimal, 0);
        assert_eq!(recs[1].question_id, 0x36);
        assert_eq!(recs[2].failsafe, 255);
    }

    #[test]
    fn scan_rejects_qid_above_u16() {
        let mut r = question_record(0x3B, 0x0DD3, 0, 0);
        r[2] = 0x01;
        let recs = scan_question_records(&r);
        assert!(recs.is_empty());
    }

    #[test]
    fn scan_skips_bare_signature_in_noise() {
        let mut body = vec![0x00u8; 128];
        body[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        body[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
        assert!(scan_question_records(&body).is_empty());
    }

    #[test]
    fn write_record_defaults_touches_exactly_two_bytes() {
        let mut body = question_record(0x3B, 0x0DD3, 0, 0);
        let before = body.clone();
        write_record_defaults(&mut body, 0, 1, 2);
        assert_eq!(body[52], 1);
        assert_eq!(body[53], 2);
        for i in 0..body.len() {
            if i != 52 && i != 53 {
                assert_eq!(body[i], before[i]);
            }
        }
    }
}
```

В `crates/uefi-engine/src/hii/mod.rs` после `pub mod string_pack;` добавить `pub mod spf;` (алфавитный порядок: между `pe_resource` и `string_pack` — `pub mod spf;` перед `pub mod string_pack;`).

- [ ] **Step 2: Запустить тесты — должны упасть (нет реализации)**

Run: `cargo test -p uefi-engine spf 2>&1 | tail -5`
Expected: FAIL — unresolved `SPF_RECORD_SIZE` и т.д.

- [ ] **Step 3: Реализация сканера**

Выше `mod tests` в `spf.rs`:

```rust
pub const SPF_RECORD_SIZE: usize = 72;
pub const SPF_RECORD_IFR_OFFSET: usize = 28;
pub const SPF_RECORD_FAILSAFE: usize = 52;
pub const SPF_RECORD_OPTIMAL: usize = 53;

const SPF_SIGNATURE: [u8; 8] = [0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
const SPF_TAIL: [u8; 4] = [0x01, 0x00, 0x01, 0x00];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpfQuestionRecord {
    pub offset: usize,
    pub question_id: u16,
    pub ifr_offset: u32,
    pub failsafe: u8,
    pub optimal: u8,
}

pub fn scan_question_records(body: &[u8]) -> Vec<SpfQuestionRecord> {
    let mut out = Vec::new();
    if body.len() < SPF_RECORD_SIZE {
        return out;
    }
    for p in 0..=body.len() - SPF_RECORD_SIZE {
        if body[p + 36..p + 44] != SPF_SIGNATURE || body[p + 44..p + 48] != SPF_TAIL {
            continue;
        }
        let qid = u32::from_le_bytes(body[p..p + 4].try_into().unwrap());
        if qid == 0 || qid > u16::MAX as u32 {
            continue;
        }
        out.push(SpfQuestionRecord {
            offset: p,
            question_id: qid as u16,
            ifr_offset: u32::from_le_bytes(
                body[p + SPF_RECORD_IFR_OFFSET..p + SPF_RECORD_IFR_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            ),
            failsafe: body[p + SPF_RECORD_FAILSAFE],
            optimal: body[p + SPF_RECORD_OPTIMAL],
        });
    }
    out
}

pub fn write_record_defaults(body: &mut [u8], offset: usize, failsafe: u8, optimal: u8) {
    body[offset + SPF_RECORD_FAILSAFE] = failsafe;
    body[offset + SPF_RECORD_OPTIMAL] = optimal;
}
```

- [ ] **Step 4: Прогнать тесты**

Run: `cargo test -p uefi-engine spf 2>&1 | tail -3`
Expected: PASS (4 теста).

- [ ] **Step 5: Полный прогон + clippy + commit**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/spf.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): page-hijack-op Task 1 — \$SPF question-record scanner (72-byte live layout, signature scan, fs/opt writer)"
```

---

### Task 2: Схема hijack (`schema.rs`)

**Files:**
- Modify: `crates/uefi-engine/src/hii/schema.rs`

**Interfaces:**
- Produces: `schema::HijackQuestionSchema { question_id: u16, prompt: String, help: String, failsafe: u8, optimal: u8 }`, `schema::HijackSchema { title: String, questions: Vec<HijackQuestionSchema> }`, `schema::parse_hijack_schema(&str) -> Result<HijackSchema, HiiError>`.

- [ ] **Step 1: Тесты (в конец `mod tests` schema.rs)**

```rust
    #[test]
    fn parse_hijack_schema_minimal() {
        let s = parse_hijack_schema(r#"{"title": "UEFIPATCHER E18"}"#).unwrap();
        assert_eq!(s.title, "UEFIPATCHER E18");
        assert!(s.questions.is_empty());
    }

    #[test]
    fn parse_hijack_schema_with_questions() {
        let s = parse_hijack_schema(
            r#"{"title": "T", "questions": [
                {"question_id": 59, "prompt": "P", "help": "H", "failsafe": 1, "optimal": 1}
            ]}"#,
        )
        .unwrap();
        assert_eq!(s.questions.len(), 1);
        assert_eq!(s.questions[0].question_id, 59);
        assert_eq!(s.questions[0].failsafe, 1);
        assert_eq!(s.questions[0].optimal, 1);
    }

    #[test]
    fn parse_hijack_schema_rejects_garbage() {
        assert!(parse_hijack_schema("{").is_err());
        assert!(parse_hijack_schema(r#"{"questions": []}"#).is_err());
    }
```

- [ ] **Step 2: Падение**

Run: `cargo test -p uefi-engine parse_hijack 2>&1 | tail -3`
Expected: FAIL — `parse_hijack_schema` не найден.

- [ ] **Step 3: Реализация (рядом с существующим `parse_schema`)**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HijackQuestionSchema {
    pub question_id: u16,
    pub prompt: String,
    pub help: String,
    pub failsafe: u8,
    pub optimal: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HijackSchema {
    pub title: String,
    #[serde(default)]
    pub questions: Vec<HijackQuestionSchema>,
}

pub fn parse_hijack_schema(json: &str) -> Result<HijackSchema, HiiError> {
    serde_json::from_str(json).map_err(|e| HiiError::InvalidSchema(e.to_string()))
}
```

- [ ] **Step 4: Прогон + clippy + commit**

```bash
cargo test -p uefi-engine parse_hijack && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/schema.rs
git commit -m "feat(uefi-engine): page-hijack-op Task 2 — hijack JSON schema + parser"
```

---

### Task 3: IFR-локаторы и same-length рерайты (`hii/form_hijack.rs`)

**Files:**
- Create: `crates/uefi-engine/src/hii/form_hijack.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`pub mod form_hijack;`)
- Modify: `crates/uefi-engine/src/hii/values.rs` (`fn walk_statements` → `pub(crate) fn walk_statements`; `fn is_question_op` → `pub(crate) fn is_question_op`)

**Interfaces:**
- Consumes: `values::walk_statements(&[u8], impl FnMut(u8, usize, usize, Option<u16>))`, `values::is_question_op(u8) -> bool`, `r_efi::hii::IFR_FORM_OP`.
- Produces (использует Task 5): `form_hijack::HijackFormSpan { form_op: usize, next_form_op: usize }`, `form_hijack::locate_form(&[u8], u16) -> Option<HijackFormSpan>`, `form_hijack::locate_questions(&[u8], u16) -> Vec<(usize, u16)>` (пары `q_off, question_id`), `form_hijack::rewrite_form_title(&mut [u8], usize, u16)`, `form_hijack::rewrite_question_strings(&mut [u8], usize, u16, u16)`.

- [ ] **Step 1: Модуль с тестами + module-first**

`crates/uefi-engine/src/hii/form_hijack.rs`:

```rust
use r_efi::hii::IFR_FORM_OP;

use super::values;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hii::ifr_builder::IfrBuilder;
    use crate::types::Guid;
    use r_efi::hii::IFR_ONE_OF_OP;
    use std::str::FromStr;

    const FORMSET_GUID: &str = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";

    fn package_with_form() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        b.emit_form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 1, 1, &[]);
        b.emit_form(7, 1);
        b.emit_one_of(2, 3, 0x11, 1, 0, 0, 1);
        b.emit_end();
        b.emit_end();
        let ifr = b.build();
        let mut pkg = vec![0u8; 4];
        let len = 4 + ifr.len() as u32;
        pkg[0] = (len & 0xFF) as u8;
        pkg[1] = ((len >> 8) & 0xFF) as u8;
        pkg[2] = ((len >> 16) & 0xFF) as u8;
        pkg[3] = r_efi::hii::PACKAGE_FORMS;
        pkg.extend_from_slice(&ifr);
        pkg
    }

    #[test]
    fn locate_form_span_and_questions() {
        let pkg = package_with_form();
        let span = locate_form(&pkg, 7).expect("form 7");
        assert!(span.form_op >= 4 && span.form_op < pkg.len());
        assert_eq!(span.next_form_op, pkg.len());
        let qs = locate_questions(&pkg, 7);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].1, 0x11);
        assert!(qs[0].0 > span.form_op);
        assert_eq!(pkg[qs[0].0], IFR_ONE_OF_OP);
        assert!(locate_form(&pkg, 8).is_none());
        assert!(locate_questions(&pkg, 8).is_empty());
    }

    #[test]
    fn rewrite_title_and_strings_same_length() {
        let mut pkg = package_with_form();
        let span = locate_form(&pkg, 7).unwrap();
        let q = locate_questions(&pkg, 7)[0].0;
        let before = pkg.clone();
        rewrite_form_title(&mut pkg, span.form_op, 0xBEEF);
        rewrite_question_strings(&mut pkg, q, 0x1111, 0x2222);
        assert_eq!(pkg.len(), before.len());
        assert_eq!(&pkg[span.form_op + 4..span.form_op + 6], &[0xEF, 0xBE]);
        assert_eq!(&pkg[q + 2..q + 4], &[0x11, 0x11]);
        assert_eq!(&pkg[q + 4..q + 6], &[0x22, 0x22]);
        for i in 0..pkg.len() {
            let touched = (i >= span.form_op + 4 && i < span.form_op + 6)
                || (i >= q + 2 && i < q + 6);
            if !touched {
                assert_eq!(pkg[i], before[i]);
            }
        }
    }
}
```

В `hii/mod.rs` добавить `pub mod form_hijack;` (после `pub mod forms;`).
В `values.rs` сменить видимость: `pub(crate) fn walk_statements(...)`, `pub(crate) fn is_question_op(...)`.

- [ ] **Step 2: Падение**

Run: `cargo test -p uefi-engine form_hijack 2>&1 | tail -3`
Expected: FAIL — `locate_form` не найден.

- [ ] **Step 3: Реализация локаторов**

Выше `mod tests` в `form_hijack.rs`:

```rust
pub struct HijackFormSpan {
    pub form_op: usize,
    pub next_form_op: usize,
}

pub fn locate_form(pkg: &[u8], form_id: u16) -> Option<HijackFormSpan> {
    let mut forms: Vec<(usize, u16)> = Vec::new();
    values::walk_statements(pkg, |op, off, len, _| {
        if op == IFR_FORM_OP && len >= 6 {
            forms.push((off, u16::from_le_bytes([pkg[off + 2], pkg[off + 3]])));
        }
    });
    let idx = forms.iter().position(|&(_, id)| id == form_id)?;
    let next_form_op = forms
        .get(idx + 1)
        .map(|&(off, _)| off)
        .unwrap_or(pkg.len());
    Some(HijackFormSpan {
        form_op: forms[idx].0,
        next_form_op,
    })
}

pub fn locate_questions(pkg: &[u8], form_id: u16) -> Vec<(usize, u16)> {
    let mut qs = Vec::new();
    values::walk_statements(pkg, |op, off, len, current_form| {
        if values::is_question_op(op) && len >= 13 && current_form == Some(form_id) {
            qs.push((off, u16::from_le_bytes([pkg[off + 6], pkg[off + 7]])));
        }
    });
    qs
}

pub fn rewrite_form_title(pkg: &mut [u8], form_op: usize, title_id: u16) {
    pkg[form_op + 4..form_op + 6].copy_from_slice(&title_id.to_le_bytes());
}

pub fn rewrite_question_strings(
    pkg: &mut [u8],
    q_off: usize,
    prompt_id: u16,
    help_id: u16,
) {
    pkg[q_off + 2..q_off + 4].copy_from_slice(&prompt_id.to_le_bytes());
    pkg[q_off + 4..q_off + 6].copy_from_slice(&help_id.to_le_bytes());
}
```

- [ ] **Step 4: Прогон + clippy + commit**

```bash
cargo test -p uefi-engine form_hijack && cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/form_hijack.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/values.rs
git commit -m "feat(uefi-engine): page-hijack-op Task 3 — IFR form/question locators + same-length string-id rewrites"
```

---

### Task 4: Резолвер $SPF-payload (`ami_patcher.rs`)

**Files:**
- Modify: `crates/uefi-engine/src/hii/ami_patcher.rs`

**Interfaces:**
- Consumes: `find_ami_module`, `find_payload_path`, `is_pfs_payload` (приватные фн этого же файла).
- Produces (использует Task 5): `ami_patcher::pfs_payload_path(&Image, Option<&Guid>) -> Result<Vec<usize>, HiiError>` — путь до leaf-секции с $SPF.

- [ ] **Step 1: Тесты (в `mod tests` ami_patcher.rs)**

```rust
    #[test]
    fn pfs_payload_path_resolves_and_rejects() {
        let img = ami_image(
            vec![pfs_section(), ui_section("AMITSESetupData")],
            vec![pe32_section(), ui_section("AMITSE")],
        );
        let path = pfs_payload_path(&img, None).unwrap();
        assert_eq!(path, vec![0, 0, 0]);

        let no_pfs = ami_image(
            vec![ui_section("AMITSESetupData")],
            vec![pe32_section(), ui_section("AMITSE")],
        );
        assert!(matches!(
            pfs_payload_path(&no_pfs, None),
            Err(HiiError::AmiFilesNotFound)
        ));

        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        assert_eq!(pfs_payload_path(&img, Some(&sd)).unwrap(), vec![0, 0, 0]);

        let bogus = Guid::try_parse("00000000-0000-0000-0000-00000000DEAD").unwrap();
        let unmatched = ami_image(
            vec![pfs_section(), ui_section("OtherModule")],
            vec![pe32_section(), ui_section("AMITSE")],
        );
        assert!(matches!(
            pfs_payload_path(&unmatched, Some(&bogus)),
            Err(HiiError::AmiFilesNotFound)
        ));
    }
```

- [ ] **Step 2: Падение**

Run: `cargo test -p uefi-engine pfs_payload_path 2>&1 | tail -3`
Expected: FAIL — функция не найдена.

- [ ] **Step 3: Реализация (рядом с `resolve_ami_payloads`)**

```rust
pub fn pfs_payload_path(
    image: &Image,
    setupdata_guid: Option<&Guid>,
) -> Result<Vec<usize>, HiiError> {
    let (vi, fi) = find_ami_module(image, setupdata_guid, "setupdata")?;
    let rel = find_payload_path(&image.root.children[vi].children[fi], is_pfs_payload)?;
    Ok([vec![vi, fi], rel].concat())
}
```

- [ ] **Step 4: Прогон + clippy + commit**

```bash
cargo test -p uefi-engine pfs_payload_path && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/ami_patcher.rs
git commit -m "feat(uefi-engine): page-hijack-op Task 4 — pub \$SPF payload resolver for hijack"
```

---

### Task 5: Ядро `hijack_form` — precheck, оркестрация, атомарность

**Files:**
- Modify: `crates/uefi-engine/src/hii/form_hijack.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`fn parse_item_id` → `pub(crate) fn parse_item_id`)
- Modify: `crates/uefi-engine/src/hii/form_add.rs` (`fn owner_guid_by_path` → `pub(crate) fn owner_guid_by_path`, `fn resource_forms_package` → `pub(crate) fn resource_forms_package`)

**Interfaces:**
- Consumes: Task 1 `spf::*`, Task 2 `schema::HijackSchema`, Task 3 локаторы, Task 4 `pfs_payload_path`; `string_pack::add_strings` / `add_strings_to_resource`; `ops::mark_rebuild_to_root_by_path`; `crate::parser::target::{parse_target, find_item_path, find_item, find_item_mut}`.
- Produces (используют Task 6-9): `form_hijack::HijackRecordEdit { question_id: u16, record_offset: usize, old_failsafe: u8, old_optimal: u8, new_failsafe: u8, new_optimal: u8 }`, `form_hijack::HijackResult { string_ids: HashMap<String, u16>, records: Vec<HijackRecordEdit>, form_ifr_start: u32, form_ifr_end: u32 }`, `form_hijack::hijack_form(&mut Image, &str, &schema::HijackSchema, Option<&Guid>) -> Result<HijackResult, HiiError>`.

- [ ] **Step 1: Тесты ядра на синтетическом bare-канале (в `mod tests` form_hijack.rs)**

Тестовым образом: флешка с двумя файлами — Setup (RAW-секции: form-пакет + строковый пакет) и AMITSESetupData (UI-секция с именем "AMITSESetupData" + RAW-секция 0x18 с $SPF-телом; имя нужно find_ami_module при setupdata_guid=None). Секции внутри файла выравниваются на 4 байта (`file_sections`) — как в реальном FFS, иначе parse_sections теряет секцию после 58-байтового form-пакета. Кейс "missing $SPF file" удаляет файл SetupData из распарсенного дерева: на UI-именованной фикстуре Some(чужой guid) не отклоняется — find_ami_module по дизайну падает в name-эвристику (класс дефекта Task 4 64bfa21). $SPF-записи строятся по ifr-offsets реального form-пакета фикстуры.

```rust
    use crate::builder::build_image;
    use crate::ffs::{
        EFI_FVH_SIGNATURE, EFI_FVB2_ERASE_POLARITY, EFI_SECTION_FREEFORM_SUBTYPE_GUID,
        EFI_SECTION_RAW, EFI_SECTION_UI, size_to_uint24,
    };
    use crate::hii::spf;
    use crate::parser::image::parse_image;

    const FILE_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";
    const SETUPDATA_GUID_STR: &str = "12345678-90AB-CDEF-1234-567890ABCDEF";

    fn section_bytes(stype: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + body.len());
        v.extend_from_slice(&size_to_uint24((4 + body.len()) as u32));
        v.push(stype);
        v.extend_from_slice(body);
        v
    }

    fn ffs_file_bytes(guid: &Guid, content: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 24];
        buf[0..16].copy_from_slice(&guid.to_bytes());
        buf[18] = crate::ffs::EFI_FV_FILETYPE_RAW;
        buf[20..23].copy_from_slice(&size_to_uint24((24 + content.len()) as u32));
        buf.extend_from_slice(content);
        buf
    }

    fn flash_with_files(files: Vec<Vec<u8>>) -> Vec<u8> {
        let mut body = Vec::new();
        for f in files {
            let aligned = (body.len() + 7) & !7;
            body.resize(aligned, 0xFF);
            body.extend_from_slice(&f);
        }
        body.extend_from_slice(&[0xFF; 2048]);
        let total = 56 + body.len();
        let mut buf = vec![0xFFu8; 32 + total];
        let fv = &mut buf[32..];
        fv[32..40].copy_from_slice(&(total as u64).to_le_bytes());
        fv[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        fv[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        fv[48..50].copy_from_slice(&56u16.to_le_bytes());
        fv[55] = 2;
        fv[56..].copy_from_slice(&body);
        buf
    }

    fn string_package_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, r_efi::hii::PACKAGE_STRINGS]);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.push(0x10);
        buf.extend_from_slice(b"first");
        buf.push(0x00);
        buf.push(0x00);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn spf_container(records: &[spf::SpfQuestionRecord], body_len: usize) -> Vec<u8> {
        let mut body = vec![0u8; body_len];
        body[16..20].copy_from_slice(b"$SPF");
        for r in records {
            let mut rec = vec![0u8; spf::SPF_RECORD_SIZE];
            rec[0..4].copy_from_slice(&(r.question_id as u32).to_le_bytes());
            rec[8..10].copy_from_slice(&6u16.to_le_bytes());
            rec[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
            rec[16] = 0x09;
            rec[28..32].copy_from_slice(&r.ifr_offset.to_le_bytes());
            rec[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
            rec[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
            rec[52] = r.failsafe;
            rec[53] = r.optimal;
            body[r.offset..r.offset + spf::SPF_RECORD_SIZE].copy_from_slice(&rec);
        }
        body
    }

    fn hijack_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = package_with_form();
        let qs = locate_questions(&pkg, 7);
        let records: Vec<spf::SpfQuestionRecord> = qs
            .iter()
            .enumerate()
            .map(|(i, &(off, qid))| spf::SpfQuestionRecord {
                offset: 0x100 + i * spf::SPF_RECORD_SIZE,
                question_id: qid,
                ifr_offset: off as u32,
                failsafe: 0,
                optimal: 0,
            })
            .collect();
        let spf_body = spf_container(&records, 0x400);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = ffs_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_UI, &ui_name("AMITSESetupData")),
                section_bytes(EFI_SECTION_FREEFORM_SUBTYPE_GUID, &spf_body),
            ]),
        );
        let flash = flash_with_files(vec![setup, sd]);
        (flash, pkg, spf_body)
    }

    fn file_sections(sections: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for s in sections {
            out.extend_from_slice(s);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        }
        out
    }

    fn ui_name(name: &str) -> Vec<u8> {
        name.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .chain(0u16.to_le_bytes())
            .collect()
    }

    fn hijack_schema() -> crate::hii::schema::HijackSchema {
        crate::hii::schema::parse_hijack_schema(
            r#"{"title": "UEFIPATCHER", "questions": [
                {"question_id": 17, "prompt": "PQ", "help": "PH", "failsafe": 1, "optimal": 1}
            ]}"#,
        )
        .unwrap()
    }

    const ITEM: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7";

    #[test]
    fn hijack_form_rewrites_ifr_and_spf_records() {
        let (flash, pkg_before, spf_before) = hijack_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let res = hijack_form(&mut img, ITEM, &hijack_schema(), None).unwrap();
        assert_eq!(res.string_ids.len(), 3);
        assert!(res.string_ids.contains_key("UEFIPATCHER"));
        assert_eq!(res.records.len(), 1);
        assert_eq!(res.records[0].question_id, 17);
        assert_eq!(res.records[0].old_failsafe, 0);
        assert_eq!(res.records[0].new_failsafe, 1);
        let built = build_image(&img).unwrap();

        let reparsed = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let sd_file = reparsed
            .root
            .children
            .iter()
            .find_map(|v| {
                v.children
                    .iter()
                    .find(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()))
            })
            .unwrap();
        let pfs_leaf = sd_file
            .children
            .iter()
            .find(|c| c.body.windows(4).any(|w| w == b"$SPF"))
            .unwrap();
        assert_eq!(pfs_leaf.body.len(), spf_before.len(), "$SPF length invariant");
        let recs = spf::scan_question_records(&pfs_leaf.body);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].question_id, 17);
        assert_eq!(recs[0].failsafe, 1);
        assert_eq!(recs[0].optimal, 1);

        let forms = crate::hii::forms::collect_forms(&reparsed);
        let form = forms
            .iter()
            .find(|f| f.form_id_ifr == 7)
            .expect("hijacked form");
        assert_eq!(form.title, "UEFIPATCHER");
        assert_eq!(
            pkg_before.len(),
            form_pkg_len(&reparsed),
            "form package length invariant"
        );
    }

    fn form_pkg_len(img: &Image) -> usize {
        let node = crate::parser::target::find_item(
            &img.root,
            &crate::parser::target::parse_target(&format!("{FILE_GUID}:0x19:0")).unwrap(),
        )
        .unwrap();
        node.body.len()
    }

    #[test]
    fn hijack_form_atomic_precheck_failures() {
        for (item, qid) in [
            ("5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#99", 17u16),
            (ITEM, 99),
        ] {
            let (flash, _, _) = hijack_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = build_image(&img).unwrap();
            let mut sc = hijack_schema();
            sc.questions[0].question_id = qid;
            assert!(
                hijack_form(&mut img, item, &sc, None).is_err(),
                "must reject {item}"
            );
            assert_eq!(build_image(&img).unwrap(), before, "no mutation on precheck failure");
        }

        let (flash, _, _) = hijack_flash_image();
        let mut img2 = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let vol = img2
            .root
            .children
            .iter_mut()
            .find(|v| v.children.iter().any(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap())))
            .unwrap();
        vol.children
            .retain(|f| f.guid != Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()));
        assert!(
            hijack_form(&mut img2, ITEM, &hijack_schema(), Some(&Guid::from_str("00000000-0000-0000-0000-00000000DEAD").unwrap())).is_err(),
            "missing $SPF file"
        );
    }
```

- [ ] **Step 2: Падение**

Run: `cargo test -p uefi-engine form_hijack 2>&1 | tail -3`
Expected: FAIL — `hijack_form` не найден.

- [ ] **Step 3: Реализация `hijack_form`**

В `form_hijack.rs` (выше тестов; `use std::collections::HashMap;` и прочие use по необходимости):

```rust
use std::collections::HashMap;

use super::ami_patcher;
use super::form_add;
use super::ifr;
use super::ops;
use super::schema;
use super::string_pack;
use super::spf;

#[derive(Debug)]
pub struct HijackRecordEdit {
    pub question_id: u16,
    pub record_offset: usize,
    pub old_failsafe: u8,
    pub old_optimal: u8,
    pub new_failsafe: u8,
    pub new_optimal: u8,
}

pub struct HijackResult {
    pub string_ids: HashMap<String, u16>,
    pub records: Vec<HijackRecordEdit>,
    pub form_ifr_start: u32,
    pub form_ifr_end: u32,
}

#[tracing::instrument(level = "debug", skip(image, hijack), fields(item_id = %item_id), err)]
pub fn hijack_form(
    image: &mut Image,
    item_id: &str,
    hijack: &schema::HijackSchema,
    setupdata_guid: Option<&Guid>,
) -> Result<HijackResult, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target, form_id, question_id) = crate::hii::parse_item_id(item_id)?;
    if question_id.is_some() {
        return Err(HiiError::NotFound);
    }
    let path = crate::parser::target::find_item_path(&image.root, &target)
        .ok_or(HiiError::NotFound)?;
    let mut ancestor = &image.root;
    for &i in &path[..path.len() - 1] {
        ancestor = &ancestor.children[i];
        if ancestor.node_type == FfsType::Section
            && (ancestor.subtype == crate::ffs::EFI_SECTION_COMPRESSION
                || ancestor.subtype == crate::ffs::EFI_SECTION_GUID_DEFINED)
            && !matches!(
                &ancestor.parsing_data,
                crate::types::ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid)
            )
        {
            return Err(HiiError::MutationBehindCompression);
        }
    }

    let bare_channel = {
        let node = crate::parser::target::find_item(&image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        if node.subtype == crate::ffs::EFI_SECTION_RAW && ifr::is_form_package(&node.body) {
            true
        } else if node.subtype == crate::ffs::EFI_SECTION_PE32 {
            form_add::resource_forms_package(&node.body).is_some()
        } else {
            return Err(HiiError::NotASetupItem);
        }
    };

    let span = {
        let pkg = package_of(&image.root, &target)?;
        locate_form(pkg, form_id).ok_or(HiiError::NotFound)?
    };
    let form_ifr_start = span.form_op as u32;
    let form_ifr_end = span.next_form_op as u32;
    {
        let pkg = package_of(&image.root, &target)?;
        let qs = locate_questions(pkg, form_id);
        for q in &hijack.questions {
            if !qs.iter().any(|&(_, qid)| qid == q.question_id) {
                return Err(HiiError::NotFound);
            }
        }
    }

    let sd_path = ami_patcher::pfs_payload_path(image, setupdata_guid)?;
    let spf_records = {
        let node = node_at(&image.root, &sd_path);
        node.body.clone()
    };
    let mut edits = Vec::with_capacity(hijack.questions.len());
    for q in &hijack.questions {
        let matched: Vec<_> = spf::scan_question_records(&spf_records)
            .into_iter()
            .filter(|r| {
                r.question_id == q.question_id
                    && r.ifr_offset >= form_ifr_start
                    && r.ifr_offset < form_ifr_end
            })
            .collect();
        if matched.len() != 1 {
            return Err(HiiError::NotFound);
        }
        let r = matched[0];
        edits.push(HijackRecordEdit {
            question_id: q.question_id,
            record_offset: r.offset,
            old_failsafe: r.failsafe,
            old_optimal: r.optimal,
            new_failsafe: q.failsafe,
            new_optimal: q.optimal,
        });
    }

    let mut strings: Vec<String> = vec![hijack.title.clone()];
    for q in &hijack.questions {
        strings.push(q.prompt.clone());
        strings.push(q.help.clone());
    }
    strings.dedup();
    let string_ids = if bare_channel {
        let owner = form_add::owner_guid_by_path(&image.root, &path);
        string_pack::add_strings(image, owner.as_ref(), &strings)?
    } else {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        match string_pack::add_strings_to_resource(&mut node.body, &strings) {
            Ok(ids) => ids,
            Err(string_pack::AddStringsToResourceError::NotFound) => {
                return Err(HiiError::StringPackageNotFound)
            }
            Err(string_pack::AddStringsToResourceError::GrowthUnsupported) => {
                return Err(HiiError::PeGrowthUnsupported)
            }
        }
    };

    {
        let root = &mut image.root;
        let pkg = package_of_mut(root, &target)?;
        apply_hijack_ifr(pkg, form_id, hijack, &string_ids)?;
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);

    {
        let node = node_at_mut(&image.root, &sd_path);
        for e in &edits {
            spf::write_record_defaults(&mut node.body, e.record_offset, e.new_failsafe, e.new_optimal);
        }
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);

    tracing::debug!(?edits, "hijack_form done");
    Ok(HijackResult {
        string_ids,
        records: edits,
        form_ifr_start,
        form_ifr_end,
    })
}

fn package_of<'a>(
    root: &'a FfsNode,
    target: &crate::types::Target,
) -> Result<&'a [u8], HiiError> {
    let node = crate::parser::target::find_item(root, target).map_err(|_| HiiError::NotFound)?;
    if node.subtype == crate::ffs::EFI_SECTION_RAW {
        Ok(&node.body)
    } else {
        let (off, len) = form_add::resource_forms_package(&node.body)
            .ok_or(HiiError::NotASetupItem)?;
        node.body.get(off..off + len).ok_or(HiiError::InvalidIfr)
    }
}

fn package_of_mut<'a>(
    root: &'a mut FfsNode,
    target: &crate::types::Target,
) -> Result<&'a mut [u8], HiiError> {
    let node = crate::parser::target::find_item_mut(root, target)
        .map_err(|_| HiiError::NotFound)?;
    if node.subtype == crate::ffs::EFI_SECTION_RAW {
        Ok(&mut node.body)
    } else {
        let (off, len) = form_add::resource_forms_package(&node.body)
            .ok_or(HiiError::NotASetupItem)?;
        let end = off.checked_add(len).ok_or(HiiError::InvalidIfr)?;
        node.body.get_mut(off..end).ok_or(HiiError::InvalidIfr)
    }
}

fn apply_hijack_ifr(
    pkg: &mut [u8],
    form_id: u16,
    hijack: &schema::HijackSchema,
    string_ids: &HashMap<String, u16>,
) -> Result<(), HiiError> {
    let span = locate_form(pkg, form_id).ok_or(HiiError::NotFound)?;
    let title_id = *string_ids.get(&hijack.title).ok_or(HiiError::InvalidIfr)?;
    rewrite_form_title(pkg, span.form_op, title_id);
    let qs = locate_questions(pkg, form_id);
    for q in &hijack.questions {
        let prompt_id = *string_ids.get(&q.prompt).ok_or(HiiError::InvalidIfr)?;
        let help_id = *string_ids.get(&q.help).ok_or(HiiError::InvalidIfr)?;
        let q_off = qs
            .iter()
            .find(|&&(_, qid)| qid == q.question_id)
            .map(|&(off, _)| off)
            .ok_or(HiiError::NotFound)?;
        rewrite_question_strings(pkg, q_off, prompt_id, help_id);
    }
    Ok(())
}

fn node_at<'a>(root: &'a FfsNode, path: &[usize]) -> &'a FfsNode {
    let mut node = root;
    for &i in path {
        node = &node.children[i];
    }
    node
}

fn node_at_mut<'a>(root: &'a mut FfsNode, path: &[usize]) -> &'a mut FfsNode {
    let mut node = root;
    for &i in path {
        node = &mut node.children[i];
    }
    node
}
```

Ключевые свойства: (1) обе фазы precheck (`locate_form`, `locate_questions`, `pfs_payload_path`, scan+фильтр записей) работают только через `&image.root` — до первой мутации; (2) после precheck Err-пути остаются только у `add_strings*` (рост) и `apply_hijack_ifr` (lookups, валидированные precheck'ом — на grown-теле формы/вопросы не исчезают); (3) `apply_hijack_ifr` для PE-канала работает по свежим смещениям — `package_of_mut` вызывается уже ПОСЛЕ роста строк (`resource_forms_package` пересчитывает off/len).

Также в `hii/mod.rs`: `fn parse_item_id(...)` → `pub(crate) fn parse_item_id(...)`; в `form_add.rs`: `fn owner_guid_by_path` и `fn resource_forms_package` → `pub(crate)`.

- [ ] **Step 4: Прогон**

Run: `cargo test -p uefi-engine form_hijack 2>&1 | tail -5`
Expected: PASS (тесты Tasks 3+5). Если `collect_forms` не находит title (строковый id не разрешается) — проверить, что фикстурный строковый пакет парсится `parse_string_package` (см. form_add tests) и что appended-строки получили id из маппинга.

- [ ] **Step 5: Полный прогон + clippy + commit**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/form_hijack.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/form_add.rs
git commit -m "feat(uefi-engine): page-hijack-op Task 5 — hijack_form core: atomic precheck, string append, same-length IFR+\$SPF rewrites"
```

---

### Task 6: LZMA slot-fit гейт (синтетика)

**Files:**
- Modify: `crates/uefi-engine/src/hii/form_hijack.rs` (тесты)

**Interfaces:**
- Consumes: `crate::compress::compress_lzma`, `crate::ffs::lzma_guid`, шаблон `lzma_guided_file_bytes` из `ami_patcher.rs` tests (скопировать локально).

- [ ] **Step 1: Тест — $SPF за LZMA переживает build, дифф confined**

Фикстура: как `hijack_flash_image()`, но sd-файл оборачивается GUIDed-LZMA (локальная копия `lzma_guided_file_bytes` из `ami_patcher.rs` tests) и идёт ПЕРВЫМ файлом (слот позиционно стабилен), setup-файл остаётся bare вторым:

```rust
    fn lzma_guided_file_bytes(guid: &Guid, children: &[u8]) -> (Vec<u8>, usize) {
        let mut stream = crate::compress::compress_lzma(children).unwrap();
        stream.resize(stream.len().max(64), 0x00);
        let mut body = crate::ffs::lzma_guid().to_bytes().to_vec();
        body.extend_from_slice(&0x18u16.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&stream);
        let sec = section_bytes(crate::ffs::EFI_SECTION_GUID_DEFINED, &body);
        let slot_len = stream.len();
        (ffs_file_bytes(guid, &sec), slot_len)
    }

    fn find_spf_leaf<'a>(node: &'a FfsNode) -> Option<&'a FfsNode> {
        if node.children.is_empty() && node.body.windows(4).any(|w| w == b"$SPF") {
            return Some(node);
        }
        node.children.iter().find_map(find_spf_leaf)
    }

    #[test]
    fn hijack_form_survives_lzma_slot_fit() {
        let (_, pkg, spf_body) = hijack_flash_image();
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = lzma_guided_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &section_bytes(EFI_SECTION_FREEFORM_SUBTYPE_GUID, &spf_body),
        );
        let data = flash_with_files(vec![sd.0.clone(), setup]);
        let sd_start = data
            .windows(16)
            .position(|w| w == sd_guid_bytes())
            .expect("sd file header at slot start");
        let sd_slot = (sd_start, sd_start + sd.0.len());
        let setup_start = (sd_slot.1 + 7) & !7;

        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let res = hijack_form(
            &mut img,
            ITEM,
            &hijack_schema(),
            Some(&Guid::from_str(SETUPDATA_GUID_STR).unwrap()),
        )
        .unwrap();
        assert_eq!(res.records.len(), 1);
        let built = build_image(&img).unwrap();
        assert_eq!(built.len(), data.len(), "slot-fit must preserve total length");
        assert_eq!(
            &built[..sd_start],
            &data[..sd_start],
            "bytes before the $SPF LZMA slot must not change"
        );

        for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
            if a != b {
                assert!(
                    (i >= sd_slot.0 && i < sd_slot.1) || i >= setup_start,
                    "byte {i:#x} changed outside the $SPF LZMA slot and the setup module slot"
                );
            }
        }

        let reparsed = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let sd_file = reparsed
            .root
            .children
            .iter()
            .find_map(|v| {
                v.children
                    .iter()
                    .find(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()))
            })
            .unwrap();
        let pfs_leaf = find_spf_leaf(sd_file).unwrap();
        let recs = spf::scan_question_records(&pfs_leaf.body);
        assert_eq!(recs[0].failsafe, 1, "fs edit must survive rebuild");
        assert_eq!(recs[0].optimal, 1);
    }
```

Дефекты исходного текста (каждый воспроизведён RED-прогоном, исправлено до реализации):

1. sd-файл за LZMA необнаружим при `setupdata_guid=None`: UI-секции среди direct children нет (только GUIDed-обёртка), а тело файла (88 байт GUIDed-секции) не кратно `AMI_RECORD_SIZE` — `find_ami_module` проходит все три фолбэка и возвращает `AmiFilesNotFound`. Фикс: `Some(&sd_guid)` — как LZMA-тест `patch_ami` (класс дефекта 81f35b8 Task 5).
2. `[...].concat()` без 4-байтового выравнивания секций: `parse_sections` шагает выровненными страйдами, строковая секция поглощается — `StringPackageNotFound`. Фикс: `file_sections(&[...])` (класс дефекта 9a46a30 Task 5).
3. Строгий confinement к одному слоту невыполним для `hijack_form`: hijack обязан мутировать bare setup-модуль (строки + IFR string-id), а `build_volume` перекладирует файлы последовательно — выросший setup сдвигает слот sd, и дифф ложится вне `[sd_start, sd_start+len)` (наблюдение: byte 0x68 внутри setup-файла). Дизайн §5 для real-image сам требует «дифф confined ровно к двум слотам (Setup-модуль: строки + IFR; AMITSESetupData: fs/opt)». Фикс: sd-файл первым (слот стабилен, прецедент — LZMA-тест ami_patcher со `sd_start = 88`), дифф confined к слоту sd ∪ региону setup-модуля; неизменность префикса `[0, sd_start)` и паддинга `[sd_slot.1, setup_start)` — гейт переполнения LZMA-слота (если recompress не влезает в budget, поток удлиняет файл и меняет паддинг).
4. Reparse-хвост искал `$SPF` только среди direct children sd-файла: за GUIDed-обёрткой лист — внук, а body обёртки — сжатый поток без plaintext `$SPF` — `unwrap()` на `None`. Фикс: рекурсивный `find_spf_leaf` (паттерн `find_leaf` из ami_patcher tests).
5. `\$SPF` — невалидный Rust-escape (утечка heredoc, класс 81f35b8); `sd_start = setup.len().max(8)` не соответствует раскладке — заменено поиском GUID-байтов заголовка sd-файла в `data` (санкционировано заметкой самого плана).

- [ ] **Step 2: Маркировка через LZMA-обёртку**

Механика маркировки от ami-patch-op уже в `hijack_form`: `mark_rebuild_to_root_by_path` от пути payload-секции (Task 4 возвращает именно его, ср. `patch_ami` ami_patcher.rs:49) поднимает Rebuild через LZMA-обёртку — отдельный фикс не требуется.

- [ ] **Step 3: Прогон + clippy + commit**

```bash
cargo test -p uefi-engine form_hijack && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/form_hijack.rs
git commit -m "test(uefi-engine): page-hijack-op Task 6 — LZMA slot-fit gate for hijack edits"
```

---

### Task 7: proto + RPC-хендлер

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-engine/src/rpc/server.rs`

**Interfaces:**
- Produces: RPC `HiiFormHijack(HiiFormHijackRequest) returns (HiiFormHijackResponse)`; сообщения `HiiFormHijackRecord { question_id, record_offset, old_failsafe, old_optimal, new_failsafe, new_optimal }`, `HiiFormHijackRequest { image_id, target, schema_json, setupdata_guid }`, `HiiFormHijackResponse { string_ids: map<string,uint32>, records: repeated HiiFormHijackRecord, form_ifr_start, form_ifr_end }`.

- [ ] **Step 1: proto-сообщения**

В `engine.proto` после строки `rpc HiiFormAdd(...)` (строка 31) добавить:

```proto
  rpc HiiFormHijack(HiiFormHijackRequest)               returns (HiiFormHijackResponse);
```

После блока `HiiFormAddResponse` (строка 192) добавить:

```proto
message HiiFormHijackRecord {
  uint32 question_id = 1;
  uint32 record_offset = 2;
  uint32 old_failsafe = 3;
  uint32 old_optimal = 4;
  uint32 new_failsafe = 5;
  uint32 new_optimal = 6;
}
message HiiFormHijackRequest {
  string image_id = 1;
  string target = 2;
  string schema_json = 3;
  string setupdata_guid = 4;
}
message HiiFormHijackResponse {
  map<string, uint32> string_ids = 1;
  repeated HiiFormHijackRecord records = 2;
  uint32 form_ifr_start = 3;
  uint32 form_ifr_end = 4;
}
```

Run: `cargo build -p uefi-proto 2>&1 | tail -3` — Expected: OK (build-script перегенерирует код).

- [ ] **Step 2: Тесты хендлера (в `mod tests` server.rs — зеркало `form_add_status`/`form_add_ok`)**

```rust
    async fn form_hijack_status(
        img: Image,
        target: &str,
        schema_json: &str,
        setupdata_guid: &str,
    ) -> Status {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let server = EngineServer {
            sm,
            images: Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)]))),
            data_dir: td.path().to_path_buf(),
        };
        server
            .hii_form_hijack(Request::new(HiiFormHijackRequest {
                image_id: "i".into(),
                target: target.into(),
                schema_json: schema_json.into(),
                setupdata_guid: setupdata_guid.into(),
            }))
            .await
            .unwrap_err()
    }

    const FORM_HIJACK_SCHEMA_JSON: &str = r#"{"title": "UEFIPATCHER", "questions": [
        {"question_id": 17, "prompt": "PQ", "help": "PH", "failsafe": 1, "optimal": 1}]}"#;

    #[tokio::test]
    async fn hii_form_hijack_maps_bad_schema_to_invalid_argument() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st = form_hijack_status(img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7", "{", "").await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn hii_form_hijack_maps_unknown_form_to_not_found() {
        let img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let st = form_hijack_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#99",
            FORM_HIJACK_SCHEMA_JSON,
            "",
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn hii_form_hijack_maps_missing_spf_to_not_found() {
        let mut img = parse_image(
            &crate::hii::form_hijack::test_fixtures::hijack_test_flash(),
            ImageMode::Write,
            "i",
            "s",
        )
        .unwrap();
        let sd_guid = Guid::try_parse("12345678-90AB-CDEF-1234-567890ABCDEF").unwrap();
        let vol = img
            .root
            .children
            .iter_mut()
            .find(|v| v.children.iter().any(|f| f.guid == Some(sd_guid)))
            .unwrap();
        vol.children.retain(|f| f.guid != Some(sd_guid));
        let st = form_hijack_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7",
            FORM_HIJACK_SCHEMA_JSON,
            "00000000-0000-0000-0000-00000000DEAD",
        )
        .await;
        assert_eq!(st.code(), tonic::Code::NotFound);
    }
```

Фикстура: выделить сборку флешки из тестов Task 5 в `pub(crate) fn hijack_test_flash() -> Vec<u8>` внутри `form_hijack.rs` (за пределами `mod tests`; возвращает `hijack_flash_image().0`) и переиспользовать в обоих местах. Пометка `#[cfg(test)]` на хелпере не нужна — `pub(crate)` + использование из `rpc/server.rs` тестов; чтобы не тащить в release-код, обернуть `#[cfg(any(test, feature = "…")))]` НЕ надо: положить в `form_hijack.rs` как `#[cfg(test)] pub(crate) fn` нельзя (не видно из другого крейта-внутреннего модуля? — можно: `#[cfg(test)]` виден всем `#[cfg(test)]`-сборкам юнит-тестов крейта, включая rpc/server.rs tests). Итог: `#[cfg(test)] pub(crate) fn hijack_test_flash() -> Vec<u8> { hijack_flash_image().0 }` рядом с tests-модулем — но тогда `hijack_flash_image` тоже должен жить вне `mod tests` или хелпер встраивает сборку напрямую. Простейший вариант: перенести `hijack_flash_image()` (и её под-хелперы section_bytes/ffs_file_bytes/flash_with_files/string_package_bytes/spf_container/package_with_form) из `mod tests` в `#[cfg(test)] pub(crate) mod test_fixtures` внутри form_hijack.rs; тесты Task 5/6 используют `test_fixtures::*`, rpc-тесты — `crate::hii::form_hijack::test_fixtures::hijack_test_flash()`.

- [ ] **Step 3: Падение**

Run: `cargo test -p uefi-engine hii_form_hijack 2>&1 | tail -3`
Expected: FAIL — метод `hii_form_hijack` не найден.

- [ ] **Step 4: Хендлер (по образцу `hii_form_set_add`, server.rs:718)**

```rust
    async fn hii_form_hijack(
        &self,
        req: Request<HiiFormHijackRequest>,
    ) -> RpcResult<HiiFormHijackResponse> {
        let r = req.into_inner();
        let schema = crate::hii::schema::parse_hijack_schema(&r.schema_json)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let setupdata_guid: Option<Guid> = if r.setupdata_guid.is_empty() {
            None
        } else {
            Some(
                Guid::try_parse(&r.setupdata_guid)
                    .map_err(|e| Status::invalid_argument(e.to_string()))?,
            )
        };
        let img = self.get_or_load_image(&r.image_id).await?;
        let result = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::form_hijack::hijack_form(img_slot, &r.target, &schema, setupdata_guid.as_ref())
                .map_err(|e| match e {
                    crate::hii::HiiError::AmiFilesNotFound => Status::not_found(e.to_string()),
                    _ => hii_error_status(e),
                })?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        Ok(Response::new(HiiFormHijackResponse {
            string_ids: result
                .string_ids
                .iter()
                .map(|(k, v)| (k.clone(), u32::from(*v)))
                .collect(),
            records: result
                .records
                .iter()
                .map(|e| HiiFormHijackRecord {
                    question_id: u32::from(e.question_id),
                    record_offset: e.record_offset as u32,
                    old_failsafe: u32::from(e.old_failsafe),
                    old_optimal: u32::from(e.old_optimal),
                    new_failsafe: u32::from(e.new_failsafe),
                    new_optimal: u32::from(e.new_optimal),
                })
                .collect(),
            form_ifr_start: result.form_ifr_start,
            form_ifr_end: result.form_ifr_end,
        }))
    }
```

Отдельные импорты не нужны: server.rs уже использует `use uefi_proto::*;` (glob покрывает HiiFormHijack*, как и соседние HiiFormAdd*).

- [ ] **Step 5: Прогон + clippy + commit**

```bash
cargo test -p uefi-engine hii_form_hijack && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-proto/proto/engine.proto crates/uefi-engine/src/rpc/server.rs crates/uefi-engine/src/hii/form_hijack.rs
git commit -m "feat(uefi-engine): page-hijack-op Task 7 — HiiFormHijack RPC (proto + handler + status mapping)"
```

---

### Task 8: CLI

**Files:**
- Modify: `crates/uefi-cli/src/client.rs`
- Modify: `crates/uefi-cli/src/commands/hii.rs`
- Modify: `crates/uefi-cli/src/main.rs`
- Modify: `crates/uefi-cli/src/output.rs`

**Interfaces:**
- Produces: `Client::hii_form_hijack(&self, &str, &str, &str, Option<&str>) -> Result<uefi_proto::HiiFormHijackResponse, AppError>`; команда `uefi-cli hii form hijack --target <item_id> --file <schema.json> [--setupdata-guid <GUID>]`.

- [ ] **Step 1: Тест парсинга аргументов (в `mod tests` main.rs, рядом с `parse_hii_formset_add_args`)**

```rust
    #[test]
    fn parse_hii_form_hijack() {
        let cli = Cli::try_parse_from([
            "uefi-cli", "hii", "form", "hijack",
            "--target", "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029",
            "--file", "schema.json",
            "--setupdata-guid", "FE612B72-203C-47B1-8560-A66D946EB371",
        ])
        .unwrap();
        let Cmd::Hii {
            sub: HiiCmd::Form {
                sub: HiiFormCmd::Hijack { target, file, setupdata_guid },
            },
        } = cli.command
        else {
            panic!("expected hii form hijack")
        };
        assert_eq!(target, "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029");
        assert_eq!(file, "schema.json");
        assert_eq!(
            setupdata_guid.as_deref(),
            Some("FE612B72-203C-47B1-8560-A66D946EB371")
        );
    }
```

(точная форма деструктуризации — по образцу существующего теста `parse_hii_form_gates_and_unlock` main.rs:472; вложенность `Cmd::Hii { sub: HiiCmd::Form { sub: HiiFormCmd::… } }`.)

- [ ] **Step 2: Падение**

Run: `cargo test -p uefi-cli parse_hii_form 2>&1 | tail -3`
Expected: FAIL — варианта `Hijack` нет.

- [ ] **Step 3: Реализация**

`main.rs` — вариант в enum подкоманд Form (рядом с Add):

```rust
        Hijack {
            #[arg(long)]
            target: String,
            #[arg(long)]
            file: String,
            #[arg(long)]
            setupdata_guid: Option<String>,
        },
```

Диспетчеризация (рядом с `HiiFormCmd::Add`):

```rust
                HiiFormCmd::Hijack { target, file, setupdata_guid } => {
                    commands::hii::form_hijack(
                        &target,
                        &file,
                        setupdata_guid.as_deref(),
                        sock,
                        format,
                    )
                    .await
                }
```

`client.rs` (рядом с `hii_form_add`, паттерн — `&mut self` + `self.inner.<method>(auth_req(&self.state, req))`):

```rust
    pub async fn hii_form_hijack(
        &mut self,
        image_id: &str,
        target: &str,
        schema_json: &str,
        setupdata_guid: Option<&str>,
    ) -> Result<uefi_proto::HiiFormHijackResponse, AppError> {
        let req = uefi_proto::HiiFormHijackRequest {
            image_id: image_id.into(),
            target: target.into(),
            schema_json: schema_json.into(),
            setupdata_guid: setupdata_guid.unwrap_or("").into(),
        };
        let resp = self
            .inner
            .hii_form_hijack(auth_req(&self.state, req))
            .await?
            .into_inner();
        Ok(resp)
    }
```

(импорты сгенерированных типов — как у соседних `HiiFormAddRequest`.)

`commands/hii.rs` (рядом с `form_add`):

```rust
pub async fn form_hijack(
    target: &str,
    file: &str,
    setupdata_guid: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let schema_json = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let resp = client
        .hii_form_hijack(&image_id, target, &schema_json, setupdata_guid)
        .await?;
    crate::output::print_form_hijack(&resp, format);
    Ok(())
}
```

`output.rs` (по образцу `print_form_add`): в text-режиме печатать заголовок, string_ids, по каждой записи `qid=… rec@0x{offset:X} fs {old}→{new} opt {old}→{new}`, диапазон `ifr [start..end)`; в json — сериализация resp.

- [ ] **Step 4: Прогон + clippy + commit**

```bash
cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings
git add crates/uefi-cli/src/client.rs crates/uefi-cli/src/commands/hii.rs crates/uefi-cli/src/main.rs crates/uefi-cli/src/output.rs
git commit -m "feat(uefi-cli): page-hijack-op Task 8 — 'hii form hijack' command"
```

---

### Task 9: Real-image гейт (HNX99TF)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs`

**Interfaces:**
- Consumes: `form_hijack::hijack_form`, `spf`, `schema::parse_hijack_schema`, существующие хелперы файла (`collect_forms`, парсинг образа, поиск слотов).

- [ ] **Step 1: Тест (в конец real_image.rs; образ по пути `../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin`)**

Цель: захват формы 10029 (хозяин 4G-вопроса 0x3B, E12-проверенный селектор `899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029`), вопрос 0x3B: title → «UEFIPATCHER E18 REALIMAGE», fs/opt → 1/1.

```rust
#[test]
#[ignore = "needs real HNX99TF firmware image"]
fn real_image_hii_form_hijack_built_bytes() {
    let data = std::fs::read("../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin").unwrap();
    let mut img = parse_image(&data, ImageMode::Write, "hijack", "s").unwrap();

    let item = "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029";
    let schema = schema::parse_hijack_schema(
        r#"{"title": "UEFIPATCHER E18 REALIMAGE", "questions": [
            {"question_id": 59, "prompt": "PATCHER 4G QUESTION", "help": "PATCHER 4G HELP",
             "failsafe": 1, "optimal": 1}]}"#,
    )
    .unwrap();

    let amitse_before = find_file_bytes(&img, "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A");
    let sd_before = find_file_bytes(&img, "FE612B72-203C-47B1-8560-A66D946EB371");

    let res = form_hijack::hijack_form(&mut img, item, &schema, None).unwrap();
    assert_eq!(res.records.len(), 1);
    assert_eq!(res.records[0].question_id, 59);
    assert_eq!(res.records[0].new_failsafe, 1);
    assert!(res.form_ifr_end > res.form_ifr_start);

    let built = builder::build_image(&img).unwrap();
    assert_eq!(built.len(), data.len(), "total flash length preserved");

    let re = parse_image(&built, ImageMode::Read, "re", "s").unwrap();
    assert_eq!(
        find_file_bytes(&re, "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A"),
        amitse_before,
        "AMITSE file must stay byte-identical"
    );
    let sd_after = find_file_bytes(&re, "FE612B72-203C-47B1-8560-A66D946EB371");
    assert_eq!(sd_after.len(), sd_before.len(), "$SPF file length invariant");
    assert_ne!(sd_after, sd_before, "$SPF content must change (fs/opt)");

    let pfs_body = find_spf_leaf_body(&re, "FE612B72-203C-47B1-8560-A66D946EB371");
    let recs = spf::scan_question_records(&pfs_body);
    let q4g = recs
        .iter()
        .find(|r| r.question_id == 59)
        .expect("4G record in built $SPF");
    assert_eq!(q4g.failsafe, 1);
    assert_eq!(q4g.optimal, 1);
    assert!(recs.len() >= 380, "record array must stay intact");

    let forms = collect_forms(&re);
    let f = forms
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .expect("hijacked form");
    assert_eq!(f.title, "UEFIPATCHER E18 REALIMAGE");

    let (setup_slot, sd_slot) = (
        find_file_range(&data, "899407D7-99FE-43D8-9A21-79EC328CAC21"),
        find_file_range(&data, "FE612B72-203C-47B1-8560-A66D946EB371"),
    );
    for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
        if a != b {
            assert!(
                setup_slot.contains(&i) || sd_slot.contains(&i),
                "byte {i:#x} changed outside the two AMI-adjacent slots"
            );
        }
    }

    eprintln!(
        "real_image hijack: form=10029 q=0x3B rec@{:#x} ifr[{:#x}..{:#x}) strings={}",
        res.records[0].record_offset,
        res.form_ifr_start,
        res.form_ifr_end,
        res.string_ids.len()
    );
}
```

Хелперы (локальные для файла, если отсутствуют): `find_file_bytes(&Image, guid) -> Vec<u8>` — тело файла (header+content) из дерева; `find_spf_leaf_body(&Image, guid) -> Vec<u8>` — DFS за leaf-секцией с `$SPF` в теле (образец — `find_leaf` из formset_add-тестов); `find_file_range(&[u8], guid) -> Range<usize>` — поиск первых 16 байт GUID файла в образе (FFS-хедеры уникальны) и диапазон `start..start + размер_файла` (размер из хедера `[20..23]` uint24 + 24 хедера; файл с выравниванием — расширить диапазон до следующего `0xFF`-паддинга не нужно: дифф confined к самому файлу, слот LZMA внутри него).

**Подводный камень:** `find_file_range` по GUID может найти GUID и в других местах образа (таблицы FV map); искать вхождение, за которым валидный FFS-хедер (filetype в `[18]`, checksum-байты ненулевые), либо — проще — искать все вхождения и брать то, что даёт консистентный размер ≤ длины образа. Если файл один — сработает первое.

- [ ] **Step 2: Прогон (explicit ignore)**

Run: `cargo test -p uefi-engine --test real_image real_image_hii_form_hijack -- --ignored 2>&1 | tail -5`
Expected: PASS. Возможные падения и их значение: `records.len() != 1` — двойной ключ qid+ifr не сошёлся (проверить, что ifr-диапазон [form_ifr_start..form_ifr_end) из результата покрывает 0xDD3); title не разрешился — appended-строки не в том строковом пакете (Setup-модуль содержит несколько string-пакетов — свериться с form-add real-image тестом, который успешно резолвит title).

- [ ] **Step 3: Полный ignored-прогон + clippy + commit**

```bash
cargo test -p uefi-engine --test real_image -- --ignored && cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): page-hijack-op Task 9 — HNX99TF real-image gate (records+strings in built bytes, diff confined to Setup+\$SPF slots)"
```

---

### Task 10: Документация + полная верификация

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Секция в TODO.md**

```markdown
### page-hijack-op: op `hii form hijack` готова, ждёт E18 на железе (2026-09-04)

Реализована in-place схема захвата страницы (спека
`docs/superpowers/specs/2026-09-04-page-hijack-op-design.md`, план
`docs/superpowers/plans/2026-09-04-page-hijack-op.md`): same-length
правки IFR (title/prompt/help string-id) + fs/opt в 72-байтовых записях
$SPF, атомарный precheck, дифф confined к Setup+$SPF слотам. Открытые
вопросы закрываются экспериментом E18 (диагностическая матрица исходов —
спека §6): строковый мост записей (0x1A3 vs 0x6C), источник заголовка
страницы, живость fs/opt. После E18: выбрать страницу-жертву с
пользователем, собрать e18-кандидат, прошить, заполнить матрицу.
```

- [ ] **Step 2: Полная верификация**

```bash
cargo test --all
cargo test -p uefi-engine --test real_image -- --ignored
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs(todo): page-hijack-op ready, E18 hardware experiment pending"
```

---

## Post-implementation (сессия, не код)

1. Пользователь выбирает страницу-жертву из живого меню HNX (критерии — спека §2/§7: видимая, скучная, ≥1 вопрос); маппинг «название → form-id» — `uefi-cli hii form list` + `hii string list`.
2. Запуск движка: `UEFIPATCHER_SOCK=/tmp/uefipatcher-e18.sock cargo run -p uefi-engine --bin engine`; образ → `hii form hijack` → `image save` → `refs/amibcp/e18-page-hijack.bin` (вне репозитория).
3. Оракул до прошивки: открыть e18-образ в AMIBCP, зафиксировать, видит ли он новые дефолты.
4. Прошивка пользователем, чеклист E18 (спека §5), заполнение диагностической матрицы (спека §6) — отчёт в `docs/reports/`.
