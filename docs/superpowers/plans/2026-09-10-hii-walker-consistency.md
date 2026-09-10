# hii-walker-consistency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Привести легаси-читатели `hii/` к консистентному состоянию: единая механика обхода IFR, честные ошибки `set_item_visibility` вместо тихого Ok, единый контракт печатаемых смещений `pkg+`.

**Architecture:** Точечные правки в `crates/uefi-engine/src/hii/` (`ifr.rs`, `gates.rs`, `mod.rs`, `form_hijack.rs`) + `rpc/server.rs` (маппинг двух новых вариантов `HiiError`). Реализация — ветка `fix/hii-walker-consistency` от master, слияние через GitHub PR. Каждый шаг — TDD: тест (красный) → реализация → зелёный → коммит.

**Tech Stack:** Rust workspace (edition 2024), r-efi 7.0 (`r_efi::hii::*`), binrw не задействован ( walkers — ручные сканы по opcodes, стиль существующего кода).

**Spec:** `docs/superpowers/specs/2026-09-10-hii-walker-consistency-design.md` (включая уточнения коммита `6239934`). План аргументируется от спеки; исполнитель читает обе.

## Global Constraints

- Ветка: `fix/hii-walker-consistency` от master (создать в Step 1 Task 1; worktree — по superpowers:using-git-worktrees при исполнении).
- Комментарии в коде: только rustdoc-контракты `///` на pub/pub(crate)-функциях и ссылки `file:line` (AGENTS.md правило 8, смягчено `a99ac8f`). Rustdoc-контракты из спеки §3.7 — явные шаги задач (не отдельная финальная задача).
- TDD: (1) тест → (2) `cargo test` красный → (3) реализация → (4) зелёный → (5) коммит. После каждой задачи: `cargo test -p uefi-engine` и `cargo clippy -p uefi-engine -- -D warnings`.
- Rule-11: расхождение плана с реальностью — отдельный коммит `docs: fix Task N in hii-walker-consistency plan (…)` ДО реализации шага.
- IFR-константы — только `r_efi::hii::*` (в коде без хардкода; в тестах допустимы литералы байтов там, где проверяется сам байтовый формат).
- Real-image тесты — `#[ignore]`, прогоняются явно при наличии `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`. Если файла нет — остановиться и отчитаться, не пропускать молча.
- Прото-файл НЕ меняется: поле `GateInfo.scope_offset` (u32) остаётся, меняется смысл базы (pkg-относительное значение) — отметить в описании PR (release note, спека §6).

## Справочник фикстур (модуль tests в `hii/mod.rs`, актуальные смещения)

`vendor_forms_pkg()` = 4-байтовый заголовок пакета + FORM_SET(scoped, 23 байта, pkg[4..27]) + FORM 10002(27..33) + SUPPRESS_IF(33..35) + UINT64(35..45) + UINT64(45..55) + EQUAL(55..57) + REF 15 байт(57..72) + END(72..74) + END(74..76) + FORM 10029(76..82) + GRAYOUT_IF(82..84) + EQ_ID_VAL(84..90) + ONE_OF 0x3B(90..102) + END×4(102..110).

Отсюда pkg-относительные смещения гейтов (используются в захардкоженных ожиданиях Task 5):

- form-гейт (suppress на REF): `scope_offset = 33 = 0x21`, EqConst-флип `offset = 35 + 10 + 2 = 47 = 0x2F` → `"pkg+0x2f: 01 -> 02"`;
- question-гейт (grayout): `scope_offset = 82 = 0x52`, EqIdVal-флип `offset = 84 + 4 = 88 = 0x58` → `"pkg+0x58: 01 00 -> ff ff"`.

Если фактический вывод теста отличается от этих чисел — НЕ подгонять вслепую: вывести фактические байты пакета и сверить, что байт по смещению равен `from` (контракт §3.3), затем зафиксировать фактическое число отдельным rule-11-коммитом (дефект плана).

---

### Task 1: `scope_balance` → общий pub-хелпер `hii::ifr` (спека §3.5)

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (новая функция + 2 теста)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (~3634: удалить локальную копию, переключить вызовы)
- Modify: `crates/uefi-engine/tests/real_image.rs` (~4162: удалить замыкание, переключить вызовы)

**Interfaces:**
- Produces: `pub fn scope_balance(pkg: &[u8]) -> i32` в `hii::ifr` — доступен как `uefi_engine::hii::ifr::scope_balance` из интеграционных тестов (real_image.rs — внешний крейт, поэтому `pub`, не `pub(crate)` — уточнение спеки `6239934`).

- [ ] **Step 1: создать ветку**

```bash
git checkout master && git pull && git checkout -b fix/hii-walker-consistency
```

- [ ] **Step 2: написать падающие тесты** — в `mod tests` файла `ifr.rs` (фикстуры `form_set`/`form`/`end`/`package`/`opcode` уже есть):

```rust
#[test]
fn scope_balance_is_zero_for_balanced_package() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(form(1, 10));
    ifr.extend(end());
    ifr.extend(end());
    assert_eq!(scope_balance(&package(&ifr)), 0);
}

#[test]
fn scope_balance_counts_unclosed_scope() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(form(1, 10));
    ifr.extend(end());
    assert_eq!(scope_balance(&package(&ifr)), 1);
}
```

- [ ] **Step 3: `cargo test -p uefi-engine scope_balance` — красный** (функция не существует, ошибка компиляции E0425).

- [ ] **Step 4: реализация** — в `ifr.rs` после `parse_form_package`:

```rust
/// Баланс скоупов IFR form-пакета: +1 на scoped-опкод (бит 0x80 в байте
/// length/scope), −1 на END. 0 на корректном пакете. Инвариант
/// железо-доказан раундом 8 дуги setup-new-page; спека
/// hii-walker-consistency §3.5. `pkg` — form-пакет с 4-байтовым заголовком.
pub fn scope_balance(pkg: &[u8]) -> i32 {
    let mut bal = 0i32;
    let mut i = 4;
    while i + 2 <= pkg.len() {
        let len = (pkg[i + 1] & 0x7F) as usize;
        if len < 2 {
            break;
        }
        if pkg[i] == IFR_END_OP {
            bal -= 1;
        } else if pkg[i + 1] & 0x80 != 0 {
            bal += 1;
        }
        i += len;
    }
    bal
}
```

- [ ] **Step 5: `cargo test -p uefi-engine scope_balance` — зелёный.**

- [ ] **Step 6: удалить дубликат в `mod.rs`** — вложенный test-модуль (~строка 3634): удалить `fn scope_balance(pkg: &[u8]) -> i32 { … }` целиком (12 строк); два вызова (~3666–3667) заменить на `crate::hii::ifr::scope_balance(&pkg_after)` / `crate::hii::ifr::scope_balance(&pkg_before)`.

- [ ] **Step 7: удалить дубликат в `real_image.rs`** — (~4162) удалить замыкание `let scope_balance = |pkg: &[u8]| { … };`; вызовы (~4180–4181) заменить на `uefi_engine::hii::ifr::scope_balance(&final_pkg)` / `uefi_engine::hii::ifr::scope_balance(&asm.pkg_pre_refs)`.

- [ ] **Step 8: `cargo test -p uefi-engine` — зелёный** (компиляция real_image.rs входит в прогон, игнорируемые тесты компилируются); `cargo clippy -p uefi-engine -- -D warnings` — чисто.

- [ ] **Step 9: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/tests/real_image.rs
git commit -m "refactor(hii): scope_balance -> pub-хелпер hii::ifr, дубли в тестах mod.rs/real_image.rs удалены (TODO:2357)"
```

---

### Task 2: `find_suppress_if_scopes` — opcode-aligned обход + общий `package_bounds` (спека §3.1, §3.8)

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (переписать `find_suppress_if_scopes`; добавить `package_bounds`; переключить bounds-блоки трёх walker'ов)
- Modify: `crates/uefi-engine/src/hii/gates.rs` (удалить приватный `package_bounds`, импортировать из `ifr`)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (фикстура унаследованного теста `set_item_visibility_patches_form_inside_pe_resource` — FORM_SET-opener, см. Step 6)
- Test: `crates/uefi-engine/src/hii/ifr.rs` (mod tests)

**Interfaces:**
- Produces: `pub(crate) fn package_bounds(body: &[u8]) -> (usize, usize)` в `hii::ifr` — `(4, plen.min(body.len()))` для form-пакета, `(0, body.len())` для прочего тела.
- Produces: `find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope>` — контракт возврата не меняется (все скоупы пакета; content между заголовком SUPPRESS_IF и его END).

- [ ] **Step 1: написать падающие тесты** — в mod tests `ifr.rs`:

```rust
fn suppress_if() -> Vec<u8> {
    opcode(IFR_SUPPRESS_IF_OP, true, &[])
}

#[test]
fn find_suppress_if_scopes_closes_on_own_end_with_nested_form() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(suppress_if());
    ifr.extend(form(9, 19));
    ifr.extend(end());
    ifr.extend(end());
    ifr.extend(end());
    let pkg = package(&ifr);
    let scopes = find_suppress_if_scopes(&pkg);
    assert_eq!(scopes.len(), 1);
    // SUPPRESS @27, контент с 29; FORM(29..35), END FORM @35, END SUPPRESS @37
    assert_eq!(scopes[0].start, 29);
    assert_eq!(scopes[0].end, 37);
}

#[test]
fn find_suppress_if_scopes_survives_scoped_operand_end_terminators() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(suppress_if());
    let mut operand = vec![0x45u8, 0x8A];
    operand.extend_from_slice(&1u64.to_le_bytes());
    ifr.extend(operand);
    ifr.extend(end());
    ifr.extend(end());
    ifr.extend(end());
    let pkg = package(&ifr);
    let scopes = find_suppress_if_scopes(&pkg);
    assert_eq!(scopes.len(), 1);
    // SUPPRESS @27, контент с 29; scoped-операнд 10 байт (29..39) с END @39;
    // END SUPPRESS @41
    assert_eq!(scopes[0].start, 29);
    assert_eq!(scopes[0].end, 41);
}

#[test]
fn find_suppress_if_scopes_ignores_tail_beyond_declared_length() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(suppress_if());
    ifr.extend(end());
    ifr.extend(end());
    let mut pkg = package(&ifr);
    pkg.extend(vec![0x0A, 0x82, 0x29, 0x02]);
    let scopes = find_suppress_if_scopes(&pkg);
    assert_eq!(scopes.len(), 1, "хвост за пределами plen не читается");
}
```

- [ ] **Step 2: `cargo test -p uefi-engine find_suppress_if_scopes` — красный** (тесты 1–2 дают неверный `end`/лишний скоуп на текущем побайтовом обходе; тест 3 находит 2 скоупа).

- [ ] **Step 3: реализация** — в `ifr.rs` добавить `package_bounds` и переписать `find_suppress_if_scopes`:

```rust
pub(crate) fn package_bounds(body: &[u8]) -> (usize, usize) {
    if is_form_package(body) {
        let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
        (4, plen.min(body.len()))
    } else {
        (0, body.len())
    }
}
```

```rust
/// Обход: opcode-aligned (`i += length`), границы — `package_bounds`
/// (u24-длина пакета); глубина +1 на любой scoped-оп (бит 0x80), −1 на END —
/// AMI-quirk операнды с END-терминаторами и вложенные FORM/GRAYOUT не ломают
/// баланс. Возвращает контент всех SUPPRESS_IF пакета: [start, end) — между
/// заголовком SUPPRESS_IF и его END. Спека hii-walker-consistency §3.1.
pub fn find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope> {
    let (start, end) = package_bounds(body);
    let mut scopes = vec![];
    let mut i = start;
    while i + 2 <= end {
        let op = body[i];
        let length = (body[i + 1] & 0x7F) as usize;
        if length < 2 || i + length > end {
            break;
        }
        if op == IFR_SUPPRESS_IF_OP && body[i + 1] & 0x80 != 0 {
            let scope_start = i + 2;
            let mut depth = 1usize;
            let mut j = scope_start;
            while j + 2 <= end {
                let inner_len = (body[j + 1] & 0x7F) as usize;
                if inner_len < 2 || j + inner_len > end {
                    return scopes;
                }
                if body[j] == IFR_END_OP {
                    depth -= 1;
                    if depth == 0 {
                        scopes.push(SuppressScope {
                            start: scope_start,
                            end: j,
                        });
                        break;
                    }
                } else if body[j + 1] & 0x80 != 0 {
                    depth += 1;
                }
                j += inner_len;
            }
            i = j + 2;
        } else {
            i += length;
        }
    }
    scopes
}
```

- [ ] **Step 4: дедупликация bounds в `ifr.rs`** — в `find_form_suppress_scope` (~61–67) и `collect_form_string_ids` (~122–127) заменить блок

```rust
let (start, end) = if is_form_package(body) {
    let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
    (4, plen.min(body.len()))
} else {
    (0, body.len())
};
```

на `let (start, end) = package_bounds(body);`. В `parse_form_package` (~402–408) заменить

```rust
let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
let end = plen.min(body.len());
```

на `let (start, end) = package_bounds(body);` и `let mut i = 4;` на `let mut i = start;`.

- [ ] **Step 5: дедупликация `package_bounds` в `gates.rs`** — удалить приватную `fn package_bounds` (строки ~89–96); импорт наверху `use super::ifr::is_form_package;` заменить на `use super::ifr::package_bounds;` (других использований `is_form_package` в gates.rs нет).

- [ ] **Step 6: `cargo test -p uefi-engine` — зелёный** (включая существующие `unsuppress_makes_block_empty`, `unsuppress_rewrites_slice_in_place_preserving_length`, `find_form_suppress_scope_walks_package_body`, тесты gates.rs). Унаследованный `set_item_visibility_patches_form_inside_pe_resource` (mod.rs) падает на вербатим-реализации §3.1: его синтетический FORMS-пакет без FORM_SET-opener'а не распознаётся `is_form_package`/`package_bounds`, и opcode-aligned обход обрывается на 4-байтовом заголовке пакета — фикстуре добавляется FORM_SET-opener (UEFI: FORM_SET — первый опкод forms-пакета), ассерт смещения `blob[24..28]` заменяется на `blob[47..51]` (SUPPRESS сдвигается на +23); диф-каунт «ровно 5 байт» и остальные ассерты не меняются. `cargo clippy -p uefi-engine -- -D warnings` — чисто.

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/gates.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "fix(hii/ifr): find_suppress_if_scopes — opcode-aligned обход, глубина по любым scoped-опам, bounds по plen; package_bounds — общий pub(crate) хелпер (TODO:200, TODO:1199 механика)"
```

---

### Task 3: edge-тесты walkers + decode_expr; rustdoc `find_form_suppress_scope`/`unsuppress` (спека §3.6, §3.7)

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (rustdoc на двух функциях; edge-тесты)
- Modify: `crates/uefi-engine/src/hii/gates.rs` (2 теста decode_expr)

**Interfaces:** — (только тесты и документация; публичные сигнатуры не меняются)

- [ ] **Step 1: rustdoc-контракт `find_form_suppress_scope`** (спека §3.7) — над функцией:

```rust
/// Ищет только suppress-скоуп, оборачивающий саму форму (железо:
/// Platform-формсет 901, E7/E8). Гейты вокруг REF в родительской форме
/// НЕ ищет — это территория gates-слоя/unlock (E12); no-op-скрытие
/// setup-модуля отдаётся наверх как NoSuppressScope. Спека
/// hii-walker-consistency §3.7.
```

- [ ] **Step 2: rustdoc-контракт `unsuppress`** — над функцией:

```rust
/// Структурный rewrite: скоуп становится пустым (END сразу после заголовка
/// SUPPRESS_IF), выражение сдвигается на 2 байта вправо, за END.
/// Железо-валиден только на Platform-формсете (901, E7/E8); на setup-модуле
/// этот класс байтов («пустой скоуп + stray-опкоды») E11-фатален (виснет
/// AMITSE) — hardware-валидный путь для setup-модуля: unlock (флипы
/// литералов). Спека hii-walker-consistency §2/§3.7.
```

- [ ] **Step 3: edge-тесты** — в mod tests `ifr.rs`:

```rust
#[test]
fn find_form_suppress_scope_ignores_stray_end_on_empty_stack() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(end());
    ifr.extend(suppress_if());
    ifr.extend(form(5, 15));
    ifr.extend(end());
    ifr.extend(end());
    let scope = find_form_suppress_scope(&package(&ifr), 5);
    assert!(scope.is_some(), "stray END до скоупа — pop пустого стека, no-op");
}

#[test]
fn find_form_suppress_scope_returns_none_on_length_below_two() {
    let mut ifr = form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 7);
    ifr.push(0x01);
    ifr.push(0x01);
    assert!(find_form_suppress_scope(&package(&ifr), 5).is_none());
}

#[test]
fn parse_form_package_rejects_short_formset_opcode() {
    let mut ifr = vec![IFR_FORM_SET_OP, 0x04, 0xAA, 0xBB];
    ifr.extend(end());
    assert!(parse_form_package(&package(&ifr)).is_none());
}

#[test]
fn find_suppress_if_scopes_returns_only_outermost_scope() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(suppress_if());
    ifr.extend(suppress_if());
    ifr.extend(end());
    ifr.extend(end());
    ifr.extend(end());
    let scopes = find_suppress_if_scopes(&package(&ifr));
    assert_eq!(scopes.len(), 1, "вложенный SUPPRESS_IF покрывается внешним скоупом");
    // внешний SUPPRESS @27 (контент с 29), внутренний END @31, внешний END @33
    assert_eq!(scopes[0].start, 29);
    assert_eq!(scopes[0].end, 33);
}

#[test]
fn find_suppress_if_scopes_does_not_match_payload_bytes() {
    let g = Guid::from_str(FORMSET_GUID).unwrap();
    let mut ifr = form_set(&g, 7);
    ifr.extend(suppress_if());
    ifr.extend(end());
    ifr.extend(opcode(IFR_TEXT_OP, false, &[0x0A, 0x82, 0x00]));
    ifr.extend(end());
    let scopes = find_suppress_if_scopes(&package(&ifr));
    assert_eq!(scopes.len(), 1);
}
```

В тест-импорты `ifr.rs` добавить `IFR_TEXT_OP` (в существующий `use r_efi::hii::{…}`).

- [ ] **Step 4: тест-фиксация мусорного байта decode_expr** — в mod tests `gates.rs`:

```rust
#[test]
fn decode_tolerates_exactly_one_trailing_garbage_byte() {
    let mut region = concat(&[uint64(1), uint64(1), equal()]);
    region.push(0xAB);
    assert_eq!(
        decode_expr(&region),
        GateExpr::EqConst { a: 1, b: 1 },
        "ровно один непарный байт в хвосте терпится (следствие END-quirk)"
    );
}

#[test]
fn decode_rejects_two_trailing_garbage_bytes() {
    let mut region = concat(&[uint64(1), uint64(1), equal()]);
    region.push(0xAB);
    region.push(0xAB);
    assert_eq!(decode_expr(&region), GateExpr::Other);
}
```

- [ ] **Step 5: `cargo test -p uefi-engine` — зелёный** (новые тесты — guards, зелёные на текущем коде; это пиннинг поведения, не red-first). `cargo clippy -p uefi-engine -- -D warnings` — чисто.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/gates.rs
git commit -m "test(hii): edge-тесты walkers (stray END, length<2, короткий FormSet, nested-порядок, 0x0A в payload, tail) + пиннинг мусорного байта decode_expr; rustdoc-контракты find_form_suppress_scope/unsuppress (TODO:206, TODO:1213)"
```

---

### Task 4: `question_storage_width` — numeric size-flags @+13 (спека §3.4)

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs` (фикстура `numeric_op`, чтение @+13, guard `length >= 14`, rustdoc)
- Test: `crates/uefi-engine/src/hii/gates.rs` (mod tests)

**Interfaces:** — (сигнатура `question_storage_width(body: &[u8], question_id: u16) -> Option<u8>` не меняется; меняется читаемый байт)

- [ ] **Step 1: написать падающий тест** — в mod tests `gates.rs`:

```rust
#[test]
fn question_storage_width_reads_numeric_flags_at_13() {
    for (flags, expect) in [
        (r_efi::hii::IFR_NUMERIC_SIZE_1, 1u8),
        (r_efi::hii::IFR_NUMERIC_SIZE_2, 2),
        (r_efi::hii::IFR_NUMERIC_SIZE_4, 4),
        (r_efi::hii::IFR_NUMERIC_SIZE_8, 8),
    ] {
        let pkg = package(&master_switch_ifr(flags));
        assert_eq!(
            question_storage_width(&pkg, 0x009A),
            Some(expect),
            "flags={flags:#x}"
        );
    }
}
```

- [ ] **Step 2: `cargo test -p uefi-engine question_storage_width` — красный** (текущий код читает qflags@+12 — нулевой байт фикстуры → для SIZE_2/4/8 возвращает Some(1)).

- [ ] **Step 3: исправить фикстуру `numeric_op`** — r-efi-раскладка IfrNumeric: prompt@+2, help@+4, qid@+6, varstore@+8, varstoreinfo@+10, qflags@+12, size-flags@+13. Фикстура кладёт флаги не туда:

```rust
fn numeric_op(question_id: u16, flags: u8) -> Vec<u8> {
    let mut p = vec![0u8; 12];
    p[4..6].copy_from_slice(&question_id.to_le_bytes());
    p[11] = flags;
    opcode(IFR_NUMERIC_OP, true, &p)
}
```

(payload 12 байт → длина опа 14, флаги на op+13).

- [ ] **Step 4: реализация** — в `question_storage_width` заменить матч по NUMERIC и добавить rustdoc-контракт:

```rust
/// Ширина storage вопроса: CHECKBOX → 1 (qid@+6); NUMERIC — numeric
/// size-flags @+13 & IFR_NUMERIC_SIZE (r-efi IfrNumeric; тот же байт читает
/// values.rs), НЕ question-flags @+12. Спека hii-walker-consistency §3.4.
pub(crate) fn question_storage_width(body: &[u8], question_id: u16) -> Option<u8> {
```

внутри заменить

```rust
IFR_NUMERIC_OP if length >= 13 => Some(1u8 << (body[i + 12] & IFR_NUMERIC_SIZE)),
```

на

```rust
IFR_NUMERIC_OP if length >= 14 => Some(1u8 << (body[i + 13] & IFR_NUMERIC_SIZE)),
```

- [ ] **Step 5: `cargo test -p uefi-engine` — зелёный.** Существующие `plan_refuses_eq_id_val_when_master_storage_is_two_bytes`, `plan_allows_eq_id_val_when_master_is_one_byte_numeric`, `plan_gates_skip_unlocked_errors_on_wide_storage` остаются зелёными (фикстура и чтение переехали согласованно). `cargo clippy -p uefi-engine -- -D warnings` — чисто.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/gates.rs
git commit -m "fix(hii/gates): question_storage_width читает numeric size-flags @+13 (не qflags@+12), guard length>=14; фикстура numeric_op выровнена по r-efi IfrNumeric (TODO:1954)"
```

---

### Task 5: Контракт печатаемых смещений `pkg+` — единая точка истины (спека §3.3)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`pkg_off`/`flip_text`, `gate_info` без base, `gates_list`, `unlock`, тесты: hardcode bare-канала, новый мульти-пакетный тест)
- Modify: `crates/uefi-engine/src/hii/gates.rs` (тексты ошибок `plan_gates`/`plan_gates_skip_unlocked` через `super::pkg_off`)
- Modify: `crates/uefi-engine/src/hii/form_hijack.rs` (~308: `flip_text(f)`)

**Interfaces:**
- Produces: `pub(crate) fn pkg_off(off: usize) -> String` в `hii::mod` — `"pkg+{off:#x}"`.
- Produces: `pub(crate) fn flip_text(flip: &gates::PlannedFlip) -> String` — новая сигнатура без `base` (старая `flip_text(base: usize, flip: &gates::PlannedFlip)` удаляется).
- Меняется смысл поля `uefi_proto::GateInfo.scope_offset`: pkg-относительное значение (без `base +`). Прото-файл не трогается.

- [ ] **Step 1: написать падающий мульти-пакетный тест** — в mod tests `mod.rs` (после `unlock_resource_channel_changes_exactly_flip_bytes`). Это ключевой тест контракта: второй form-пакет в PE-ресурсе имеет `start > 0`, старый код печатал body-абсолют под меткой `pkg+`:

```rust
#[test]
fn gates_and_unlock_print_pkg_relative_offsets_in_second_package() {
    let list_guid = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
    let pkg1 = forms_pkg(vec![g_form(9), g_end(), g_end()].concat());
    let pkg2 = vendor_forms_pkg();
    let mut list = list_guid.to_bytes().to_vec();
    let total = 20 + pkg1.len() + pkg2.len() + 4;
    list.extend_from_slice(&(total as u32).to_le_bytes());
    list.extend_from_slice(&pkg1);
    list.extend_from_slice(&pkg2);
    list.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
    let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
    let mut image = vendor_image_with(0x10, pe);
    let item = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029:0x3B";

    let gates = gates_list(&image, item).unwrap();
    assert_eq!(gates.len(), 1);
    assert_eq!(gates[0].scope_offset, 0x52);
    assert_eq!(gates[0].flip, "pkg+0x58: 01 00 -> ff ff");

    let node = &image.root.children[0].children[0].children[0];
    let ranges = form_package_ranges(node);
    assert_eq!(ranges.len(), 2);
    let (start2, _) = ranges[1];
    assert_eq!(
        &node.body[start2 + 0x58..start2 + 0x5A],
        &[0x01, 0x00],
        "байт по напечатанному смещению == from"
    );

    let outcome = unlock(&mut image, item).unwrap();
    assert_eq!(outcome.applied, vec!["pkg+0x58: 01 00 -> ff ff".to_string()]);
    let node = &image.root.children[0].children[0].children[0];
    assert_eq!(&node.body[start2 + 0x58..start2 + 0x5A], &[0xFF, 0xFF]);
    let ranges_after = form_package_ranges(node);
    let (start1, len1) = ranges_after[0];
    assert_eq!(&node.body[start1..start1 + len1], &pkg1[..], "первый пакет нетронут");
}
```

- [ ] **Step 2: `cargo test -p uefi-engine print_pkg_relative` — красный** (`scope_offset == start2 + 0x52`, `flip == "pkg+{start2+0x58}…"` на старом коде).

- [ ] **Step 3: хелперы в `mod.rs`** — заменить `flip_text` (строки ~241–248) на:

```rust
/// Все печатаемые смещения — pkg+: от начала form-пакета, включая его
/// 4-байтовый заголовок (u24 length + kind). Единая точка истины для
/// GateInfo.flip/scope_offset, UnlockOutcome.applied и текстов ошибок
/// планировщика. Спека hii-walker-consistency §3.3.
pub(crate) fn pkg_off(off: usize) -> String {
    format!("pkg+{off:#x}")
}

pub(crate) fn flip_text(flip: &gates::PlannedFlip) -> String {
    format!("{}: {} -> {}", pkg_off(flip.offset), hex(&flip.from), hex(&flip.to))
}
```

- [ ] **Step 4: `gate_info`/`gates_list`** — сигнатура `fn gate_info(pkg: &[u8], gate: &gates::Gate) -> uefi_proto::GateInfo` (без `base`); внутри: `flip: flip.as_ref().map(|f| flip_text(f)).unwrap_or_default(),` и `scope_offset: gate.scope_offset as u32,`. В `gates_list` вызов → `out.push(gate_info(pkg, &gate));`.

- [ ] **Step 5: `unlock`** — в цикле по диапазонам: `infos.push(gate_info(pkg, gate));`; ветка `Ok(flips)`:

```rust
Ok(flips) => {
    for gate in &found {
        infos.push(gate_info(pkg, gate));
    }
    applied.extend(flips.iter().map(flip_text));
    absolute_flips.extend(flips.into_iter().map(|f| gates::PlannedFlip {
        offset: start + f.offset,
        from: f.from,
        to: f.to,
    }));
}
```

Удалить строку `applied = absolute_flips.iter().map(|f| flip_text(0, f)).collect();` (~372). `apply_flips` продолжает работать по body-абсолюту (`absolute_flips`) — внутреннее представление, не печатается.

- [ ] **Step 6: `gates.rs`** — в `plan_gates` и `plan_gates_skip_unlocked` заменить формат ошибки (обе функции, одинаково):

```rust
return Err(format!(
    "{} gate at {} wrapping {:?}: expression [{}] is not a hardware-validated flip class",
    gate.kind.as_str(),
    super::pkg_off(gate.scope_offset),
    gate.wraps,
    region
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
));
```

(`super::pkg_off` — модуль gates лежит внутри hii; взаимные ссылки модулей внутри крейта допустимы.)

- [ ] **Step 7: `form_hijack.rs`** (~308): `.map(|f| crate::hii::flip_text(0, f))` → `.map(crate::hii::flip_text)`.

- [ ] **Step 8: hardcode bare-канала** — в тесте `gates_list_bare_channel_reports_form_and_question_gates` заменить relation-ассерты (TODO:1240):

```rust
assert_eq!(gates[0].scope_offset, 0x21);
assert_eq!(gates[0].flip, "pkg+0x2f: 01 -> 02");
```

и

```rust
assert_eq!(qgates[0].scope_offset, 0x52);
assert_eq!(qgates[0].flip, "pkg+0x58: 01 00 -> ff ff");
```

(числа — из «Справочника фикстур» выше; для bare-канала base был 0, значения не меняются — меняется способ проверки).

- [ ] **Step 9: `cargo test -p uefi-engine` — зелёный**; `cargo clippy -p uefi-engine -- -D warnings` — чисто.

- [ ] **Step 10: Commit**

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/gates.rs crates/uefi-engine/src/hii/form_hijack.rs
git commit -m "fix(hii): единый контракт печатаемых смещений pkg+ (от начала пакета с 4-б заголовком) — pkg_off/flip_text, GateInfo.scope_offset и UnlockOutcome.applied pkg-относительные; hardcode офсетов в тестах; мульти-пакетный PE-тест (TODO:1994, TODO:1240)"
```

---

### Task 6: Честные ошибки `set_item_visibility` (спека §3.2)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (2 варианта `HiiError`, логика `set_item_visibility`, тесты)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_error_status` + тест)

**Interfaces:**
- Produces: `HiiError::NoSuppressScope` (RPC → `not_found`) и `HiiError::HidingUnsupported` (RPC → `invalid_argument`).
- Потребители CLI/gateway/TUI НЕ меняются: тексты приходят из RPC-статуса, маппинг tonic→HTTP уже есть (gateway 404/400). Тесты потребителей тихий Ok не фиксируют (аудит `6239934`) — обновлять нечего.

- [ ] **Step 1: написать падающие тесты** — в mod tests `mod.rs`:

```rust
#[test]
fn set_item_visibility_false_is_hiding_unsupported() {
    let mut image = vendor_image_with(0x19, vendor_forms_pkg());
    let err = set_item_visibility(&mut image, VENDOR_FORM_ITEM, false).unwrap_err();
    assert!(matches!(err, HiiError::HidingUnsupported));
}

#[test]
fn set_item_visibility_without_own_scope_is_no_suppress_scope() {
    let mut image = vendor_image_with(0x19, vendor_forms_pkg());
    // форма 10029 скрыта REF-гейтом родительской формы 10002;
    // собственного suppress-скоупа у неё нет
    let err = set_item_visibility(&mut image, VENDOR_FORM_ITEM, true).unwrap_err();
    assert!(matches!(err, HiiError::NoSuppressScope));
}
```

- [ ] **Step 2: `cargo test -p uefi-engine set_item_visibility` — красный** (не компилируется: вариантов нет; либо — если добавить заглушки раньше — старый код возвращает Ok).

- [ ] **Step 3: варианты `HiiError`** — добавить в enum (после `ValueOpUnsupported`):

```rust
#[error("form has no suppress-if scope of its own; REF-parent gates are the unlock op's domain")]
NoSuppressScope,
#[error("hiding (visible=false) is not implemented: only unsuppress exists")]
HidingUnsupported,
```

- [ ] **Step 4: логика `set_item_visibility`** — после проверки `node.node_type != FfsType::Section` (и до каналов):

```rust
if !visible {
    return Err(HiiError::HidingUnsupported);
}
```

Оберточные `if visible { … }` в обоих каналах убрать (условие всегда истинно после раннего возврата); в конце функции заменить

```rust
if changed {
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
}
```

на

```rust
if changed {
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
} else {
    return Err(HiiError::NoSuppressScope);
}
```

- [ ] **Step 5: `hii_error_status`** — в `rpc/server.rs`: добавить `crate::hii::HiiError::NoSuppressScope` в arm `not_found`; `crate::hii::HiiError::HidingUnsupported` — в arm `invalid_argument`. Тест рядом с `hii_error_status_maps_preconditions_not_found_and_bad_target`:

```rust
#[test]
fn hii_error_status_maps_no_suppress_scope_and_hiding() {
    let st = hii_error_status(crate::hii::HiiError::NoSuppressScope);
    assert_eq!(st.code(), tonic::Code::NotFound);
    assert!(st.message().contains("REF-parent"));
    let st = hii_error_status(crate::hii::HiiError::HidingUnsupported);
    assert_eq!(st.code(), tonic::Code::InvalidArgument);
    assert!(st.message().contains("not implemented"));
}
```

- [ ] **Step 6: `cargo test -p uefi-engine` — зелёный** (существующие тесты `set_item_visibility_*` не задевают новые пути: их цели имеют скоуп / Read-режим / не-секцию). `cargo clippy -p uefi-engine -- -D warnings` — чисто.

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(hii): set_item_visibility — честные ошибки вместо тихого Ok: NoSuppressScope (RPC not_found) и HidingUnsupported (RPC invalid_argument) (TODO:1199 семантика)"
```

---

### Task 7: Real-image гейты (спека §5)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новый тест NoSuppressScope; ассерты printed-offset ↔ байты в `real_image_hii_unlock_matches_e12`)

**Interfaces:**
- Consumes: `HiiError::NoSuppressScope` (Task 6), pkg-относительные `applied`/`GateInfo` (Task 5).

- [ ] **Step 1: новый тест** — исходная жалоба «no-op на setup-модуле» фиксируется как явная ошибка (рядом с `real_image_hii_unlock_matches_e12`):

```rust
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
#[test]
fn real_image_set_visibility_no_own_scope_is_explicit_error() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let err = uefi_engine::hii::set_item_visibility(
        &mut img,
        &format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029"),
        true,
    )
    .expect_err("форма 10029 скрыта REF-гейтом родителя, собственного скоупа нет");
    assert!(matches!(
        err,
        uefi_engine::hii::HiiError::NoSuppressScope
    ));
}
```

- [ ] **Step 2: printed-offset ↔ байты в E12-тесте** — в `real_image_hii_unlock_matches_e12`:

(a) сразу после ассертов `question_gates` (до unlock) — проверка полей GateInfo против `pkg_before`:

```rust
let parse_flip_str = |s: &str| -> (usize, Vec<u8>, Vec<u8>) {
    let (off, rest) = s.strip_prefix("pkg+").unwrap().split_once(':').unwrap();
    let off = usize::from_str_radix(off.trim_start_matches("0x"), 16).unwrap();
    let (from, to) = rest.split_once("->").unwrap();
    let bytes = |t: &str| -> Vec<u8> {
        t.trim()
            .split(' ')
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    };
    (off, bytes(from), bytes(to))
};
for gi in [&form_gates[0], &question_gates[0]] {
    // литералы опкодов — проверка самого байтового формата (IFR_SUPPRESS_IF_OP
    // = 0x0A, IFR_GRAY_OUT_IF_OP = 0x0D)
    assert_eq!(
        pkg_before[gi.scope_offset as usize],
        if gi.gate_kind == "suppress" { 0x0A } else { 0x0D },
        "scope_offset указывает на опкод гейта (контракт pkg+)"
    );
    let (off, from, _to) = parse_flip_str(&gi.flip);
    assert_eq!(
        &pkg_before[off..off + from.len()],
        &from[..],
        "GateInfo.flip {gi:?} указывает на from-байты"
    );
}
```

(b) захватить исходы unlock:

```rust
let form_out = uefi_engine::hii::unlock(&mut img, &form_item).expect("unlock page");
let q_out = uefi_engine::hii::unlock(&mut img, &question_item).expect("unlock question");
```

(c) после вычисления `pkg_after` (за ассертом `E12_FLIP_BYTES`) добавить:

```rust
for s in form_out.applied.iter().chain(&q_out.applied) {
    let (off, from, to) = parse_flip_str(s);
    assert_eq!(
        &pkg_before[off..off + from.len()],
        &from[..],
        "напечатанное смещение {s} указывает на from-байты (контракт pkg+)"
    );
    assert_eq!(&pkg_after[off..off + to.len()], &to[..]);
}
```

- [ ] **Step 3: компиляция** — `cargo test -p uefi-engine --no-run` (real-image тесты компилируются в обычном прогоне, но убедиться явно).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(real_image): #10029 -> NoSuppressScope (фиксация исходного no-op); ассерт «байт по напечатанному pkg+-смещению == from/to» в E12-тесте"
```

- [ ] **Step 5: полный ignore-прогон** (обязательный гейт цикла, спека §5/§7) — при наличии образа:

```bash
ls ../refs/fw/HNX99TF_200525_original_E5C88C6F.bin \
  && cargo test -p uefi-engine -- --ignored --test-threads=1
```

Ожидание: все зелёные (38 существующих + 1 новый = 39; актуальное количество — по выводу). Особо проконтролировать регрессии: `real_image_hii_form_visibility_round_trip`, `real_image_unhide_rebuild_keeps_layout` (Platform-канал unsuppress), `real_image_hii_unlock_matches_e12`. Если файла нет — остановиться и отчитаться владельцу, гейт не засчитывать.

---

### Task 8: TODO-актуализация (спека §8)

**Files:**
- Modify: `TODO.md` (8 пунктов → `[x]` с формулировками пересмотра; 1 новый кандидат)

- [ ] **Step 1: закрыть пункты** (отметить `[x]`, добавить строку «Закрыто:» в конец каждого пункта):

1. `TODO.md:200` «walker игнорирует заявленную длину пакета»: `Закрыто: аудит 2026-09-10 — bounds (4, plen.min(len)) уже были во всех walker'ах, кроме find_suppress_if_scopes (ходил до body.len()); переведён на общий package_bounds — цикл hii-walker-consistency.`
2. `TODO.md:206` «недостающие edge-тесты»: `Закрыто: добавлены (stray END, length<2, короткий IfrFormSet, nested-порядок, tail за plen, 0x0A в payload) — цикл hii-walker-consistency.`
3. `TODO.md:1199` «quirk 0x8a не перенесён в легаси-walker'ы»: `Закрыто: диагноз пересмотрен (аудит 2026-09-10) — маскирование & 0x7F есть во всех walker'ах с ff1a5ab; реальные причины no-op: REF-гейт родителя (семантика, территория gates/unlock) и баланс глубины find_suppress_if_scopes (механика, исправлено); no-op стал явной ошибкой NoSuppressScope — цикл hii-walker-consistency.`
4. `TODO.md:1213` «decoder допускает один мусорный байт»: `Закрыто: зафиксирован парой тестов (1 байт терпится / 2 байта — Other) — цикл hii-walker-consistency.`
5. `TODO.md:1240` «тест gates_list_bare_channel выводит офсет из тестируемого scope_offset»: `Закрыто: ожидания захардкожены (pkg+0x21/0x2f/0x52/0x58) + мульти-пакетный PE-тест с base>0 — цикл hii-walker-consistency.`
6. `TODO.md:1954` «question_storage_width читает qflags@+12»: `Закрыто: numeric size-flags @+13 с guard length>=14, фикстура numeric_op выровнена по r-efi IfrNumeric — цикл hii-walker-consistency.`
7. `TODO.md:1994` «напечатанные смещения флипов нестабильны ±4»: `Закрыто: единый контракт pkg+ (от начала form-пакета с 4-байтовым заголовком), хелпер pkg_off/flip_text; GateInfo.scope_offset и UnlockOutcome.applied — pkg-относительные; пиннинг юнит-тестом с base>0 и real-image ассертом «байт по напечатанному смещению == from» — цикл hii-walker-consistency.`
8. `TODO.md:2357` «поднять scope_balance в pub(crate) хелпер»: `Закрыто: pub hii::ifr::scope_balance (pub — real_image.rs внешний крейт), дубли удалены — цикл hii-walker-consistency.`

- [ ] **Step 2: новый кандидат** — в раздел «Находки ревью мини-цикла unlock-op» (после пункта про bounds-guard'ы, ~1228) добавить:

```markdown
* [ ] **hii: пересадка set_item_visibility на gates-слой** — REF-гейты
  (скрытие suppress'ом вокруг REF в родительской форме) остаются территорией
  unlock; расширение осознанно не вошло в цикл hii-walker-consistency
  (спека §4): меняет класс мутации существующего RPC. Контекст: отдельный
  мини-цикл при живом прецеденте.
```

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs(todo): закрыто 8 пунктов пачки 1 с пересмотренными диагнозами (0x8a, plen, width@+13, pkg+-смещения, scope_balance, edge-тесты, мусорный байт, hardcode офсетов); кандидат: пересадка set_item_visibility на gates-слой"
```

---

### Task 9: Финальная верификация и PR (спека §8)

**Files:** — (только проверки и git)

- [ ] **Step 1: полные гейты репозитория**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```

Ожидание: всё зелёное/чистое.

- [ ] **Step 2: повторный полный ignore-прогон** (если Task 7 Step 5 выполнялся позже чем последние правки engine — актуально всегда после Task 8):

```bash
cargo test -p uefi-engine -- --ignored --test-threads=1
```

- [ ] **Step 3: push и PR**

```bash
git push -u origin fix/hii-walker-consistency
gh pr create --base master --title "fix(hii): hii-walker-consistency — пачка 1 (walker-механика, честные ошибки, контракт pkg+)" --body <(cat <<'EOF'
## Что внутри
- find_suppress_if_scopes: opcode-aligned обход, глубина по любым scoped-опам, bounds по plen (package_bounds — общий pub(crate) хелпер)
- set_item_visibility: NoSuppressScope (RPC not_found) / HidingUnsupported (RPC invalid_argument) вместо тихого Ok
- Контракт печатаемых смещений pkg+: единая точка pkg_off/flip_text; GateInfo.scope_offset и UnlockOutcome.applied — pkg-относительные
- question_storage_width: numeric size-flags @+13 (r-efi IfrNumeric)
- scope_balance → pub hii::ifr; edge-тесты walkers + пиннинг мусорного байта decode_expr
- rustdoc-контракты (AGENTS.md правило 8, a99ac8f)

## Semantics change (release note)
GateInfo.flip / GateInfo.scope_offset / UnlockOutcome.applied теперь всегда pkg-относительные (от начала form-пакета, включая 4-байтовый заголовок); ранее applied печатал body-абсолют под меткой pkg+ (E15 ±4). hii_set_form_visibility: not_found=нет собственного suppress-скоупа, invalid_argument=visible=false.

## Спека/план
docs/superpowers/specs/2026-09-10-hii-walker-consistency-design.md
docs/superpowers/plans/2026-09-10-hii-walker-consistency.md

## Гейты
cargo test --all; clippy -D warnings; fmt --check; полный ignore-прогон real-image (39) зелёный.
EOF
)
```

- [ ] **Step 4:** отчитаться владельцу: ссылка на PR, результат ignore-прогонов, напоминание перепроверить живым прогоном консистентность pkg+-базы для его E15/E29-скриптов (спека §7).

---

## Покрытие спеки задачами (для self-review)

| Спека | Задача |
|---|---|
| §3.1 механика find_suppress_if_scopes | Task 2 |
| §3.2 честные ошибки + RPC | Task 6 |
| §3.3 контракт pkg+ (4 места + hijack) | Task 5 |
| §3.4 width @+13 | Task 4 |
| §3.5 scope_balance pub | Task 1 |
| §3.6 edge-тесты + мусорный байт | Task 3 (+Task 2 tail/payload) |
| §3.7 rustdoc-контракты | Task 2 (find_suppress_if_scopes, package_bounds), Task 3 (find_form_suppress_scope, unsuppress), Task 4 (question_storage_width), Task 5 (pkg_off) |
| §3.8 package_bounds общий | Task 2 |
| §5 unit-тесты | Task 2/3/4/5 |
| §5 real-image (901-регресс, #10029, printed-offset, ignore-прогон) | Task 7 (+существующие round-trip/unhide-тесты) |
| §6 потребители | Task 6 (только hii_error_status; CLI/gateway код не меняется — аудит 6239934) |
| §8 процесс (ветка/PR, TODO) | Task 1 Step 1, Task 8, Task 9 |
