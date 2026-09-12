# Formset-Unlock / перенос IIO-бифуркации (U1–U4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать формсет-уровневый unlock (кросс-формсетные suppress-гейты вокруг REF3/REF4 + fallback-инжект REF3) и перенос вопросов IIO-бифуркации в видимый Setup через varstore-декларации при question add; закрыть live-гейтом на 450x.

**Architecture:** (1) REF-варианты различаются length-байтом при едином опкоде 0x0F — новый парсер `hii/ref_variant.rs` как единственная точка чтения REF-целей; (2) `GateTarget` расширяется formset-guid → `find_gates` эмитит `CrossFormsetRef`; драйвер `hii/cross_formset.rs` обходит образ (паттерн ref_tree), unlock/gates_list мутируют донорскую секцию по `Target::Path`; (3) `QuestionAddRefSchema.formset_guid` → emit REF3 (fallback-гейто-инжект); (4) `QuestionAddList.varstores` → `emit_var_store_efi` в пролог формсета со сдвигом $SPF-записей; (5) real-гейт на 450x фиксирует фактическую ветку (флип существующего гейта или инжект).

**Tech Stack:** Rust workspace, uefi-engine (+uefi-proto/uefi-cli/uefi-tui точки отображения), tonic-proto `engine.proto`, r-efi (структуры есть, опкод-константа одна), существующие харнессы `spawn_engine_on`, `real_image.rs`, фикстуры `hii/mod.rs` tests / `form_hijack.rs` test_fixtures.

**Spec:** `docs/superpowers/specs/2026-09-12-formset-unlock-design.md` (факты REF-грамматики §2, модель/решения §3, карта NVRAM §6). Первоисточник находок: `TODO.md` раздел «Находки live-сессии 450x» (записи про кросс-формсетные REF и дугу U1–U4, факт-фикс c6da6e2).

## Global Constraints

- AGENTS.md: TDD-порядок (тест падает → реализация → тест проходит → commit); один коммит на шаг с `git commit`; после каждой задачи `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`; для задач, трогающих тесты с литералами, — `--all-targets`.
- Правило 11 (AGENTS.md): расхождение плана с реальностью — отдельный коммит `docs: fix Task N in formset-unlock plan (...)` ДО реализации.
- Никаких narration-комментариев; только короткие rustdoc `///` на pub/pub(crate)-контрактах (что делает / чего НЕ делает + ссылка на спеку §).
- REF-грамматика — только по спеке §2: опкод всегда 0x0F, точные длины {13,15,17,33,35}, никаких «REF2_OP-констант».
- Реальные образы — только в `#[ignore]`-тестах с путём `env!("CARGO_MANIFEST_DIR")/../../../refs/...` + env-override (паттерн `amibcp_path()` в `tests/real_image.rs:24-29`).
- Прогон real-тестов: `cargo test -p uefi-engine -- --ignored`.
- Фоновые процессы/сокет (для recon-шагов Task 8): `pkill -x engine`; `rm -f "$UEFIPATCHER_SOCK"` перед стартом; сокет/данные — `/tmp/uefipatcher-test/`.

---

### Task 1 (U1): `hii/ref_variant.rs` — length-дискриминация REF-вариантов

**Files:**
- Create: `crates/uefi-engine/src/hii/ref_variant.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (объявление модуля, рядом с `mod gates;` и др.)
- Test: `crates/uefi-engine/src/hii/ref_variant.rs` (tests mod)

**Interfaces:**
- Consumes: `crate::types::Guid` (uguid: `from_bytes([u8;16])`, `to_bytes()`, Copy), `r_efi::hii::IFR_REF_OP`.
- Produces (для Task 2/4/5):
  ```rust
  pub(crate) enum RefTarget {
      Form { form_id: u16 },
      FormQuestion { form_id: u16, question_id: u16 },
      Formset { formset_guid: Guid, form_id: u16, question_id: u16 },
      Dynamic,
  }
  pub(crate) fn parse_ref(op: u8, stmt: &[u8]) -> Option<RefTarget>;
  ```
  `stmt` — весь стейтмент с op-байтом; `None` = не-REF/нестандартная длина.

- [ ] **Step 1: Создать файл с тестами + объявить модуль (module-first)**

`crates/uefi-engine/src/hii/ref_variant.rs`:

```rust
use crate::types::Guid;

/// REF-цель по UEFI 2.10 §33.3.8.3.59: один опкод 0x0F, варианты
/// различаются length. Спека formset-unlock §2. REF5 (len 13) —
/// Dynamic: цель приходит из runtime-value вопроса, статически
/// неразрешима — гейты/рёбра по нему не строятся.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RefTarget {
    Form { form_id: u16 },
    FormQuestion { form_id: u16, question_id: u16 },
    Formset { formset_guid: Guid, form_id: u16, question_id: u16 },
    Dynamic,
}

/// Парсит REF-стейтмент (opcode + length + тело). Принимает только
/// канонические длины {13, 15, 17, 33, 35}; прочие — None.
#[allow(dead_code)]
pub(crate) fn parse_ref(op: u8, stmt: &[u8]) -> Option<RefTarget> {
    if op != r_efi::hii::IFR_REF_OP || stmt.len() < 2 {
        return None;
    }
    let len = (stmt[1] & 0x7F) as usize;
    if len > stmt.len() {
        return None;
    }
    let u16_at = |at: usize| -> Option<u16> {
        stmt.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    match len {
        13 => Some(RefTarget::Dynamic),
        15 => u16_at(13).map(|form_id| RefTarget::Form { form_id }),
        17 => {
            let form_id = u16_at(13)?;
            let question_id = u16_at(15)?;
            Some(RefTarget::FormQuestion {
                form_id,
                question_id,
            })
        }
        33 | 35 => {
            let form_id = u16_at(13)?;
            let question_id = u16_at(15)?;
            let guid: [u8; 16] = stmt.get(17..33)?.try_into().ok()?;
            Some(RefTarget::Formset {
                formset_guid: Guid::from_bytes(guid),
                form_id,
                question_id,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const CROSS_FORMSET: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    /// question-header 11 байт (prompt/help/qid/vsid/voff/flags — EDK2
    /// EFI_IFR_QUESTION_HEADER, спека §2) + хвост варианта.
    fn ref_stmt(total_len: usize, tail: &[u8]) -> Vec<u8> {
        assert_eq!(total_len, 2 + 11 + tail.len());
        let mut v = vec![r_efi::hii::IFR_REF_OP, total_len as u8];
        v.extend_from_slice(&[0u8; 11]);
        v.extend_from_slice(tail);
        v
    }

    #[test]
    fn parse_ref_plain_and_ref2() {
        let plain = ref_stmt(15, &1u16.to_le_bytes());
        assert_eq!(
            parse_ref(plain[0], &plain),
            Some(RefTarget::Form { form_id: 1 })
        );
        let ref2 = ref_stmt(17, &[1u16.to_le_bytes(), 0x24u16.to_le_bytes()].concat());
        assert_eq!(
            parse_ref(ref2[0], &ref2),
            Some(RefTarget::FormQuestion {
                form_id: 1,
                question_id: 0x24
            })
        );
    }

    #[test]
    fn parse_ref3_reads_formset_guid_at_17() {
        let g = Guid::from_str(CROSS_FORMSET).unwrap();
        let tail = [
            1u16.to_le_bytes().as_slice(),
            0xFFFFu16.to_le_bytes().as_slice(),
            g.to_bytes().as_slice(),
        ]
        .concat();
        let ref3 = ref_stmt(33, &tail);
        assert_eq!(
            parse_ref(ref3[0], &ref3),
            Some(RefTarget::Formset {
                formset_guid: g,
                form_id: 1,
                question_id: 0xFFFF,
            })
        );
    }

    #[test]
    fn parse_ref4_treated_as_formset_device_path_ignored() {
        let g = Guid::from_str(CROSS_FORMSET).unwrap();
        let tail = [
            2u16.to_le_bytes().as_slice(),
            0xFFFFu16.to_le_bytes().as_slice(),
            g.to_bytes().as_slice(),
            0x00AAu16.to_le_bytes().as_slice(),
        ]
        .concat();
        let ref4 = ref_stmt(35, &tail);
        assert_eq!(
            parse_ref(ref4[0], &ref4),
            Some(RefTarget::Formset {
                formset_guid: g,
                form_id: 2,
                question_id: 0xFFFF,
            })
        );
    }

    #[test]
    fn parse_ref5_is_dynamic() {
        let ref5 = ref_stmt(13, &[]);
        assert_eq!(parse_ref(ref5[0], &ref5), Some(RefTarget::Dynamic));
    }

    #[test]
    fn parse_ref_rejects_other_opcodes_and_lengths() {
        let mut weird = ref_stmt(15, &1u16.to_le_bytes());
        weird[0] = r_efi::hii::IFR_ONE_OF_OP;
        assert_eq!(parse_ref(weird[0], &weird), None);
        for len in [12u8, 14, 16, 18, 32, 34, 36] {
            let mut v = vec![r_efi::hii::IFR_REF_OP, len];
            v.resize(v.len() + len as usize, 0);
            assert_eq!(parse_ref(v[0], &v), None, "len={len}");
        }
    }
}
```

В `crates/uefi-engine/src/hii/mod.rs` рядом с прочими `mod …;` добавить (в том же шаге, до прогона тестов — module-first rule):

```rust
mod ref_variant;
```

- [ ] **Step 2: Прогнать тесты — падают до реализации**

Реализация уже в файле из Step 1 (файл создаётся целиком). Если предпочитешь строгий red-first: закомментируй тело `parse_ref` (`unimplemented!()`), прогони, верни тело. Практический минимум: убедиться, что тесты ВИДНЫ (не «0 из 0»).

Run: `cargo test -p uefi-engine ref_variant`
Expected: 5 passed (не «0 filtered out» — module-first).

- [ ] **Step 3: Прогнать крейт + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings`
Expected: PASS, 0 warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/src/hii/ref_variant.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): ref_variant — length-дискриминация REF2-REF5 (formset-unlock U1)"
```

---

### Task 2 (U1): кросс-формсетные гейты в `gates.rs`

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs:24-29` (Wraps), `:49-53` (GateTarget), `:201-210` (REF-ветка emit_gates)
- Modify: `crates/uefi-engine/src/hii/form_hijack.rs:142,146` (GateTarget-литералы)
- Modify: `crates/uefi-engine/src/hii/mod.rs:270-285` (gate_info match), `:306,330` (GateTarget-литералы)
- Modify: `crates/uefi-engine/tests/real_image.rs:2242,2246,2444,2448,2462` (GateTarget-литералы)
- Test: `crates/uefi-engine/src/hii/gates.rs` (tests mod)

**Interfaces:**
- Consumes: `super::ref_variant::{parse_ref, RefTarget}` (Task 1).
- Produces (для Task 3): `GateTarget { form_id: u16, question_id: Option<u16>, formset_guid: Option<Guid> }` (остаётся `Copy` — `uguid::Guid` копируем); `Wraps::CrossFormsetRef { form_id: u16, host_form_id: u16, formset_guid: Guid }`. Семантика: `formset_guid = None` — прежнее поведение (свои REF/REF2); `Some(g)` — матч только REF3/REF4 с `FormSetGuid@17 == g && FormId@13 == form_id`.

- [ ] **Step 1: Написать падающие тесты (в tests mod gates.rs, рядом с `find_gates_reports_ref_suppress_for_target_form`)**

```rust
    const CROSS_SET: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    /// REF3: header(11) + FormId + QuestionId(0xFFFF) + FormSetGuid → len 33.
    fn ref3_op(form_id: u16, formset: &str) -> Vec<u8> {
        let g = Guid::from_str(formset).unwrap();
        let payload = [
            vec![0u8; 11],
            form_id.to_le_bytes().to_vec(),
            0xFFFFu16.to_le_bytes().to_vec(),
            g.to_bytes().to_vec(),
        ]
        .concat();
        opcode(IFR_REF_OP, false, &payload)
    }

    fn cross_ifr() -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10002, 20));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(ref3_op(1, CROSS_SET));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    #[test]
    fn find_gates_reports_cross_formset_ref_gate() {
        let pkg = package(&cross_ifr());
        let gates = find_gates(
            &pkg,
            &GateTarget {
                form_id: 1,
                question_id: None,
                formset_guid: Some(Guid::from_str(CROSS_SET).unwrap()),
            },
        );
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].kind, GateKind::Suppress);
        assert_eq!(
            gates[0].wraps,
            Wraps::CrossFormsetRef {
                form_id: 1,
                host_form_id: 10002,
                formset_guid: Guid::from_str(CROSS_SET).unwrap(),
            }
        );
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 1 });
        let flips = plan_gates(&pkg, &gates).unwrap();
        assert_eq!(flips.len(), 1, "кросс-гейт флипается той же механикой");
    }

    #[test]
    fn find_gates_cross_target_ignores_plain_and_foreign_ref3() {
        let mut mixed = cross_ifr();
        mixed.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        mixed.extend(ref_op(1, 0x003A));
        mixed.extend(end());
        let pkg = package(&mixed);
        let gt = GateTarget {
            form_id: 1,
            question_id: None,
            formset_guid: Some(Guid::from_str(CROSS_SET).unwrap()),
        };
        assert_eq!(find_gates(&pkg, &gt).len(), 1);
        let foreign = GateTarget {
            formset_guid: Some(Guid::from_str(FORMSET_GUID).unwrap()),
            ..gt
        };
        assert!(find_gates(&pkg, &foreign).is_empty());
    }

    #[test]
    fn find_gates_ref3_no_longer_matches_as_plain_ref() {
        let pkg = package(&cross_ifr());
        let plain_target = GateTarget {
            form_id: 1,
            question_id: None,
            formset_guid: None,
        };
        assert!(
            find_gates(&pkg, &plain_target).is_empty(),
            "REF3 без formset-таргета не считается intra-formset REF"
        );
    }
```

- [ ] **Step 2: Прогнать — падают (нет поля/варианта)**

Run: `cargo test -p uefi-engine gates`
Expected: FAIL (E0560 нет поля formset_guid / нет варианта CrossFormsetRef).

- [ ] **Step 3: Реализация**

`gates.rs`, Wraps:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wraps {
    Form { form_id: u16 },
    Ref { form_id: u16, host_form_id: u16 },
    CrossFormsetRef { form_id: u16, host_form_id: u16, formset_guid: Guid },
    Question { form_id: u16, question_id: u16 },
}
```

GateTarget:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateTarget {
    pub form_id: u16,
    pub question_id: Option<u16>,
    pub formset_guid: Option<Guid>,
}
```

REF-ветка emit_gates (заменяет блок `op == IFR_REF_OP && length >= 15 …`):

```rust
    } else if op == IFR_REF_OP && target.question_id.is_none() {
        match super::ref_variant::parse_ref(op, &body[stmt_offset..stmt_offset + length]) {
            Some(super::ref_variant::RefTarget::Formset {
                formset_guid,
                form_id,
                ..
            }) if target.formset_guid == Some(formset_guid) && form_id == target.form_id => {
                Some(Wraps::CrossFormsetRef {
                    form_id,
                    host_form_id: current_form.unwrap_or(0),
                    formset_guid,
                })
            }
            Some(super::ref_variant::RefTarget::Form { form_id })
            | Some(super::ref_variant::RefTarget::FormQuestion { form_id, .. })
                if target.formset_guid.is_none() && form_id == target.form_id =>
            {
                Some(Wraps::Ref {
                    form_id,
                    host_form_id: current_form.unwrap_or(0),
                })
            }
            _ => None,
        }
    }
```

Обновить литералы `GateTarget { … }` добавлением `formset_guid: None`:
- `gates.rs` tests: consts `FORM_GATE_TARGET`/`QUESTION_GATE_TARGET` и все inline-литералы (~7 мест);
- `form_hijack.rs:142-149` (2);
- `hii/mod.rs:306,330` (2);
- `tests/real_image.rs:2242-2462` (5).

`hii/mod.rs` gate_info match — новый arm (перед `Question`):

```rust
        gates::Wraps::CrossFormsetRef {
            form_id,
            host_form_id,
            ..
        } => ("cross_ref", form_id as u32, host_form_id as u32, 0),
```

- [ ] **Step 4: Прогнать крейт + clippy (существующие gates-тесты — регрессия)**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings`
Expected: PASS (включая hijack- и question-гейт-тесты с `formset_guid: None`).

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/gates.rs crates/uefi-engine/src/hii/form_hijack.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/tests/real_image.rs
git commit -m "feat(engine): кросс-формсетные гейты Wraps::CrossFormsetRef + GateTarget.formset_guid (formset-unlock U1)"
```

---

### Task 3 (U2a): драйвер кросс-гейтов + unlock/gates_list по донорским секциям

**Files:**
- Create: `crates/uefi-engine/src/hii/cross_formset.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (объявление модуля; `form_package_ranges` → `pub(crate)`; unlock `:327-398`; gates_list `:299-318`)
- Modify: `crates/uefi-proto/proto/engine.proto:249-259` (GateInfo + source_target)
- Modify: `crates/uefi-cli/src/output.rs:203-247` (print_gates)
- Modify (литералы GateInfo — добавить поле): `crates/uefi-cli/tests/mock_server.rs`, `crates/uefi-tui/tests/mock_server.rs`, `crates/uefi-tui/src/forms.rs` (gate-строки в form_details_text)
- Test: `crates/uefi-engine/src/hii/cross_formset.rs` (tests mod), `crates/uefi-engine/src/hii/mod.rs` (unlock-тест)

**Interfaces:**
- Consumes: Task 2 (`GateTarget.formset_guid`, `Wraps::CrossFormsetRef`), `hii::form_package_ranges(&FfsNode) -> Vec<(usize, usize)>` (стало pub(crate)), `parser::target::find_item_path`, `ops::mark_rebuild_to_root_by_path`, `resolve_writable_path`, `gates::{find_gates, plan_gates_skip_unlocked, apply_flips, PlannedFlip}`.
- Produces (для Task 8):
  ```rust
  pub(crate) struct CrossGateSite {
      pub path: Vec<usize>,        // путь до донорской секции от root
      pub source_ffs: String,      // upper-GUID FFS-владельца (для GateInfo.source_target)
      pub pkg_start: usize,        // смещение form-пакета в node.body
      pub pkg_len: usize,
      pub gates: Vec<gates::Gate>,
  }
  pub fn find_cross_gates(image: &Image, skip_path: &[usize], gt: &GateTarget) -> Vec<CrossGateSite>;
  ```
  Formset-фильтр цели живёт в `GateTarget.formset_guid` (Task 2) — отдельного параметра не нужно. Proto: `GateInfo { … string source_target = 10; }` — пусто для гейтов собственной секции.

- [ ] **Step 1: Протонизировать GateInfo.source_target**

`crates/uefi-proto/proto/engine.proto`, message GateInfo — добавить последним полем:

```proto
  string source_target = 10;   // upper-GUID FFS донора (кросс-формсетные гейты)
```

Собрать: `cargo build -p uefi-proto`. Обновить литералы `GateInfo { … }` (добавить `source_target: String::new()`) в: `crates/uefi-cli/tests/mock_server.rs`, `crates/uefi-tui/tests/mock_server.rs`, `crates/uefi-tui/src/forms.rs` (gate-рендер), `crates/uefi-engine/src/hii/mod.rs` (gate_info). Прогнать `cargo test --all` — всё зелёное (поле additive).

- [ ] **Step 2: Написать падающий тест драйвера**

`crates/uefi-engine/src/hii/cross_formset.rs` (создать сразу с tests mod; `mod cross_formset;` в `hii/mod.rs` — тем же шагом). Фикстуры — стиль `ref_tree.rs` tests (`mk_node`/`mk_image`): образ с ДВУМЯ файлами:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hii::gates::GateTarget;
    use crate::types::{Action, FfsNode, FfsType, Guid, Image, ImageMode, ParsingData};
    use r_efi::hii::{IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP,
        IFR_UINT64_OP, IFR_EQUAL_OP, PACKAGE_FORMS};
    use std::str::FromStr;

    const SETUP_FILE: &str = "899407d7-99fe-43d8-9a21-79ec328cac21";
    const RC_SET: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    fn opcode(op: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }
    fn package(ifr: &[u8]) -> Vec<u8> {
        let total = 4 + ifr.len();
        let mut b = vec![
            (total & 0xFF) as u8, ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8, PACKAGE_FORMS,
        ];
        b.extend_from_slice(ifr);
        b
    }

    /// Пакет корневого Setup: форма 10001 c suppressed REF3 → RC_SET#1.
    fn donor_pkg() -> Vec<u8> {
        let g = Guid::from_str(RC_SET).unwrap();
        let ref3 = [
            vec![0u8; 11],
            1u16.to_le_bytes().to_vec(),
            0xFFFFu16.to_le_bytes().to_vec(),
            g.to_bytes().to_vec(),
        ].concat();
        let mut ifr = opcode(IFR_FORM_SET_OP, true, &[0u8; 21]);
        ifr.extend(opcode(IFR_FORM_OP, true, &[10001u16.to_le_bytes(), 20u16.to_le_bytes()].concat()));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(vec![IFR_UINT64_OP, 0x0A].iter().copied().chain(1u64.to_le_bytes()).collect::<Vec<u8>>());
        ifr.extend(vec![IFR_UINT64_OP, 0x0A].iter().copied().chain(1u64.to_le_bytes()).collect::<Vec<u8>>());
        ifr.extend(vec![IFR_EQUAL_OP, 0x02]);
        ifr.extend(opcode(r_efi::hii::IFR_REF_OP, false, &ref3));
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        package(&ifr)
    }

    /// Пакет IntelRCSetup: formset GUID RC_SET, форма 1 без гейтов.
    fn target_pkg() -> Vec<u8> {
        let g = Guid::from_str(RC_SET).unwrap();
        let mut p = g.to_bytes().to_vec();
        p.extend_from_slice(&7u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0);
        let mut ifr = opcode(IFR_FORM_SET_OP, true, &p);
        ifr.extend(opcode(IFR_FORM_OP, true, &[1u16.to_le_bytes(), 21u16.to_le_bytes()].concat()));
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        package(&ifr)
    }

    fn mk_node(guid: Option<Guid>, node_type: FfsType, subtype: u8, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid, node_type, subtype, offset: 0, header: vec![], body, tail: vec![],
            children, action: Action::NoAction, parsing_data: ParsingData::None,
            fixed: false, compressed: false, alignment_bytes: vec![],
        }
    }

    fn two_file_image() -> Image {
        let donor_sec = mk_node(None, FfsType::Section, 0x19, donor_pkg(), vec![]);
        let donor_file = mk_node(
            Some(Guid::from_str(SETUP_FILE).unwrap()), FfsType::File, 0x07,
            vec![], vec![donor_sec],
        );
        let target_sec = mk_node(None, FfsType::Section, 0x19, target_pkg(), vec![]);
        let target_file = mk_node(
            Some(Guid::from_str("abbce13d-e25a-4d9f-a1f9-2f7710786892").unwrap()),
            FfsType::File, 0x07, vec![], vec![target_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![donor_file, target_file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        Image { image_id: "img".into(), session_id: "s".into(), root, mode: ImageMode::Write }
    }

    fn skip_path_of_target(image: &Image) -> Vec<usize> {
        crate::parser::target::find_item_path(
            &image.root,
            &crate::types::Target::GuidSection {
                guid: Guid::from_str("abbce13d-e25a-4d9f-a1f9-2f7710786892").unwrap(),
                section_type: 0x19,
                section_index: Some(0),
            },
        ).unwrap()
    }

    #[test]
    fn find_cross_gates_locates_donor_site_and_skips_own_section() {
        let image = two_file_image();
        let gt = GateTarget {
            form_id: 1, question_id: None,
            formset_guid: Some(Guid::from_str(RC_SET).unwrap()),
        };
        let sites = find_cross_gates(&image, &skip_path_of_target(&image), &gt);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].source_ffs, "899407D7-99FE-43D8-9A21-79EC328CAC21");
        assert_eq!(sites[0].pkg_start, 0);
        assert_eq!(sites[0].gates.len(), 1);
        // сайт резолвится по своему path в ту же секцию с REF3
        let node = node_by_path(&image.root, &sites[0].path);
        assert_eq!(node.subtype, 0x19);
        assert!(node
            .body
            .windows(2)
            .any(|w| w[0] == r_efi::hii::IFR_REF_OP && w[1] == 33));
    }

    fn node_by_path<'a>(root: &'a FfsNode, path: &[usize]) -> &'a FfsNode {
        let mut n = root;
        for &i in path { n = &n.children[i]; }
        n
    }

    #[test]
    fn find_cross_gates_empty_when_no_donor() {
        // тот же образ, но таргет-форма без внешних REF → пусто
        let image = two_file_image();
        let gt = GateTarget {
            form_id: 42, question_id: None,
            formset_guid: Some(Guid::from_str(RC_SET).unwrap()),
        };
        assert!(find_cross_gates(&image, &skip_path_of_target(&image), &gt).is_empty());
    }
}
```

- [ ] **Step 3: Прогнать — падают (нет модуля/функции)**

Run: `cargo test -p uefi-engine cross_formset`
Expected: FAIL (функция не определена — сначала заглука `unimplemented!()` не нужна, пишем реализацию сразу после красного прогона структуры).

- [ ] **Step 4: Реализация cross_formset.rs (prod-часть, над tests mod)**

```rust
use crate::ffs::{EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW};
use crate::hii::form_package_ranges;
use crate::hii::gates::{self, GateTarget};
use crate::types::{FfsNode, FfsType, Guid, Image, guid_to_upper_string};

pub(crate) struct CrossGateSite {
    pub path: Vec<usize>,
    pub source_ffs: String,
    pub pkg_start: usize,
    pub pkg_len: usize,
    pub gates: Vec<gates::Gate>,
}

/// Кросс-формсетные гейты по всему образу (спека formset-unlock §3 U2a):
/// form-пакеты всех секций, кроме собственной (skip_path). Фильтр цели —
/// в GateTarget.formset_guid. Порядок — порядок обхода дерева. Только
/// поиск, без мутации.
pub fn find_cross_gates(
    image: &Image,
    skip_path: &[usize],
    gt: &GateTarget,
) -> Vec<CrossGateSite> {
    let mut out = Vec::new();
    let mut path: Vec<usize> = Vec::new();
    walk_files(&image.root, &mut path, skip_path, gt, &mut out);
    out
}

fn walk_files(
    node: &FfsNode,
    path: &mut Vec<usize>,
    skip_path: &[usize],
    gt: &GateTarget,
    out: &mut Vec<CrossGateSite>,
) {
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if child.node_type == FfsType::File && child.guid.is_some() {
            walk_sections(child, path, child.guid.as_ref().unwrap(), skip_path, gt, out);
        }
        walk_files(child, path, skip_path, gt, out);
        path.pop();
    }
}

fn walk_sections(
    file: &FfsNode,
    path: &mut Vec<usize>,
    file_guid: &Guid,
    skip_path: &[usize],
    gt: &GateTarget,
    out: &mut Vec<CrossGateSite>,
) {
    for (i, sec) in file.children.iter().enumerate() {
        if sec.node_type != FfsType::Section {
            continue;
        }
        path.push(i);
        match sec.subtype {
            EFI_SECTION_RAW | EFI_SECTION_PE32 => {
                scan_section(sec, path, file_guid, skip_path, gt, out);
            }
            EFI_SECTION_COMPRESSION | EFI_SECTION_GUID_DEFINED => {
                walk_sections(sec, path, file_guid, skip_path, gt, out);
            }
            _ => {}
        }
        path.pop();
    }
}

fn scan_section(
    sec: &FfsNode,
    path: &[usize],
    file_guid: &Guid,
    skip_path: &[usize],
    gt: &GateTarget,
    out: &mut Vec<CrossGateSite>,
) {
    if path == skip_path {
        return;
    }
    for (start, len) in form_package_ranges(sec) {
        let pkg = &sec.body[start..start + len];
        let found = gates::find_gates(pkg, gt);
        if found.is_empty() {
            continue;
        }
        out.push(CrossGateSite {
            path: path.to_vec(),
            source_ffs: guid_to_upper_string(file_guid),
            pkg_start: start,
            pkg_len: len,
            gates: found,
        });
    }
}
```

В `hii/mod.rs`: `mod cross_formset;`, `form_package_ranges` → `pub(crate) fn`.

- [ ] **Step 5: Прогнать cross_formset-тесты**

Run: `cargo test -p uefi-engine cross_formset`
Expected: PASS (2).

- [ ] **Step 6: Написать падающий unlock-тест (mod.rs tests, рядом с unlock-тестами; фикстуры two_file_image/skip_path_of_target вынеси из `cross_formset::tests` в `#[cfg(test)] pub(crate) mod cross_fixtures` внутри cross_formset.rs и переиспользуй из mod.rs)**

```rust
    #[test]
    fn unlock_flips_cross_formset_gate_in_donor_section() {
        let mut image = cross_formset::cross_fixtures::two_file_image();
        let item = "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1";
        let before = {
            let path = crate::parser::target::find_item_path(
                &image.root,
                &crate::types::Target::GuidSection {
                    guid: crate::types::Guid::try_parse(
                        "899407d7-99fe-43d8-9a21-79ec328cac21").unwrap(),
                    section_type: 0x19,
                    section_index: Some(0),
                },
            ).unwrap();
            let mut n = &image.root;
            for &i in &path { n = &n.children[i]; }
            n.body.clone()
        };
        let outcome = unlock(&mut image, item).unwrap();
        assert!(!outcome.applied.is_empty(), "кросс-гейт донора флипнут");
        assert!(outcome.gates.iter().any(|g| g.wraps == "cross_ref"
            && g.source_target == "899407D7-99FE-43D8-9A21-79EC328CAC21"));
        // донорская секция мутировала in-place, длина сохранена
        let after = { /* повторить чтение before */ before_placeholder() };
        let _ = after;
    }
```

(В тесте вместо `before_placeholder()` — второй блок чтения тела донора, идентичный первому; ассерты: `after != before`, `after.len() == before.len()`, diff ровно 1 байт `1 → 2` по смещению из outcome.gates[0].)

Плюс тест no-op паритета: `unlock` на образе без кросс-гейтов (target_pkg без донора) → `applied.is_empty()`, тело донора не тронуто (P0-семантика).

Fix round 1 (review finding, Important): skip-own сделать наблюдаемо покрытым — в `cross_fixtures` добавить `own_cross_pkg()` (пакет целевого формсета RC_SET, форма 10001 с suppressed REF3 → RC_SET#1 — «собственный» матчащий кросс-гейт) и конструктор `two_file_image_with(donor, target)` (донор и таргет с параметризуемыми телами). Ассерты: `find_cross_gates` на образе, где ОБЕ секции несут матчащий гейт, возвращает ровно 1 сайт (донор), `sites[0].path != skip_path`; `gates_list` на том же образе не дублирует гейт собственной секции (`len == 1`, `source_target` — донорский GUID).

- [ ] **Step 7: Реализация — wiring в unlock/gates_list (hii/mod.rs)**

В `unlock`, после существующего own-node блока (после закрывающей `}` привязки `mutated`), до `if mutated`:

```rust
    let cross_applied = if gt.question_id.is_none() {
        apply_cross_formset_gates(image, &target, gt, &mut infos, &mut applied)?
    } else {
        false
    };
    let mutated = mutated || cross_applied;
```

Fix round 1 (review finding, Important): кросс-фаза — только для form-таргетов (`gt.question_id.is_none()`). Причина: Question-рука `find_gates` матчится по голой паре (form_id, question_id) без привязки к формсету — question-таргетный unlock мог бы флипать гейты в неродственных донорах по совпадающей (form, qid) паре; кросс-фаза дуги формсет-скоуплена по спеке §3 U2a (form-only таргеты). Эквивалентный guard — первой строкой `apply_cross_formset_gates` (`if gt.question_id.is_some() { return Ok(false); }`) и в условии кросс-фазы `gates_list`. Негатив-тест: question-таргет на образе, где донор несёт гейт на совпадающую (form, qid) пару, → донор не тронут (byte-identity, нет rebuild-меток), кросс-инфосов нет.

Дефект-фикс: в текущем own-node блоке ветка `absolute_flips.is_empty()` делает ранний `return Ok(UnlockOutcome { .. })` — с таким ранним выходом кросс-фаза никогда не выполняется для таргета без собственных флипов (основной кросс-случай: target-форма без гейтов, донор с REF3-гейтом). Ветку заменить на fall-through: `node.body = body; false` (без `return`), чтобы поток дошёл до кросс-фазы. P0-семантика сохраняется: при нулевых флипах суммарно `mutated == false` → `mark_rebuild_to_root_by_path` не вызывается, тело не тронуто.

Новая функция в mod.rs (использует helpers `node_at`/`node_at_mut` уже существующие):

```rust
/// Кросс-формсетная фаза unlock (спека §3 U2a): гейты в чужих секциях,
/// мутация донора по path. Возвращает true, если хоть один флип применён.
fn apply_cross_formset_gates(
    image: &mut Image,
    target: &crate::types::Target,
    gt: gates::GateTarget,
    infos: &mut Vec<uefi_proto::GateInfo>,
    applied: &mut Vec<String>,
) -> Result<bool, HiiError> {
    if gt.question_id.is_some() {
        return Ok(false);
    }
    let (skip_path, own_formset) = {
        let node = crate::parser::target::find_item(&image.root, target)
            .map_err(|_| HiiError::NotFound)?;
        let mut own = None;
        for (start, len) in form_package_ranges(node) {
            if let Some(fs) = ifr::parse_form_package(&node.body[start..start + len]) {
                own = Some(fs.guid);
                break;
            }
        }
        let Some(own_formset) = own else {
            return Ok(false);
        };
        let skip_path = crate::parser::target::find_item_path(&image.root, target)
            .ok_or(HiiError::NotFound)?;
        (skip_path, own_formset)
    };
    let gt_cross = gates::GateTarget {
        formset_guid: Some(own_formset),
        ..gt
    };
    let sites = cross_formset::find_cross_gates(image, &skip_path, &gt_cross);
    let mut any = false;
    for site in sites {
        resolve_writable_path(image, &crate::types::Target::Path(site.path.clone()))?;
        let pkg = {
            let node = node_at(&image.root, &site.path);
            node.body[site.pkg_start..site.pkg_start + site.pkg_len].to_vec()
        };
        let flips = gates::plan_gates_skip_unlocked(&pkg, &site.gates)
            .map_err(HiiError::GateExpressionUnsupported)?;
        for gate in &site.gates {
            let mut gi = gate_info(&pkg, gate);
            gi.source_target = site.source_ffs.clone();
            infos.push(gi);
        }
        if flips.is_empty() {
            continue;
        }
        applied.extend(flips.iter().map(flip_text));
        let absolute: Vec<gates::PlannedFlip> = flips
            .into_iter()
            .map(|f| gates::PlannedFlip {
                offset: site.pkg_start + f.offset,
                from: f.from,
                to: f.to,
            })
            .collect();
        let node = node_at_mut(&mut image.root, &site.path);
        gates::apply_flips(&mut node.body, &absolute)
            .map_err(HiiError::GateExpressionUnsupported)?;
        ops::mark_rebuild_to_root_by_path(&mut image.root, &site.path);
        any = true;
    }
    Ok(any)
}
```

В `gates_list` — та же кросс-фаза read-only (после существующего цикла): сформировать `gt_cross`, `find_cross_gates`, `infos.push` с `source_target` — только при `gt.question_id.is_none()` (guard fix round 1: Question-рука `find_gates` не формсет-скоуплена, спека §3 U2a).

Замечания: `resolve_writable_path` уже проверяет `ImageMode::Write` и барьеры — вызов ДО планирования даёт честный `NotWritable`/`MutationBehindCompression` без частичной мутации. `node_at`/`node_at_mut` уже существуют как приватные в hii/mod.rs (используются set_value/add_question) и продублированы приватными копиями в form_hijack.rs — «перенос» сводится к удалению копий из form_hijack.rs и переключению его двух call-site'ов на `super::node_at`/`super::node_at_mut` (в mod.rs сделать их `pub(crate)`); тесты form_hijack — регрессия.

- [ ] **Step 8: CLI-печать source_target**

`crates/uefi-cli/src/output.rs` print_gates: TSV — колонка `source_target` в header и строках; Text — суффикс при непустом значении:

```rust
                let src = if g.source_target.is_empty() {
                    String::new()
                } else {
                    format!(" @{}", g.source_target)
                };
                println!(
                    "{:<8} {:<8} form {} host {} qid {} expr '{}' flip '{}' @pkg+{:#x}{}",
                    // … прежние аргументы …
                    src
                );
```

TUI: в `crates/uefi-tui/src/forms.rs` там, где GateInfo рендерится в детали формы (grep `gate_kind`/`GateInfo`), добавить тот же суффикс ` @{source_target}` при непустом поле. Обновить `crates/uefi-tui/src/forms.rs` gate-фикстуры (`source_target: …` в литералах) и оба mock_server.rs.

- [ ] **Step 9: Полный прогон + clippy**

Run: `cargo test --all && cargo clippy --all --all-targets -- -D warnings`
Expected: PASS (моки/фикстуры с новым полем собраны).

- [ ] **Step 10: Commit**

```bash
git add -A crates/uefi-engine/src/hii crates/uefi-proto/proto/engine.proto crates/uefi-cli/src/output.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-tui
git commit -m "feat(engine,cli,tui): формсет-unlock — кросс-гейты по донорским секциям, GateInfo.source_target (formset-unlock U2a)"
```

---

### Task 4 (U2b): emit REF3 — fallback-инжект кросс-формсетного GOTO

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr_builder.rs:190-207` (+emit_ref3)
- Modify: `crates/uefi-engine/src/hii/schema.rs:244-251` (QuestionAddRefSchema + formset_guid)
- Modify: `crates/uefi-engine/src/hii/mod.rs:1401-1412` (build_ref_ops)
- Test: `crates/uefi-engine/src/hii/ifr_builder.rs` (tests), `crates/uefi-engine/src/hii/mod.rs` (tests)

**Interfaces:**
- Consumes: Task 1 `RefTarget::Formset` (проверка эмит-байтов ре-парсом).
- Produces: `IfrBuilder::emit_ref3(&mut self, prompt_id: u16, help_id: u16, qid: u16, form_id: u16, formset: &Guid)` — total 33; schema-поле `QuestionAddRefSchema.formset_guid: Option<String>` (serde default/None, GUID валидируется `Guid::try_parse` → `HiiError::InvalidSchema` при ошибке).

- [ ] **Step 1: Падающий builder-тест (ifr_builder.rs tests)**

```rust
    #[test]
    fn emit_ref3_layout() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        b.emit_ref3(0x40, 0x41, 0x7F10, 1, &g);
        let buf = b.build();
        assert_eq!(buf[0], OP_REF);
        assert_eq!(buf[1] & 0x7F, 33);
        assert_eq!(u16::from_le_bytes([buf[13], buf[14]]), 1);
        assert_eq!(u16::from_le_bytes([buf[15], buf[16]]), 0xFFFF);
        assert_eq!(&buf[17..33], &g.to_bytes());
    }
```

- [ ] **Step 2: Прогнать — FAIL (нет метода)**

Run: `cargo test -p uefi-engine emit_ref3`

- [ ] **Step 3: Реализация emit_ref3 (после emit_ref)**

```rust
    /// REF3 (спека formset-unlock §2): кросс-формсетный GOTO, total 33.
    /// QuestionId — 0xFFFF (EFI_QUESTION_ID_INVALID, паттерн EDK2 CIfrRef3).
    pub fn emit_ref3(
        &mut self,
        prompt_id: u16,
        help_id: u16,
        qid: u16,
        form_id: u16,
        formset: &Guid,
    ) {
        self.write_header(OP_REF, false, 31);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
        self.buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
        self.buf.push(0);
        self.buf.extend_from_slice(&form_id.to_le_bytes());
        self.buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
        self.buf.extend_from_slice(&guid_to_bytes(formset));
    }
```

- [ ] **Step 4: Падающий schema/mod-тест**

`schema.rs` tests:

```rust
    #[test]
    fn parse_ref_schema_with_formset_guid() {
        let s = parse_question_add_schema(
            r#"{ "refs": [ { "form_id": 1, "prompt": "P", "help": "H",
                "question_id": 32800,
                "formset_guid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9" } ] }"#,
        ).unwrap();
        assert_eq!(s.refs[0].formset_guid.as_deref(), Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"));
    }
```

`mod.rs` tests — по образцу существующего add_ref-теста (grep `add_ref`): после add_ref со схемой formset_guid → re-parse пакета цели: `ref_variant::parse_ref` на вставленном стейтменте даёт `Formset { formset_guid: …, form_id: 1 }`. Плюс негативный: formset_guid `"not-a-guid"` → `Err(HiiError::InvalidSchema)`.

- [ ] **Step 5: Реализация schema + build_ref_ops**

`schema.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionAddRefSchema {
    pub form_id: u16,
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formset_guid: Option<String>,
}
```

`mod.rs` build_ref_ops:

```rust
fn build_ref_ops(schema: &schema::QuestionAddRefSchema, prompt_id: u16, help_id: u16) -> Result<Vec<u8>, HiiError> {
    let mut b = ifr_builder::IfrBuilder::new();
    match &schema.formset_guid {
        None => b.emit_ref(prompt_id, help_id, schema.question_id, 0, 0xFFFF, schema.form_id),
        Some(gs) => {
            let g = crate::types::Guid::try_parse(gs).map_err(|_| {
                HiiError::InvalidSchema(format!("formset_guid '{gs}' is not a GUID"))
            })?;
            b.emit_ref3(prompt_id, help_id, schema.question_id, schema.form_id, &g);
        }
    }
    Ok(b.build())
}
```

Прокинуть `Result` по двум call-сайтам build_ref_ops (add_ref + preflight-путь в check_ref_add — grep `build_ref_ops`).

- [ ] **Step 6: Прогнать крейт + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr_builder.rs crates/uefi-engine/src/hii/schema.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): emit REF3 + QuestionAddRefSchema.formset_guid — инжект кросс-формсетного GOTO (formset-unlock U2b)"
```

---

### Task 5 (U1c): кросс-рёбра ref_tree + FormEdge.target_formset_guid + TUI

**Files:**
- Modify: `crates/uefi-engine/src/hii/ref_tree.rs:11-28` (package_edges), `:94-106` (push_edges)
- Modify: `crates/uefi-proto/proto/engine.proto:177-181` (FormEdge + target_formset_guid)
- Modify (литералы FormEdge): `crates/uefi-tui/tests/mock_server.rs`, `crates/uefi-tui/src/forms.rs` (tests-хелпер `edge`), `crates/uefi-tui/src/app.rs` (tests), `crates/uefi-tui/src/commands.rs`
- Modify: `crates/uefi-tui/src/forms.rs` (build_tree_rows — резолв кросс-детей)
- Test: `crates/uefi-engine/src/hii/ref_tree.rs` (tests), `crates/uefi-tui/src/forms.rs` (tests)

**Interfaces:**
- Consumes: Task 1 `ref_variant::parse_ref`.
- Produces: `package_edges(pkg) -> Vec<(u16, u16, Option<Guid>)>` (parent, child, child-формсет); proto `FormEdge { … string target_formset_guid = 4; }` (пусто = intra). TUI: кросс-ребро резолвится в строку формы чужого формсета по паре (target_formset_guid, form_id); нерезолвленное — DanglingRef.

- [ ] **Step 1: Прото + литералы**

`engine.proto` FormEdge:

```proto
message FormEdge {
  string formset_guid = 1;
  uint32 parent_form_id = 2;
  uint32 form_id = 3;             // цель REF-вопроса
  string target_formset_guid = 4; // непусто = кросс-формсетный REF3/REF4
}
```

`cargo build -p uefi-proto`; во всех литералах `FormEdge { … }` добавить `target_formset_guid: String::new()` (5 файлов выше; в тест-хелпере `edge()` TUI — параметр `target: &str`).

- [ ] **Step 2: Падающий тест ref_tree (рядом с package_edges_dedup_and_order)**

```rust
    fn ref3_op(qid: u16, target_form: u16, formset: &str) -> Vec<u8> {
        let g = Guid::from_str(formset).unwrap();
        let payload = [
            question_header(0x10, qid, 0xFFFF, 0),
            target_form.to_le_bytes().to_vec(),
            0xFFFFu16.to_le_bytes().to_vec(),
            g.to_bytes().to_vec(),
        ].concat();
        opcode(IFR_REF_OP, false, &payload)
    }

    #[test]
    fn package_edges_marks_cross_formset_children() {
        let pkg = package(
            &[
                form_set(),
                form(10001),
                ref_op(0x30, 10019),
                ref3_op(0x31, 1, "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
                end(),
                end(),
                end(),
            ].concat(),
        );
        let edges = package_edges(&pkg);
        assert_eq!(
            edges,
            vec![
                (10001, 10019, None),
                (
                    10001,
                    1,
                    Some(Guid::from_str("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap())
                ),
            ]
        );
    }

    #[test]
    fn collect_edges_carries_target_formset_guid() {
        // тот же pkg в mk_image → FormEdge.target_formset_guid == "EC87D643-…"
        let pkg = package(
            &[
                form_set(),
                form(10001),
                ref3_op(0x31, 1, "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
                end(),
                end(),
                end(),
            ].concat(),
        );
        let edges = collect_edges(&mk_image(pkg));
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target_formset_guid, "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9");
    }
```

- [ ] **Step 3: Прогнать — FAIL**

Run: `cargo test -p uefi-engine ref_tree`

- [ ] **Step 4: Реализация package_edges/push_edges**

```rust
/// REF-рёбра одного form-пакета: (родитель, цель, формсет цели).
/// REF3/REF4 (len 33/35) несут FormSetGuid@17 — цель чужого формсета;
/// REF5 (Dynamic) рёбер не даёт. Спека formset-unlock §3 U1c.
pub(crate) fn package_edges(pkg: &[u8]) -> Vec<(u16, u16, Option<crate::types::Guid>)> {
    let mut out: Vec<(u16, u16, Option<crate::types::Guid>)> = Vec::new();
    walk_statements(pkg, |op, off, len, current| {
        if op != IFR_REF_OP || len < 15 {
            return;
        }
        let Some(parent) = current else { return };
        let (child, cross) = match crate::hii::ref_variant::parse_ref(op, &pkg[off..off + len]) {
            Some(crate::hii::ref_variant::RefTarget::Formset { formset_guid, form_id, .. }) => {
                (form_id, Some(formset_guid))
            }
            Some(crate::hii::ref_variant::RefTarget::Form { form_id })
            | Some(crate::hii::ref_variant::RefTarget::FormQuestion { form_id, .. }) => {
                (form_id, None)
            }
            _ => return,
        };
        if !out.contains(&(parent, child, cross)) {
            out.push((parent, child, cross));
        }
    });
    out
}
```

push_edges:

```rust
fn push_edges(pkg: &[u8], formset: &crate::types::Guid, out: &mut Vec<uefi_proto::FormEdge>) {
    let guid = guid_to_upper_string(formset);
    for (parent, child, cross) in package_edges(pkg) {
        let e = uefi_proto::FormEdge {
            formset_guid: guid.clone(),
            parent_form_id: u32::from(parent),
            form_id: u32::from(child),
            target_formset_guid: cross.as_ref().map(guid_to_upper_string).unwrap_or_default(),
        };
        if !out.contains(&e) {
            out.push(e);
        }
    }
}
```

Существующие тесты package_edges (дедуп/порядок, short/orphan, dangling) — обновить ожидания `None`-третьим элементом.

- [ ] **Step 5: TUI build_tree_rows — кросс-дети (forms.rs)**

В `build_tree_rows`: перед циклом по формсетам построить глобальную карту `(formset_guid, form_id) -> &FormInfo`; при наполнении `children` для ребра с непустым `target_formset_guid` целое ребро попадает в children родителя как обычно, но:
- резолв ребёнка при рендере — сначала по локальному `by_id`, затем по глобальной карте `(target_formset_guid, form_id)`;
- найденный кросс-ребёнок рендерится строкой `FormsRow::Form { key: FormKey { target: f.form_id-таргет, formset_guid: target_formset_guid, form_id_ifr, title } }` (существующая структура);
- нерезолвленный — `FormsRow::DanglingRef` с фактическим formset-суффиксом в тексте не нужен (текст DanglingRef уже несёт form_id; глубина = глубина родителя + 1 — паттерн существующих висячих REF).

Тест (forms.rs tests, по образцу существующих tree-тестов; развёрнут ТОЛЬКО родительский формсет — при `expanded_all()` форма-цель даёт корневую строку depth 1 в своём формсете и без кросс-резолва, ассерт `depth == 1` был бы ложноположительным; кросс-ребёнок корня depth 1 рендерится на depth 2):

```rust
    #[test]
    fn cross_edge_attaches_foreign_formset_form() {
        let setup = "7B59104A-366D-4C6F-8147-633AA5D8E0D4";
        let chipset = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";
        let forms = vec![
            fi("t1", setup, 10001, "Main", true),
            fi("t2", chipset, 1, "Chipset", true),
        ];
        let edges = vec![edge(setup, 10001, 1, chipset)];
        let ex: HashSet<String> = [setup.to_string(), format!("{setup}#10001")].into();
        let rows = build_tree_rows(&forms, &edges, &ex);
        // EC87D643-форма-1 — ребёнок 10001 в дереве Setup (depth 2),
        // корня в своём (свёрнутом) формсете нет — ровно одна строка
        let cross: Vec<_> = rows
            .iter()
            .filter(|r| matches!(r, FormsRow::Form { key, .. }
                if key.formset_guid == chipset && key.form_id_ifr == 1))
            .collect();
        assert_eq!(cross.len(), 1, "ровно одна строка кросс-ребёнка");
        assert!(matches!(
            cross[0],
            FormsRow::Form { depth: 2, path, .. } if path == "Main → Chipset"
        ));
    }

    #[test]
    fn cross_edge_unresolvable_target_is_dangling() {
        let setup = "7B59104A-366D-4C6F-8147-633AA5D8E0D4";
        let forms = vec![fi("t1", setup, 10001, "Main", true)];
        let edges = vec![edge(setup, 10001, 7, "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9")];
        let ex: HashSet<String> = [setup.to_string(), format!("{setup}#10001")].into();
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert!(matches!(
            rows.last(),
            Some(FormsRow::DanglingRef { form_id: 7, depth: 2 })
        ));
    }
```

(Хелперы — существующие фикстуры tests mod: `fi` и `edge`, расширенная параметром `target` в Step 1; отдельный `edge_cross` не нужен.)

- [ ] **Step 6: Полный прогон + clippy**

Run: `cargo test --all && cargo clippy --all --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A crates/uefi-engine/src/hii/ref_tree.rs crates/uefi-proto/proto/engine.proto crates/uefi-tui
git commit -m "feat(engine,tui): кросс-формсетные REF-рёбра FormEdge.target_formset_guid + дерево форм TUI (formset-unlock U1c)"
```

---

### Task 6 (U3a): schema varstores + emit_var_store_efi

**Files:**
- Modify: `crates/uefi-engine/src/hii/schema.rs:20-35` (VarStoreSchema + attributes), `:253-271` (QuestionAddList + varstores, parse-валидация)
- Modify: `crates/uefi-engine/src/hii/ifr_builder.rs` (+emit_var_store_efi)
- Test: оба файла tests mod

**Interfaces:**
- Consumes: `VarStoreSchema` (общий с FormSetSchema; `attributes` — serde default, обратная совместимость), r-efi `IFR_VARSTORE_EFI_OP`.
- Produces (для Task 7): `QuestionAddList { questions, refs, varstores: Vec<VarStoreSchema> }`; `IfrBuilder::emit_var_store_efi(&mut self, id: u16, guid: &Guid, size: u16, name: &str, attributes: u32)` — EDK2-раскладка `VarStoreId@2, Guid@4, Attributes@20, Size@24, Name@26` (UCS-2+NUL), зеркально читающей `values::varstore_map` (`values.rs:195-203`).

- [ ] **Step 1: Падающий builder-тест**

```rust
    #[test]
    fn emit_var_store_efi_layout() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        b.emit_var_store_efi(0x7F01, &g, 0x1670, "IntelSetup", 7);
        let buf = b.build();
        assert_eq!(buf[0], OP_VARSTORE_EFI);
        // total = 2 header + id(2) + guid(16) + attr(4) + size(2) + имя
        // UCS-2 (10 симв. × 2 + NUL 2) = 48
        assert_eq!(buf[1] & 0x7F, 48);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x7F01);
        assert_eq!(&buf[4..20], &g.to_bytes());
        assert_eq!(u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]), 7);
        assert_eq!(u16::from_le_bytes([buf[24], buf[25]]), 0x1670);
        assert_eq!(&buf[26..48], "I\0n\0t\0e\0l\0S\0e\0t\0u\0p\0\0\0".as_bytes());
    }
```

- [ ] **Step 2: FAIL → реализация**

```rust
    /// IfrVarStoreEfi (EDK2): VarStoreId@2, Guid@4, Attributes@20,
    /// Size@24, Name@26 UCS-2+NUL. Зеркаленно values::varstore_map.
    pub fn emit_var_store_efi(
        &mut self,
        id: u16,
        guid: &Guid,
        size: u16,
        name: &str,
        attributes: u32,
    ) {
        let name_bytes: Vec<u8> = name
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .chain([0, 0])
            .collect();
        self.write_header(OP_VARSTORE_EFI, false, 24 + name_bytes.len());
        self.buf.extend_from_slice(&id.to_le_bytes());
        self.buf.extend_from_slice(&guid_to_bytes(guid));
        self.buf.extend_from_slice(&attributes.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(&name_bytes);
    }
```

- [ ] **Step 3: Падающие schema-тесты**

```rust
    #[test]
    fn parse_question_add_with_varstores() {
        let s = parse_question_add_schema(
            r#"{ "varstores": [ { "id": 32513, "guid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
                "size": 5744, "name": "IntelSetup", "type": "efi" } ],
              "questions": [ { "form_id": 7, "prompt": "P", "help": "H",
                "question_id": 32800, "var_store_id": 32513, "var_offset": 1329,
                "size": 1, "options": [ { "text": "x4x4x4x4", "value": 0 } ] } ] }"#,
        ).unwrap();
        assert_eq!(s.varstores.len(), 1);
        assert_eq!(s.varstores[0].var_type, VarStoreType::Efi);
        assert_eq!(s.varstores[0].attributes, 7);
    }

    #[test]
    fn parse_question_add_rejects_duplicate_varstore_ids() {
        let e = parse_question_add_schema(
            r#"{ "varstores": [ { "id": 1, "guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
                "size": 16, "name": "A", "type": "efi" },
              { "id": 1, "guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
                "size": 16, "name": "B", "type": "buffer" } ],
              "questions": [] , "refs": [ { "form_id": 1, "prompt": "P", "help": "H", "question_id": 2 } ] }"#,
        ).unwrap_err();
        assert!(matches!(e, HiiError::InvalidSchema(_)));
    }
```

- [ ] **Step 4: Реализация schema**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarStoreSchema {
    pub id: u16,
    pub guid: String,
    pub size: u16,
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VarStoreType,
    /// Только для type=efi (IfrVarStoreEfi Attributes). Default 7 =
    /// NV|BS|RT (EDK2-паттерн setup-переменных; сверяется с живыми
    /// байтами 450x на гейте — спека formset-unlock §3 U3).
    #[serde(default = "default_varstore_attributes")]
    pub attributes: u32,
}

fn default_varstore_attributes() -> u32 {
    7
}
```

QuestionAddList:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionAddList {
    #[serde(default)]
    pub questions: Vec<QuestionAddSchema>,
    #[serde(default)]
    pub refs: Vec<QuestionAddRefSchema>,
    #[serde(default)]
    pub varstores: Vec<VarStoreSchema>,
}
```

parse_question_add_schema — после serde, до empty-проверки:

```rust
    let mut seen: Vec<u16> = Vec::new();
    for vs in &s.varstores {
        if seen.contains(&vs.id) {
            return Err(HiiError::InvalidSchema(format!(
                "duplicate varstore id {:#x}",
                vs.id
            )));
        }
        seen.push(vs.id);
        if crate::types::Guid::try_parse(&vs.guid).is_err() {
            return Err(HiiError::InvalidSchema(format!(
                "varstore guid '{}' is not a GUID",
                vs.guid
            )));
        }
    }
```

- [ ] **Step 5: Прогнать крейт + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings`
Expected: PASS (FormSetSchema-тесты с varstores без attributes — проходят, serde default).

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/schema.rs crates/uefi-engine/src/hii/ifr_builder.rs
git commit -m "feat(engine): QuestionAddList.varstores + emit_var_store_efi (formset-unlock U3a)"
```

---

### Task 7 (U3b): вставка varstore-деклараций в пролог формсета + $SPF-сдвиги

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (+locate_formset_prelude_end, +splice_varstore_ops)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (+splice_varstore_ops_into_resource, +add_varstores, +shift_resolving_records; check_question_add — валидация деклараций)
- Modify: `crates/uefi-engine/src/rpc/server.rs:1063-1090` (hii_question_add: применить varstores до вопросов)
- Test: `crates/uefi-engine/src/hii/ifr.rs` (tests), `crates/uefi-engine/src/hii/mod.rs` (tests), `crates/uefi-engine/src/rpc/server.rs` (tests)

**Interfaces:**
- Consumes: Task 6 (schema/builder), `spf::{scan_question_records, fixup_selected_record_ifr_offsets}` (прочитать сигнатуру `crates/uefi-engine/src/hii/spf.rs` перед использованием), `hii::spf_record_resolves` (`mod.rs:948`), `form_add::resource_forms_package`, `pe_resource::{plan_rsrc_blob_growth, try_grow_rsrc_tail, write_length_chain}` (паттерн `splice_question_ops_into_resource`, `mod.rs:910-937`).
- Produces:
  ```rust
  // ifr.rs
  pub(crate) fn locate_formset_prelude_end(package: &[u8]) -> Option<usize>; // offset первого IFR_FORM_OP
  pub fn splice_varstore_ops(package: &mut Vec<u8>, ops: &[u8]) -> Result<(usize, usize), HiiError>;
  // mod.rs
  pub fn add_varstores(image: &mut Image, item_id: &str, varstores: &[schema::VarStoreSchema])
      -> Result<Vec<u16>, HiiError>;
  ```
  Контракт add_varstores: вставляет декларации в пролог формсета целевой секции (bare/resource), сдвигает резолвящиеся $SPF-записи на delta, возвращает вставленные id. Повторный вызов с тем же id → `InvalidSchema` (коллизия по `varstore_map`).

- [ ] **Step 1: Падающий ifr-тест**

```rust
    #[test]
    fn splice_varstore_ops_inserts_before_first_form() {
        let mut pkg = two_form_package(); // формы 100 и 200 (существующая фикстура)
        let prelude_end = locate_formset_prelude_end(&pkg).unwrap();
        // IFR_VARSTORE_EFI_OP = 0x26 (r-efi; тестовый ifr.rs-модуль уже импортирует константу)
        let ops = opcode(IFR_VARSTORE_EFI_OP, false, &[vec![0u8; 24], b"I\0n\0t\0e\0l\0".to_vec()].concat());
        let (at, delta) = splice_varstore_ops(&mut pkg, &ops).unwrap();
        assert_eq!(at, prelude_end);
        assert_eq!(delta, ops.len());
        assert_eq!(&pkg[at..at + ops.len()], &ops[..]);
        assert_eq!(
            pkg[0] as usize | (pkg[1] as usize) << 8 | (pkg[2] as usize) << 16,
            pkg.len()
        );
        // формы остались после вставки, счётчик пакетной длины обновлён
        assert!(pkg[prelude_end + ops.len()..].starts_with(&[IFR_FORM_OP]));
    }
```

- [ ] **Step 2: FAIL → реализация (ifr.rs, рядом со splice_question_ops:311)**

```rust
/// Конец пролога формсета = offset первого IFR_FORM_OP (точка вставки
/// formset-уровневых деклараций: varstore/default-store). Спека
/// formset-unlock §3 U3.
pub(crate) fn locate_formset_prelude_end(package: &[u8]) -> Option<usize> {
    if !is_form_package(package) {
        return None;
    }
    let mut first: Option<usize> = None;
    walk_statements(package, |op, off, _len, _| {
        if op == IFR_FORM_OP && first.is_none() {
            first = Some(off);
        }
    });
    first
}

pub fn splice_varstore_ops(
    package: &mut Vec<u8>,
    ops: &[u8],
) -> Result<(usize, usize), HiiError> {
    if ops.is_empty() {
        return Err(HiiError::InvalidSchema("empty ops".into()));
    }
    let Some(insert_at) = locate_formset_prelude_end(package) else {
        return Err(HiiError::NotFound);
    };
    package.splice(insert_at..insert_at, ops.iter().copied());
    let plen = (package[0] as usize | (package[1] as usize) << 8 | (package[2] as usize) << 16)
        + ops.len();
    package[0] = (plen & 0xFF) as u8;
    package[1] = ((plen >> 8) & 0xFF) as u8;
    package[2] = ((plen >> 16) & 0xFF) as u8;
    Ok((insert_at, ops.len()))
}
```

(Примечание скетча было инвертировано: walk_statements живёт в values.rs (pub(crate)) и в ifr.rs НЕ импортирован — добавить `use super::values::walk_statements;`; IFR_FORM_OP уже импортирован в ifr.rs.)

- [ ] **Step 3: Падающий mod-тест (рядом с question-add тестами; фикстуры `question_add_flash_image` и `sd_file_*` — `mod.rs` tests:1814+)**

```rust
    #[test]
    fn add_varstores_declares_efi_varstore_and_shifts_spf_records() {
        let (flash, _, spf_before) = question_add_flash_image(); // ресурсный канал: PE32 @ :0x10, формы 10019/10020
        let mut img = crate::parser::image::parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let target = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10019";
        let module = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0";
        let pkg_len_before = pkg_of(&img, module).len();
        let rec_before = first_resolving_record(&img, module); // хелпер: spf_leaf_of + scan_question_records + spf_record_resolves
        let vs = schema::VarStoreSchema {
            id: 0x7F01,
            guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
            size: 0x1670,
            name: "IntelSetup".into(),
            var_type: schema::VarStoreType::Efi,
            attributes: 7,
        };
        let ids = add_varstores(&mut img, target, &[vs.clone()]).unwrap();
        assert_eq!(ids, vec![0x7F01]);
        let pkg = pkg_of(&img, module);
        let delta = 26 + 22; // 2 header + 24 фикс-поля + «IntelSetup» UCS-2+NUL (10×2+2) = 48
        assert_eq!(pkg.len(), pkg_len_before + delta);
        // varstore объявлен до первой формы и читается varstore_map
        let maps = values::varstore_map(&pkg);
        assert!(maps.iter().any(|m| m.id == 0x7F01 && m.size == 0x1670
            && m.name == "IntelSetup"));
        // $SPF-записи, резолвящиеся в пакет, сдвинулись на delta (0x3B/0x55);
        // чужая (0x66) не сдвинулась
        let rec_after = first_resolving_record(&img, module);
        assert_eq!(rec_after.ifr_offset, rec_before.ifr_offset + delta as u32);
        let recs = spf::scan_question_records(spf_leaf_of(&img));
        let rec1_after = recs.iter().find(|r| r.question_id == 0x55).unwrap();
        let rec1_before = spf::scan_question_records(&spf_before)
            .into_iter().find(|r| r.question_id == 0x55).unwrap();
        assert_eq!(rec1_after.ifr_offset, rec1_before.ifr_offset + delta as u32);
        let foreign_after = recs.iter().find(|r| r.question_id == 0x66).unwrap();
        let foreign_before = spf::scan_question_records(&spf_before)
            .into_iter().find(|r| r.question_id == 0x66).unwrap();
        assert_eq!(foreign_after.ifr_offset, foreign_before.ifr_offset);
        // коллизия: тот же id повторно — InvalidSchema
        assert!(matches!(
            add_varstores(&mut img, target, &[vs]),
            Err(HiiError::InvalidSchema(_))
        ));
    }
```

(Дефекты исходного скетча, вскрытые сверкой с кодом: фикстура `question_add_flash_image` — РЕСУРСНЫЙ канал (PE32 @ `:0x10:0`), а не RAW `:0x19:0`, и формы в ней 10019/10020, а не 100 — таргет и `pkg_of` исправлены на `:0x10:0#10019`/`:0x10:0`; хелпера `image_from_flash` нет — фактический хелпер `crate::parser::image::parse_image`; `delta = 26 + 10*2` = 46 забывал NUL-юнит UCS-2 — верно 26+22 = 48 (сверено с тестом emit_var_store_efi_layout Task 6: total 48, name slice 26..48). Хелпер `first_resolving_record` определяется рядом с тестом; `pkg_of`/`spf_leaf_of` уже есть в question_add_tests (mod.rs:3603/3592). Фикстура УЖЕ имеет $SPF-записи, резолвящиеся в целевой пакет (0x3B→q10019, 0x55→q10020) и чужую 0x66 — расширения `question_add_spf_body_for` не нужно.)

- [ ] **Step 4: FAIL → реализация (mod.rs)**

```rust
/// Вставка varstore-деклараций в пролог формсета + сдвиг $SPF-записей.
/// Спека formset-unlock §3 U3. Возвращает вставленные id.
#[tracing::instrument(level = "debug", skip(image, varstores), fields(item_id = %item_id), err)]
pub fn add_varstores(
    image: &mut Image,
    item_id: &str,
    varstores: &[schema::VarStoreSchema],
) -> Result<Vec<u16>, HiiError> {
    if varstores.is_empty() {
        return Ok(Vec::new());
    }
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target, _form_id, _qid) = parse_item_id(item_id)?;
    let path = resolve_writable_path(image, &target)?;
    let sd_path = ami_patcher::discover_pfs_payload_path(image)?;
    let mut ops = Vec::new();
    for vs in varstores {
        let g = crate::types::Guid::try_parse(&vs.guid).map_err(|_| {
            HiiError::InvalidSchema(format!("varstore guid '{}' is not a GUID", vs.guid))
        })?;
        let mut b = ifr_builder::IfrBuilder::new();
        match vs.var_type {
            schema::VarStoreType::Buffer => b.emit_var_store(vs.id, &g, vs.size, &vs.name),
            schema::VarStoreType::Efi => {
                b.emit_var_store_efi(vs.id, &g, vs.size, &vs.name, vs.attributes)
            }
        }
        ops.extend(b.build());
    }
    let node = crate::parser::target::find_item(&image.root, &target)
        .map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    for vs in varstores {
        for (start, len) in form_package_ranges(node) {
            if values::varstore_map(&node.body[start..start + len])
                .iter()
                .any(|m| m.id == vs.id)
            {
                return Err(HiiError::InvalidSchema(format!(
                    "varstore id {:#x} already exists in the formset",
                    vs.id
                )));
            }
        }
    }
    let pkg_before = form_package_ranges(node)
        .into_iter()
        .next()
        .map(|(s, l)| node.body[s..s + l].to_vec())
        .ok_or(HiiError::NotASetupItem)?;
    let (insert_at, delta) = {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.subtype == crate::ffs::EFI_SECTION_RAW {
            ifr::splice_varstore_ops(&mut node.body, &ops)?
        } else {
            splice_varstore_ops_into_resource(&mut node.body, &ops)?
        }
    };
    // fix round 1: селектор снимка (form_package_ranges) и селектор вставки
    // (resource_forms_package) — разные пути; на multi-package PE их
    // расхождение должно падать ГРОМКО, а не молча сдвигать чужие
    // $SPF-записи. Проверяем: пакет, снятый в pkg_before, вырос ровно на
    // delta и ops лежат на insert_at (префикс/суффикс на месте).
    {
        let node = crate::parser::target::find_item(&image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        let (start, len) = form_package_ranges(node)
            .into_iter()
            .next()
            .ok_or(HiiError::NotASetupItem)?;
        let spliced = node
            .body
            .get(start..start + len)
            .ok_or(HiiError::InvalidIfr)?;
        verify_spliced_snapshot(spliced, &pkg_before, &ops, insert_at, delta)?;
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    {
        let node = node_at_mut(&mut image.root, &sd_path);
        shift_resolving_records(&mut node.body, &pkg_before, insert_at, delta);
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);
    Ok(varstores.iter().map(|vs| vs.id).collect())
}

/// Пост-splice инвариант: spliced — это pkg_before с ops, вставленными
/// на insert_at (длина тела и u24-заголовок выросли ровно на delta,
/// вставка на insert_at, остальной префикс/суффикс на месте; байты 0..3 —
/// u24-длина — единственное допустимое отличие вне вставки).
/// Мismatch → InvalidIfr (unit-вариант без payload — конвенция «структура
/// пакета не такая, как ожидается», как в check_rsrc_question_splice;
/// числовые детали — в tracing::warn). Ревью Task 7 fix r1.
fn verify_spliced_snapshot(
    spliced: &[u8],
    pkg_before: &[u8],
    ops: &[u8],
    insert_at: usize,
    delta: usize,
) -> Result<(), HiiError> {
    let plen = |b: &[u8]| b[0] as usize | (b[1] as usize) << 8 | (b[2] as usize) << 16;
    let ok = spliced.len() == pkg_before.len() + delta
        && plen(spliced) == plen(pkg_before) + delta
        && spliced.get(3..insert_at) == pkg_before.get(3..insert_at)
        && spliced.get(insert_at..insert_at + delta) == Some(ops)
        && spliced.get(insert_at + delta..) == pkg_before.get(insert_at..);
    if !ok {
        tracing::warn!(
            spliced_len = spliced.len(),
            before_len = pkg_before.len(),
            insert_at,
            delta,
            "form package snapshot diverged from the splice target"
        );
        return Err(HiiError::InvalidIfr);
    }
    Ok(())
}

/// Сдвиг $SPF-записей, резолвящихся в целевой пакет, после вставки в
/// пролог (insert_at меньше offset'ов всех вопросов). Записи чужих
/// пакетов не трогаются (spf_record_resolves по pre-insert снимку).
fn shift_resolving_records(
    sd_body: &mut [u8],
    pkg_before: &[u8],
    insert_at: usize,
    delta: usize,
) {
    // fixup_selected_record_ifr_offsets принимает &[usize] — ОФФСЕТЫ записей
    // в sd_body (не SpfQuestionRecord): .map(|r| r.offset), паттерн
    // select_resolving_records (mod.rs:1065)
    let selected: Vec<usize> = spf::scan_question_records(sd_body)
        .into_iter()
        .filter(|r| {
            r.ifr_offset >= insert_at as u32
                && spf_record_resolves(pkg_before, r.question_id, r.ifr_offset)
        })
        .map(|r| r.offset)
        .collect();
    spf::fixup_selected_record_ifr_offsets(sd_body, &selected, insert_at as u32, delta as u32);
}
```

`splice_varstore_ops_into_resource` — зеркало `splice_question_ops_into_resource` (`mod.rs:1012-1039`), отличия: локация пакета через `form_add::resource_forms_package(pe)` (не по форме), вставка `ifr::splice_varstore_ops`. ВНИМАНИЕ: у `RsrcBlobGrowthPlan` НЕТ полей `blob_end`/`new_blob_len`/`new_total` (только `entry_off`/`blob_off`/`blob_len`/`grow`) — эти величины считаются pre-check'ом по образцу `check_rsrc_question_splice` (`mod.rs:973-1010`) через `package_list::parse_package_list`:

```rust
fn splice_varstore_ops_into_resource(pe: &mut Vec<u8>, ops: &[u8]) -> Result<(usize, usize), HiiError> {
    let (pkg_off, old_len) =
        form_add::resource_forms_package(pe).ok_or(HiiError::NotASetupItem)?;
    let pkg = pe.get(pkg_off..pkg_off + old_len).ok_or(HiiError::InvalidIfr)?;
    ifr::locate_formset_prelude_end(pkg).ok_or(HiiError::NotFound)?;
    let (_, blob_off, blob_len) = pe_resource::hii_entry_locations(pe)
        .first()
        .copied()
        .ok_or(HiiError::InvalidIfr)?;
    let blob_end = blob_off.checked_add(blob_len).ok_or(HiiError::InvalidIfr)?;
    let blob = pe.get(blob_off..blob_end).ok_or(HiiError::InvalidIfr)?;
    let list = package_list::parse_package_list(blob).ok_or(HiiError::InvalidIfr)?;
    let sum: usize = list.packages.iter().map(|p| p.bytes.len()).sum();
    let new_blob_len = blob_len
        .checked_add(ops.len())
        .ok_or(HiiError::PeGrowthUnsupported)?;
    let new_total = 20u64 + sum as u64 + ops.len() as u64 + 4;
    if new_total > u32::MAX as u64 || new_blob_len > u32::MAX as usize {
        return Err(HiiError::PeGrowthUnsupported);
    }
    let plan =
        pe_resource::plan_rsrc_blob_growth(pe, ops.len()).ok_or(HiiError::PeGrowthUnsupported)?;
    if plan.grow > 0 && !pe_resource::can_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    let mut pkg = pe[pkg_off..pkg_off + old_len].to_vec();
    let res = ifr::splice_varstore_ops(&mut pkg, ops)?;
    let delta = pkg.len() - old_len;
    if plan.grow > 0 && !pe_resource::try_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    pe.copy_within(pkg_off + old_len..blob_end, pkg_off + pkg.len());
    pe[pkg_off..pkg_off + pkg.len()].copy_from_slice(&pkg);
    pe_resource::write_length_chain(
        pe,
        plan.entry_off,
        plan.blob_off,
        new_blob_len as u32,
        new_total as u32,
    );
    Ok(res)
}
```

(Сверить поля `plan`/сигнатуру `write_length_chain` с `mod.rs:919-936` — при расхождении привести к ней; расхождение — docs: fix по правилу 11.)

- [ ] **Step 5: Валидация деклараций в check_question_add + wiring сервера**

`check_question_add` — новый параметр `varstores: &[schema::VarStoreSchema]`; после `resolve_question_target` (коллизия по всем form-пакетам ноды, `form_package_ranges(node)`):

```rust
    for vs in varstores {
        for (start, len) in form_package_ranges(node) {
            if values::varstore_map(&node.body[start..start + len])
                .iter()
                .any(|m| m.id == vs.id)
            {
                return Err(HiiError::InvalidSchema(format!(
                    "varstore id {:#x} already exists in the formset",
                    vs.id
                )));
            }
        }
    }
```

и в цикле схем (до `check_question_slots`) — var_store_id вопросов должен быть объявлен:

```rust
        let declared = varstores.iter().any(|v| v.id == schema.var_store_id)
            || values::varstore_map(&qt.pkg).iter().any(|m| m.id == schema.var_store_id);
        if schema.var_store_id != 0 && !declared {
            return Err(HiiError::InvalidSchema(format!(
                "var store id {:#x} is not declared (add it to schema varstores)",
                schema.var_store_id
            )));
        }
```

ВНИМАНИЕ (rule-11 фикс): `check_question_slots` (mod.rs:1282) УЖЕ валидирует var_store_id вопросов — «var store {} is not declared in the formset» + size-границы `var_offset+size > vs.size` по `values::varstore_map(pkg)`. Без знания о schema-декларациях вопрос по id из schema varstores ложно падает на этом чеке. Поэтому `check_question_slots` получает параметр `extra_varstores: &[schema::VarStoreSchema]`: lookup id в пакете ИЛИ в extra, size-границы — по найденному (у schema — `vs.size`). Вызов из `add_question` (mod.rs:1372) — `&[]` (после `add_varstores` декларация уже в пакете). Смена сигнатуры `check_question_add` также трогает внешние вызовы: `rpc/server.rs:1078` (wiring ниже) и `tests/real_image.rs:4129/4143/4587` (механически `&[]`).

`server.rs` `hii_question_add`: после parse — `crate::hii::check_question_add(img_slot, &r.target, &schema.questions, &schema.varstores)`; после check_ref_add, до цикла вопросов: `if !schema.varstores.is_empty() { crate::hii::add_varstores(img_slot, &r.target, &schema.varstores)?; }`. Тест сервера: положительный прогон по bare-фикстуре `question_add_bare_flash_image` (schema с varstores + вопрос по декларированному id → Ok; в пакете ноды появляется varstore 0x7F01) + отрицательный — вопрос по недекларированному id → InvalidArgument.

- [ ] **Step 6: Прогнать крейт + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings`
Expected: PASS; существующие question-add-тесты, передающие var_store_id без декларации, — обновить схемами с varstores (если фикстуры объявляли varstore в пакете — пройдут как есть).

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(engine): add_varstores — декларации в пролог формсета + сдвиг SPF-записей (formset-unlock U3b)"
```

---

### Task 8 (U4a): live-гейт на 450x — unlock формсета ИЛИ inject REF3 + разведка NVRAM-карты

**Files:**
- Test: `crates/uefi-engine/tests/real_image.rs` (+`real_amibcp_450x_formset_unlock`, рядом с `real_amibcp_450x_build_round_trip:44`)
- Modify (по итогам): `docs/superpowers/specs/2026-09-12-formset-unlock-design.md` §6 (карта offsets), §7-заметка о фактической ветке

**Interfaces:**
- Consumes: Task 3 (`hii::unlock` с кросс-фазой), Task 4 (`add_ref` + formset_guid), Task 5 (`ref_tree::collect_edges`), `amibcp_path()` (`real_image.rs:24-29`), `builder::build_image`.
- Produces: факт-фиксация ветки 450x (флип существующего suppress-гейта в корневом Setup / инжект) + живые offsets/attributes для §6.

- [ ] **Step 1: Живой разведочный прогон (движок на тестовом окружении)**

```bash
pkill -x engine || true
export UEFIPATCHER_DATA=/tmp/uefipatcher-test/data UEFIPATCHER_SOCK=/tmp/uefipatcher-test/uefipatcher.sock
rm -f "$UEFIPATCHER_SOCK"
./target/debug/engine > /tmp/uefipatcher-test/engine.log 2>&1 &
sleep 1
cd /tmp/uefipatcher-test/agent-cli
UEFIPATCHER_SOCK=$UEFIPATCHER_SOCK <workspace>/target/debug/uefi-cli session init --name u14-recon
UEFIPATCHER_SOCK=$UEFIPATCHER_SOCK <workspace>/target/debug/uefi-cli image open \
  '/var/home/dsevosty/git/IMPLEMENTATION/refs/amibcp/450x — копия.bin' --mode write --name 450x-u4
# кросс-гейты формсета (CLI-глаголы фактические: `hii form gates`, НЕ `hii gates list`;
# item-синтаксис вопросa — `#form:qid` через двоеточие, НЕ `#form@qid`):
UEFIPATCHER_SOCK=$UEFIPATCHER_SOCK <workspace>/target/debug/uefi-cli hii form gates \
  'abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0#1'
# offsets бифуркации (12 вопросов) — таблица для спеки §6:
for q in '118:0x242' '118:0x243' '118:0x244' '119:0x257' '119:0x258' '119:0x259' \
         '422:0x26b' '422:0x26c' '422:0x26d' '423:0x27f' '423:0x280' '423:0x281'; do
  form=${q%%:*}; qid=${q##*:}
  UEFIPATCHER_SOCK=$UEFIPATCHER_SOCK <workspace>/target/debug/uefi-cli hii question info \
    "abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0#${form}:${qid}"
done
pkill -x engine
```

Зафиксировать вывод (source_target/host/expression кросс-гейтов; var_offset каждого вопроса; varstore-строки).

- [ ] **Step 2: Написать real-тест (детерминированный для 450x: ветка a при наличии кросс-гейтов, иначе b)**

```rust
#[test]
#[ignore = "requires external real AMI image under refs/amibcp/ (gitignored)"]
fn real_amibcp_450x_formset_unlock() {
    let data = std::fs::read(amibcp_path()).unwrap();
    let mut image = parse_amibcp_image_write(&data); // хелпер по образцу real_amibcp_450x_build_round_trip:45-46
    let item = "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x10:0#1";
    let gates = uefi_engine::hii::gates_list(&image, item).unwrap();
    let has_cross = gates.iter().any(|g| !g.source_target.is_empty());
    if has_cross {
        let outcome = uefi_engine::hii::unlock(&mut image, item).unwrap();
        assert!(
            !outcome.applied.is_empty(),
            "кросс-гейт найден, но флип не применён: {gates:?}"
        );
        let rebuilt = uefi_engine::builder::build_image(&image).unwrap();
        assert_eq!(rebuilt.len(), data.len(), "unlock length-preserving на 450x");
    } else {
        // ветка b: инжект REF3 в видимую форму Chipset 10008 корневого Setup
        let schema_json = r#"{
            "refs": [ { "form_id": 1, "prompt": "Intel RC Setup",
                        "help": "Intel RC Setup Configuration",
                        "question_id": 32800,
                        "formset_guid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9" } ]
        }"#;
        let list = uefi_engine::hii::schema::parse_question_add_schema(schema_json).unwrap();
        uefi_engine::hii::add_ref(&mut image, "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10008",
            &list.refs[0]).unwrap();
        let edges = uefi_engine::hii::ref_tree::collect_edges(&image);
        assert!(
            edges.iter().any(|e| e.parent_form_id == 10008
                && e.form_id == 1
                && e.target_formset_guid.contains("EC87D643")),
            "кросс-ребро 10008 → IntelRCSetup#1 не появилось"
        );
        let rebuilt = uefi_engine::builder::build_image(&image).unwrap();
        assert!(rebuilt.len() >= data.len(), "growth = reloc-aware сдвиг .rsrc");
    }
}
```

Точность сигнатур: сверить `parse_amibcp_image_write`/`add_ref`/`hii::gates_list`-видимость (pub) с реальными (`real_amibcp_450x_build_round_trip` — как строит Image: `parse_image(&data, ImageMode::Write, "t", "s")`; `hii::add_ref(image, item_id, schema)` — сигнатура `mod.rs:1773`). `growth` — не фиксированная константа: заменить на `assert!(rebuilt.len() >= data.len())` + re-parse `parse_image(&rebuilt)` не пуст (валидация живого движка). Любое расхождение сигнатур — docs: fix (правило 11) ДО подгонки кода.

**Дефект ветки b, обнаружен живым прогоном 2026-09-12 (правило 11):** `add_ref` на 450x
падает `NotFound` в `plan_spf_append` — `spf::scan_string_controls` (эвристика HNX99TF
«u16 5 @p, 0 @p+4, 78 @p+0xA») находит 0 контрол-блоков в $SPF 450x (поколение HuaNian;
ближайшие сигнатуры — size-поле 504–508, не 5). При этом `add_ref` из плана использует
только `selected_records` (IFR-offset fixup): `ctrl_template` нужен исключительно
`apply_spf_question` (путь add_question; np3 на HNX99TF подтверждает — REF не добавляет
$SPF-записей, 393 = 389 + 4 вопроса). Фикс ДО теста: `SpfAppendPlan.ctrl_template:
Option<usize>`, `plan_spf_append` не требует контролы, требование — **upfront в
`add_question`/`check_question_add` ДО первой мутации** (add_question на образе без
контролов → честный NotFound; в `apply_spf_question` — только unreachable-expect) +
юнит-тест план-без-контролов.

**Ревью fix round 1 (правило 11, до правок кода):** релаксация затрагивает и `add_page` —
на control-less $SPF план теперь строится (раньше NotFound), `add_page` использует из
плана только `page_offset`/`page_slot` и **успешно регистрирует страницу** — это
задокументированное поведение (юнит-тест add_page на control-less фикстуре), не побочный
эффект.

- [ ] **Step 3: Прогнать real-гейт**

Run: `cargo test -p uefi-engine real_amibcp_450x -- --ignored --nocapture`
Expected: PASS; в выводе видно, какая ветка сработала (лог добавленного ассерта/принта).

- [ ] **Step 4: Зафиксировать факты в спеке**

`docs/superpowers/specs/2026-09-12-formset-unlock-design.md` §6 — заполнить offsets 12 вопросов из Step 1, attributes IntelSetup (если отличается от default 7 — docs: fix к Task 6 + при необходимости правка default), §3 U2 — пометка, какая ветка фактическая на 450x.

- [ ] **Step 5: Полный прогон real-серии + clippy**

Run: `cargo test -p uefi-engine -- --ignored && cargo clippy -p uefi-engine --all-targets -- -D warnings`
Expected: PASS (все 450x/HNX99TF real-тесты).

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs docs/superpowers/specs/2026-09-12-formset-unlock-design.md
git commit -m "test(engine): real_amibcp_450x_formset_unlock — живой гейт формсет-unlock + карта NVRAM в спеку (formset-unlock U4a)"
```

---

### Task 9 (U4b): финальные гейты дуги

**Files:**
- Modify: `TODO.md` (закрыть записи дуги U1–U4 + кросс-формсетные REF со ссылками на коммиты)
- Modify: `roadmap.md` (статус дуги — исполнена, до владельческого гейта)

**Interfaces:**
- Consumes: всё дуги.
- Produces: чистые гейты + актуализированные docs.

- [ ] **Step 1: Полная верификация**

```bash
cargo fmt --all -- --check
cargo clippy --all --all-targets -- -D warnings
cargo test --all
cargo test -p uefi-engine -- --ignored
```

Expected: всё зелёное (real-серия — при наличии образов в refs/).

- [ ] **Step 2: TUI-дым на живом движке (владельческий сценарий подготовлен)**

Задокументировать в §7 спеки сценарий владельческого прогона: открыть 450x write → Forms View → курсор на IntelRCSetup (`ABBCE13D…:0x10:0#1`) → `u`: ветка a — статус «unlock …: pkg+0x…: 00 00 -> ff ff» (applied flips; литерального «flips applied» в TUI нет — commands.rs:670), пункт появляется в корневом меню; ветка b (факт 450x по live-гейту U4 — кросс-гейтов нет) — `u` честно отвечает «no flippable gates (0 gates)», GOTO добавляет `:hii question add 899407D7…:0x10:0#10008 <refs-schema.json>` (refs-запись с formset_guid, add_ref — не unlock-путь) — форма Chipset 10008 получает REF3-пункт. Не исполнять от имени владельца — оставить гейт за ним.

- [ ] **Step 3: TODO/roadmap actualization + commit**

TODO.md: записи «кросс-формсетные REF не поддержаны» и «Дуга U1–U4» — дополнить «Закрыто дугой formset-unlock (коммиты …)» (по факту мерджа). roadmap.md — секция дуги: статус «исполнена, владельческий гейт pending».

```bash
git add TODO.md roadmap.md docs/superpowers/specs/2026-09-12-formset-unlock-design.md
git commit -m "docs(todo,roadmap,spec): дуга formset-unlock исполнена — статусы, сценарий владельческого гейта"
```

---

## Отложенное (не эта дуга)

- REF5-эмит/динамические GOTO — вне скоупа навсегда (спека §4).
- Formset-декларация при form-add (varstores в FormSchema) — при живом прецеденте.
- TUI-скролл Forms View, lost-update flush_image, completion-меню, `--ffs`-порядок — свои записи TODO.
- Gateway/WebUI-поверхность новых полей (source_target/target_formset_guid) — цикл WebUI rework.
- Мульти-донорская атомарность кросс-фазы unlock (двухпроходный plan-all-then-apply-all) — отложено в TODO.md (fix round 1, review finding): ранний донор может быть флипнут и помечен rebuild до того, как поздний провалит планирование (`GateExpressionUnsupported`) → частичная кросс-мутация сохранится при save после неудачного unlock. Окно: образы с несколькими донорами (у 450x донор один). Спека §3 U2a; код — Task 3 `apply_cross_formset_gates` (hii/mod.rs).
