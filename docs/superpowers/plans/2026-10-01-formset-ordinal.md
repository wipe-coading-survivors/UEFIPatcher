# Formset-Ordinal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Discoverability дискриминатора `TARGET#<n>` — FormInfo несёт formset-ordinal writable-канала, CLI/TUI/WebUI показывают; заодно чинится атрибуция форм на multi-formset-пакетах.

**Architecture:** Новый пер-формсетный проход форм-пакета (`parse_form_package_sets`) в ifr.rs; collect_forms нумерует ordinal'ы только в writable-канале add_form (RAW-тело; PE32 = первый resource-entry × первый PACKAGE_FORMS), вне канала — None; поле `optional uint32 formset_ordinal` через prost/protobuf-ts регены расходится по потребителям.

**Tech Stack:** Rust workspace (prost/tonic codegen via uefi-proto build.rs), SvelteKit webui (protobuf-ts, `npm run proto` — нужен protoc в PATH).

Спека: `docs/superpowers/specs/2026-10-01-formset-ordinal-design.md`.

## Global Constraints

- Никаких комментариев в коде (rustdoc `///`-контракты на pub-функциях — можно).
- Bounds — только через общий хелпер `package_bounds`; арифметика checked; усечённые чтения — None/NotFound, не молчаливый 0.
- Тест на обрезанный пакет — с точным ожиданием.
- После каждой задачи: `cargo test -p <crate>` + `cargo clippy -p <crate> --all-targets -- -D warnings`; финальный гейт: `cargo clippy --all --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cd webui && npm run check && npm run test`.
- Один коммит на задачу (шаг «Commit»).

---

### Task 1: ifr.rs — пер-формсетный проход `parse_form_package_sets`

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (после `parse_form_package`, ~:600)

**Interfaces:**
- Produces: `pub fn parse_form_package_sets(body: &[u8]) -> Option<Vec<(usize, FormSetInfo)>>` — на каждый IFR_FORM_SET_OP свой `(ordinal, FormSetInfo)`; ordinal = порядковый номер FORM_SET_OP в пакете (0..); формы до первого FORM_SET_OP пропускаются; malformed-опкод → None (паритет `parse_form_package`).

- [ ] **Step 1: Write the failing tests** (в существующий `mod tests` ifr.rs, рядом с тестами `parse_form_package`; хелпер `package(&ifr)` уже есть в тестах)

```rust
#[test]
fn parse_form_package_sets_splits_two_formsets() {
    let g1: [u8; 16] = [1; 16];
    let g2: [u8; 16] = [2; 16];
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&[7, 0, 0, 0, 0]); // title sid 7 + help sid 0 + flags 0
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 8, 0]); // form 1, title 8
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g2);
    ifr.extend_from_slice(&[9, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 2, 0, 10, 0]); // form 2, title 10
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    let sets = parse_form_package_sets(&package(&ifr)).unwrap();
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].0, 0);
    assert_eq!(sets[0].1.guid, Guid::from_bytes(g1));
    assert_eq!(sets[0].1.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(), vec![1]);
    assert_eq!(sets[1].0, 1);
    assert_eq!(sets[1].1.guid, Guid::from_bytes(g2));
    assert_eq!(sets[1].1.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn parse_form_package_sets_skips_forms_before_first_set() {
    let g: [u8; 16] = [3; 16];
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 5, 0, 1, 0]); // мусор до FORM_SET
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g);
    ifr.extend_from_slice(&[1, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 2, 0]);
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    let sets = parse_form_package_sets(&package(&ifr)).unwrap();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].1.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(), vec![1]);
}

#[test]
fn parse_form_package_sets_rejects_malformed_formset_header() {
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 10]); // length < 23
    let body = package(&ifr);
    assert!(parse_form_package_sets(&body).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p uefi-engine parse_form_package_sets`
Expected: FAIL — «cannot find function `parse_form_package_sets`»

- [ ] **Step 3: Implement** (после `parse_form_package`; зеркалит его проход)

```rust
/// Пер-формсетный проход form-пакета: на каждый IFR_FORM_SET_OP — свой
/// `(ordinal, FormSetInfo)` с guid/title/формами до следующего FORM_SET_OP.
/// Ordinal = индекс FORM_SET_OP в пакете — семантика дискриминатора `#n`
/// (locate_formset_insert_points). Формы до первого FORM_SET_OP не
/// атрибуцируются (пропуск). Malformed-опкод → None, как у
/// parse_form_package. НЕ используется мутациями — только списки/просмотр.
pub fn parse_form_package_sets(body: &[u8]) -> Option<Vec<(usize, FormSetInfo)>> {
    if !is_form_package(body) {
        return None;
    }
    let (start, end) = package_bounds(body);
    let mut sets: Vec<(usize, FormSetInfo)> = Vec::new();
    let mut cur: Option<FormSetInfo> = None;
    let mut scope_stack: Vec<u8> = Vec::new();
    let mut i = start;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        match op_code {
            IFR_FORM_SET_OP => {
                if length < 23 {
                    return None;
                }
                if let Some(fs) = cur.take() {
                    sets.push((sets.len(), fs));
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&body[i + 2..i + 18]);
                cur = Some(FormSetInfo {
                    guid: Guid::from_bytes(arr),
                    title: u16::from_le_bytes([body[i + 18], body[i + 19]]),
                    forms: Vec::new(),
                });
            }
            IFR_FORM_OP => {
                if length < 6 {
                    return None;
                }
                if let Some(fs) = cur.as_mut() {
                    fs.forms.push(RawForm {
                        form_id: u16::from_le_bytes([body[i + 2], body[i + 3]]),
                        title: u16::from_le_bytes([body[i + 4], body[i + 5]]),
                        suppressed: scope_stack.contains(&IFR_SUPPRESS_IF_OP),
                    });
                }
            }
            IFR_END_OP => {
                scope_stack.pop();
            }
            _ => {
                tracing::trace!(op_code, offset = i, "unknown ifr opcode skipped");
            }
        }
        if length_and_scope & 0x80 != 0 {
            scope_stack.push(op_code);
        }
        i += length;
    }
    if let Some(fs) = cur.take() {
        sets.push((sets.len(), fs));
    }
    Some(sets)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p uefi-engine parse_form_package_sets`
Expected: PASS (3 теста)

- [ ] **Step 5: Crate gates + commit**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/ifr.rs
git commit -m "feat(engine): parse_form_package_sets — пер-формсетный проход форм-пакета с ordinal (formset-ordinal §3.1)"
```

---

### Task 2: forms.rs — threading ordinal через collect_forms

**Files:**
- Modify: `crates/uefi-engine/src/hii/forms.rs` (walk_sections/drain_list_packages/collect_file_forms, :23-115)

**Interfaces:**
- Consumes: `parse_form_package_sets` (Task 1), `resource_forms_package`-семантика (первый resource-entry × первый PACKAGE_FORMS), существующие `hii_resource_ranges/hii_resource_blobs/parse_package_list/bare_form_packages`.
- Produces: `found: Vec<(String, FormSetInfo, Option<u32>)>` внутри walk-функций; FormInfo заполняет `formset_ordinal` ПОСЛЕ Task 3 (поле ещё не существует — в этой задаче Ordinal живёт в кортеже, FormInfo-литерал не трогаем). Тестируем через промежуточный доступ: тесты в этой задаче проверяют порядок/значения через новый pub-хелпер `collect_forms_with_ordinals(image) -> Vec<(FormInfo, Option<u32>)>`, который collect_forms после Task 3 свернёт в поле.

- [ ] **Step 1: Write the failing tests** (в `mod tests` forms.rs; хелперы `mk_node`, `FILE_GUID`, `form_pkg(n)`, `string_pkg`, `hii_list`, `synth_hii_pe`, `formset_pkg`-аналоги уже есть — смотри `collect_forms_sees_forms_inside_pe_resources` :410 и соседние)

```rust
#[test]
fn collect_forms_ordinal_bare_two_formsets() {
    // form_pkg строит single-formset RAW-пакет; для двух формсетов собери
    // тело вручную по образцу two_formset_flash_image (form_add.rs:646):
    // FORM_SET(g1) FORM(1) END FORM_SET(g2) FORM(2) END внутри package()
    let g1: [u8; 16] = [1; 16];
    let g2: [u8; 16] = [2; 16];
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&[1, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 1, 0]);
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g2);
    ifr.extend_from_slice(&[2, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 2, 0, 2, 0]);
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    let raw = mk_node(None, FfsType::Section, EFI_SECTION_RAW, package(&ifr), vec![]);
    let file = mk_node(Some(Guid::from_str(FILE_GUID).unwrap()), FfsType::File, 0x07, vec![], vec![raw]);
    let image = img_of(file);
    let forms = collect_forms_with_ordinals(&image);
    let f1 = forms.iter().find(|(f, _)| f.form_id_ifr == 1).unwrap();
    let f2 = forms.iter().find(|(f, _)| f.form_id_ifr == 2).unwrap();
    assert_eq!(f1.1, Some(0));
    assert_eq!(f2.1, Some(1));
    assert_eq!(f1.0.formset_guid, crate::guid_to_upper_string(&Guid::from_bytes(g1)));
    assert_eq!(f2.0.formset_guid, crate::guid_to_upper_string(&Guid::from_bytes(g2)));
}

#[test]
fn collect_forms_ordinal_pe32_only_first_resource_forms_pkg() {
    // первый resource-entry, в его списке ДВА PACKAGE_FORMS:
    // pkg1 (два формсета: формы 1,2) — writable → Some(0)/Some(1);
    // pkg2 (форма 3) — не writable → None
    let g1: [u8; 16] = [1; 16];
    let g3: [u8; 16] = [3; 16];
    let mut ifr1 = Vec::new();
    ifr1.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr1.extend_from_slice(&g1);
    ifr1.extend_from_slice(&[1, 0, 0, 0, 0]);
    ifr1.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 1, 0]);
    ifr1.extend_from_slice(&[IFR_END_OP, 2]);
    ifr1.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr1.extend_from_slice(&[2u8; 16]);
    ifr1.extend_from_slice(&[2, 0, 0, 0, 0]);
    ifr1.extend_from_slice(&[IFR_FORM_OP, 6, 2, 0, 2, 0]);
    ifr1.extend_from_slice(&[IFR_END_OP, 2]);
    let pkg1 = package(&ifr1);
    let mut ifr2 = Vec::new();
    ifr2.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr2.extend_from_slice(&g3);
    ifr2.extend_from_slice(&[3, 0, 0, 0, 0]);
    ifr2.extend_from_slice(&[IFR_FORM_OP, 6, 3, 0, 3, 0]);
    ifr2.extend_from_slice(&[IFR_END_OP, 2]);
    let pkg2 = package(&ifr2);

    let list = hii_list_two_forms(&pkg1, &pkg2, &string_pkg());
    let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
    let pe_sec = mk_node(None, FfsType::Section, EFI_SECTION_PE32, pe, vec![]);
    let file = mk_node(
        Some(Guid::from_str(FILE_GUID).unwrap()),
        FfsType::File,
        0x07,
        vec![],
        vec![pe_sec],
    );
    let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
    let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
    let image = Image {
        image_id: "img".into(),
        session_id: "s".into(),
        root,
        mode: ImageMode::Read,
    };
    let forms = collect_forms_with_ordinals(&image);
    let ord_of = |fid: u32| {
        forms
            .iter()
            .find(|(f, _)| f.form_id_ifr == fid)
            .unwrap()
            .1
    };
    assert_eq!(ord_of(1), Some(0));
    assert_eq!(ord_of(2), Some(1));
    assert_eq!(ord_of(3), None);
}

#[test]
fn collect_forms_ordinal_freeform_list_is_none() {
    let list = hii_list_raw(&form_pkg(1), &string_pkg());
    let image = mk_0x18_image(list, true);
    let forms = collect_forms_with_ordinals(&image);
    assert_eq!(forms.len(), 2);
    assert!(forms.iter().all(|(_, ord)| ord.is_none()));
}
```

(Имена `img_of`, `hii_list_two_forms` — локальные тестовые хелперы этой задачи; `img_of(file)` сворачивает file→volume→root→Image по образцу `collect_forms_sees_forms_inside_pe_resources` :410. `hii_list_two_forms(form1, form2, string)` — зеркало `hii_list` (:339) с вторым form-пакетом в теле и `total = 20 + form1.len() + form2.len() + string.len() + 4`. Случай «второй resource-entry → None» не покрывается фикстурой отдельно — покрывается конструкцией `writable_blob = ranges.first()` и тестом первого-entry; если при исполнении захочется явный кейс — собери второй entry по образцу `synth_hii_pe`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p uefi-engine collect_forms_ordinal`
Expected: FAIL — «cannot find function `collect_forms_with_ordinals`»

- [ ] **Step 3: Implement**

`walk_sections`/`drain_list_packages`/`collect_file_forms` (forms.rs:23-115):

```rust
// сигнатуры: found: &mut Vec<(String, FormSetInfo, Option<u32>)>

// RAW-ветка walk_sections (сейчас: parse_form_package):
} else if let Some(sets) = parse_form_package_sets(&child.body) {
    for (i, fs) in sets {
        found.push((target, fs, Some(i as u32)));
    }
}

// PE32-ветка walk_sections (сейчас: for blob in hii_resource_blobs):
let ranges = hii_resource_ranges(&child.body);
let writable_blob = ranges.first().copied();
for &(off, len) in &ranges {
    let Some(blob) = child.body.get(off..off + len) else { continue };
    let Some(list) = parse_package_list(blob) else { continue };
    drain_list_packages(&list, &target, titles, found, Some((off, len)) == writable_blob);
}

// bare PE32-пакеты (после цикла блобов) — как сейчас, но ordinal None:
for pkg in bare_form_packages(&child.body, &ranges) {
    if let Some(sets) = parse_form_package_sets(pkg) {
        for (_, fs) in sets {
            found.push((target.clone(), fs, None));
        }
    }
}

// drain_list_packages — новый параметр writable: bool:
fn drain_list_packages(
    list: &HiiPackageList<'_>,
    target: &str,
    titles: &mut HashMap<u16, String>,
    found: &mut Vec<(String, FormSetInfo, Option<u32>)>,
    writable: bool,
) {
    let mut ordinal: Option<u32> = writable.then_some(0);
    for pkg in &list.packages {
        match pkg.kind {
            PACKAGE_FORMS => {
                let base = ordinal.take(); // только первый PACKAGE_FORMS списка
                if let Some(sets) = parse_form_package_sets(pkg.bytes) {
                    for (i, fs) in sets {
                        let ord = base.map(|b| b + i as u32);
                        found.push((target.to_string(), fs, ord));
                    }
                }
            }
            PACKAGE_STRINGS => { /* как сейчас */ }
            _ => {}
        }
    }
}

// FREEFORM-ветка walk_sections: drain_list_packages(..., false)

// collect_file_forms: пушить FormInfo как сейчас (без нового поля — его
// ещё нет), но вернуть найденные кортежи наружу:

pub fn collect_forms_with_ordinals(image: &Image) -> Vec<(FormInfo, Option<u32>)> {
    // тот же обход, что collect_forms, но walk сохраняет (target, fs, ord)
    // и на выходе: (FormInfo { form_id: target, formset_guid, form_id_ifr,
    // title, visible }, ord)
}

pub fn collect_forms(image: &Image) -> Vec<FormInfo> {
    collect_forms_with_ordinals(image)
        .into_iter()
        .map(|(f, _)| f)
        .collect()
}
```

(Реализуй `collect_forms_with_ordinals` как основной обход, `collect_forms` — свёртка. Существующие тесты `collect_forms` должны остаться зелёными без правок ожиданий.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p uefi-engine collect_forms`
Expected: PASS (новые 3 + все существующие)

- [ ] **Step 5: Crate gates + commit**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/src/hii/forms.rs
git commit -m "feat(engine): collect_forms_with_ordinals — ordinal только writable-канала (RAW/первый resource-PACKAGE_FORMS), остальное None (formset-ordinal §2-§3.1)"
```

---

### Task 3: proto-поле `formset_ordinal` + compile-fix волна

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto:175-181` (message FormInfo)
- Modify: все `FormInfo {`-литералы без `..Default::default()`: `crates/uefi-engine/src/hii/forms.rs` (1), `crates/uefi-cli/src/commands/hii.rs` (2), `crates/uefi-cli/tests/mock_server.rs` (2), `crates/uefi-tui/tests/mock_server.rs` (3), `crates/uefi-tui/src/ui/forms.rs` (2), `crates/uefi-tui/src/app.rs` (3), `crates/uefi-tui/src/commands.rs` (12), `crates/uefi-tui/src/forms.rs` (5)

**Interfaces:**
- Consumes: Task 2 (`collect_forms_with_ordinals`).
- Produces: `FormInfo.formset_ordinal: Option<u32>` (prost); gateway НЕ трогается (generic proto3-JSON мост `/rpc/:method`).

- [ ] **Step 1: Add the proto field**

```proto
message FormInfo {
  string form_id = 1;
  string formset_guid = 2;
  uint32 form_id_ifr = 3;
  string title = 4;
  bool visible = 5;
  optional uint32 formset_ordinal = 6;
}
```

- [ ] **Step 2: Wire into collect_forms**

В `collect_forms_with_ordinals` (forms.rs): вместо свёртки-потери — заполнить поле:

```rust
pub fn collect_forms(image: &Image) -> Vec<FormInfo> {
    collect_forms_with_ordinals(image)
        .into_iter()
        .map(|(mut f, ord)| {
            f.formset_ordinal = ord;
            f
        })
        .collect()
}
```

- [ ] **Step 3: Compile-fix волна**

```bash
cargo build --all 2>&1 | grep -c 'error\[' || true
```
Каждый литерал `FormInfo { ... }` без `..Default::default()` дополняется `formset_ordinal: None` (кроме engine forms.rs — там значение уже течёт из Task 2). Тесты: тестовые литералы в mock-серверах при желании могут выставлять осмысленные ordinal для своих сценариев — не требуется, минимум `None`.

- [ ] **Step 4: Run all gates**

```bash
cargo test --all && cargo clippy --all --all-targets -- -D warnings
```
Expected: PASS (число тестов растёт только от новых ассертов, если добавляли)

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(proto): FormInfo.formset_ordinal (optional uint32) + подключение в collect_forms + литерал-волна (formset-ordinal §3.2)"
```

---

### Task 4: CLI — колонка `n` и справки

**Files:**
- Modify: `crates/uefi-cli/src/output.rs:170-195` (print_forms)
- Modify: `crates/uefi-cli/src/main.rs` — about у `HiiFormCmd::Add` (найти по «add a form») и `HiiVarstoreCmd::List` (:351)
- Test: `crates/uefi-cli/src/output.rs` (mod tests :878)

**Interfaces:**
- Consumes: `FormInfo.formset_ordinal: Option<u32>` (Task 3).

- [ ] **Step 1: Write the failing test** (output.rs tests)

```rust
#[test]
fn print_forms_tsv_has_ordinal_column() {
    // захват stdout недоступен — тестируем форматтер-хелпер (см. Step 3)
    let f = FormInfo {
        form_id: "G:0x19:0".into(),
        formset_guid: "S".into(),
        form_id_ifr: 1,
        title: "Main".into(),
        visible: true,
        formset_ordinal: Some(1),
    };
    assert_eq!(ordinal_cell(f.formset_ordinal), "#1");
    assert_eq!(ordinal_cell(None), "—");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p uefi-cli ordinal_cell`
Expected: FAIL — «cannot find function `ordinal_cell`»

- [ ] **Step 3: Implement**

```rust
/// Ячейка колонки `n` списка форм: `#<n>` для writable-канала, `—` —
/// формсет не адресуем `form add TARGET#n` (read-only список шире).
pub(crate) fn ordinal_cell(ord: Option<u32>) -> String {
    match ord {
        Some(n) => format!("#{n}"),
        None => "—".to_string(),
    }
}
```
TSV: хедер `form_id\tformset_guid\tform_id_ifr\ttitle\tvisible\tn`; строка + `\t{}` от `ordinal_cell(f.formset_ordinal)`.
Text: строка получает `\t{}` от `ordinal_cell после form_id_ifr (перед title? — НЕТ: после последней колонки, чтобы не ломать существующие парсеры: `{target}\t{guid}\t{ifr}\t{title}\tvisible={}\t{}`).
JSON: без правок (serde увезёт поле).
Справки main.rs:

```
about = "add a form to a formset; target = TARGET[#n] — ordinal n из колонки n `hii form list` (default #0)"
about = "list varstore declarations of a formset; item_id = TARGET[#n] — ordinal из колонки n `hii form list` (default #0); e.g. `hii varstore list 899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0`"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p uefi-cli`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-cli/src/output.rs crates/uefi-cli/src/main.rs
git commit -m "feat(cli): колонка n (#ordinal/—) в print_forms + справки TARGET[#n] у form add/varstore list (formset-ordinal §3.3)"
```

---

### Task 5: TUI — строка Ordinal в деталях формы

**Files:**
- Modify: `crates/uefi-tui/src/forms.rs` (form_panel, header-блок :302-315)
- Test: `crates/uefi-tui/src/forms.rs` (mod tests)

**Interfaces:**
- Consumes: `FormInfo.formset_ordinal` (Task 3); `FormsData.forms: Vec<FormInfo>` уже в App.

- [ ] **Step 1: Write the failing test** (по образцу `form_panel_bottom_gate_matches_selected_qid`, Task-fix-1 из tui-fixes)

```rust
#[test]
fn form_panel_header_shows_ordinal_line() {
    let mut app = crate::app::App::new();
    app.forms.forms = vec![uefi_proto::FormInfo {
        form_id: "t1".into(),
        formset_guid: "S".into(),
        form_id_ifr: 1,
        title: "Main".into(),
        visible: true,
        formset_ordinal: Some(1),
    }];
    app.forms.expanded = ["S".into()].into();
    let rows = app.forms_rows();
    // курсор на строку формы
    let idx = rows.iter().position(|r| matches!(r, FormsRow::Form { .. })).unwrap();
    let p = form_panel(&app.forms, &rows, idx);
    assert!(p.header.iter().any(|l| l.contains("Ordinal: #1")));
}

#[test]
fn form_panel_header_ordinal_none_renders_dash() {
    // тот же фикстур с formset_ordinal: None → "Ordinal: —"
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p uefi-tui form_panel_header`
Expected: FAIL — нет строки Ordinal

- [ ] **Step 3: Implement** (form_panel, после `format!("Target:  {}", key.target)`)

```rust
let ordinal = forms
    .forms
    .iter()
    .find(|f| f.form_id == key.target && f.formset_guid == key.formset_guid && f.form_id_ifr == key.form_id_ifr)
    .and_then(|f| f.formset_ordinal);
header.push(match ordinal {
    Some(n) => format!("Ordinal: #{n}"),
    None => "Ordinal: —".to_string(),
});
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p uefi-tui`
Expected: PASS (197+ lib)

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-tui/src/forms.rs
git commit -m "feat(tui): строка Ordinal: #n/— в деталях формы Forms-view (formset-ordinal §3.4)"
```

---

### Task 6: WebUI — реген типов + dim-суффикс `#n` в FormsTree

**Files:**
- Modify: `webui/src/lib/proto/engine.ts` (реген: `cd webui && npm run proto` — нужен protoc)
- Modify: `webui/src/lib/components/FormsTree.svelte:55-60`
- Test: `webui/src/lib/components/FormsTree.test.ts`

**Interfaces:**
- Consumes: proto-поле (Task 3) через generic-мост (proto3-JSON `formsetOrdinal`).

- [ ] **Step 1: Write the failing test** (FormsTree.test.ts, по образцу существующих row-тестов)

```typescript
it('показывает #n у формы с ordinal', () => {
    const form = { ...baseForm, formsetOrdinal: 1 };
    // render FormsTree с rows=[{ form, children: [], expanded: false, key: 'k' }]
    // assert textContent содержит '#1'
});
it('не показывает #n без ordinal', () => {
    const form = { ...baseForm }; // formsetOrdinal: undefined
    // assert НЕ содержит '#'
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd webui && npm run test -- FormsTree`
Expected: FAIL

- [ ] **Step 3: Implement**

```bash
cd webui && npm run proto
```
FormsTree.svelte, внутри `<button class="label">…` после `{row.form.formIdIfr} {row.form.title}`:

```svelte
{#if row.form.formsetOrdinal != null}<span class="dim">#{row.form.formsetOrdinal}</span>{/if}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd webui && npm run test && npm run check`
Expected: PASS, svelte-check 0/0

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/proto/engine.ts webui/src/lib/components/FormsTree.svelte webui/src/lib/components/FormsTree.test.ts
git commit -m "feat(webui): dim-суффикс #ordinal у строки формы в FormsTree + реген типов (formset-ordinal §3.5)"
```

---

### Task 7: Round-trip инвариант list→add + закрытие TODO + финальные гейты

**Files:**
- Test: `crates/uefi-engine/src/hii/form_add.rs` (mod tests, рядом с `add_form_bare_channel_targets_formset_ordinal` :646)
- Modify: `TODO.md` (запись «FormInfo: formset-порядковый номер не виден»), `roadmap.md` (запись мини-цикла, как у tui-live)

**Interfaces:**
- Consumes: хелперы тестов form_add.rs: `two_formset_flash_image()`, `parse_image`, `add_form_schema()`, `section_of`; `crate::hii::forms::collect_forms`.

- [ ] **Step 1: Write the failing-style invariant test** (фикстура уже даёт 2 формсета: fs1 forms [1,2], fs2 пустой до add)

```rust
#[test]
fn list_add_roundtrip_formset_ordinal() {
    let data = two_formset_flash_image();
    let target = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1";

    let img = parse_image(&data, ImageMode::Read, "i", "s").unwrap();
    let before = crate::hii::forms::collect_forms(&img);
    assert_eq!(
        before
            .iter()
            .filter(|f| f.form_id == target)
            .map(|f| f.formset_ordinal)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(0)]
    ); // fs1: forms 1,2; fs2 до add пуст — строк нет
    assert!(!before
        .iter()
        .any(|f| f.form_id == target && f.formset_ordinal == Some(1)));
    // guid-атрибутция fs1:
    let g1 = before
        .iter()
        .find(|f| f.form_id == target && f.form_id_ifr == 1)
        .unwrap()
        .formset_guid
        .clone();

    let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
    add_form(&mut img, &format!("{target}#1"), &add_form_schema()).unwrap();
    let after = crate::hii::forms::collect_forms(&img);
    let f42 = after.iter().find(|f| f.form_id == target && f.form_id_ifr == 42).unwrap();
    assert_eq!(f42.formset_ordinal, Some(1));
    assert_ne!(f42.formset_guid, g1); // форма 42 приписана fs2, не fs1
    assert_eq!(
        after.iter().filter(|f| f.form_id == target && f.formset_ordinal == Some(0))
            .map(|f| f.form_id_ifr).collect::<Vec<_>>(),
        vec![1, 2]
    );
}
```
(Тест исполняется после Task 2-3: поле `formset_ordinal` появляется в Task 3; если падает — это дефект инварианта, не двигайся дальше без понимания.)

- [ ] **Step 2: Run** `cargo test -p uefi-engine list_add_roundtrip` → PASS (после Task 2-3; если падает — это дефект инварианта, не двигайся дальше без понимания).

- [ ] **Step 3: Close TODO entry**

TODO.md, запись `* [ ] **FormInfo: formset-порядковый номер не виден в \`hii form list\`**` → `[x]` + строка: `Закрыто циклом formset-ordinal (2026-10-01): FormInfo.formset_ordinal (optional), колонка n в CLI, Ordinal-строка в TUI, #n в WebUI; attribution-дефект multi-formset тоже закрыт (спека 2026-10-01-formset-ordinal-design.md).`

- [ ] **Step 4: roadmap.md** — запись мини-цикла по образцу tui-live (после соответствующей строки).

- [ ] **Step 5: Финальные гейты**

```bash
cargo test --all && cargo clippy --all --all-targets -- -D warnings && cargo fmt --all -- --check && cd webui && npm run check && npm run test
```

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/form_add.rs TODO.md roadmap.md
git commit -m "test(engine): list→add round-trip инвариант formset-ordinal + закрытие TODO (formset-ordinal §4)"
```

---

## Self-Review (выполнен при написании)

- Спека §2 (каналы) → Task 2; §3.1 → Task 1-2; §3.2 → Task 3; §3.3 → Task 4; §3.4 → Task 5; §3.5 → Task 6; §4 инварианты 1→Task 2 (bare) + Task 7 (атрибуция), 2→Task 7, 3→существующие (сохранены Task 1 паритетом), 4→Task 2 (PE32), 5→паритет malformed-None Task 1.
- Placeholders: нет — все тесты приведены полным кодом; локальные хелперы задач (`hii_list_two_forms`, `img_of`) специфицированы сигнатурой + образцом для зеркалирования.
- Типы: `parse_form_package_sets -> Option<Vec<(usize, FormSetInfo)>>` (Task 1) согласован с Task 2; `ordinal_cell(Option<u32>) -> String` локален Task 4; `formset_ordinal: Option<u32>` сквозной (prost optional).
