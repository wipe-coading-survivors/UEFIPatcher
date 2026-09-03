# Мини-цикл «unlock-op» (unlock вопросов/страниц, флипы литералов) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать движку и CLI операцию «unlock» страницы/вопроса Setup флипами литералов в гейтящих IFR-выражениях (единственный HW-валидированный класс, E12), с read-only картой гейтинга (u1), атомарными инвариантами длин (u2), удалением фальсифицированной UPG/PRC-механики (u3), real-image приемкой против эталона E12 (u4) и CLI-командами (u5).

**Architecture:** Новый модуль `hii/gates.rs`: грамматик-осведомлённый walker FORM-пакетов (выражение до первого statement'а обходится линейно — лечит вендорский quirk scope-бита на операндах), декодер выражений ровно трёх E12-классов, планировщик/применитель флипов in-place. Интеграция `hii/mod.rs`: `parse_item_id` (`#<form>[:<qid>]`), общие гейты с `set_item_visibility` (`resolve_writable_path`), `gates_list` (чтение) и `unlock` (мутация, атомарно). Proto: `HiiGatesList`/`HiiUnlock` + `GateInfo`. CLI: `hii form|question gates|unlock`.

**Tech Stack:** Rust workspace (edition 2024), `r-efi` 7.0 (IFR-константы/структуры — своих не определять), tonic RPC, clap, prost+serde.

**Spec:** `docs/superpowers/specs/2026-09-03-hii-unlock-op-design.md` (коммит `b7bef16`). Источник данных: `docs/reports/2026-09-02-hw-validation-hnx99tf.md` §10–12 (E10–E12), TODO «Мини-цикл „unlock-op“».

## Global Constraints

- AGENTS.md обязательны: TDD-порядок (тест падает → реализация → проходит), один коммит на шаг с сообщением из задачи, **никаких комментариев в коде** (кроме ссылок `file:line`), `cargo test -p <crate>` + `cargo clippy -p <crate> -- -D warnings` после каждой задачи, `cargo fmt --all` перед коммитом задачи.
- **Module-first rule:** `pub mod gates;` в `crates/uefi-engine/src/hii/mod.rs` — тем же шагом, что создание `gates.rs`, ДО первого `cargo test`.
- **Правило plan-defect (AGENTS.md п.11):** расхождение плана с реальностью — сначала отдельный коммит `docs: fix Task N in unlock-op plan (<дефекты>)`, затем реализация. Первый кандидат: базис pkg-офсетов `0x67A`/`0xDD1` (см. спеку §7).
- IFR-константы/структуры — только `r_efi::hii::*` (константы: `IFR_UINT64_OP=0x45`, `IFR_EQUAL_OP=0x2F`, `IFR_EQ_ID_VAL_OP=0x12`, `IFR_TRUE_OP=0x46`, `IFR_NUMERIC_SIZE=0x03` и т.д. — в коде без хардкода).
- Real-image тесты — `#[ignore]`, прогон при наличии `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`: `cargo test -p uefi-engine --test real_image -- --ignored`.
- Proto-сериализация: каждое новое сообщение требует явного `.message_attribute("engine.<Msg>", "#[derive(serde::Serialize)]")` в `crates/uefi-proto/build.rs` — иначе JSON-вывод CLI молча не сработает.
- Edition 2024: let-chains разрешены и используются в кодовой базе.
- Контейнер: если `/run/.containerenv` — сборка через `podman-remote` (`docker/rust-builder.containerfile`).
- Mock-сервер `crates/uefi-cli/tests/mock_server.rs` реализует весь server-trait: proto-изменения (Task 6) и mock-заглушки — одна задача, иначе крейт uefi-cli не собирается.

---

### Task 1: `hii/gates.rs` — модель + декодер выражений

**Files:**
- Create: `crates/uefi-engine/src/hii/gates.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs:12` (после `pub mod formset_add;` по алфавиту — вставить `pub mod gates;`)
- Test: `crates/uefi-engine/src/hii/gates.rs` (модуль `tests`)

**Interfaces:**
- Consumes: `super::ifr::is_form_package` (существует).
- Produces (для Tasks 2–5):
  - `pub enum GateKind { Suppress, Grayout }` (+ `impl GateKind { pub fn as_str(self) -> &'static str }`)
  - `pub enum Wraps { Form { form_id: u16 }, Ref { form_id: u16, host_form_id: u16 }, Question { form_id: u16, question_id: u16 } }`
  - `pub enum GateExpr { EqConst { a: u64, b: u64 }, EqIdVal { question_id: u16, value: u16 }, True, Other }`
  - `pub struct Gate { pub kind: GateKind, pub wraps: Wraps, pub scope_offset: usize, pub expr_offset: usize, pub expr_end: usize, pub expr: GateExpr }`
  - `pub struct GateTarget { pub form_id: u16, pub question_id: Option<u16> }`
  - `pub(crate) fn decode_expr(region: &[u8]) -> GateExpr`

- [ ] **Step 1: Создать модуль с тестами и типами (module-first)**

`crates/uefi-engine/src/hii/mod.rs`, строка 6 (алфавитный порядок модулей):

```rust
pub mod gates;
```

`crates/uefi-engine/src/hii/gates.rs` — каркас: типы из Interfaces + пустой `mod tests`, пока без реализации `decode_expr` (тесты Step 2 не скомпилируются — это и есть «красная» фаза).

```rust
use r_efi::hii::{IFR_EQ_ID_VAL_OP, IFR_EQUAL_OP, IFR_TRUE_OP, IFR_UINT64_OP};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    Suppress,
    Grayout,
}

impl GateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GateKind::Suppress => "suppress",
            GateKind::Grayout => "grayout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wraps {
    Form { form_id: u16 },
    Ref { form_id: u16, host_form_id: u16 },
    Question { form_id: u16, question_id: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateExpr {
    EqConst { a: u64, b: u64 },
    EqIdVal { question_id: u16, value: u16 },
    True,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    pub kind: GateKind,
    pub wraps: Wraps,
    pub scope_offset: usize,
    pub expr_offset: usize,
    pub expr_end: usize,
    pub expr: GateExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateTarget {
    pub form_id: u16,
    pub question_id: Option<u16>,
}
```

- [ ] **Step 2: Написать падающие тесты декодера**

В конец `gates.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn uint64(v: u64) -> Vec<u8> {
        let mut b = vec![IFR_UINT64_OP, 0x0A];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }

    fn uint64_quirk(v: u64) -> Vec<u8> {
        let mut b = uint64(v);
        b[1] = 0x8A;
        b
    }

    fn equal() -> Vec<u8> {
        vec![IFR_EQUAL_OP, 0x02]
    }

    fn eq_id_val(question_id: u16, value: u16) -> Vec<u8> {
        let mut b = vec![IFR_EQ_ID_VAL_OP, 0x06];
        b.extend_from_slice(&question_id.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        b
    }

    fn true_op() -> Vec<u8> {
        vec![IFR_TRUE_OP, 0x02]
    }

    fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn decode_eq_const_pair_with_equal() {
        let region = concat(&[uint64(1), uint64(1), equal()]);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqConst { a: 1, b: 1 }
        );
    }

    #[test]
    fn decode_eq_const_ignores_scope_bit_quirk() {
        let region = concat(&[uint64_quirk(1), uint64(1), equal()]);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqConst { a: 1, b: 1 },
            "vendor firmware sets the scope bit on expression operands ([45 8a])"
        );
    }

    #[test]
    fn decode_eq_id_val() {
        let region = eq_id_val(0x009A, 1);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqIdVal { question_id: 0x009A, value: 1 }
        );
    }

    #[test]
    fn decode_true() {
        assert_eq!(decode_expr(&true_op()), GateExpr::True);
    }

    #[test]
    fn decode_equal_consts_are_not_a_gate_class_when_distinct() {
        let region = concat(&[uint64(1), uint64(2), equal()]);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqConst { a: 1, b: 2 }
        );
    }

    #[test]
    fn decode_unknown_opcode_is_other() {
        let region = concat(&[vec![0x42u8, 0x03, 0x07], equal()]);
        assert_eq!(decode_expr(&region), GateExpr::Other);
    }

    #[test]
    fn decode_wrong_shape_is_other() {
        assert_eq!(decode_expr(&uint64(1)), GateExpr::Other);
        assert_eq!(decode_expr(&concat(&[uint64(1), equal()])), GateExpr::Other);
        assert_eq!(decode_expr(&concat(&[eq_id_val(1, 1), true_op()])), GateExpr::Other);
        assert_eq!(decode_expr(&[]), GateExpr::Other);
    }

    #[test]
    fn decode_truncated_region_is_other() {
        let mut region = concat(&[uint64(1), uint64(1), equal()]);
        region.truncate(region.len() - 3);
        assert_eq!(decode_expr(&region), GateExpr::Other);
    }
}
```

- [ ] **Step 3: Запустить — убедиться в падении**

Run: `cargo test -p uefi-engine gates`
Expected: FAIL (compile error: `decode_expr` не найдена).

- [ ] **Step 4: Реализовать decode_expr**

В `gates.rs` (prod-часть):

```rust
pub(crate) fn decode_expr(region: &[u8]) -> GateExpr {
    let mut ops: Vec<(u8, &[u8])> = Vec::new();
    let mut i = 0usize;
    while i + 2 <= region.len() {
        let op = region[i];
        let len = (region[i + 1] & 0x7F) as usize;
        if len < 2 || i + len > region.len() {
            return GateExpr::Other;
        }
        ops.push((op, &region[i + 2..i + len]));
        i += len;
    }
    if i != region.len() {
        return GateExpr::Other;
    }
    match ops.as_slice() {
        [(IFR_UINT64_OP, a), (IFR_UINT64_OP, b), (IFR_EQUAL_OP, _)]
            if a.len() == 8 && b.len() == 8 =>
        {
            GateExpr::EqConst {
                a: u64::from_le_bytes(a.try_into().unwrap()),
                b: u64::from_le_bytes(b.try_into().unwrap()),
            }
        }
        [(IFR_EQ_ID_VAL_OP, p)] if p.len() == 4 => GateExpr::EqIdVal {
            question_id: u16::from_le_bytes([p[0], p[1]]),
            value: u16::from_le_bytes([p[2], p[3]]),
        },
        [(IFR_TRUE_OP, _)] => GateExpr::True,
        _ => GateExpr::Other,
    }
}
```

Импорты в `use` — ровно используемые этим шагом (`-D warnings`); `IFR_END_OP`/`IFR_FORM_OP`/`is_form_package` и statement-константы добавит Task 2 вместе с walker'ом.

- [ ] **Step 5: Прогнать + lint + коммит**

Run: `cargo test -p uefi-engine gates && cargo clippy -p uefi-engine --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS / clean.

```bash
git add crates/uefi-engine/src/hii/gates.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii gates expression decoder (E12 classes)"
```

---

### Task 2: `find_gates` — грамматик-осведомлённый walker гейтов

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs`
- Test: там же, расширить `mod tests`

**Interfaces:**
- Consumes: Task 1 (`Gate`, `GateTarget`, `decode_expr`), `is_form_package`.
- Produces: `pub fn find_gates(body: &[u8], target: &GateTarget) -> Vec<Gate>` (Task 5 вызывает на срезе FORM-пакета); `fn package_bounds(body: &[u8]) -> (usize, usize)` — private.

- [ ] **Step 1: Написать падающие тесты walker'а**

Дополнить `mod tests` (фикстуры `opcode`/`form_set`/`form`/`end`/`package` дублируются из `ifr.rs` tests сознательно — конвенция развязки prod-модулей, TODO «фикстуры дублируются»):

```rust
    use r_efi::hii::{IFR_FORM_OP, IFR_FORM_SET_OP, IFR_GRAY_OUT_IF_OP, IFR_ONE_OF_OP, IFR_SUPPRESS_IF_OP, PACKAGE_FORMS};
    use crate::types::Guid;
    use std::str::FromStr;

    const FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn form_set(title: u16) -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16, title: u16) -> Vec<u8> {
        opcode(IFR_FORM_OP, true, &[id.to_le_bytes(), title.to_le_bytes()].concat())
    }

    fn end() -> Vec<u8> {
        vec![r_efi::hii::IFR_END_OP, 0x02]
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

    fn ref_op(form_id: u16, question_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 12];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        let mut v = vec![r_efi::hii::IFR_REF_OP, 0x11];
        v.extend_from_slice(&p);
        v.extend_from_slice(&form_id.to_le_bytes());
        v
    }

    fn one_of_op(question_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 10];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        opcode(IFR_ONE_OF_OP, true, &p)
    }

    fn vendor_ifr() -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10002, 20));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(ref_op(10029, 0x003A));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    const FORM_GATE_TARGET: GateTarget = GateTarget { form_id: 10029, question_id: None };
    const QUESTION_GATE_TARGET: GateTarget = GateTarget { form_id: 10029, question_id: Some(0x003B) };

    #[test]
    fn find_gates_reports_ref_suppress_for_target_form() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].kind, GateKind::Suppress);
        assert_eq!(
            gates[0].wraps,
            Wraps::Ref { form_id: 10029, host_form_id: 10002 }
        );
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 1 });
        assert_eq!(pkg[gates[0].scope_offset], IFR_SUPPRESS_IF_OP);
        assert_eq!(pkg[gates[0].expr_offset], IFR_UINT64_OP);
        assert!(gates[0].expr_offset < gates[0].expr_end);
        assert_eq!(&pkg[gates[0].expr_end..gates[0].expr_end + 2], &ref_op(10029, 0x003A)[..2]);
    }

    #[test]
    fn find_gates_reports_question_grayout() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].kind, GateKind::Grayout);
        assert_eq!(
            gates[0].wraps,
            Wraps::Question { form_id: 10029, question_id: 0x003B }
        );
        assert_eq!(
            gates[0].expr,
            GateExpr::EqIdVal { question_id: 0x009A, value: 1 }
        );
        assert_eq!(pkg[gates[0].scope_offset], IFR_GRAY_OUT_IF_OP);
    }

    #[test]
    fn find_gates_walks_despite_scope_bit_quirk_in_expr() {
        let mut ifr = vendor_ifr();
        let quirk_pos = ifr
            .windows(2)
            .position(|w| w == [IFR_SUPPRESS_IF_OP, 0x82])
            .unwrap()
            + 2;
        assert_eq!(ifr[quirk_pos + 1], 0x0A);
        ifr[quirk_pos + 1] = 0x8A;
        let pkg = package(&ifr);
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert_eq!(gates.len(), 1, "scope bit on expression operand must not derail the walk");
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 1 });
    }

    #[test]
    fn find_gates_reports_direct_form_suppress() {
        let mut ifr = form_set(7);
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(form(901, 30));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let gates = find_gates(&package(&ifr), &GateTarget { form_id: 901, question_id: None });
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].wraps, Wraps::Form { form_id: 901 });
    }

    #[test]
    fn find_gates_reports_every_enclosing_gate_innermost_first() {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let gates = find_gates(&package(&ifr), &QUESTION_GATE_TARGET);
        assert_eq!(gates.len(), 2);
        assert_eq!(gates[0].kind, GateKind::Grayout);
        assert_eq!(gates[0].expr, GateExpr::EqIdVal { question_id: 0x009A, value: 1 });
        assert_eq!(gates[1].kind, GateKind::Suppress);
        assert_eq!(gates[1].expr, GateExpr::Other, "outer suppress region contains the nested grayout opcode");
    }

    #[test]
    fn find_gates_ignores_question_in_other_form() {
        let mut ifr = vendor_ifr();
        let pkg = package(&ifr);
        let wrong_form = find_gates(
            &pkg,
            &GateTarget { form_id: 10028, question_id: Some(0x003B) },
        );
        assert!(wrong_form.is_empty());
    }

    #[test]
    fn find_gates_unknown_target_is_empty() {
        let pkg = package(&vendor_ifr());
        assert!(find_gates(&pkg, &GateTarget { form_id: 4242, question_id: None }).is_empty());
        assert!(find_gates(&pkg, &GateTarget { form_id: 4242, question_id: Some(9) }).is_empty());
    }

    #[test]
    fn find_gates_stops_gracefully_on_truncated_package() {
        let mut pkg = package(&vendor_ifr());
        pkg.truncate(pkg.len() - 3);
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert!(gates.len() <= 1);
    }
```

- [ ] **Step 2: Запустить — убедиться в падении**

Run: `cargo test -p uefi-engine gates`
Expected: FAIL (compile error: `find_gates` не найдена).

- [ ] **Step 3: Реализовать walker**

В prod-часть `gates.rs` (восстановить полный импорт: добавить `IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DATE_OP, IFR_DEFAULT_OP, IFR_FORM_OP, IFR_GRAY_OUT_IF_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP, IFR_ORDERED_LIST_OP, IFR_PASSWORD_OP, IFR_REF_OP, IFR_STRING_OP, IFR_SUBTITLE_OP, IFR_SUPPRESS_IF_OP, IFR_TEXT_OP, IFR_TIME_OP`):

```rust
fn package_bounds(body: &[u8]) -> (usize, usize) {
    if is_form_package(body) {
        let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
        (4, plen.min(body.len()))
    } else {
        (0, body.len())
    }
}

fn is_gate_op(op: u8) -> bool {
    op == IFR_SUPPRESS_IF_OP || op == IFR_GRAY_OUT_IF_OP
}

fn is_statement_op(op: u8) -> bool {
    matches!(
        op,
        IFR_FORM_OP
            | IFR_SUBTITLE_OP
            | IFR_TEXT_OP
            | IFR_REF_OP
            | IFR_ONE_OF_OP
            | IFR_CHECKBOX_OP
            | IFR_NUMERIC_OP
            | IFR_PASSWORD_OP
            | IFR_ORDERED_LIST_OP
            | IFR_STRING_OP
            | IFR_DATE_OP
            | IFR_TIME_OP
            | IFR_ACTION_OP
            | IFR_ONE_OF_OPTION_OP
            | IFR_DEFAULT_OP
    )
}

fn is_question_op(op: u8) -> bool {
    matches!(
        op,
        IFR_ONE_OF_OP
            | IFR_CHECKBOX_OP
            | IFR_NUMERIC_OP
            | IFR_PASSWORD_OP
            | IFR_ORDERED_LIST_OP
            | IFR_STRING_OP
            | IFR_DATE_OP
            | IFR_TIME_OP
            | IFR_ACTION_OP
    )
}

struct Frame {
    op: u8,
    offset: usize,
    expr_end: Option<usize>,
    form_id: Option<u16>,
}

pub fn find_gates(body: &[u8], target: &GateTarget) -> Vec<Gate> {
    let (start, end) = package_bounds(body);
    let mut gates = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = start;
    while i + 2 <= end {
        let op = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            break;
        }
        if op == IFR_END_OP {
            stack.pop();
            i += length;
            continue;
        }
        let in_gate_expr =
            matches!(stack.last(), Some(f) if is_gate_op(f.op) && f.expr_end.is_none());
        if in_gate_expr && !is_statement_op(op) && !is_gate_op(op) {
            i += length;
            continue;
        }
        let mut form_id = None;
        if is_statement_op(op) {
            for f in stack.iter_mut() {
                if f.expr_end.is_none() {
                    f.expr_end = Some(i);
                }
            }
            emit_gates(body, &stack, i, op, length, target, &mut gates);
            if op == IFR_FORM_OP && length >= 6 {
                form_id = Some(u16::from_le_bytes([body[i + 2], body[i + 3]]));
            }
        }
        if length_and_scope & 0x80 != 0 {
            stack.push(Frame { op, offset: i, expr_end: None, form_id });
        }
        i += length;
    }
    gates
}

fn emit_gates(
    body: &[u8],
    stack: &[Frame],
    stmt_offset: usize,
    op: u8,
    length: usize,
    target: &GateTarget,
    gates: &mut Vec<Gate>,
) {
    let current_form = stack.iter().rev().find_map(|f| f.form_id);
    let wraps = if op == IFR_FORM_OP && length >= 6 {
        let fid = u16::from_le_bytes([body[stmt_offset + 2], body[stmt_offset + 3]]);
        if fid == target.form_id && target.question_id.is_none() {
            Some(Wraps::Form { form_id: fid })
        } else {
            None
        }
    } else if op == IFR_REF_OP && length >= 16 && target.question_id.is_none() {
        let fid = u16::from_le_bytes([body[stmt_offset + 14], body[stmt_offset + 15]]);
        if fid == target.form_id {
            Some(Wraps::Ref { form_id: fid, host_form_id: current_form.unwrap_or(0) })
        } else {
            None
        }
    } else if is_question_op(op) && length >= 8 {
        let qid = u16::from_le_bytes([body[stmt_offset + 6], body[stmt_offset + 7]]);
        if target.question_id == Some(qid) && current_form == Some(target.form_id) {
            Some(Wraps::Question { form_id: target.form_id, question_id: qid })
        } else {
            None
        }
    } else {
        None
    };
    let Some(wraps) = wraps else { return };
    for f in stack.iter().rev() {
        if !is_gate_op(f.op) {
            continue;
        }
        let expr_end = f.expr_end.unwrap_or(stmt_offset);
        gates.push(Gate {
            kind: if f.op == IFR_SUPPRESS_IF_OP { GateKind::Suppress } else { GateKind::Grayout },
            wraps,
            scope_offset: f.offset,
            expr_offset: f.offset + 2,
            expr_end,
            expr: decode_expr(&body[f.offset + 2..expr_end.min(body.len())]),
        });
    }
}
```

`expr_end` в нормальном пути выставлен statement-циклом до вызова `emit_gates`; `unwrap_or(stmt_offset)` — защита от дегенеративного пакета (пустое выражение).

- [ ] **Step 4: Прогнать + lint + коммит**

Run: `cargo test -p uefi-engine gates && cargo clippy -p uefi-engine --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS / clean.

```bash
git add crates/uefi-engine/src/hii/gates.rs
git commit -m "feat(uefi-engine): hii gates walker (grammar-aware, vendor scope-bit quirk)"
```

---

### Task 3: Планировщик и применитель флипов (строго E12)

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs`
- Test: там же

**Interfaces:**
- Consumes: Task 2 (`find_gates`, `Gate`).
- Produces (для Task 5):
  - `pub struct PlannedFlip { pub offset: usize, pub from: Vec<u8>, pub to: Vec<u8> }`
  - `pub(crate) fn plan_flip(body: &[u8], gate: &Gate) -> Option<PlannedFlip>`
  - `pub fn plan_gates(body: &[u8], gates: &[Gate]) -> Result<Vec<PlannedFlip>, String>`
  - `pub(crate) fn apply_flips(body: &mut [u8], flips: &[PlannedFlip]) -> Result<(), String>`
  - `pub(crate) fn question_storage_width(body: &[u8], question_id: u16) -> Option<u8>`

- [ ] **Step 1: Написать падающие тесты**

Дополнить `mod tests` (импорт `IFR_NUMERIC_OP, IFR_NUMERIC_SIZE` по месту):

```rust
    fn numeric_op(question_id: u16, flags: u8) -> Vec<u8> {
        let mut p = vec![0u8; 11];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        p[10] = flags;
        opcode(IFR_NUMERIC_OP, true, &p)
    }

    fn master_switch_ifr(width_flags: u8) -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(numeric_op(0x009A, width_flags));
        ifr.extend(end());
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    #[test]
    fn plan_eq_const_flip_targets_second_operand_lsb() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        let flips = plan_gates(&pkg, &gates).unwrap();
        assert_eq!(flips.len(), 1);
        assert_eq!(flips[0].offset, gates[0].expr_offset + 12);
        assert_eq!(flips[0].from, vec![1]);
        assert_eq!(flips[0].to, vec![2]);
    }

    #[test]
    fn plan_eq_id_val_flip_rewrites_value_to_unreachable() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        let flips = plan_gates(&pkg, &gates).unwrap();
        assert_eq!(flips.len(), 1);
        assert_eq!(flips[0].offset, gates[0].expr_offset + 4);
        assert_eq!(flips[0].from, vec![1, 0]);
        assert_eq!(flips[0].to, vec![0xFF, 0xFF]);
    }

    #[test]
    fn plan_eq_const_distinct_operands_have_no_flip() {
        let mut ifr = form_set(7);
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(2));
        ifr.extend(equal());
        ifr.extend(form(901, 30));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let gates = find_gates(&pkg, &GateTarget { form_id: 901, question_id: None });
        assert!(plan_gates(&pkg, &gates).is_err());
    }

    #[test]
    fn plan_refuses_eq_id_val_when_master_storage_is_two_bytes() {
        let pkg = package(&master_switch_ifr(r_efi::hii::IFR_NUMERIC_SIZE_2));
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        let err = plan_gates(&pkg, &gates).unwrap_err();
        assert!(err.contains("pkg+"), "diagnostics must carry the gate offset: {err}");
    }

    #[test]
    fn plan_allows_eq_id_val_when_master_is_one_byte_numeric() {
        let pkg = package(&master_switch_ifr(r_efi::hii::IFR_NUMERIC_SIZE_1));
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert!(plan_gates(&pkg, &gates).is_ok());
    }

    #[test]
    fn plan_allows_eq_id_val_when_master_absent_from_package() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert!(
            plan_gates(&pkg, &gates).is_ok(),
            "vendor fixture has no 0x009A statement at all — width unknown, flip allowed"
        );
    }

    #[test]
    fn apply_flips_write_in_place_preserving_length() {
        let pkg = package(&vendor_ifr());
        let before = pkg.clone();
        let mut body = pkg.clone();
        for target in [FORM_GATE_TARGET, QUESTION_GATE_TARGET] {
            let flips = plan_gates(&body, &find_gates(&body, &target)).unwrap();
            apply_flips(&mut body, &flips).unwrap();
        }
        assert_eq!(body.len(), before.len());
        let diff: Vec<(usize, u8, u8)> = before
            .iter()
            .zip(body.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();
        assert_eq!(diff.len(), 3);
        assert!(diff.iter().any(|(i, a, b)| *a == 1 && *b == 2));
        assert!(diff.iter().filter(|(i, _, b)| *b == 0xFF).count() == 2);
    }

    #[test]
    fn apply_flips_rejects_stale_from_bytes() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        let mut flips = plan_gates(&pkg, &gates).unwrap();
        let mut body = pkg.clone();
        let stale = flips[0].offset;
        body[stale] = 0x77;
        assert!(apply_flips(&mut body, &flips).is_err());
        assert_eq!(body[stale], 0x77);
        flips[0].from = vec![0x77];
        flips[0].to = vec![0x78];
        apply_flips(&mut body, &flips).unwrap();
        assert_eq!(body[stale], 0x78);
    }

    #[test]
    fn plan_after_flip_refuses_second_unlock() {
        let pkg = package(&vendor_ifr());
        let mut body = pkg.clone();
        let flips = plan_gates(&body, &find_gates(&body, &FORM_GATE_TARGET)).unwrap();
        apply_flips(&mut body, &flips).unwrap();
        let gates = find_gates(&body, &FORM_GATE_TARGET);
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 2 });
        assert!(plan_gates(&body, &gates).is_err(), "already-false expression must not flip again");
    }
```

Ширина мастера известна только для NUMERIC/CHECKBOX (`question_storage_width`); CHECKBOX-ветка (`IFR_CHECKBOX_OP => Some(1)`) покрыта ревью реализации — отдельной фикстуры не требует (CHECKBOX-гейтнутые мастера на живых образах не встречены, см. спеку §7).

- [ ] **Step 2: Запустить — падение**

Run: `cargo test -p uefi-engine gates`
Expected: FAIL (compile error: `PlannedFlip`/`plan_gates`/`apply_flips` не найдены).

- [ ] **Step 3: Реализовать планировщик/применитель**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFlip {
    pub offset: usize,
    pub from: Vec<u8>,
    pub to: Vec<u8>,
}

pub(crate) fn question_storage_width(body: &[u8], question_id: u16) -> Option<u8> {
    let (start, end) = package_bounds(body);
    let mut i = start;
    while i + 2 <= end {
        let op = body[i];
        let length = (body[i + 1] & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        if is_question_op(op)
            && length >= 8
            && u16::from_le_bytes([body[i + 6], body[i + 7]]) == question_id
        {
            return match op {
                IFR_CHECKBOX_OP => Some(1),
                IFR_NUMERIC_OP if length >= 13 => {
                    Some(1u8 << (body[i + 12] & IFR_NUMERIC_SIZE))
                }
                _ => None,
            };
        }
        i += length;
    }
    None
}

pub(crate) fn plan_flip(body: &[u8], gate: &Gate) -> Option<PlannedFlip> {
    match gate.expr {
        GateExpr::EqConst { a, b } if a == b => {
            let first_len = (body[gate.expr_offset + 1] & 0x7F) as usize;
            let offset = gate.expr_offset + first_len + 2;
            let from = body[offset];
            Some(PlannedFlip {
                offset,
                from: vec![from],
                to: vec![from.wrapping_add(1)],
            })
        }
        GateExpr::EqIdVal { question_id, value } => {
            if question_storage_width(body, question_id).is_some_and(|w| w > 1) {
                return None;
            }
            Some(PlannedFlip {
                offset: gate.expr_offset + 4,
                from: value.to_le_bytes().to_vec(),
                to: 0xFFFFu16.to_le_bytes().to_vec(),
            })
        }
        _ => None,
    }
}

pub fn plan_gates(body: &[u8], gates: &[Gate]) -> Result<Vec<PlannedFlip>, String> {
    let mut flips = Vec::new();
    for gate in gates {
        match plan_flip(body, gate) {
            Some(flip) => flips.push(flip),
            None => {
                let region = &body[gate.expr_offset..gate.expr_end.min(body.len())];
                return Err(format!(
                    "{} gate at pkg+{:#x} wrapping {:?}: expression [{}] is not a hardware-validated flip class",
                    gate.kind.as_str(),
                    gate.scope_offset,
                    gate.wraps,
                    region
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
    }
    Ok(flips)
}

pub(crate) fn apply_flips(body: &mut [u8], flips: &[PlannedFlip]) -> Result<(), String> {
    for flip in flips {
        if flip.offset + flip.from.len() > body.len()
            || flip.offset + flip.to.len() > body.len()
            || body[flip.offset..flip.offset + flip.from.len()] != flip.from.as_slice()
        {
            return Err(format!("flip at pkg+{:#x} precondition failed", flip.offset));
        }
    }
    for flip in flips {
        body[flip.offset..flip.offset + flip.to.len()].copy_from_slice(&flip.to);
    }
    Ok(())
}
```

- [ ] **Step 4: Прогнать + lint + коммит**

Run: `cargo test -p uefi-engine gates && cargo clippy -p uefi-engine --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS / clean.

```bash
git add crates/uefi-engine/src/hii/gates.rs
git commit -m "feat(uefi-engine): hii gates literal-flip planner (E12 classes, width guard)"
```

---

### Task 4: u3 — удалить фальсифицированную UPG/PRC-механику

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (строки 49–52, 108–165; `plan_prc_entries` 193–256; тесты)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_error_status` + тест)
- Modify: `crates/uefi-engine/tests/real_image.rs` (тест `real_image_unhide_patches_prc_tokens`, UPG-ассерт)

**Interfaces:**
- Consumes: ничего нового.
- Produces: `set_item_visibility` без UPG/PRC-ветки (сигнатура прежняя); `HiiError` без варианта `PrcPatchUnsupported`; `string_pack::insert_strings_at_ids_in_resource` остаётся (используется `form_add`/`formset_add`).

- [ ] **Step 1: Удалить prod-код PRC в hii/mod.rs**

1. Удалить варианты и константы: `HiiError::PrcPatchUnsupported` (строки 49–50), `PRC_TOKEN_LANGUAGE`, `PRC_NAME_PREFIX` (193–194).
2. Удалить функцию `plan_prc_entries` (196–256).
3. PE32-ветка `set_item_visibility` (107–165) приводится к виду:

```rust
        } else if node.subtype == EFI_SECTION_PE32 {
            let Some(packages) = pe_resource_form_packages(&node.body) else {
                return Err(HiiError::NotASetupItem);
            };
            if visible {
                for (start, len) in packages {
                    let scope = {
                        let seg = &node.body[start..start + len];
                        match form_id {
                            Some(fid) => ifr::find_form_suppress_scope(seg, fid),
                            None => ifr::find_suppress_if_scopes(seg).into_iter().next(),
                        }
                    };
                    if let Some(scope) = scope {
                        ifr::unsuppress(&mut node.body[start..start + len], &scope);
                        changed = true;
                        break;
                    }
                }
            }
        } else {
```

- [ ] **Step 2: Удалить тесты PRC и почистить хелперы**

`hii/mod.rs` tests: удалить `set_item_visibility_patches_prc_tokens_for_unhidden_form`, `set_item_visibility_prc_growth_failure_leaves_image_untouched`; удалить хелперы `token_sibt`, `prc_blob` (стали unused); `sppkg`, `display_sibt`, `pe32_image_with`, `suppressed_form_901_pkg`, `resource_string_pkgs`, `image_snapshot` оставить только те, что остались нужны (`set_item_visibility_no_token_package_is_noop_unhide` использует `sppkg`, `display_sibt`, `pe32_image_with`, `suppressed_form_901_pkg`) — `resource_string_pkgs` и `image_snapshot` удалить. Тест `set_item_visibility_no_token_package_is_noop_unhide` остаётся без изменений.

`rpc/server.rs`: в `hii_error_status` удалить arm `PrcPatchUnsupported`; в тесте `hii_error_status_maps_id_occupied_and_prc_patch_unsupported` удалить ассерт про PrcPatchUnsupported и переименовать тест в `hii_error_status_maps_id_occupied`.

`tests/real_image.rs`: удалить тест `real_image_unhide_patches_prc_tokens` и хелпер `form_901_suppressed` (использовался только им — сверить `grep -n form_901_suppressed`); в `real_image_unhide_rebuild_keeps_layout` удалить строки с `let token = …` и UPG-ассертом (1395–1400).

- [ ] **Step 3: Прогнать**

Run: `cargo test -p uefi-engine && cargo test -p uefi-engine --test real_image -- --ignored && cargo clippy -p uefi-engine --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS (real-image — при наличии образа; иначе пропустить с пометкой).

- [ ] **Step 4: Коммит**

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/rpc/server.rs crates/uefi-engine/tests/real_image.rs
git commit -m "refactor(uefi-engine): drop falsified UPG/PRC patch path (E8)"
```

---

### Task 5: Интеграция — `gates_list` + `unlock` в `hii/mod.rs`

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs`
- Test: там же (`mod tests`)

**Interfaces:**
- Consumes: Tasks 1–3 (`gates::{GateTarget, GateKind, Wraps, GateExpr, Gate, find_gates, plan_gates, apply_flips}`), `crate::parser::target::{parse_target, find_item_path, find_item_mut}`, `pe_resource_form_packages`, `ops::mark_rebuild_to_root_by_path`, `uefi_proto::GateInfo`.
- Produces (для Tasks 6, 8):
  - `fn parse_item_id(item_id: &str) -> Result<(crate::parser::target::Target, u16, Option<u16>), HiiError>` — private
  - `fn resolve_writable_path(image: &Image, target: &crate::parser::target::Target) -> Result<Vec<usize>, HiiError>` — private
  - `pub struct UnlockOutcome { pub gates: Vec<uefi_proto::GateInfo>, pub applied: Vec<String> }`
  - `pub fn gates_list(image: &Image, item_id: &str) -> Result<Vec<uefi_proto::GateInfo>, HiiError>`
  - `pub fn unlock(image: &mut Image, item_id: &str) -> Result<UnlockOutcome, HiiError>`
  - `pub enum HiiError::GateExpressionUnsupported(String)` — новый вариант (после `SibtBlockUnsupported`)

- [ ] **Step 1: Написать падающие тесты**

В `hii/mod.rs` `mod tests` добавить фикстуры и тесты. Гейт-фикстуры дублируют `gates.rs` tests (конвенция — см. TODO):

```rust
    use crate::hii::gates::{GateExpr, GateTarget};

    const VENDOR_FORMSET_GUID_STR: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn g_opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn g_uint64(v: u64) -> Vec<u8> {
        let mut b = vec![r_efi::hii::IFR_UINT64_OP, 0x0A];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }

    fn g_equal() -> Vec<u8> {
        vec![r_efi::hii::IFR_EQUAL_OP, 0x02]
    }

    fn g_eq_id_val(question_id: u16, value: u16) -> Vec<u8> {
        let mut b = vec![r_efi::hii::IFR_EQ_ID_VAL_OP, 0x06];
        b.extend_from_slice(&question_id.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        b
    }

    fn g_form(id: u16) -> Vec<u8> {
        g_opcode(r_efi::hii::IFR_FORM_OP, true, &[id.to_le_bytes(), 7u16.to_le_bytes()].concat())
    }

    fn g_ref(form_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 12];
        let mut v = vec![r_efi::hii::IFR_REF_OP, 0x11];
        v.extend_from_slice(&p);
        v.extend_from_slice(&form_id.to_le_bytes());
        v
    }

    fn g_one_of(question_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 10];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        g_opcode(r_efi::hii::IFR_ONE_OF_OP, true, &p)
    }

    fn g_end() -> Vec<u8> {
        vec![r_efi::hii::IFR_END_OP, 0x02]
    }

    fn forms_pkg(extra_ifr: Vec<u8>) -> Vec<u8> {
        let g = Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&7u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0);
        let mut ifr = g_opcode(r_efi::hii::IFR_FORM_SET_OP, true, &p);
        ifr.extend(extra_ifr);
        let total = 4 + ifr.len();
        let mut pkg = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend(ifr);
        pkg
    }

    fn vendor_forms_pkg() -> Vec<u8> {
        forms_pkg(
            vec![
                g_form(10002),
                g_opcode(r_efi::hii::IFR_SUPPRESS_IF_OP, true, &[]),
                g_uint64(1),
                g_uint64(1),
                g_equal(),
                g_ref(10029),
                g_end(),
                g_end(),
                g_form(10029),
                g_opcode(r_efi::hii::IFR_GRAY_OUT_IF_OP, true, &[]),
                g_eq_id_val(0x009A, 1),
                g_one_of(0x003B),
                g_end(),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        )
    }

    fn vendor_image_with(section_subtype: u8, body: Vec<u8>) -> Image {
        let mut section = mk_node(FfsType::Section, body, vec![]);
        section.subtype = section_subtype;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Write }
    }

    fn vendor_hii_blob() -> Vec<u8> {
        let g = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let forms = vendor_forms_pkg();
        let mut b = g.to_bytes().to_vec();
        let total = 20 + forms.len() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(&forms);
        b.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        b
    }

    const VENDOR_TARGET: &str = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0";
    const VENDOR_FORM_ITEM: &str = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029";
    const VENDOR_QUESTION_ITEM: &str = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:0x3B";
```

Тесты:

```rust
    #[test]
    fn parse_item_id_accepts_form_and_question_forms() {
        let (_, form_id, qid) = parse_item_id(VENDOR_FORM_ITEM).unwrap();
        assert_eq!(form_id, 10029);
        assert_eq!(qid, None);
        let (_, form_id, qid) = parse_item_id(VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(form_id, 10029);
        assert_eq!(qid, Some(0x3B));
        let (_, _, qid) = parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:59").unwrap();
        assert_eq!(qid, Some(59));
    }

    #[test]
    fn parse_item_id_rejects_garbage() {
        assert!(parse_item_id("no-discriminator").is_err());
        assert!(parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#nope").is_err());
        assert!(parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:zz").is_err());
    }

    #[test]
    fn set_item_visibility_rejects_question_suffix() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        let err = set_item_visibility(&mut image, VENDOR_QUESTION_ITEM, true).unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn gates_list_bare_channel_reports_form_and_question_gates() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        image.mode = ImageMode::Read;
        let gates = gates_list(&image, VENDOR_FORM_ITEM).unwrap();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].gate_kind, "suppress");
        assert_eq!(gates[0].wraps, "ref");
        assert_eq!(gates[0].host_form_id, 10002);
        assert_eq!(gates[0].expression, "1 == 1");
        assert!(gates[0].flippable);
        assert!(gates[0].flip.contains("-> 02"));
        let qgates = gates_list(&image, VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(qgates.len(), 1);
        assert_eq!(qgates[0].gate_kind, "grayout");
        assert_eq!(qgates[0].expression, "0x009A == 0x0001");
        assert_eq!(qgates[0].flip, format!("pkg+{:#x}: 01 00 -> ff ff", qgates[0].scope_offset + 4));
    }

    #[test]
    fn gates_list_works_behind_non_recompressable_wrapper() {
        let mut inner_section = mk_node(FfsType::Section, vendor_forms_pkg(), vec![]);
        inner_section.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner_section]);
        wrapper.subtype = 0x02;
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Read };
        let gates = gates_list(&image, VENDOR_FORM_ITEM).unwrap();
        assert_eq!(gates.len(), 1);
    }

    #[test]
    fn gates_list_resource_channel() {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &vendor_hii_blob());
        let mut image = vendor_image_with(0x10, pe);
        image.mode = ImageMode::Read;
        let gates = gates_list(&image, "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029").unwrap();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].wraps, "ref");
        assert!(gates[0].flip.contains("-> 02"));
    }

    #[test]
    fn gates_list_unknown_target_is_not_found() {
        let image = vendor_image_with(0x19, vendor_forms_pkg());
        assert!(matches!(
            gates_list(&image, "00000000-0000-0000-0000-000000000001:0x19:0#10029"),
            Err(HiiError::NotFound)
        ));
    }

    #[test]
    fn unlock_flips_gates_in_place_and_cascades_rebuild() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        let before = image.root.children[0].children[0].children[0].body.clone();
        let outcome = unlock(&mut image, VENDOR_FORM_ITEM).unwrap();
        assert_eq!(outcome.applied.len(), 1);
        {
            let section = &image.root.children[0].children[0].children[0];
            let gates = crate::hii::gates::find_gates(
                &section.body,
                &GateTarget { form_id: 10029, question_id: None },
            );
            assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 2 });
            assert_eq!(section.action, Action::Rebuild);
            assert_eq!(image.root.action, Action::Rebuild);
            assert_eq!(section.body.len(), before.len());
        }
        let outcome = unlock(&mut image, VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(outcome.applied.len(), 1);
        let section = &image.root.children[0].children[0].children[0];
        let diff = before
            .iter()
            .zip(section.body.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(diff, 3, "form + question unlock together must flip exactly 3 bytes");
    }

    #[test]
    fn unlock_resource_channel_changes_exactly_flip_bytes() {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &vendor_hii_blob());
        let mut image = vendor_image_with(0x10, pe.clone());
        let outcome = unlock(&mut image, "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029:0x3B")
            .unwrap();
        assert_eq!(outcome.applied.len(), 1);
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(section.body.len(), pe.len());
        let diff = section
            .body
            .iter()
            .zip(pe.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(diff, 2, "eq_id_val flip touches exactly the two value bytes");
    }

    #[test]
    fn unlock_refuses_read_only_mode() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        image.mode = ImageMode::Read;
        assert!(matches!(
            unlock(&mut image, VENDOR_FORM_ITEM),
            Err(HiiError::NotWritable)
        ));
    }

    #[test]
    fn unlock_refuses_mutation_behind_compression() {
        let mut inner = mk_node(FfsType::Section, vendor_forms_pkg(), vec![]);
        inner.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = 0x02;
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Write };
        assert!(matches!(
            unlock(&mut image, VENDOR_FORM_ITEM),
            Err(HiiError::MutationBehindCompression)
        ));
    }

    #[test]
    fn unlock_no_gates_is_ok_empty() {
        let pkg = forms_pkg(vec![g_form(10029), g_end(), g_end()].concat());
        let mut image = vendor_image_with(0x19, pkg);
        let outcome = unlock(&mut image, VENDOR_FORM_ITEM).unwrap();
        assert!(outcome.gates.is_empty());
        assert!(outcome.applied.is_empty());
        assert_eq!(image.root.children[0].children[0].children[0].action, Action::NoAction);
    }

    #[test]
    fn unlock_unflippable_gate_refuses_without_mutation() {
        let pkg = forms_pkg(
            vec![
                g_form(10002),
                g_opcode(r_efi::hii::IFR_SUPPRESS_IF_OP, true, &[]),
                vec![r_efi::hii::IFR_TRUE_OP, 0x02],
                g_ref(10029),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        );
        let mut image = vendor_image_with(0x19, pkg.clone());
        let err = unlock(&mut image, VENDOR_FORM_ITEM).unwrap_err();
        assert!(matches!(err, HiiError::GateExpressionUnsupported(_)));
        assert_eq!(image.root.children[0].children[0].children[0].body, pkg);
    }
```

- [ ] **Step 2: Запустить — падение**

Run: `cargo test -p uefi-engine hii::`
Expected: FAIL (compile error: `gates_list`/`unlock`/`parse_item_id` не найдены).

- [ ] **Step 3: Реализовать интеграцию**

В `hii/mod.rs` prod-часть (после `set_item_visibility`; `set_item_visibility` рефакторится на `parse_item_id` + `resolve_writable_path` с сохранением поведения — его существующие тесты являются регрессией):

```rust
fn parse_u16_loose(s: &str) -> Option<u16> {
    if let Some(hex) = s.strip_prefix("0x") {
        u16::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u16>().ok()
    }
}

fn parse_item_id(
    item_id: &str,
) -> Result<(crate::parser::target::Target, u16, Option<u16>), HiiError> {
    let (target_str, disc) = item_id.rsplit_once('#').ok_or(HiiError::NotFound)?;
    let (form_str, qid_str) = match disc.split_once(':') {
        Some((f, q)) => (f, Some(q)),
        None => (disc, None),
    };
    let form_id = form_str.parse::<u16>().map_err(|_| HiiError::NotFound)?;
    let question_id = match qid_str {
        Some(q) => Some(parse_u16_loose(q).ok_or(HiiError::NotFound)?),
        None => None,
    };
    let target =
        crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    Ok((target, form_id, question_id))
}

fn resolve_writable_path(
    image: &Image,
    target: &crate::parser::target::Target,
) -> Result<Vec<usize>, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let path =
        crate::parser::target::find_item_path(&image.root, target).ok_or(HiiError::NotFound)?;
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
    Ok(path)
}

fn form_package_ranges(node: &FfsNode) -> Vec<(usize, usize)> {
    if node.subtype == EFI_SECTION_RAW || ifr::is_form_package(&node.body) {
        if ifr::is_form_package(&node.body) {
            vec![(0, node.body.len())]
        } else {
            vec![]
        }
    } else if node.subtype == EFI_SECTION_PE32 {
        pe_resource_form_packages(&node.body).unwrap_or_default()
    } else {
        vec![]
    }
}

fn expr_text(expr: &gates::GateExpr, region: &[u8]) -> String {
    match expr {
        gates::GateExpr::EqConst { a, b } => format!("{a} == {b}"),
        gates::GateExpr::EqIdVal { question_id, value } => {
            format!("{question_id:#06X} == {value:#06X}")
        }
        gates::GateExpr::True => "true".to_string(),
        gates::GateExpr::Other => format!(
            "[{}]",
            region
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn flip_text(base: usize, flip: &gates::PlannedFlip) -> String {
    let hex = |bs: &[u8]| bs.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    format!(
        "pkg+{:#x}: {} -> {}",
        base + flip.offset,
        hex(&flip.from),
        hex(&flip.to)
    )
}

fn gate_info(pkg: &[u8], gate: &gates::Gate, base: usize) -> uefi_proto::GateInfo {
    let region = &pkg[gate.expr_offset..gate.expr_end.min(pkg.len())];
    let flip = gates::plan_flip(pkg, gate);
    let (wraps, form_id, host_form_id, question_id) = match gate.wraps {
        gates::Wraps::Form { form_id } => ("form", form_id as u32, form_id as u32, 0),
        gates::Wraps::Ref { form_id, host_form_id } => ("ref", form_id as u32, host_form_id as u32, 0),
        gates::Wraps::Question { form_id, question_id } => {
            ("question", form_id as u32, form_id as u32, question_id as u32)
        }
    };
    uefi_proto::GateInfo {
        gate_kind: gate.kind.as_str().to_string(),
        wraps: wraps.to_string(),
        form_id,
        host_form_id,
        question_id,
        expression: expr_text(&gate.expr, region),
        flippable: flip.is_some(),
        flip: flip.as_ref().map(|f| flip_text(base, f)).unwrap_or_default(),
        scope_offset: (base + gate.scope_offset) as u32,
    }
}

pub fn gates_list(image: &Image, item_id: &str) -> Result<Vec<uefi_proto::GateInfo>, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let node =
        crate::parser::target::find_item(&image.root, &target).map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let gt = gates::GateTarget { form_id, question_id };
    let mut out = Vec::new();
    for (start, len) in form_package_ranges(node) {
        let pkg = &node.body[start..start + len];
        for gate in gates::find_gates(pkg, &gt) {
            out.push(gate_info(pkg, &gate, start));
        }
    }
    Ok(out)
}

pub struct UnlockOutcome {
    pub gates: Vec<uefi_proto::GateInfo>,
    pub applied: Vec<String>,
}

#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id), err)]
pub fn unlock(image: &mut Image, item_id: &str) -> Result<UnlockOutcome, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let path = resolve_writable_path(image, &target)?;
    let gt = gates::GateTarget { form_id, question_id };
    let mut infos = Vec::new();
    let mut applied = Vec::new();
    let mut mutated = false;
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        let ranges = form_package_ranges(node);
        let mut body = std::mem::take(&mut node.body);
        let mut absolute_flips: Vec<gates::PlannedFlip> = Vec::new();
        let mut plan_err: Option<HiiError> = None;
        for (start, len) in ranges {
            let pkg = &body[start..start + len];
            let found = gates::find_gates(pkg, &gt);
            if found.is_empty() {
                continue;
            }
            match gates::plan_gates(pkg, &found) {
                Ok(flips) => {
                    for gate in &found {
                        infos.push(gate_info(pkg, gate, start));
                    }
                    absolute_flips.extend(flips.into_iter().map(|f| gates::PlannedFlip {
                        offset: start + f.offset,
                        from: f.from,
                        to: f.to,
                    }));
                }
                Err(e) => {
                    plan_err = Some(HiiError::GateExpressionUnsupported(e));
                    break;
                }
            }
        }
        if let Some(err) = plan_err {
            node.body = body;
            return Err(err);
        }
        if absolute_flips.is_empty() {
            node.body = body;
            return Ok(UnlockOutcome { gates: infos, applied });
        }
        let body_len = body.len();
        if let Err(e) = gates::apply_flips(&mut body, &absolute_flips) {
            node.body = body;
            return Err(HiiError::GateExpressionUnsupported(e));
        }
        assert_eq!(body.len(), body_len, "unlock is length-preserving");
        applied = absolute_flips.iter().map(|f| flip_text(0, f)).collect();
        node.body = body;
        mutated = true;
    }
    if mutated {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(flips = applied.len(), "unlock done");
    Ok(UnlockOutcome { gates: infos, applied })
}
```

Примечание к реализации: `apply_flips` получает флипы с **абсолютными** офсетами по телу секции и вызывается один раз — его первая фаза проверяет все предусловия до первой записи, поэтому apply атомарен на весь набор даже при нескольких FORM-пакетах.

Также: `HiiError` — добавить вариант

```rust
    #[error("gating expression not reducible to a hardware-validated flip: {0}")]
    GateExpressionUnsupported(String),
```

и `set_item_visibility` переключить на `parse_item_id` + `resolve_writable_path` (существующие тесты — регрессия: reject Read-mode, malformed discriminator, compression — уже покрывают рефакторинг).

- [ ] **Step 4: Прогнать + lint + коммит**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS / clean.

```bash
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): gates_list + unlock hii ops (atomic E12 literal flips)"
```

---

### Task 6: Proto + RPC `HiiGatesList`/`HiiUnlock` + маппинг ошибок

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-proto/build.rs`
- Modify: `crates/uefi-engine/src/rpc/server.rs`
- Modify: `crates/uefi-cli/tests/mock_server.rs` (заглушки — иначе uefi-cli не соберётся)

**Interfaces:**
- Consumes: Task 5 (`hii::gates_list`, `hii::unlock`, `UnlockOutcome`, `HiiError::GateExpressionUnsupported`), `hii_error_status`.
- Produces (для Tasks 7–8): RPC-методы `hii_gates_list`/`hii_unlock`; `uefi_proto::{GateInfo, HiiGatesListRequest, HiiGatesListResponse, HiiUnlockRequest, HiiUnlockResponse}` (serde-Serialize у GateInfo).

- [ ] **Step 1: Прописать proto**

`engine.proto`, в `service EngineService` после `HiiFormAdd`:

```proto
  rpc HiiGatesList(HiiGatesListRequest)                 returns (HiiGatesListResponse);
  rpc HiiUnlock(HiiUnlockRequest)                       returns (HiiUnlockResponse);
```

После `message HiiFormAddResponse`:

```proto
message HiiGatesListRequest  { string image_id = 1; string item_id = 2; }
message GateInfo {
  string gate_kind = 1;
  string wraps = 2;
  uint32 form_id = 3;
  uint32 host_form_id = 4;
  uint32 question_id = 5;
  string expression = 6;
  bool   flippable = 7;
  string flip = 8;
  uint32 scope_offset = 9;
}
message HiiGatesListResponse { repeated GateInfo gates = 1; }

message HiiUnlockRequest     { string image_id = 1; string item_id = 2; }
message HiiUnlockResponse    { repeated GateInfo gates = 1; repeated string applied_flips = 2; }
```

`crates/uefi-proto/build.rs` — строка после `engine.ArtifactInfo`:

```rust
        .message_attribute("engine.GateInfo", "#[derive(serde::Serialize)]")
```

- [ ] **Step 2: Handlers в rpc/server.rs**

По образцу `hii_set_form_visibility` (server.rs:666) и `hii_list_forms` (687). Импорты дополнить (`HiiGatesListRequest, HiiGatesListResponse, HiiUnlockRequest, HiiUnlockResponse` — в существующий use-список proto-типов):

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn hii_gates_list(
        &self,
        req: Request<HiiGatesListRequest>,
    ) -> RpcResult<HiiGatesListResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let gates = crate::hii::gates_list(&img, &r.item_id).map_err(hii_error_status)?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, count = gates.len(), "hii gates listed");
        Ok(Response::new(HiiGatesListResponse { gates }))
    }

    #[tracing::instrument(skip(self, req), err)]
    async fn hii_unlock(
        &self,
        req: Request<HiiUnlockRequest>,
    ) -> RpcResult<HiiUnlockResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let outcome = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::unlock(img_slot, &r.item_id).map_err(hii_error_status)?
        };
        self.flush_image(&r.image_id).await?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, flips = outcome.applied.len(), "hii unlock");
        Ok(Response::new(HiiUnlockResponse {
            gates: outcome.gates,
            applied_flips: outcome.applied,
        }))
    }
```

В `hii_error_status` добавить arm (рядом с NotWritable-группой):

```rust
        crate::hii::HiiError::GateExpressionUnsupported(_) => Status::failed_precondition(e.to_string()),
```

- [ ] **Step 3: Заглушки mock-сервера (uefi-cli собирается)**

`crates/uefi-cli/tests/mock_server.rs` — по образцу `hii_list_forms` (172):

```rust
    async fn hii_gates_list(
        &self,
        req: Request<HiiGatesListRequest>,
    ) -> RpcResult<HiiGatesListResponse> {
        let _ = req;
        Ok(Response::new(HiiGatesListResponse {
            gates: vec![GateInfo {
                gate_kind: "suppress".into(),
                wraps: "ref".into(),
                form_id: 10029,
                host_form_id: 10002,
                question_id: 0,
                expression: "1 == 1".into(),
                flippable: true,
                flip: "pkg+0x67a: 01 -> 02".into(),
                scope_offset: 0x674,
            }],
        }))
    }

    async fn hii_unlock(
        &self,
        req: Request<HiiUnlockRequest>,
    ) -> RpcResult<HiiUnlockResponse> {
        let _ = req;
        Ok(Response::new(HiiUnlockResponse {
            gates: vec![GateInfo {
                gate_kind: "grayout".into(),
                wraps: "question".into(),
                form_id: 10029,
                host_form_id: 10029,
                question_id: 0x3B,
                expression: "0x009A == 0x0001".into(),
                flippable: true,
                flip: "pkg+0xdd1: 01 00 -> ff ff".into(),
                scope_offset: 0xDC8,
            }],
            applied_flips: vec!["pkg+0xdd1: 01 00 -> ff ff".into()],
        }))
    }
```

- [ ] **Step 4: Тест маппинга + прогон + коммит**

В `rpc/server.rs` tests (рядом с `hii_error_status_maps_preconditions_not_found_and_bad_target`):

```rust
    #[test]
    fn hii_error_status_maps_gate_expression_unsupported() {
        let st = hii_error_status(crate::hii::HiiError::GateExpressionUnsupported(
            "suppress gate at pkg+0x674".into(),
        ));
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
    }
```

Run: `cargo test -p uefi-engine && cargo test -p uefi-proto && cargo test -p uefi-cli && cargo clippy -p uefi-engine -p uefi-cli --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS / clean (mock-заглушки возвращают данные, клиентских вызовов ещё нет).

```bash
git add crates/uefi-proto/proto/engine.proto crates/uefi-proto/build.rs crates/uefi-engine/src/rpc/server.rs crates/uefi-cli/tests/mock_server.rs
git commit -m "feat(uefi-proto,uefi-engine): HiiGatesList/HiiUnlock RPC"
```

---

### Task 7: CLI — `hii form|question gates|unlock`

**Files:**
- Modify: `crates/uefi-cli/src/client.rs`
- Modify: `crates/uefi-cli/src/commands/hii.rs`
- Modify: `crates/uefi-cli/src/main.rs` (clap + dispatch)
- Modify: `crates/uefi-cli/src/output.rs`
- Test: `main.rs` (parse-тесты), `tests/e2e.rs` (content-ассерты stdout)

**Interfaces:**
- Consumes: Task 6 (`uefi_proto::{GateInfo, HiiGatesListRequest, HiiGatesListResponse, HiiUnlockRequest, HiiUnlockResponse}`, client-стабы), `Client::active_image`, `state::require_state`, `OutputFormat`.
- Produces: `Client::hii_gates_list(&mut self, image_id: &str, item_id: &str) -> Result<Vec<GateInfo>, AppError>`; `Client::hii_unlock(&mut self, image_id: &str, item_id: &str) -> Result<(Vec<GateInfo>, Vec<String>), AppError>`; команды CLI.

- [ ] **Step 1: Клиентские методы (client.rs)**

В существующий use-список proto-типов добавить `HiiGatesListRequest, HiiUnlockRequest` (`GateInfo` уже в `uefi_proto::*`). После `hii_set_form_visibility` (client.rs:302–317):

```rust
    pub async fn hii_gates_list(
        &mut self,
        image_id: &str,
        item_id: &str,
    ) -> Result<Vec<GateInfo>, AppError> {
        let req = HiiGatesListRequest {
            image_id: image_id.into(),
            item_id: item_id.into(),
        };
        Ok(self
            .inner
            .hii_gates_list(auth_req(&self.state, req))
            .await?
            .into_inner()
            .gates)
    }

    pub async fn hii_unlock(
        &mut self,
        image_id: &str,
        item_id: &str,
    ) -> Result<(Vec<GateInfo>, Vec<String>), AppError> {
        let req = HiiUnlockRequest {
            image_id: image_id.into(),
            item_id: item_id.into(),
        };
        let resp = self
            .inner
            .hii_unlock(auth_req(&self.state, req))
            .await?
            .into_inner();
        Ok((resp.gates, resp.applied_flips))
    }
```

- [ ] **Step 2: Output (output.rs)** — по образцу `print_forms` (output.rs:112)

```rust
pub fn print_gates(item_id: &str, gates: &[GateInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(gates).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("gate_kind\twraps\tform_id\thost_form_id\tquestion_id\texpression\tflippable\tflip\tscope_offset");
            for g in gates {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    g.gate_kind,
                    g.wraps,
                    g.form_id,
                    g.host_form_id,
                    g.question_id,
                    g.expression,
                    g.flippable,
                    g.flip,
                    g.scope_offset
                );
            }
        }
        OutputFormat::Text => {
            if gates.is_empty() {
                println!("no gates for {item_id}");
            }
            for g in gates {
                println!(
                    "{:<8} {:<8} form {} host {} qid {} expr '{}' flip '{}' @pkg+{:#x}",
                    g.gate_kind,
                    g.wraps,
                    g.form_id,
                    g.host_form_id,
                    g.question_id,
                    g.expression,
                    if g.flippable { g.flip.as_str() } else { "-" },
                    g.scope_offset
                );
            }
        }
    }
}

pub fn print_unlock(
    item_id: &str,
    gates: &[GateInfo],
    applied: &[String],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            let gates_json =
                serde_json::to_string(gates).unwrap_or_else(|_| "[]".into());
            let applied_json =
                serde_json::to_string(applied).unwrap_or_else(|_| "[]".into());
            println!(
                "{{\"item_id\":\"{item_id}\",\"gates\":{gates_json},\"applied\":{applied_json}}}"
            );
        }
        _ => {
            print_gates(item_id, gates, format);
            if applied.is_empty() {
                println!("nothing to unlock for {item_id}");
            }
            for f in applied {
                println!("applied {f}");
            }
        }
    }
}
```

В use-список output.rs добавить `GateInfo` (к `FormInfo, StringInfo`).

- [ ] **Step 3: Команды (commands/hii.rs)**

```rust
async fn gates(item_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let gates = client.hii_gates_list(&image_id, item_id).await?;
    crate::output::print_gates(item_id, &gates, format);
    Ok(())
}

async fn unlock(item_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (gs, applied) = client.hii_unlock(&image_id, item_id).await?;
    crate::output::print_unlock(item_id, &gs, &applied, format);
    Ok(())
}

pub async fn form_gates(item_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    gates(item_id, cli_sock, format).await
}

pub async fn form_unlock(item_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    unlock(item_id, cli_sock, format).await
}

pub async fn question_gates(item_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    gates(item_id, cli_sock, format).await
}

pub async fn question_unlock(item_id: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    unlock(item_id, cli_sock, format).await
}
```

- [ ] **Step 4: Clap + dispatch (main.rs)**

`HiiFormCmd` — добавить варианты (после `SetVisibility`):

```rust
    Gates {
        item_id: String,
    },
    Unlock {
        item_id: String,
    },
```

`HiiCmd` — добавить вариант (после `FormSet`):

```rust
    Question {
        #[command(subcommand)]
        sub: HiiQuestionCmd,
    },
```

Новый enum (рядом с `HiiStringCmd`):

```rust
#[derive(Subcommand)]
enum HiiQuestionCmd {
    Gates {
        item_id: String,
    },
    Unlock {
        item_id: String,
    },
}
```

Dispatch (в `Cmd::Hii { sub }` match, после arm'а `HiiCmd::FormSet`):

```rust
            HiiCmd::Question { sub } => match sub {
                HiiQuestionCmd::Gates { item_id } => {
                    commands::hii::question_gates(&item_id, sock, format).await
                }
                HiiQuestionCmd::Unlock { item_id } => {
                    commands::hii::question_unlock(&item_id, sock, format).await
                }
            },
```

и в `HiiFormCmd`-match после `SetVisibility`:

```rust
                HiiFormCmd::Gates { item_id } => {
                    commands::hii::form_gates(&item_id, sock, format).await
                }
                HiiFormCmd::Unlock { item_id } => {
                    commands::hii::form_unlock(&item_id, sock, format).await
                }
```

- [ ] **Step 5: Тесты (parse + e2e content-ассерты)**

`main.rs` tests (по образцу `parse_hii_form_add_args`):

```rust
    #[test]
    fn parse_hii_form_gates_and_unlock() {
        let cli = Cli::try_parse_from([
            "uefi-cli", "hii", "form", "gates", "g:0x10:0#10029",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub: HiiCmd::Form { sub: HiiFormCmd::Gates { item_id } },
            } => assert_eq!(item_id, "g:0x10:0#10029"),
            _ => panic!("expected hii form gates"),
        }
        let cli = Cli::try_parse_from([
            "uefi-cli", "hii", "question", "unlock", "g:0x10:0#10029:0x3B",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub: HiiCmd::Question { sub: HiiQuestionCmd::Unlock { item_id } },
            } => assert_eq!(item_id, "g:0x10:0#10029:0x3B"),
            _ => panic!("expected hii question unlock"),
        }
    }
```

`tests/e2e.rs` (по образцу `hii_list_output_content`, e2e.rs:57–84):

```rust
#[tokio::test(flavor = "multi_thread")]
async fn hii_gates_and_unlock_output_content() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("e2e-hii-unlock.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["hii", "form", "gates", "0#10029"])
        .assert()
        .success()
        .stdout(predicates::str::contains("suppress"))
        .stdout(predicates::str::contains("1 == 1"));

    cli(&sock, cwd)
        .args(["hii", "question", "unlock", "0#10029:0x3B"])
        .assert()
        .success()
        .stdout(predicates::str::contains("grayout"))
        .stdout(predicates::str::contains("applied pkg+0xdd1"));

    cli(&sock, cwd)
        .args(["hii", "form", "gates", "0#42", "--format", "tsv"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
```

- [ ] **Step 6: Прогон + lint + коммит**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli --all-targets -- -D warnings && cargo fmt --all`
Expected: PASS / clean.

```bash
git add crates/uefi-cli/src/client.rs crates/uefi-cli/src/commands/hii.rs crates/uefi-cli/src/main.rs crates/uefi-cli/src/output.rs crates/uefi-cli/tests/e2e.rs
git commit -m "feat(uefi-cli): hii form/question gates/unlock commands"
```

---

### Task 8: u4 — real-image приемка против эталона E12

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs`
- Test: `real_image_hii_unlock_matches_e12` (`#[ignore]`)

**Interfaces:**
- Consumes: Task 5 (`uefi_engine::hii::{gates_list, unlock}`), существующие хелперы `load_fw`, `parse_image`, `setup_pe32_node_path`-паттерн, `node_at_path`, `file_extent`.
- Produces: константа `PCI_SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21"`; хелперы `module_pe32_node_path(img, guid)`, `module_form_package(img, path)`.

**Ground truth (спека §3):** диф FORM-пакета = ровно `[(0x67A, 1, 2), (0xDD1, 1, 0xFF), (0xDD2, 1, 0xFF)]`; форма 10029 гейтнута одним REF-suppress в форме 10002; вопрос 0x003B — одним grayout `EQ_ID_VAL(0x009A, 1)`.

- [ ] **Step 1: Написать тест**

В конец `real_image.rs`:

```rust
const PCI_SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
const E12_FLIP_BYTES: [(usize, u8, u8); 3] = [(0x67A, 1, 2), (0xDD1, 1, 0xFF), (0xDD2, 1, 0xFF)];

fn module_pe32_node_path(img: &Image, guid: &str) -> Vec<usize> {
    let target =
        uefi_engine::parser::target::parse_target(&format!("{guid}:0x10:0")).unwrap();
    uefi_engine::parser::target::find_item_path(&img.root, &target)
        .expect("module PE32 node")
}

fn module_form_package(img: &Image, path: &[usize]) -> Vec<u8> {
    let pe = &node_at_path(img, path).body;
    for (off, len) in uefi_engine::hii::pe_resource::hii_resource_ranges(pe) {
        let Some(blob) = pe.get(off..off + len) else {
            continue;
        };
        let Some(list) = uefi_engine::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_FORMS {
                return pkg.bytes.to_vec();
            }
        }
    }
    panic!("FORM package not found in module .rsrc HII blob");
}

#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
#[test]
fn real_image_hii_unlock_matches_e12() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let pe_path = module_pe32_node_path(&img, PCI_SETUP_MODULE_GUID);
    let (file_start, file_end) = file_extent(&img, &pe_path);
    let pkg_before = module_form_package(&img, &pe_path);

    let form_item = format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029");
    let question_item = format!("{form_item}:0x3B");

    let form_gates = uefi_engine::hii::gates_list(&img, &form_item).expect("gates_list form");
    assert_eq!(form_gates.len(), 1, "form 10029 is gated only by the REF suppress in form 10002");
    assert_eq!(form_gates[0].gate_kind, "suppress");
    assert_eq!(form_gates[0].wraps, "ref");
    assert_eq!(form_gates[0].host_form_id, 10002);
    assert_eq!(form_gates[0].expression, "1 == 1");
    assert!(form_gates[0].flippable, "flip plan: {}", form_gates[0].flip);

    let question_gates =
        uefi_engine::hii::gates_list(&img, &question_item).expect("gates_list question");
    assert_eq!(question_gates.len(), 1, "Above 4G Decoding is gated only by its personal grayout");
    assert_eq!(question_gates[0].gate_kind, "grayout");
    assert_eq!(question_gates[0].wraps, "question");
    assert_eq!(question_gates[0].expression, "0x009A == 0x0001");
    assert!(question_gates[0].flippable);

    uefi_engine::hii::unlock(&mut img, &form_item).expect("unlock page");
    uefi_engine::hii::unlock(&mut img, &question_item).expect("unlock question");
    let built = uefi_engine::builder::build_image(&img).expect("build_image");

    assert_eq!(built.len(), data.len(), "total flash length preserved");
    assert_eq!(&built[..file_start], &data[..file_start], "bytes before the Setup file untouched");
    assert_eq!(&built[file_end..], &data[file_end..], "bytes after the Setup file untouched (slot-fit)");

    let rebuilt = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let new_path = module_pe32_node_path(&rebuilt, PCI_SETUP_MODULE_GUID);
    let pkg_after = module_form_package(&rebuilt, &new_path);
    assert_eq!(pkg_after.len(), pkg_before.len(), "unlock is length-preserving");
    let diff: Vec<(usize, u8, u8)> = pkg_before
        .iter()
        .zip(pkg_after.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| (i, *a, *b))
        .collect();
    assert_eq!(
        diff,
        E12_FLIP_BYTES.to_vec(),
        "engine unlock must reproduce the hardware-validated E12 dataflip byte-for-byte"
    );

    let (_, new_file_end) = file_extent(&rebuilt, &new_path);
    assert_eq!(new_file_end, file_end, "slot-fit keeps the Setup file extent");

    let gates_after = uefi_engine::hii::gates_list(&rebuilt, &form_item).unwrap();
    assert_eq!(gates_after[0].expression, "1 == 2");
    assert!(!gates_after[0].flippable, "already-false gate offers no flip");
    let qgates_after = uefi_engine::hii::gates_list(&rebuilt, &question_item).unwrap();
    assert_eq!(qgates_after[0].expression, "0x009A == 0xFFFF");

    let forms = uefi_engine::hii::forms::collect_forms(&rebuilt);
    assert!(
        forms.iter().any(|f| f.form_id_ifr == 10029),
        "form 10029 stays discoverable after unlock"
    );
}
```

- [ ] **Step 2: Прогон real-image (при наличии образа)**

Run: `cargo test -p uefi-engine --test real_image -- --ignored`
Expected: PASS, включая все прежние ignore-тесты (регрессия: full-flash round-trip, repatch-stability, hii forms/strings, visibility round-trip, rebuild-keeps-layout).

Если диф-позиции не совпали (0x67A/0xDD1 отсчитываются не от начала FORM-пакета) — AGENTS.md п.11: коммит `docs: fix Task 8 in unlock-op plan (pkg-offset basis)` с уточнением базиса (сверка пробником `refs/amibcp/probes/suppress_probe.py`), затем правка теста.

- [ ] **Step 3: Коммит**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): real-image unlock matches E12 dataflip"
```

---

### Task 9: Документация — актуализация TODO (+ опционально E13-чеклист)

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Актуализировать TODO.md**

В секции «Мини-цикл „unlock-op“»: пункты u1–u5 отметить `[x]` с кратким перечнем коммитов (по факту реализации); в пункте «Критические находки» отметить закрытие подпункта про PRC (удалён) и «hii string list»-пункта, если он закрывался этим циклом (по факту — он закрыт ранее, циклом 2026-09-02). Добавить отложенные миноры из ревью (по факту находок реализации): TRUE→FALSE-класс; quirk 0x8a в легаси-walker'ах `ifr.rs` (путь set-visibility на setup-модуле — no-op); FormInfo «gated»-поле для REF-гейтнутых форм (клиенты TUI/WebUI).

- [ ] **Step 2: Коммит**

```bash
git add TODO.md
git commit -m "docs(todo): unlock-op mini-cycle complete"
```

- [ ] **Step 3 (опционально, по желанию пользователя): HW-кандидат E13**

Собрать кандидат чистым движковым путём (`image open` Write → `hii form unlock` → `hii question unlock` → `image save`), прогнать побайтовую верификацию (`refs/amibcp/probes/fvmap.py`), добавить §13-чеклист в `docs/reports/2026-09-02-hw-validation-hnx99tf.md` (POST/видео; Advanced → «PCI Subsystem Settings» — пункт есть; «Above 4G Decoding» — текст, не серый, значения переключаются и сохраняются; соседи серые). Прошивает пользователь.

---

## Regression checklist (после каждой задачи; полный — после Task 8)

```bash
cargo test --all
cargo clippy --all --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test -p uefi-engine --test real_image -- --ignored   # при наличии образа
```
