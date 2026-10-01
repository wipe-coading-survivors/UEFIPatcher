# Ref-Guard-True-Flip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Третий flip-класс `ConstantTrue` (suppress_if/grayout_if TRUE → FALSE, 1 байт) + валидация цели REF в `add_ref`/`check_ref_add` (intra-формсет и REF3 глобально).

**Architecture:** `GateExpr::False` декодируется симметрично `True`; `plan_flip` получает arm для `True` (opcode swap 0x46→0x47 @ expr_offset); `is_unlocked_expr` считает `False` открытым. `validate_ref_target(image, qt, schema)` — общий хелпер: plain REF против форм формсета-владельца (`parse_form_package_sets(qt.pkg)[qt.formset_idx]`), REF3 против глобальной карты `collect_forms`; вызывается в `add_ref` до мутаций и в цикле `check_ref_add`.

**Tech Stack:** Rust workspace, r-efi (`IFR_TRUE_OP`/`IFR_FALSE_OP` уже есть). CLI/TUI/RPC/WebUI — без изменений.

Спека: `docs/superpowers/specs/2026-10-01-ref-guard-true-flip-design.md`.

## Global Constraints

- Никаких комментариев в коде (rustdoc `///`-контракты на pub/pub(crate)-функциях — можно).
- Никаких `wrapping_*`/молчаливых сужений; bounds — через `package_bounds`.
- Порядок ошибок мутаторов: InvalidItemId → NotFound → InvalidSchema (цель) → InvalidSchema (коллизии qid) → NotWritable остаётся первым гейтом режима (как сегодня).
- Тест на обрезанный/патологический ввод — с точным ожиданием.
- После каждой задачи: `cargo test -p uefi-engine` + `cargo clippy -p uefi-engine --all-targets -- -D warnings`; финальный гейт: `cargo test --all`, `cargo clippy --all --all-targets -- -D warnings`, `cargo fmt --all -- --check`.
- Один коммит на задачу (шаг «Commit»).

---

### Task 1: gates.rs — `GateExpr::False` декодирование

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs` (enum `GateExpr` ~:46, `decode_expr` ~:70, tests)

**Interfaces:**
- Produces: вариант `GateExpr::False`; `decode_expr` распознаёт одиночный `IFR_FALSE_OP` (импорт константы из r_efi::hii).

- [ ] **Step 1: RED-тесты** (в `mod tests`, рядом с `decode_true`; добавить хелпер `false_op() -> Vec<u8>` = `[IFR_FALSE_OP, 0x02]`)

```rust
#[test]
fn decode_false() {
    assert_eq!(decode_expr(&false_op()), GateExpr::False);
}

#[test]
fn decode_true_and_false_are_distinct() {
    assert_eq!(decode_expr(&true_op()), GateExpr::True);
    assert_eq!(
        decode_expr(&concat(&[true_op(), false_op()])),
        GateExpr::Other,
        "не одиночный опкод — не константный класс"
    );
}
```

- [ ] **Step 2:** `cargo test -p uefi-engine gates::` — падает (нет варианта `False`).
- [ ] **Step 3: GREEN** — вариант + arm в `decode_expr` (`[(IFR_FALSE_OP, _)] => GateExpr::False`), импорт `IFR_FALSE_OP`.
- [ ] **Step 4:** `cargo test -p uefi-engine` зелёный; clippy clean.
- [ ] **Step 5: Commit** — `feat(engine): GateExpr::False — декодирование константного FALSE (ref-guard-true-flip §1)`

### Task 2: gates.rs — arm `ConstantTrue` в plan_flip + is_unlocked_expr

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs` (`plan_flip` ~:312, `is_unlocked_expr` ~:395, tests)

**Interfaces:**
- Produces: `plan_flip(GateExpr::True)` → `Some(PlannedFlip { offset: expr_offset, from: [0x46], to: [0x47] })`; `is_unlocked_expr(False) = true`.

- [ ] **Step 1: RED-тесты** (хелпер `true_gate_ifr()`: form_set + suppress_if(TRUE) + form(901) + END'ы — по образцу `plan_eq_const_distinct_operands_have_no_flip`)

```rust
#[test]
fn plan_true_flip_swaps_opcode_byte() {
    let pkg = package(&true_gate_ifr());
    let gates = find_gates(&pkg, &GateTarget { form_id: 901, question_id: None, formset_guid: None });
    assert_eq!(gates[0].expr, GateExpr::True);
    let flips = plan_gates(&pkg, &gates).unwrap();
    assert_eq!(flips.len(), 1);
    assert_eq!(flips[0].offset, gates[0].expr_offset);
    assert_eq!(flips[0].from, vec![IFR_TRUE_OP]);
    assert_eq!(flips[0].to, vec![IFR_FALSE_OP]);
}

#[test]
fn plan_true_after_flip_is_unlocked_and_skipped() {
    let pkg = package(&true_gate_ifr());
    let mut body = pkg.clone();
    let flips = plan_gates(&body, &find_gates(&body, ...)).unwrap();
    apply_flips(&mut body, &flips).unwrap();
    let gates = find_gates(&body, ...);
    assert_eq!(gates[0].expr, GateExpr::False);
    assert!(plan_gates_skip_unlocked(&body, &gates).unwrap().is_empty());
    assert!(plan_gates(&body, &gates).is_err(), "повторное открытие запрещено");
}

#[test]
fn plan_true_errs_when_expr_offset_beyond_body() {
    // hand_gate(GateExpr::True, expr_offset = pkg.len()+10) → Err "gate bounds out of package"
}

#[test]
fn is_unlocked_expr_false_is_unlocked() {
    assert!(is_unlocked_expr(&GateExpr::False));
    assert!(!is_unlocked_expr(&GateExpr::True));
}
```

- [ ] **Step 2:** падают (True → Ok(None) сегодня).
- [ ] **Step 3: GREEN** — arm `GateExpr::True =>` в `plan_flip` после EqIdVal: `from`-байт сверяется с телом (`body.get(expr_offset) == Some(&IFR_TRUE_OP)`, иначе Err — ручной Gate с рассинхроном декодера не паникует); `GateExpr::False => true` в `is_unlocked_expr`.
- [ ] **Step 4:** весь crate зелёный; clippy clean.
- [ ] **Step 5: Commit** — `feat(engine): plan_flip ConstantTrue — TRUE→FALSE length-preserving (ref-guard-true-flip §1)`

### Task 3: mod.rs — expr_text(False) + интеграционный unlock TRUE-гейта

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`expr_text` ~:285, tests)

**Interfaces:**
- Produces: `expr_text(False) = "false"`; `gates_list`/`unlock` едят TRUE-гейты (flippable, applied `"pkg+0x…: 46 -> 47"`).

- [ ] **Step 1: RED-тесты** (по образцу существующих unlock-тестов hii::tests; фикстура: suppress_if TRUE → GOTO/форма в writable-образе)

```rust
#[test]
fn unlock_flips_constant_true_gate() {
    // фикстура с suppress_if(TRUE) вокруг формы-таргета
    let out = unlock(&mut image, ITEM).unwrap();
    assert!(out.applied.iter().any(|t| t.contains("46 -> 47")));
    // повторный unlock: no-op, без ошибки; gates_list до: flippable=true
}

#[test]
fn expr_text_false() {
    assert_eq!(expr_text(&gates::GateExpr::False, &[]), "false");
}
```

- [ ] **Step 2:** падают.
- [ ] **Step 3: GREEN** — arm `False` в `expr_text` (если тест unlock уже проходит за счёт Task 2 — зафиксировать как регрессию, RED-стадия объясняет это в коммите).
- [ ] **Step 4:** crate зелёный; clippy clean.
- [ ] **Step 5: Commit** — `feat(engine): unlock ConstantTrue-гейтов — expr_text/integration (ref-guard-true-flip §1)`

### Task 4: mod.rs — validate_ref_target: plain REF (intra-формсет) + wiring

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (новый `validate_ref_target` рядом с `check_ref_slots` ~:2192; вызовы в `add_ref` ~:2246 и `check_ref_add` ~:2340; тест `add_ref_allows_dangling_destination_form` ~:6394)

**Interfaces:**
- Produces: `fn validate_ref_target(image: &Image, qt: &QuestionTarget, schema: &schema::QuestionAddRefSchema) -> Result<(), HiiError>`; plain: `parse_form_package_sets(&qt.pkg)` → `sets[qt.formset_idx].forms` содержит `schema.form_id` (suppressed — валидны); отказ `InvalidSchema("ref target form {:#x} not declared in owning formset {guid} (have: {sorted ids})")`. Fail-closed: `parse_form_package_sets → None` или `formset_idx` вне диапазона (не бывает после `locate_form_attribution`, но не паникуем) → `InvalidSchema("owning form package malformed")`.

- [ ] **Step 1: RED-тесты**

```rust
#[test]
fn add_ref_rejects_dangling_destination_form() {  // инверсия allows_dangling
    // тот же вход: Err(InvalidSchema, "not declared in owning formset"),
    // образ байт-идентичен (snapshot)
}

#[test]
fn add_ref_accepts_existing_formset_form() { /* форма формсета-владельца → Ok */ }

#[test]
fn add_ref_rejects_form_of_neighbor_formset() {
    // multi-formset фикстура: цель в sets[i+1] → InvalidSchema
}

#[test]
fn check_ref_add_rejects_dangling_target() { /* InvalidSchema, без мутации */ }
```

- [ ] **Step 2:** падают (dangling проходит сегодня).
- [ ] **Step 3: GREEN** — хелпер + вызовы (add_ref: после `resolve_writable_path`, до `plan_spf_fixup`; check_ref_add: в цикле до `check_ref_slots`).
- [ ] **Step 4:** crate зелёный; существующие positive add_ref-тесты (`add_ref_inserts_goto_before_form_end` и пр.) остаются зелёными — их цели существуют; падение = фикстура ссылалась на несуществующую форму, чинится в тесте осознанно; clippy clean.
- [ ] **Step 5: Commit** — `feat(engine): add_ref/check_ref_add валидируют intra-цель REF (ref-guard-true-flip §2)`

### Task 5: mod.rs — REF3-резолв (глобальная карта)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`validate_ref_target`: ветка `Some(gs)`; tests)

**Interfaces:**
- Produces: GUID parse рано (InvalidSchema при невалидной строке — паритет build_ref_ops); `forms::collect_forms(image)` матчит `(formset_guid, form_id_ifr)`; отказы: «form … not found in formset … (have: …)» / «formset … not found in image (have: …)». Read-only каналы — валидные цели.

- [ ] **Step 1: RED-тесты**

```rust
#[test]
fn add_ref_accepts_cross_formset_target() { /* REF3 на существующие формсет+форму → Ok */ }
#[test]
fn add_ref_rejects_unknown_formset_guid() { /* InvalidSchema "formset … not found in image" */ }
#[test]
fn add_ref_rejects_missing_form_in_known_formset() { /* InvalidSchema "form … not found in formset" */ }
#[test]
fn add_ref_accepts_suppressed_cross_target() { /* suppressed-форма — валидная цель */ }
#[test]
fn add_ref_rejects_malformed_formset_guid_early() { /* невалидная строка GUID → InvalidSchema до прочих проверок */ }
```

- [ ] **Step 2:** падают.
- [ ] **Step 3: GREEN** — ветка REF3 в `validate_ref_target`.
- [ ] **Step 4:** crate зелёный; clippy clean.
- [ ] **Step 5: Commit** — `feat(engine): add_ref валидирует REF3-цель по глобальной карте форм (ref-guard-true-flip §2)`

### Task 6: финальный гейт + docs

- [ ] **Step 1:** `cargo test --all`; `cargo clippy --all --all-targets -- -D warnings`; `cargo fmt --all -- --check`.
- [ ] **Step 2:** закрыть TODO:4027/4038 со ссылками на коммиты; roadmap-запись цикла.
- [ ] **Step 3: Commit** — `docs: закрытие TODO ref-guard-true-flip + roadmap`
