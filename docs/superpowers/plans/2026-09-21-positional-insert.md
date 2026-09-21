# positional-insert Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Позиционная вставка REF/вопроса внутрь формы (`insert_before` по GOTO-цели или qid) — кейс: вкладка «IntelRCSetup» между Advanced и Server Mgmt в баре корневого Setup 450x.

**Architecture:** Якорь резолвится внутри `splice_question_ops` (новый `InsertPos {End|BeforeGoto|BeforeQuestion}` + `locate_insert_at` с подъёмом к внешнему scope-блоку); JSON-поле `insert_before` в обеих схемах; `$SPF`-механика уже пороговая — без изменений; RPC/CLI не меняются (схема течёт строкой).

**Tech Stack:** Rust workspace (edition 2024), r-efi (IFR-константы), serde (deny_unknown_fields).

**Spec:** `docs/superpowers/specs/2026-09-21-positional-insert-design.md` (коммиты `a68d592`, `4e91bf2`)

## Global Constraints

- Ветка `positional-insert` (код — только в ней; от master, где уже лежит спека).
- TDD строго: (1) failing-тест → (2) `cargo test` red → (3) реализация → (4) green → (5) commit.
- После каждой задачи: `cargo test -p uefi-engine`, `cargo clippy -p uefi-engine -- -D warnings`, `cargo fmt --all -- --check`.
- Комментарии в коде — только rustdoc `///`-контракты на pub/pub(crate)-функциях (что делает / чего НЕ делает + `спека positional-insert §N`); narration-комментарии нет.
- IFR-константы — только `r_efi::hii::*` (`IFR_REF_OP`, `IFR_TRUE_OP` = 0x46, `IFR_SUPPRESS_IF_OP`, `IFR_END_OP`, `IFR_FORM_OP`, `IFR_FORM_SET_OP`, `PACKAGE_FORMS`); своих констант не вводить.
- JSON-схемы — `#[serde(deny_unknown_fields)]` сохраняется; новое поле опциональное (`#[serde(default, skip_serializing_if = "Option::is_none")]`) — старые схемы парсятся без изменений.
- Real-гейты (`#[ignore]`) не запускаются в обычном прогоне; явный запуск при наличии образов: `cargo test -p uefi-engine --test real_image -- --ignored real_amibcp_450x_positional_insert` (450x) / `real_image_ops_insert_positional_np` (HNX).
- Один коммит на шаг с `git commit`; сообщения — из плана.
- Дефекты плана/спеки, найденные в шаге — отдельный docs-коммит ДО реализации (AGENTS.md правило 11).

## Контекст для исполнителя (файлы и точки)

- `crates/uefi-engine/src/hii/ifr.rs:312` — `splice_question_ops` (вставка перед END формы); `:368` — `locate_form_end`; тест-хелперы в `mod tests`: `opcode(op, scope, payload)`, `form_set(&g, title)`, `form(form_id, title)`, `end()`, `package(&ifr)`, `one_of_question()` (one_of qid 0x31, vsid 1, voff 0x40), `two_form_package()`, константа `FORMSET_GUID`.
- `crates/uefi-engine/src/hii/schema.rs:225` — `QuestionAddSchema`; `:255` — `QuestionAddRefSchema`; `:275` — `parse_question_add_schema`.
- `crates/uefi-engine/src/hii/mod.rs`: `:1113` `check_rsrc_question_splice`, `:1152` `splice_question_ops_into_resource`, `:1406` `preflight_question_splice`, `:1475` `check_question_slots`, `:1544` `add_question`, `:1617` bare-splice, `:1897` `check_ref_slots`, `:1925` `add_ref`, `:1982` bare-splice; тесты `mod tests` c `:2429`.
- `crates/uefi-engine/src/hii/ref_variant.rs:26` — `parse_ref(op, stmt) -> Option<RefTarget>`; `RefTarget::{Form{form_id}, FormQuestion{form_id,..}, Formset{form_id,..}, Dynamic}` (pub(crate)).
- `crates/uefi-engine/src/hii/values.rs:106` — `is_question_op(op) -> bool` (pub(crate)); `:130` — `walk_statements`.
- REF-layout (ref_variant.rs, спека formset-unlock §2): header 11 байт, qid u16@+6, FormId u16@+13; REF len 15, REF3 len 33 (FormSetGuid@+17).
- `crates/uefi-engine/tests/real_image.rs`: `:24 amibcp_path()`, `:62-64` константы `RC_SETUP_FFS`/`ROOT_SETUP_FFS`/`RC_FORMSET_GUID`, `:71` образец real-гейта, `:1787 module_pe32_node_path`, `:1792 module_form_package`, `:4156-4165` NP-константы (`NP_SETUP_MODULE_GUID`, `NP_SETUPDATA_GUID`, `NP_PARENT_FORM_ID` = 10019, `NP_SERIAL_DIR`), `:4189 np_find_ref_op`, `find_spf_leaf_body`.
- `crates/uefi-tui/src/commands.rs:1008` и `:1209` — usage-строки `:hii question add`.
- Proto не меняется: `HiiQuestionAddRequest.schema_json` — строка (`crates/uefi-proto/proto/engine.proto:324`).

---

### Task 1: Схема: `insert_before` + правила qid 0

**Files:**
- Modify: `crates/uefi-engine/src/hii/schema.rs` (структуры `:225`/`:255`, валидация `:275`, tests)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (литералы тест-хелперов `:4441` `question_add_schema`, `:5088` `ref_add_schema` — добавить `insert_before: None`)
- Modify: `crates/uefi-engine/tests/real_image.rs` (литералы `:3980`, `:4029` `live_question_schema`, `:4205` `np_smoke_schema` — добавить `insert_before: None`)
- Create: ветка `positional-insert`

**Interfaces:**
- Consumes: `QuestionAddSchema`, `QuestionAddRefSchema`, `HiiError::InvalidSchema(String)`.
- Produces: `pub struct InsertBefore { pub goto_form_id: Option<u16>, pub question_id: Option<u16> }`; поля `pub insert_before: Option<InsertBefore>` в обеих схемах (serde default None) — Task 4 читает их конвертером `insert_pos_of`.

- [ ] **Step 1: Создать ветку**

```bash
git checkout -b positional-insert
```

- [ ] **Step 2: Написать failing-тесты** (в конец `mod tests` schema.rs, после существующих)

```rust
    #[test]
    fn parse_ref_insert_before_goto_form_id() {
        let s = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 0, "formset_guid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
                "insert_before": { "goto_form_id": 4104 } } ] }"#,
        )
        .unwrap();
        assert_eq!(s.refs[0].insert_before.unwrap().goto_form_id, Some(4104));
    }

    #[test]
    fn parse_insert_before_question_id() {
        let s = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 5, "insert_before": { "question_id": 528 } } ] }"#,
        )
        .unwrap();
        assert_eq!(s.refs[0].insert_before.unwrap().question_id, Some(528));
    }

    #[test]
    fn parse_insert_before_both_keys_rejected() {
        let err = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 5, "insert_before": { "goto_form_id": 2, "question_id": 3 } } ] }"#,
        );
        assert!(matches!(err, Err(HiiError::InvalidSchema(_))));
    }

    #[test]
    fn parse_insert_before_no_keys_rejected() {
        let err = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 5, "insert_before": {} } ] }"#,
        );
        assert!(matches!(err, Err(HiiError::InvalidSchema(_))));
    }

    #[test]
    fn parse_insert_before_unknown_key_rejected() {
        let err = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 5, "insert_before": { "offset": 16 } } ] }"#,
        );
        assert!(matches!(err, Err(HiiError::InvalidSchema(_))));
    }

    #[test]
    fn parse_question_qid_zero_rejected() {
        let err = parse_question_add_schema(
            r#"{ "questions": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 0, "var_store_id": 1, "var_offset": 0, "size": 1,
                "options": [] } ] }"#,
        );
        assert!(matches!(err, Err(HiiError::InvalidSchema(_))));
    }

    #[test]
    fn parse_ref_qid_zero_accepted() {
        let s = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 0 } ] }"#,
        )
        .unwrap();
        assert_eq!(s.refs[0].question_id, 0);
    }
```

- [ ] **Step 3: Запустить — убедиться в падении**

Run: `cargo test -p uefi-engine schema::`
Expected: FAIL — serde «unknown field `insert_before`» (deny_unknown_fields) в тестах с якорем; `parse_question_qid_zero_rejected` тоже FAIL (сейчас qid 0 проходит parse).

- [ ] **Step 4: Реализовать** (schema.rs)

Новое после `QuestionAddRefSchema` (`:255-262`):

```rust
/// Якорь позиционной вставки (спека positional-insert §1): ровно один
/// из ключей. goto_form_id — первый REF-стейтмент формы с этой целью
/// (бар: qid-0 GOTO); question_id — первый вопрос с этим qid.
/// НЕ резолвит якорь в байтах — это уровень splice (ifr::locate_insert_at).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertBefore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goto_form_id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<u16>,
}

fn validate_insert_before(ib: &InsertBefore) -> Result<(), HiiError> {
    match (ib.goto_form_id, ib.question_id) {
        (Some(_), Some(_)) => Err(HiiError::InvalidSchema(
            "insert_before: exactly one of goto_form_id/question_id is required, got both".into(),
        )),
        (None, None) => Err(HiiError::InvalidSchema(
            "insert_before: exactly one of goto_form_id/question_id is required, got none".into(),
        )),
        _ => Ok(()),
    }
}
```

В `QuestionAddSchema` (после `defaults`) и в `QuestionAddRefSchema` (после `formset_guid`) добавить одинаковое поле:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_before: Option<InsertBefore>,
```

В `parse_question_add_schema` (после проверки пустоты questions/refs, перед `Ok(s)`) добавить:

```rust
    for q in &s.questions {
        if q.question_id == 0 {
            return Err(HiiError::InvalidSchema(
                "question_id 0 is navigation-only (refs); storage questions require a nonzero question_id".into(),
            ));
        }
        if let Some(ib) = q.insert_before.as_ref() {
            validate_insert_before(ib)?;
        }
    }
    for r in &s.refs {
        if let Some(ib) = r.insert_before.as_ref() {
            validate_insert_before(ib)?;
        }
    }
```

- [ ] **Step 5: Прогнать тесты**

Run: `cargo test -p uefi-engine schema::`
Expected: PASS (все новые + существующие).

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check`
Expected: PASS (вне схемы никто не конструирует структуры буквально; при ошибках компиляции из-за нового поля — это структурные литералы в тестах/коде: добавить `insert_before: None`).

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/schema.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/tests/real_image.rs
git commit -m "feat(engine): insert_before {goto_form_id|question_id} в схемах add_question/add_ref — ровно один ключ, qid 0 только для refs (спека positional-insert §1)"
```

---

### Task 2: ifr.rs — `form_span` (реструктуризация `locate_form_end`)

Чистый рефакторинг: тот же проход возвращает и начало формы. `locate_form_end` становится обёрткой — поведение байт-в-байт прежнее.

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs:368-436` (+ tests)

**Interfaces:**
- Consumes: `locate_form_end(body, formset_idx, form_id) -> Option<usize>`.
- Produces: `pub(crate) fn form_span(body: &[u8], formset_idx: usize, form_id: u16) -> Option<(usize, usize)>` — (offset `IFR_FORM_OP`, offset END формы); `locate_form_end` остаётся (обёртка) для существующих вызовов mod.rs. Task 3 использует `form_span` в `locate_insert_at`.

- [ ] **Step 1: Failing-тест** (в `mod tests` ifr.rs, рядом с `splice_question_ops_*`)

```rust
    #[test]
    fn form_span_returns_form_header_and_end_offsets() {
        let pkg = two_form_package();
        let (start, end_off) = form_span(&pkg, 0, 100).unwrap();
        assert_eq!(pkg[start], IFR_FORM_OP);
        assert_eq!(u16::from_le_bytes([pkg[start + 2], pkg[start + 3]]), 100);
        assert_eq!(pkg[end_off], IFR_END_OP);
        assert!(end_off > start);
        assert!(form_span(&pkg, 0, 999).is_none());
        assert!(form_span(&pkg, 1, 100).is_none());
    }
```

- [ ] **Step 2: Red**

Run: `cargo test -p uefi-engine form_span`
Expected: FAIL — `form_span` не определён.

- [ ] **Step 3: Реализация**

Переименовать тело `locate_form_end` (`:368`) в `form_span`, сменив возвраты: `return Some(k);` (строка `:410`) → `return Some((j, k));` (j — offset FORM_OP, k — END). Над ним — обёртка:

```rust
pub(crate) fn locate_form_end(body: &[u8], formset_idx: usize, form_id: u16) -> Option<usize> {
    form_span(body, formset_idx, form_id).map(|(_, end)| end)
}

/// Границы формы (offset IFR_FORM_OP, offset её END) в формсете
/// formset_idx. Спека positional-insert §2.
pub(crate) fn form_span(
    body: &[u8],
    formset_idx: usize,
    form_id: u16,
) -> Option<(usize, usize)> {
    // ... тело прежнего locate_form_end с Some((j, k)) ...
}
```

- [ ] **Step 4: Green + регрессия**

Run: `cargo test -p uefi-engine ifr::`
Expected: PASS (новый + все `splice_question_ops_*`).

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs
git commit -m "refactor(engine): locate_form_end → form_span (start, end) — границы формы для позиционного якоря (спека positional-insert §2)"
```

---

### Task 3: ifr.rs — `InsertPos` + `locate_insert_at` + параметризованный splice

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (`:312` сигнатура, новый `locate_insert_at`, tests)
- Modify: `crates/uefi-engine/src/hii/mod.rs:1159,1617,1982` — механическая миграция на `InsertPos::End` (без изменения поведения)

**Interfaces:**
- Consumes: `form_span` (Task 2), `super::ref_variant::{parse_ref, RefTarget}`, `super::values::is_question_op`.
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum InsertPos { End, BeforeGoto(u16), BeforeQuestion(u16) }`
  - `pub(crate) fn locate_insert_at(body: &[u8], formset_idx: usize, form_id: u16, pos: InsertPos) -> Result<usize, HiiError>` — `Err(NotFound)` нет формы/формсета; `Err(InvalidSchema(msg))` якорь не найден (msg содержит листинг `0x{target:x}@0x{off:x}` доступных REF-целей / `{qid:#x}@0x{off:x}` qid'ов).
  - `pub fn splice_question_ops(package, formset_idx, form_id, pos: InsertPos, ops) -> Result<(usize, usize), HiiError>` — Task 4 передаёт реальные позиции.

- [ ] **Step 1: Failing-тесты** (в `mod tests` ifr.rs)

Фикстуры (рядом с `two_form_package`):

```rust
    fn goto(target_form: u16, qid: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0x51u16.to_le_bytes());
        p.extend_from_slice(&0x52u16.to_le_bytes());
        p.extend_from_slice(&qid.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        p.extend_from_slice(&target_form.to_le_bytes());
        opcode(IFR_REF_OP, false, &p)
    }

    fn bar_form_package() -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(100, 10));
        ifr.extend(goto(200, 0));
        ifr.extend(goto(300, 0));
        ifr.extend(goto(400, 0));
        ifr.extend(end());
        ifr.extend(end());
        package(&ifr)
    }
```

Тесты:

```rust
    #[test]
    fn splice_question_ops_inserts_before_anchor_goto() {
        let mut pkg = bar_form_package();
        let ops = opcode(
            0x05,
            true,
            &[0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10],
        );
        let (at, delta) =
            splice_question_ops(&mut pkg, 0, 100, InsertPos::BeforeGoto(300), &ops).unwrap();
        assert_eq!(delta, ops.len());
        assert_eq!(&pkg[at..at + delta], &ops[..]);
        let prev = at - 15;
        assert_eq!(pkg[prev], IFR_REF_OP);
        assert_eq!(u16::from_le_bytes([pkg[prev + 13], pkg[prev + 14]]), 200);
        let next = at + delta;
        assert_eq!(pkg[next], IFR_REF_OP);
        assert_eq!(u16::from_le_bytes([pkg[next + 13], pkg[next + 14]]), 300);
    }

    #[test]
    fn splice_question_ops_inserts_before_anchor_question() {
        let mut pkg = two_form_package(); // форма 100 c one_of qid 0x31
        let ops = opcode(
            0x05,
            true,
            &[0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10],
        );
        let (at, delta) =
            splice_question_ops(&mut pkg, 0, 100, InsertPos::BeforeQuestion(0x31), &ops).unwrap();
        // ops стоят ПЕРЕД one_of (не перед END): сразу за вставкой — question-op якоря
        assert_eq!(pkg[at + delta], IFR_ONE_OF_OP);
        assert_eq!(
            u16::from_le_bytes([pkg[at + delta + 6], pkg[at + delta + 7]]),
            0x31
        );
    }

    #[test]
    fn splice_anchor_inside_suppress_lifts_to_block_start() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(100, 10));
        ifr.extend(goto(200, 0));
        let mut sup = opcode(IFR_SUPPRESS_IF_OP, true, &[IFR_TRUE_OP, 0x02]);
        sup.extend(goto(300, 0));
        sup.extend(end());
        ifr.extend(sup);
        ifr.extend(goto(400, 0));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        let ops = opcode(
            0x05,
            true,
            &[0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10],
        );
        let (at, delta) =
            splice_question_ops(&mut pkg, 0, 100, InsertPos::BeforeGoto(300), &ops).unwrap();
        // вставка ПЕРЕД suppress-блоком (depth 0), не внутрь: за вставкой — SUPPRESS_IF
        assert_eq!(pkg[at + delta], IFR_SUPPRESS_IF_OP);
        let prev = at - 15;
        assert_eq!(pkg[prev], IFR_REF_OP, "перед вставкой — GOTO→200");
    }

    #[test]
    fn splice_question_ops_anchor_not_found_lists_targets() {
        let mut pkg = bar_form_package();
        let before = pkg.clone();
        let ops = opcode(
            0x05,
            true,
            &[0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10],
        );
        let err =
            splice_question_ops(&mut pkg, 0, 100, InsertPos::BeforeGoto(999), &ops).unwrap_err();
        match err {
            HiiError::InvalidSchema(msg) => {
                assert!(msg.contains("goto_form_id 0x3e7 not found"), "{msg}");
                assert!(msg.contains("0xc8@"), "REF-цель 200 в листинге: {msg}");
                assert!(msg.contains("0x12c@"), "REF-цель 300 в листинге: {msg}");
            }
            other => panic!("expected InvalidSchema, got {other:?}"),
        }
        assert_eq!(pkg, before, "пакет не изменён при ошибке якоря");
    }

    #[test]
    fn splice_question_ops_question_anchor_not_found_lists_qids() {
        let mut pkg = two_form_package();
        let ops = opcode(
            0x05,
            true,
            &[0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10],
        );
        let err =
            splice_question_ops(&mut pkg, 0, 100, InsertPos::BeforeQuestion(0x77), &ops)
                .unwrap_err();
        match err {
            HiiError::InvalidSchema(msg) => {
                assert!(msg.contains("question_id 0x77 not found"), "{msg}");
                assert!(msg.contains("0x31@"), "qid 0x31 в листинге: {msg}");
            }
            other => panic!("expected InvalidSchema, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Red**

Run: `cargo test -p uefi-engine splice`
Expected: FAIL компиляция — `InsertPos` не определён (и старые тесты не компилируются с новой сигнатурой — это ожидаемо, мигрируются в Step 3).

- [ ] **Step 3: Реализация (ifr.rs)**

Импорт наверху ifr.rs расширить (строка `:6`):

```rust
use super::ref_variant::{parse_ref, RefTarget};
use super::values::is_question_op;
```

(если `is_question_op` конфликтует — квалифицировать `values::is_question_op`).

```rust
/// Позиция вставки в форме (спека positional-insert §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPos {
    End,
    BeforeGoto(u16),
    BeforeQuestion(u16),
}

/// Offset вставки в форме: End — перед END формы; BeforeGoto/
/// BeforeQuestion — offset якорного стейтмента, поднятый к началу
/// самого внешнего охватывающего scope-блока (SUPPRESS_IF/GRAY_OUT_IF),
/// чтобы вставка не наследовала suppress якоря (спека
/// positional-insert §2, живой кейс GOTO→10008 в форме 10000).
/// NotFound — нет формы/формсета; InvalidSchema — якорь не найден,
/// сообщение содержит листинг доступных якорей формы. НЕ мутирует pkg.
pub(crate) fn locate_insert_at(
    body: &[u8],
    formset_idx: usize,
    form_id: u16,
    pos: InsertPos,
) -> Result<usize, HiiError> {
    let Some((form_op, form_end)) = form_span(body, formset_idx, form_id) else {
        return Err(HiiError::NotFound);
    };
    if pos == InsertPos::End {
        return Ok(form_end);
    }
    let form_hdr = (body[form_op + 1] & 0x7F) as usize;
    let mut refs: Vec<String> = Vec::new();
    let mut qids: Vec<String> = Vec::new();
    let mut scopes: Vec<usize> = Vec::new();
    let mut anchor: Option<usize> = None;
    let mut i = form_op + form_hdr;
    while i + 2 <= form_end {
        let op = body[i];
        let ls = body[i + 1];
        let len = (ls & 0x7F) as usize;
        if len < 2 || i + len > form_end {
            break;
        }
        if op == IFR_END_OP {
            scopes.pop();
            i += len;
            continue;
        }
        if op == IFR_REF_OP && len >= 15 {
            let target = match parse_ref(op, &body[i..i + len]) {
                Some(RefTarget::Form { form_id }) => Some(form_id),
                Some(RefTarget::FormQuestion { form_id, .. }) => Some(form_id),
                Some(RefTarget::Formset { form_id, .. }) => Some(form_id),
                _ => None,
            };
            if let Some(t) = target {
                refs.push(format!("{t:#x}@0x{i:x}"));
                if pos == InsertPos::BeforeGoto(t) && anchor.is_none() {
                    anchor = Some(scopes.first().copied().unwrap_or(i));
                }
            }
        }
        if is_question_op(op) && len >= 8 {
            let q = u16::from_le_bytes([body[i + 6], body[i + 7]]);
            qids.push(format!("{q:#x}@0x{i:x}"));
            if pos == InsertPos::BeforeQuestion(q) && anchor.is_none() {
                anchor = Some(scopes.first().copied().unwrap_or(i));
            }
        }
        if ls & 0x80 != 0 {
            scopes.push(i);
        }
        i += len;
    }
    anchor.map(Ok).unwrap_or_else(|| {
        Err(match pos {
            InsertPos::BeforeGoto(t) => HiiError::InvalidSchema(format!(
                "insert_before goto_form_id {t:#x} not found in form {form_id:#x}; available REF targets: {}",
                if refs.is_empty() { "(none)".to_string() } else { refs.join(", ") }
            )),
            InsertPos::BeforeQuestion(q) => HiiError::InvalidSchema(format!(
                "insert_before question_id {q:#x} not found in form {form_id:#x}; available question ids: {}",
                if qids.is_empty() { "(none)".to_string() } else { qids.join(", ") }
            )),
            InsertPos::End => unreachable!("handled above"),
        })
    })
}
```

`splice_question_ops` (`:312`) — новая сигнатура (параметр `pos` перед `ops`):

```rust
pub fn splice_question_ops(
    package: &mut Vec<u8>,
    formset_idx: usize,
    form_id: u16,
    pos: InsertPos,
    ops: &[u8],
) -> Result<(usize, usize), HiiError> {
    if ops.is_empty() {
        return Err(HiiError::InvalidSchema("empty ops".into()));
    }
    let insert_at = locate_insert_at(package, formset_idx, form_id, pos)?;
    // ... splice + u24-фиксап без изменений (прежние строки :324-330) ...
}
```

Миграция вызовов на `InsertPos::End`:
- ifr.rs tests: 6 мест (`:1181`, `:1214`, `:1217`, `:1256`, `:1260`, `:1271`) — добавить `InsertPos::End,` перед `&ops`.
- mod.rs: `:1159` (в `splice_question_ops_into_resource`), `:1617` (add_question bare), `:1982` (add_ref bare) — добавить `ifr::InsertPos::End,`.

- [ ] **Step 4: Green + регрессия**

Run: `cargo test -p uefi-engine`
Expected: PASS — новые 5 тестов + все существующие (мигрированные `End`-вызовы байт-в-байт прежнее поведение).

Run: `cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): InsertPos + locate_insert_at — позиционный splice (BeforeGoto/BeforeQuestion с подъёмом к scope-блоку, листинг якорей в ошибке); вызовы мигрированы на End (спека positional-insert §2)"
```

---

### Task 4: mod.rs — карве-аут qid 0 + конвертер + проброс позиций

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`check_ref_slots :1897`, `add_ref :1925`, `add_question :1544`, `preflight_question_splice :1406`, `check_rsrc_question_splice :1113`, `splice_question_ops_into_resource :1152`, `check_ref_add :2001`, `check_question_add :1651`; tests)
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (удаление обёртки `locate_form_end` после миграции последних вызывателей)

**Interfaces:**
- Consumes: `schema::InsertBefore` (Task 1), `ifr::InsertPos`/`locate_insert_at` (Task 3).
- Produces:
  - `fn insert_pos_of(ib: Option<&schema::InsertBefore>) -> Result<ifr::InsertPos, HiiError>` (private) — None→End; ровно один ключ → Before*, иначе InvalidSchema.
  - Сигнатуры (Task 5/6 и будущие вызовы): `preflight_question_splice(image, target, bare_channel, form_id, pos: ifr::InsertPos, ops_len, strings)`; `check_rsrc_question_splice(pe, form_id, pos: ifr::InsertPos, ops_len)`; `splice_question_ops_into_resource(pe, form_id, pos: ifr::InsertPos, ops)`.

- [ ] **Step 1: Failing-тесты** (в `mod tests` mod.rs)

```rust
    #[test]
    fn insert_pos_of_maps_schema_anchor() {
        assert!(matches!(insert_pos_of(None).unwrap(), ifr::InsertPos::End));
        assert!(matches!(
            insert_pos_of(Some(&schema::InsertBefore {
                goto_form_id: Some(0x1008),
                question_id: None,
            }))
            .unwrap(),
            ifr::InsertPos::BeforeGoto(0x1008)
        ));
        assert!(matches!(
            insert_pos_of(Some(&schema::InsertBefore {
                question_id: Some(528),
                goto_form_id: None,
            }))
            .unwrap(),
            ifr::InsertPos::BeforeQuestion(528)
        ));
        assert!(matches!(
            insert_pos_of(Some(&schema::InsertBefore {
                goto_form_id: Some(2),
                question_id: Some(3),
            })),
            Err(HiiError::InvalidSchema(_))
        ));
    }

    fn mini_ref_package() -> Vec<u8> {
        let mut ifr = Vec::new();
        let mut fs = vec![r_efi::hii::IFR_FORM_SET_OP, (2 + 20) | 0x80];
        fs.extend(std::iter::repeat_n(0u8, 20));
        ifr.extend_from_slice(&fs);
        let mut f = vec![r_efi::hii::IFR_FORM_OP, (2 + 4) | 0x80];
        f.extend_from_slice(&100u16.to_le_bytes());
        f.extend_from_slice(&10u16.to_le_bytes());
        ifr.extend_from_slice(&f);
        let mut r = vec![r_efi::hii::IFR_REF_OP, 15];
        r.extend_from_slice(&0x51u16.to_le_bytes());
        r.extend_from_slice(&0x52u16.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes());
        r.push(0);
        r.extend_from_slice(&200u16.to_le_bytes());
        ifr.extend_from_slice(&r);
        let mut q = vec![r_efi::hii::IFR_ONE_OF_OP, 13];
        q.extend_from_slice(&0x61u16.to_le_bytes());
        q.extend_from_slice(&0x62u16.to_le_bytes());
        q.extend_from_slice(&0x31u16.to_le_bytes());
        q.extend_from_slice(&1u16.to_le_bytes());
        q.extend_from_slice(&0x40u16.to_le_bytes());
        q.push(0);
        ifr.extend_from_slice(&q);
        ifr.extend_from_slice(&[r_efi::hii::IFR_END_OP, 0x02]);
        ifr.extend_from_slice(&[r_efi::hii::IFR_END_OP, 0x02]);
        let mut pkg = vec![0u8, 0, 0, r_efi::hii::PACKAGE_FORMS];
        let len = 4 + ifr.len();
        pkg[0] = (len & 0xFF) as u8;
        pkg[1] = ((len >> 8) & 0xFF) as u8;
        pkg[2] = ((len >> 16) & 0xFF) as u8;
        pkg.extend_from_slice(&ifr);
        pkg
    }

    #[test]
    fn check_ref_slots_allows_navigation_qid_zero() {
        let pkg = mini_ref_package(); // REF qid 0 → 200 и ONE_OF qid 0x31 в форме 100
        let nav = schema::QuestionAddRefSchema {
            form_id: 100,
            prompt: "P".into(),
            help: "H".into(),
            question_id: 0,
            formset_guid: None,
            insert_before: None,
        };
        check_ref_slots(&nav, &pkg, &[])
            .expect("qid 0 навигационный: без pending-коллизий");
        check_ref_slots(&nav, &pkg, &[0])
            .expect("два qid-0 REF в одном запросе легитимны (карве-аут pending_qids)");
        let mut clash = nav.clone();
        clash.question_id = 0x31;
        assert!(matches!(
            check_ref_slots(&clash, &pkg, &[]),
            Err(HiiError::InvalidSchema(_))
        ));
    }
```

- [ ] **Step 2: Red**

Run: `cargo test -p uefi-engine insert_pos_of && cargo test -p uefi-engine check_ref_slots`
Expected: FAIL — `insert_pos_of` не определён (компиляция); `check_ref_slots_allows_navigation_qid_zero` падает на втором expect: сегодня `pending_qids`, содержащий 0, даёт `InvalidSchema` (слот-чек REF'ов не видит — `is_question_op` без `IFR_REF_OP`; уточнение спеки §3 от 2026-09-21).

- [ ] **Step 3: Реализация (mod.rs)**

Карве-аут в `check_ref_slots` (`:1897`) — оба коллизия-чека только для ненулевых qid:

```rust
/// Коллизии qid для refs (спека positional-insert §3): qid 0 —
/// навигационный (барные GOTO), exempt от коллизий; ненулевой qid
/// не должен существовать в формсете и не дублируется в запросе.
fn check_ref_slots(
    schema: &schema::QuestionAddRefSchema,
    pkg: &[u8],
    pending_qids: &[u16],
) -> Result<(), HiiError> {
    let slots = values::scan_question_slots(pkg);
    if schema.question_id != 0 && slots.iter().any(|s| s.question_id == schema.question_id) {
        return Err(HiiError::InvalidSchema(format!(
            "question id {:#x} already exists in the formset",
            schema.question_id
        )));
    }
    if schema.question_id != 0 && pending_qids.contains(&schema.question_id) {
        return Err(HiiError::InvalidSchema(format!(
            "question id {:#x} duplicates an earlier question in the same request",
            schema.question_id
        )));
    }
    Ok(())
}
```

Конвертер (рядом с `check_ref_slots`):

```rust
/// Schema-якорь → позиция splice (спека positional-insert §3).
/// Ровно один ключ проверяется и здесь: add_ref/add_question принимают
/// схему в обход parse (структурой из кода).
fn insert_pos_of(ib: Option<&schema::InsertBefore>) -> Result<ifr::InsertPos, HiiError> {
    match ib {
        None => Ok(ifr::InsertPos::End),
        Some(a) => match (a.goto_form_id, a.question_id) {
            (Some(f), None) => Ok(ifr::InsertPos::BeforeGoto(f)),
            (None, Some(q)) => Ok(ifr::InsertPos::BeforeQuestion(q)),
            _ => Err(HiiError::InvalidSchema(
                "insert_before: exactly one of goto_form_id/question_id is required".into(),
            )),
        },
    }
}
```

Проброс позиций (все правки механические, по образцу существующего кода):

- `check_rsrc_question_splice(pe, form_id, pos, ops_len)`: строку `ifr::locate_form_end(pkg, 0, form_id).ok_or(HiiError::NotFound)?;` (`:1122`) → `ifr::locate_insert_at(pkg, 0, form_id, pos)?;`
- `splice_question_ops_into_resource(pe, form_id, pos, ops)`: `:1157` чек с `pos`, `:1159` → `ifr::splice_question_ops(&mut pkg, 0, form_id, pos, ops)?`
- `preflight_question_splice(image, target, bare_channel, form_id, pos, ops_len, strings)`: bare-ветка `:1417-1419` → `ifr::locate_insert_at(&node.body, 0, form_id, pos).map(|_| ())` — ошибка течёт как есть: NotFound — нет формы, InvalidSchema с листингом — нет якоря (спека §3 «message листинга идентичен runtime-ошибке»; правка от 2026-09-21 — исходный вариант `.map(|_| ()).ok_or(...)` не компилировался: `ok_or` для Option, а `locate_insert_at` возвращает Result); resource-ветка `:1431` → `check_rsrc_question_splice(&post_strings, form_id, pos, ops_len)?`
- `add_question` (`:1544`): после `let qt = resolve_question_target…` добавить `let pos = insert_pos_of(schema.insert_before.as_ref())?;`; `:1584` preflight — добавить `pos,`; `:1613-1620` — bare: `ifr::splice_question_ops(&mut node.body, 0, qt.form_id, pos, &ops)?`, resource: `splice_question_ops_into_resource(&mut node.body, qt.form_id, pos, &ops)?`
- `add_ref` (`:1925`): аналогично — `let pos = insert_pos_of(schema.insert_before.as_ref())?;` после qt; `:1950` preflight `pos,`; `:1978-1986` — `pos` в оба канала
- `check_question_add` (`:1651`) и `check_ref_add` (`:2001`): в циклах preflight получает `insert_pos_of(s.insert_before.as_ref())?` для каждой схемы (`pos` вычисляется в цикле — там где берётся `schema`/`refs[i]`).

Позиции вычисляются один раз на вызов и идут и в preflight, и в splice (оба резолвят по свежему телу — string-pack меняет тело между preflight и splice, координаты не переносятся).

После миграции `:1122` и `:1417` у обёртки `ifr::locate_form_end` не остаётся вызывателей — удалить её в этой же задаче (иначе `dead_code` под `clippy -D warnings`; спека positional-insert §2, уточнение от 2026-09-21: обёртка — временная, на время Task 2-3).

- [ ] **Step 4: Green + регрессия**

Run: `cargo test -p uefi-engine`
Expected: PASS.

Run: `cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/ifr.rs
git commit -m "feat(engine): проброс insert_before в add_ref/add_question (оба канала + preflight), карве-аут навигационного qid 0 в check_ref_slots (спека positional-insert §3)"
```

---

### Task 5: NP-интеграционный тест — позиционный add_ref на resource-канале + $SPF-порог

Полный путь: schema→preflight→string-pack→позиционный splice→$SPF-fixup→build→re-parse. Полигон HNX (тот же, что `real_image_ops_insert_serial_np3`). Якорь — **первый depth-0 question-op формы 10019 по прямому IFR-обходу** (qid u16@+6 из байтов стейтмента; без hardcoded qid). Почему не `$SPF`-запись: живой факт 2026-09-21 (раунд 2) — единственный $SPF-рекорд формы q35 CHECKBOX@0x954 скоуплен (SUPPRESS_IF@0x94c), depth-0 записей в `$SPF` нет (depth-0 ONE_OF q34@0x921 без записи); depth-0 якорь нужен, потому что скоупленный лифтится перед блок (спека §2) и ассерт «следующий стейтмент — якорь» не выполняется. `$SPF`-записи остаются для порогового инварианта: insert_at == anchor_off (depth-0 — лифт identity), сдвиг 15 для ifr_offset >= anchor_off.

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новый тест после `real_image_ops_insert_serial_np3`, ~`:5140`)

**Interfaces:**
- Consumes: `add_ref(&mut image, &item, &schema) -> AddRefResult`; `parse_question_add_schema`; `np_ref.json` fixture (`{NP_SERIAL_DIR}/np_ref.json`, REF qid 528, форма `NP_NEW_FORM_ID` — поле перезаписываем на `NP_PARENT_FORM_ID`); `module_form_package`, `module_pe32_node_path`, `find_spf_leaf_body`, `np_find_ref_op(pkg, form_id, qid)`, `uefi_engine::hii::spf_record_resolves`, `spf::scan_question_records`, `form_hijack::locate_form(pkg, form_id)` (поля `form_op`/`next_form_op`); константы `NP_SETUP_MODULE_GUID`, `NP_SETUPDATA_GUID`, `NP_PARENT_FORM_ID` (= 10019).

- [ ] **Step 1: Написать тест**

```rust
/// Позиционная вставка на NP-полигоне (спека positional-insert
/// acceptance 2): add_ref c insert_before {question_id} в форму 10019
/// (resource-канал) — REF встаёт перед якорным stock-вопросом
/// (якорь — первый depth-0 question-op формы по IFR-обходу: вне
/// gate-блоков, лифт identity — вставка ровно перед вопросом;
/// $SPF-записи формы скоуплены, якорем быть не могут);
/// $SPF-записи с ifr_offset >= anchor_off (= insert_at) сдвигаются
/// на 15, до — нет.
#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_positional_np() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::schema::parse_question_add_schema;
    use uefi_engine::hii::spf;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "i1", "s").unwrap();

    let stock_pkg = module_form_package(&img, &module_pe32_node_path(&img, NP_SETUP_MODULE_GUID));
    let stock_spf = find_spf_leaf_body(&img, NP_SETUPDATA_GUID);
    let stock_recs: Vec<(u16, u32, usize)> = spf::scan_question_records(&stock_spf)
        .into_iter()
        .filter(|r| uefi_engine::hii::spf_record_resolves(&stock_pkg, r.question_id, r.ifr_offset))
        .map(|r| (r.question_id, r.ifr_offset, r.offset))
        .collect();
    let span = uefi_engine::hii::form_hijack::locate_form(&stock_pkg, NP_PARENT_FORM_ID)
        .expect("форма 10019 в stock");
    let form_hdr = (stock_pkg[span.form_op + 1] & 0x7F) as usize;
    let mut depth = 0usize;
    let mut anchor: Option<(u16, u32)> = None;
    let mut i = span.form_op + form_hdr;
    while i + 2 <= span.next_form_op {
        let ls = stock_pkg[i + 1];
        let len = (ls & 0x7F) as usize;
        if len < 2 || i + len > span.next_form_op {
            break;
        }
        let op = stock_pkg[i];
        if op == r_efi::hii::IFR_END_OP {
            depth = depth.saturating_sub(1);
        } else {
            let is_question = matches!(
                op,
                r_efi::hii::IFR_ONE_OF_OP
                    | r_efi::hii::IFR_CHECKBOX_OP
                    | r_efi::hii::IFR_NUMERIC_OP
                    | r_efi::hii::IFR_PASSWORD_OP
                    | r_efi::hii::IFR_ORDERED_LIST_OP
                    | r_efi::hii::IFR_STRING_OP
                    | r_efi::hii::IFR_DATE_OP
                    | r_efi::hii::IFR_TIME_OP
                    | r_efi::hii::IFR_ACTION_OP
            );
            if depth == 0 && is_question && len >= 8 && anchor.is_none() {
                anchor = Some((
                    u16::from_le_bytes([stock_pkg[i + 6], stock_pkg[i + 7]]),
                    i as u32,
                ));
            }
            if ls & 0x80 != 0 {
                depth += 1;
            }
        }
        i += len;
    }
    let (anchor_qid, anchor_off) = anchor.expect("depth-0 question-op в форме 10019");

    let ref_json = std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("np_ref.json"))
        .unwrap();
    let mut list = parse_question_add_schema(&ref_json).unwrap();
    let rs = &mut list.refs[0];
    rs.form_id = NP_PARENT_FORM_ID;
    rs.question_id = 528;
    rs.insert_before = Some(uefi_engine::hii::schema::InsertBefore {
        goto_form_id: None,
        question_id: Some(anchor_qid),
    });
    let item = format!("{NP_SETUP_MODULE_GUID}:0x10:0#{NP_PARENT_FORM_ID}");
    uefi_engine::hii::add_ref(&mut img, &item, rs)
        .expect("позиционный add_ref перед stock-вопросом 10019");

    let built = build_image(&img).unwrap();
    assert_eq!(built.len(), data.len(), "total flash length preserved");

    let re = parse_image(&built, ImageMode::Read, "re", "s").unwrap();
    let final_pkg = module_form_package(&re, &module_pe32_node_path(&re, NP_SETUP_MODULE_GUID));
    let r_off = np_find_ref_op(&final_pkg, NP_PARENT_FORM_ID, 528).expect("REF 528 в 10019");
    assert_eq!(final_pkg[r_off + 1] & 0x7F, 15, "plain REF");
    // вставлен ПЕРЕД якорным вопросом: следующий стейтмент — якорь
    let next = r_off + 15;
    assert!(
        next + 8 <= final_pkg.len(),
        "за REF должен быть стейтмент якоря"
    );
    assert_eq!(
        u16::from_le_bytes([final_pkg[next + 6], final_pkg[next + 7]]),
        anchor_qid,
        "за REF — якорный stock-вопрос (не END формы)"
    );

    let final_spf = find_spf_leaf_body(&re, NP_SETUPDATA_GUID);
    let final_recs = spf::scan_question_records(&final_spf);
    for (qid, off, _) in &stock_recs {
        let fr = final_recs
            .iter()
            .find(|f| f.question_id == *qid)
            .expect("stock live record must survive");
        let expected = off + if *off >= anchor_off { 15 } else { 0 };
        assert_eq!(
            fr.ifr_offset, expected,
            "$SPF-порог q{qid}: записи после insert_at сдвигаются на 15, до — нет"
        );
        assert!(
            uefi_engine::hii::spf_record_resolves(&final_pkg, fr.question_id, fr.ifr_offset),
            "record q{qid} остаётся resolving в final"
        );
    }
    eprintln!(
        "np positional: REF 528 перед q{anchor_qid} (insert@{anchor_off:#x}), $SPF-записей проверено {}",
        stock_recs.len()
    );
}
```

- [ ] **Step 2: Прогнать (компиляция + обычный прогон без ignore)**

Run: `cargo test -p uefi-engine --test real_image`
Expected: PASS (тест пропущен как ignored; сборка без ошибок).

- [ ] **Step 3: Прогнать на живом образе (если refs/fw/ доступен)**

Run: `cargo test -p uefi-engine --test real_image -- --ignored real_image_ops_insert_positional_np`
Expected: PASS. Если образа нет — шаг пропускается, отмечается в отчёте (гейт за живыми данными).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(engine): NP-гейт позиционного add_ref — REF перед якорем, \$SPF-порог 15 байт (спека positional-insert acceptance 2)"
```

---

### Task 6: Real-гейт 450x — REF3 qid 0 перед GOTO→10008 в форме 10000

Порядок целей формы 10000 зафиксирован вердиктом §7 formset-unlock (живые данные 2026-09-12: 10001/10002/10008/10009/10010/10012). При расхождении на живом прогоне — отдельный docs-коммит с фактом (правило 11), не молчаливая правка теста.

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новый тест после `real_amibcp_450x_formset_unlock`, ~`:186`)

**Interfaces:**
- Consumes: `amibcp_path()`, `count_files`, `ROOT_SETUP_FFS`, `module_form_package`, `module_pe32_node_path`, `form_hijack::locate_form`, `parse_image(&[u8], ImageMode, &str, &str)`, `uefi_engine::hii::{add_ref, schema::parse_question_add_schema}`, `uefi_engine::builder::build_image`.

- [ ] **Step 1: Написать тест**

```rust
/// Живой гейт позиционной вставки на 450x (спека positional-insert
/// acceptance 4): REF3 qid 0 insert_before {goto_form_id: 10008} в
/// корневую форму 10000 — пункт «IntelRCSetup» между Advanced и Server
/// Mgmt; вставка перед suppress-блоком Chipset (depth 0), не внутрь.
#[test]
#[ignore = "requires external real AMI image under refs/amibcp/ (gitignored)"]
fn real_amibcp_450x_positional_insert() {
    let data = std::fs::read(amibcp_path()).unwrap();
    let mut image = parse_image(&data, ImageMode::Write, "t", "s").unwrap();
    let files_before = count_files(&image.root);
    let schema_json = r#"{
        "refs": [ { "form_id": 1, "prompt": "Intel RC Setup",
                    "help": "Intel RC Setup Configuration",
                    "question_id": 0,
                    "formset_guid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
                    "insert_before": { "goto_form_id": 10008 } } ]
    }"#;
    let list = uefi_engine::hii::schema::parse_question_add_schema(schema_json).unwrap();
    let root_item = format!("{ROOT_SETUP_FFS}:0x10:0#10000");
    uefi_engine::hii::add_ref(&mut image, &root_item, &list.refs[0])
        .expect("позиционный REF3 qid 0 перед GOTO→10008 в форме 10000");

    let rebuilt = uefi_engine::builder::build_image(&image).unwrap();
    assert!(
        rebuilt.len() >= data.len(),
        "reloc-aware рост .rsrc не должен уменьшать образ: {} < {}",
        rebuilt.len(),
        data.len()
    );
    let re = parse_image(&rebuilt, ImageMode::Read, "t2", "s").unwrap();
    assert_eq!(
        count_files(&re.root),
        files_before,
        "вставка не должна терять файлы"
    );

    let re_pkg = module_form_package(&re, &module_pe32_node_path(&re, ROOT_SETUP_FFS));
    let span = uefi_engine::hii::form_hijack::locate_form(&re_pkg, 10000).expect("форма 10000");
    let mut seen: Vec<(u16, u16, usize, usize)> = Vec::new(); // (target, qid, off, scope_depth)
    let mut scopes: Vec<usize> = Vec::new();
    let mut i = span.form_op;
    while i + 2 <= span.next_form_op {
        let len = (re_pkg[i + 1] & 0x7F) as usize;
        if len < 2 {
            break;
        }
        let op = re_pkg[i];
        if op == r_efi::hii::IFR_END_OP {
            scopes.pop();
        } else {
            if op == r_efi::hii::IFR_REF_OP && len >= 15 {
                seen.push((
                    u16::from_le_bytes([re_pkg[i + 13], re_pkg[i + 14]]),
                    u16::from_le_bytes([re_pkg[i + 6], re_pkg[i + 7]]),
                    i,
                    scopes.len(),
                ));
            }
            if re_pkg[i + 1] & 0x80 != 0 {
                scopes.push(i);
            }
        }
        i += len;
    }
    let targets: Vec<u16> = seen.iter().map(|(t, ..)| *t).collect();
    let pos_new = targets
        .iter()
        .position(|t| *t == 1)
        .expect("REF3 → IntelRCSetup#1 в форме 10000");
    assert_eq!(
        pos_new, 2,
        "новый REF третий — после 10001/10002, перед 10008: {targets:?}"
    );
    assert_eq!(&targets[..2], &[10001, 10002]);
    assert_eq!(&targets[3..], &[10008, 10009, 10010, 10012]);
    assert_eq!(seen[pos_new].1, 0, "qid 0 — вкладочный");
    assert_eq!(
        seen[pos_new].3, 0,
        "вставка на depth 0 (перед suppress-блоком Chipset, не внутрь)"
    );
    eprintln!(
        "450x positional: порядок REF-целей формы 10000: {targets:?}, рост образа {} байт",
        rebuilt.len() - data.len()
    );
}
```

- [ ] **Step 2: Прогон (компиляция; без ignore)**

Run: `cargo test -p uefi-engine --test real_image`
Expected: PASS (ignored-пропуск, сборка чистая).

- [ ] **Step 3: Живой прогон при наличии образа**

Run: `cargo test -p uefi-engine --test real_image -- --ignored real_amibcp_450x_positional_insert`
Expected: PASS. Нет образа — отметить пропуск в отчёте.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(engine): 450x-гейт позиционной вставки — REF3 qid 0 перед GOTO→10008 в 10000, порядок бара и depth 0 (спека positional-insert acceptance 4)"
```

---

### Task 7: TUI help + финальный прогон

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs:1209-1216` (usage `:hii question add`)

**Interfaces:**
- Consumes: готовая функциональность Tasks 1-6.
- Produces: упоминание `insert_before` в help; зелёный полный прогон.

- [ ] **Step 1: Правка usage-строк** (`:1209`, `:1213`, `:1216`)

`:1209` (ветка wrong-subcommand) — заменить:

```rust
return Err("usage: :hii question add TARGET#FORM FILE".into());
```

на:

```rust
return Err(
    "usage: :hii question add TARGET#FORM FILE (insert_before {goto_form_id|question_id} в JSON задаёт позицию; без — конец формы)".into(),
);
```

`:1213` и `:1216` (`.ok_or(...)`) — заменить каждую:

```rust
.ok_or("usage: :hii question add TARGET#FORM FILE")?;
```

на:

```rust
.ok_or("usage: :hii question add TARGET#FORM FILE (insert_before {goto_form_id|question_id} в JSON задаёт позицию; без — конец формы)")?;
```

(Главная usage-строка `:1008` не меняется — уже длинная.)

- [ ] **Step 2: Прогон TUI**

Run: `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all -- --check`
Expected: PASS (если в тестах TUI есть ассерты на точный текст usage — обновить литералы вместе со строками).

- [ ] **Step 3: Финальный полный прогон**

Run: `cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-tui/src/commands.rs
git commit -m "feat(tui): help :hii question add упоминает insert_before (спека positional-insert §4)"
```

- [ ] **Step 5: Live-гейт за владельцем (не код)**

Сценарий — спека §Live: чистая копия `refs/amibcp/450x — копия.bin` → `:hii question add 899407D7-…:0x10:0#10000 <refs-schema.json>` (schema из §1: qid 0, formset EC87D643-…, форма 1, `insert_before {goto_form_id: 10008}`) → `:image save` → прошивка → SOL-проверка вкладки «IntelRCSetup» между Advanced и Server Mgmt (F9 после флеша обязателен). Развилки — спека §Live п.4. По вердикту — закрыть TODO:3896 docs-коммитом в master.
