# Formset-Ordinal-Followups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Формсет-скоуп varstore-семантики: `list_varstores TARGET#n` чтит ординал, write-валидация/вставка деклараций скоупится по целевому формсету (form add + question add + add_varstores); заодно — writable-детект collect_forms по индексу resource-записи с dedup, явные фикстуры «вторая resource-запись → None», extraction print_forms-форматтера.

**Architecture:** Подход A — scoped-хелперы поверх существующих проходов: `ifr::formset_spans` (единый источник границ FORM_SET, read/write), `values::varstore_map_formset` (карта деклараций спана), `form_hijack::locate_form_attribution` (формсет-владелец формы); `splice_varstore_ops`/`locate_formset_prelude_end` получают `formset_idx`; question-add splice-цепочка (preflight → check_rsrc → splice_question_ops_into_resource) проходит idx от атрибуции.

**Tech Stack:** Rust workspace (uefi-engine, uefi-cli). Proto/TUI/WebUI не трогаются.

Спека: `docs/superpowers/specs/2026-10-01-formset-ordinal-followups-design.md`.

## Global Constraints

- Никаких комментариев в коде (rustdoc `///`-контракты на pub-функциях — можно).
- Bounds — только через общий хелпер; арифметика checked; усечённые чтения — None/NotFound, не молчаливый 0; `unwrap_or_default` на каналах чтения запрещён.
- Тест на обрезанный/мусорный пакет — с точным ожиданием.
- После каждой задачи: `cargo test -p uefi-engine` (или `-p uefi-cli` для Task 6) + `cargo clippy -p <crate> -- -D warnings`. Финальный гейт (Task 7): `cargo test --all`, `cargo clippy --all --all-targets -- -D warnings`, `cargo fmt --all -- --check`.
- Один коммит на задачу (шаг «Commit»). Сообщения — из плана.
- Дефект плана (несуществующий API/тест не может пройти) — отдельный коммит `docs: fix Task N in formset-ordinal-followups plan (...)` ДО реализации, не молча.

---

### Task 1: `formset_spans` + `varstore_map_formset` + scoped `list_varstores`

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs` (`locate_formset_insert_points` :269 — рефактор на общий спан-проход)
- Modify: `crates/uefi-engine/src/hii/values.rs` (`varstore_map` :203 — extraction + новый `varstore_map_formset` рядом)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`list_varstores` :862; тест `list_varstores_returns_formset_declarations` :6634)

**Interfaces:**
- Produces: `pub(crate) fn ifr::formset_spans(body: &[u8]) -> Option<Vec<(usize, usize)>>` — (after_header, matching END offset) на каждый FORM_SET_OP; гейт `is_form_package` → None; malformed → None. Паритет семантики `locate_formset_insert_points`.
- Produces: `pub fn values::varstore_map_formset(pkg: &[u8], formset_idx: usize) -> Option<Vec<VarStoreMap>>` — разбор как `varstore_map`, но только ops внутри спана idx; None = out-of-range/malformed.
- Produces: `list_varstores(image, "TARGET[#n]")`: out-of-range `#n` → `HiiError::NotFound` (гейт `ifr::formset_at`), карта — из `varstore_map_formset`.

- [ ] **Step 1: Failing tests — ifr.rs `formset_spans`** (в `mod tests` ifr.rs, рядом с `insert_form_into_package_rejects_out_of_range_formset_idx`; хелпер `package(&ifr)` уже есть)

```rust
#[test]
fn formset_spans_returns_span_per_formset() {
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
    let spans = formset_spans(&package(&ifr)).unwrap();
    assert_eq!(spans, vec![(4 + 23, 4 + 23 + 6), (4 + 23 + 6 + 2 + 23, 4 + 23 + 6 + 2 + 23 + 6)]);
}

#[test]
fn formset_spans_rejects_unterminated_formset() {
    let g1: [u8; 16] = [1; 16];
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&[1, 0, 0, 0, 0]);
    assert!(formset_spans(&package(&ifr)).is_none());
}
```

(`formset_spans` виден тестам без `super::` — тесты в том же файле; константы IFR_* уже импортированы в тестах ifr.rs. Дефект-фикс: конец спана = offset парного END-op (как в существующем `locate_formset_insert_points` :298 — `insert_form_into_package` вставляет форму по `before_end` ПЕРЕД END-op), не past-END; исходное ожидание ошибочно включало длину END-op в offset.)

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p uefi-engine formset_spans`
Expected: FAIL «cannot find function `formset_spans`».

- [ ] **Step 3: Implement `formset_spans` + рефактор `locate_formset_insert_points`** (ifr.rs; заменить тело `locate_formset_insert_points` :269-312; новый pub(crate) — над ним)

```rust
pub(crate) fn formset_spans(body: &[u8]) -> Option<Vec<(usize, usize)>> {
    if !is_form_package(body) {
        return None;
    }
    let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
    let end = plen.min(body.len());
    let mut spans = Vec::new();
    let mut i = 4;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        if op_code == IFR_FORM_SET_OP {
            let mut depth = 1usize;
            let mut j = i + length;
            while j + 2 <= end {
                let inner_op = body[j];
                let inner_ls = body[j + 1];
                let inner_len = (inner_ls & 0x7F) as usize;
                if inner_len < 2 || j + inner_len > end {
                    return None;
                }
                if inner_op == IFR_END_OP {
                    depth -= 1;
                    if depth == 0 {
                        spans.push((i + length, j));
                        break;
                    }
                } else if inner_ls & 0x80 != 0 {
                    depth += 1;
                }
                j += inner_len;
            }
            if depth != 0 {
                return None;
            }
        }
        i += length;
    }
    Some(spans)
}

fn locate_formset_insert_points(body: &[u8], formset_idx: usize) -> Option<(usize, usize)> {
    formset_spans(body)?.get(formset_idx).copied()
}
```

- [ ] **Step 4: Run — verify pass (включая паритет-тесты locate)**

Run: `cargo test -p uefi-engine`
Expected: PASS (все, включая `insert_form_into_package_rejects_out_of_range_formset_idx` — рефактор сохраняет семантику).

- [ ] **Step 5: Failing tests — values.rs `varstore_map_formset`** (в `mod tests` values.rs рядом с `varstore_map_reads_buffer_and_efi_declarations` :610; дополнить use r_efi::hii недостающими `IFR_FORM_SET_OP, IFR_FORM_OP, IFR_END_OP`)

```rust
fn two_formsets_varstores_pkg() -> Vec<u8> {
    let g1: [u8; 16] = [0x11; 16];
    let g2: [u8; 16] = [0x22; 16];
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&[1, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[IFR_VARSTORE_OP, 28]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&1u16.to_le_bytes());
    ifr.extend_from_slice(&0x100u16.to_le_bytes());
    ifr.extend_from_slice(b"Setup\0");
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 1, 0]);
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    ifr.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g2);
    ifr.extend_from_slice(&[2, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[IFR_VARSTORE_OP, 29]);
    ifr.extend_from_slice(&g2);
    ifr.extend_from_slice(&2u16.to_le_bytes());
    ifr.extend_from_slice(&0x80u16.to_le_bytes());
    ifr.extend_from_slice(b"Setup2\0");
    ifr.extend_from_slice(&[IFR_FORM_OP, 6, 2, 0, 2, 0]);
    ifr.extend_from_slice(&[IFR_END_OP, 2]);
    let len = 4 + ifr.len() as u32;
    let mut pkg = vec![
        (len & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        ((len >> 16) & 0xFF) as u8,
        r_efi::hii::PACKAGE_FORMS,
    ];
    pkg.extend_from_slice(&ifr);
    pkg
}

#[test]
fn varstore_map_formset_scopes_declarations_to_span() {
    let pkg = two_formsets_varstores_pkg();
    let m0 = varstore_map_formset(&pkg, 0).unwrap();
    assert_eq!(m0.len(), 1);
    assert_eq!(m0[0].id, 1);
    assert_eq!(m0[0].size, 0x100);
    assert_eq!(m0[0].name, "Setup");
    let m1 = varstore_map_formset(&pkg, 1).unwrap();
    assert_eq!(m1.len(), 1);
    assert_eq!(m1[0].id, 2);
    assert_eq!(m1[0].name, "Setup2");
    assert!(varstore_map_formset(&pkg, 2).is_none());
    let whole = varstore_map(&pkg);
    assert_eq!(whole.len(), 2, "целая карта = объединение спанов");
}
```

- [ ] **Step 6: Run — verify fail**

Run: `cargo test -p uefi-engine varstore_map_formset`
Expected: FAIL «cannot find function».

- [ ] **Step 7: Implement extraction + `varstore_map_formset`** (values.rs; тело трёх arm'ов из нынешнего `varstore_map` :203-246 переносится в `parse_varstore_op`; наверху файла — `use super::ifr::formset_spans;`)

```rust
pub fn varstore_map(pkg: &[u8]) -> Vec<VarStoreMap> {
    let mut out = Vec::new();
    walk_statements(pkg, |op, off, len, _| {
        if let Some(m) = parse_varstore_op(pkg, op, off, len) {
            out.push(m);
        }
    });
    out
}

/// Карта деклараций одного формсета (formset-ordinal-followups §2):
/// те же op-разборы, что varstore_map, но только ops внутри спана
/// formset_idx. None = out-of-range/malformed.
pub fn varstore_map_formset(pkg: &[u8], formset_idx: usize) -> Option<Vec<VarStoreMap>> {
    let (span_start, span_end) = formset_spans(pkg)?.get(formset_idx).copied()?;
    let mut out = Vec::new();
    walk_statements(pkg, |op, off, len, _| {
        if (span_start..span_end).contains(&off)
            && let Some(m) = parse_varstore_op(pkg, op, off, len)
        {
            out.push(m);
        }
    });
    Some(out)
}

fn parse_varstore_op(pkg: &[u8], op: u8, off: usize, len: usize) -> Option<VarStoreMap> {
    match op {
        IFR_VARSTORE_OP if len >= 22 => {
            let mut guid_bytes = [0u8; 16];
            guid_bytes.copy_from_slice(&pkg[off + 2..off + 18]);
            Some(VarStoreMap {
                id: u16::from_le_bytes([pkg[off + 18], pkg[off + 19]]),
                guid: Some(Guid::from_bytes(guid_bytes)),
                size: u16::from_le_bytes([pkg[off + 20], pkg[off + 21]]),
                name: ascii_strz(&pkg[off + 22..off + len]),
                kind: VarStoreKind::Buffer,
                attributes: 0,
            })
        }
        IFR_VARSTORE_EFI_OP if len >= 26 => {
            let mut guid_bytes = [0u8; 16];
            guid_bytes.copy_from_slice(&pkg[off + 4..off + 20]);
            Some(VarStoreMap {
                id: u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]),
                guid: Some(Guid::from_bytes(guid_bytes)),
                size: u16::from_le_bytes([pkg[off + 24], pkg[off + 25]]),
                name: ucs2_strz(&pkg[off + 26..off + len]),
                kind: VarStoreKind::Efi,
                attributes: u32::from_le_bytes([
                    pkg[off + 20],
                    pkg[off + 21],
                    pkg[off + 22],
                    pkg[off + 23],
                ]),
            })
        }
        IFR_VARSTORE_NAME_VALUE_OP if len >= 6 => Some(VarStoreMap {
            id: u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]),
            guid: None,
            size: 0,
            name: ucs2_strz(&pkg[off + 4..off + len]),
            kind: VarStoreKind::NameValue,
            attributes: 0,
        }),
        _ => None,
    }
}
```

- [ ] **Step 8: Run — verify pass**

Run: `cargo test -p uefi-engine`
Expected: PASS (существующие `varstore_map_*`-тесты не меняются — extraction сохраняет поведение).

- [ ] **Step 9: Failing test — mod.rs scoped `list_varstores`** (новый тест рядом с `list_varstores_returns_formset_declarations` :6634, в том же тестовом модуле; фикстура-хелпер — рядом; хелперы `section_bytes`/`ffs_file_bytes`/`flash_with_files`/`parse_image` уже в этом дереве тестов)

```rust
fn two_formset_varstores_pkg() -> Vec<u8> {
    let mut b = crate::hii::ifr_builder::IfrBuilder::new();
    let g1 = Guid::from_str("A1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
    b.emit_form_set(&g1, 1, 1, &[]);
    b.emit_var_store(1, &g1, 0x100, "Setup");
    b.emit_form(1, 1);
    b.emit_end();
    b.emit_end();
    let g2 = Guid::from_str("B1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
    b.emit_form_set(&g2, 2, 2, &[]);
    b.emit_var_store(2, &g2, 0x80, "Setup2");
    b.emit_form(2, 2);
    b.emit_end();
    b.emit_end();
    let ifr = b.build();
    let len = 4 + ifr.len() as u32;
    let mut pkg = vec![
        (len & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        ((len >> 16) & 0xFF) as u8,
        r_efi::hii::PACKAGE_FORMS,
    ];
    pkg.extend_from_slice(&ifr);
    pkg
}

#[test]
fn list_varstores_scopes_map_by_formset_ordinal() {
    let content = section_bytes(
        crate::ffs::EFI_SECTION_RAW,
        &two_formset_varstores_pkg(),
    );
    let flash = flash_with_files(vec![ffs_file_bytes(
        &Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap(),
        &content,
    )]);
    let img = parse_image(&flash, ImageMode::Read, "i", "s").unwrap();
    let target = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0";
    let ids = |s: Vec<uefi_proto::VarStoreInfo>| {
        s.into_iter().map(|v| (v.id, v.name)).collect::<Vec<_>>()
    };
    assert_eq!(
        ids(list_varstores(&img, target).unwrap()),
        vec![(1, "Setup".into())]
    );
    assert_eq!(
        ids(list_varstores(&img, &format!("{target}#1")).unwrap()),
        vec![(2, "Setup2".into())]
    );
    let err = list_varstores(&img, &format!("{target}#2")).unwrap_err();
    assert!(matches!(err, HiiError::NotFound), "out-of-range #n, got {err:?}");
}

#[test]
fn list_varstores_truncated_package_is_not_found() {
    let g1: [u8; 16] = [0x11; 16];
    let mut ifr = Vec::new();
    ifr.extend_from_slice(&[r_efi::hii::IFR_FORM_SET_OP, 23 | 0x80]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&[1, 0, 0, 0, 0]);
    ifr.extend_from_slice(&[r_efi::hii::IFR_VARSTORE_OP, 28]);
    ifr.extend_from_slice(&g1);
    ifr.extend_from_slice(&1u16.to_le_bytes());
    ifr.extend_from_slice(&0x100u16.to_le_bytes());
    ifr.extend_from_slice(b"Setup\0");
    let len = 4 + ifr.len() as u32;
    let mut pkg = vec![
        (len & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        ((len >> 16) & 0xFF) as u8,
        r_efi::hii::PACKAGE_FORMS,
    ];
    pkg.extend_from_slice(&ifr);
    let content = section_bytes(crate::ffs::EFI_SECTION_RAW, &pkg);
    let flash = flash_with_files(vec![ffs_file_bytes(
        &Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap(),
        &content,
    )]);
    let img = parse_image(&flash, ImageMode::Read, "i", "s").unwrap();
    let err = list_varstores(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0").unwrap_err();
    assert!(
        matches!(err, HiiError::NotFound),
        "FORM_SET без парного END: formset_spans None → NotFound, не молчаливая карта; got {err:?}"
    );
}
```

Имена констант — напрямую `r_efi::hii::IFR_*` (мод-тесты уже используют `r_efi::hii::*`); тест кладётся в модуль, где видны `section_bytes`/`ffs_file_bytes`/`flash_with_files`/`parse_image` (тот же, где `list_varstores_returns_formset_declarations` — фикстуры question_add уже импортированы в его scope). Если какой-то хелпер недоступен — дефект плана: docs-коммит (Global Constraints).

- [ ] **Step 10: Run — verify fail**

Run: `cargo test -p uefi-engine list_varstores_scopes list_varstores_truncated`
Expected: `..._scopes_...` FAIL — `#1` возвращает обе декларации (карта не скоупится); `..._truncated_...` FAIL — возвращает `Ok([Setup])` (walk молча обрывает на конце пакета).

- [ ] **Step 11: Implement scoped `list_varstores`** (mod.rs :858-901; заменить rustdoc и тело)

```rust
/// Карта varstore-деклараций формсета (спека varstore-contract §3;
/// formset-ordinal-followups §2.1): item_id — грамматика `hii form add`
/// (`<target>` или `<target>#<n>`); `#n` выбирает n-й FORM_SET пакета,
/// читаются только декларации его спана; out-of-range `#n` → NotFound.
/// Read-only, образ в любом режиме. guid="" — name-value декларация.
pub fn list_varstores(
    image: &Image,
    item_id: &str,
) -> Result<Vec<uefi_proto::VarStoreInfo>, HiiError> {
    let (target_str, formset_idx) = match item_id.rsplit_once('#') {
        Some((t, n)) => match n.parse::<u16>() {
            Ok(idx) => (t, idx as usize),
            Err(_) => return Err(HiiError::NotFound),
        },
        None => (item_id, 0),
    };
    let target = crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    let node =
        crate::parser::target::find_item(&image.root, &target).map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let pkg: &[u8] = if node.subtype == EFI_SECTION_RAW && ifr::is_form_package(&node.body) {
        &node.body
    } else if node.subtype == EFI_SECTION_PE32 {
        let (off, len) =
            form_add::resource_forms_package(&node.body).ok_or(HiiError::NotASetupItem)?;
        node.body.get(off..off + len).ok_or(HiiError::InvalidIfr)?
    } else {
        return Err(HiiError::NotASetupItem);
    };
    if !ifr::formset_at(pkg, formset_idx) {
        return Err(HiiError::NotFound);
    }
    let map = values::varstore_map_formset(pkg, formset_idx).ok_or(HiiError::InvalidIfr)?;
    Ok(map
        .into_iter()
        .map(|m| uefi_proto::VarStoreInfo {
            id: u32::from(m.id),
            guid: m
                .guid
                .as_ref()
                .map(crate::guid_to_upper_string)
                .unwrap_or_default(),
            size: u32::from(m.size),
            name: m.name,
        })
        .collect())
}
```

И обновить пиннящее сообщение в `list_varstores_returns_formset_declarations` (:6649-6653):

```rust
                assert_eq!(
                    list_varstores(&img, &format!("{ITEM_FORMSET}#0")).unwrap(),
                    stores,
                    "фикстура одно-формсетная: scoped #0 == вся карта пакета (formset-ordinal-followups §2.1)"
                );
```

- [ ] **Step 12: Run — verify pass + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 13: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/values.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): list_varstores чтит формсет-ординал TARGET#n — formset_spans + varstore_map_formset (formset-ordinal-followups §2-2.1)"
```

---

### Task 2: `validate_form_varstores` скоупится по формсету n

**Files:**
- Modify: `crates/uefi-engine/src/hii/form_add.rs` (`validate_form_varstores` :209, вызов :93)
- Test: `crates/uefi-engine/src/hii/form_add.rs` (`mod tests`, рядом с `list_add_roundtrip_formset_ordinal` :680)

**Interfaces:**
- Consumes: `values::varstore_map_formset` (Task 1).
- Produces: `fn validate_form_varstores(image, target, bare_channel, formset_idx: usize, schema) -> Result<(), HiiError>` (private); коллизии/ссылки проверяются в целевом формсете, None-карта → `HiiError::InvalidIfr` (недостижимо после гейта `formset_at` в `add_form` — отказ громкий, не дефолт).

- [ ] **Step 1: Failing tests** (в form_add.rs tests; фикстуры — по образцу `two_formset_package` :406 / `two_formset_flash_image` :498; константы `FORMSET_GUID`/`FORMSET2_GUID`/`FILE_GUID` есть :346-348)

```rust
    fn two_formset_pkg_varstores() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        let g1 = Guid::try_parse(FORMSET_GUID).unwrap();
        b.emit_form_set(&g1, 1, 1, &[]);
        b.emit_var_store(7, &g1, 64, "OldV");
        b.emit_form(1, 1);
        b.emit_end();
        b.emit_end();
        let g2 = Guid::try_parse(FORMSET2_GUID).unwrap();
        b.emit_form_set(&g2, 2, 2, &[]);
        b.emit_var_store(2, &g2, 32, "Fs2V");
        b.emit_form(2, 2);
        b.emit_end();
        b.emit_end();
        hii_pkg(PACKAGE_FORMS, &b.build())
    }

    fn two_formset_varstores_flash_image() -> Vec<u8> {
        let content = [
            section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            section_bytes(EFI_SECTION_RAW, &two_formset_pkg_varstores()),
        ]
        .concat();
        flash_with_files(vec![ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &content,
        )])
    }

    #[test]
    fn add_form_varstore_collision_in_other_formset_is_allowed() {
        let data = two_formset_varstores_flash_image();
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1#1",
            &add_form_schema(),
        )
        .unwrap();
        let pkg = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1")
            .body
            .clone();
        let fs2 = crate::hii::values::varstore_map_formset(&pkg, 1).unwrap();
        assert!(
            fs2.iter().any(|m| m.id == 7 && m.name == "VStore"),
            "декларация схемы (id 7) встала в формсет #1, got {fs2:?}"
        );
        let fs1 = crate::hii::values::varstore_map_formset(&pkg, 0).unwrap();
        assert_eq!(
            fs1.iter().filter(|m| m.id == 7).count(),
            1,
            "в формсете #0 осталась только старая OldV"
        );
        assert!(fs1.iter().all(|m| m.name != "VStore"));
    }

    #[test]
    fn add_form_varstore_collision_in_target_formset_is_rejected() {
        let data = two_formset_varstores_flash_image();
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1#0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(
            matches!(err, HiiError::InvalidSchema(ref m) if m.contains("varstore id 0x7 already exists in the formset")),
            "id 7 уже декларирован в ЦЕЛЕВОМ формсете #0, got {err:?}"
        );
    }
```

(`add_form_schema` декларирует varstore id 7 "VStore" — :425-432.)

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p uefi-engine add_form_varstore_collision`
Expected: `..._in_other_formset_is_allowed` FAIL с `InvalidSchema("varstore id 0x7 already exists...")`; `..._in_target_formset_is_rejected` уже PASS (коллизия в #0 ловится и целой картой).

- [ ] **Step 3: Implement** (form_add.rs; сигнатура + тело :209-241; вызов в `add_form` :93)

Вызов:

```rust
    validate_form_varstores(image, &target, bare_channel, formset_idx, schema)?;
```

Функция (rustdoc и первые строки меняются, остальное тело без изменений):

```rust
/// Спека varstore-contract §2 + formset-ordinal-followups §2.1: per-item
/// ссылки и декларации проверяются против карты ЦЕЛЕВОГО формсета
/// (не всего пакета) ДО первой мутации образа.
fn validate_form_varstores(
    image: &Image,
    target: &crate::types::Target,
    bare_channel: bool,
    formset_idx: usize,
    schema: &schema::FormSetSchema,
) -> Result<(), HiiError> {
    let pkg = question_forms_package(&image.root, target, bare_channel)?.to_vec();
    let Some(existing) = super::values::varstore_map_formset(&pkg, formset_idx) else {
        return Err(HiiError::InvalidIfr);
    };
    let mut seen: Vec<u16> = Vec::new();
    // ... далее без изменений (:217-239)
```

- [ ] **Step 4: Run — verify pass + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (включая `list_add_roundtrip_formset_ordinal` — одно-формсетных регрессий нет).

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/form_add.rs
git commit -m "feat(engine): validate_form_varstores скоупится по формсету n — коллизии/ссылки целевого формсета (formset-ordinal-followups §2.1)"
```

---

### Task 3: `locate_form_attribution` + question-add проверки/вставка в формсет-владелец

**Files:**
- Modify: `crates/uefi-engine/src/hii/form_hijack.rs` (`locate_form` :22 — рядом новый `locate_form_attribution`)
- Modify: `crates/uefi-engine/src/hii/mod.rs`: `QuestionTarget` :1636 (+`formset_idx`), `resolve_question_target` :1644, `check_question_slots` :1676 (+idx), `preflight_question_splice` :1605 (+idx, убрать `0` на :1617), `check_rsrc_question_splice` :1310 (+idx, `locate_insert_at(pkg, 0, ...)` :1320 → idx), `splice_question_ops_into_resource` :1350 (+idx, :1358), `add_question` :1823/:1825 (qt.formset_idx), `check_question_add` :1875-1891 (scoped-петля) и :1912 (declared-check); сигнатурно-обязательные (Plan-fix 2026-10-01): `add_ref` :2180 (preflight) и :2216/:2218 (splice), `check_ref_add` :2263 (preflight), тест `preflight_bare_channel_bad_anchor...` :5610
- Test: `crates/uefi-engine/src/hii/mod.rs` (`mod question_add_tests` :5172; фикстуры — в `question_add_fixtures` рядом с `question_add_forms_pkg` :2406 и `question_add_flash_image` :2531)

**Interfaces:**
- Consumes: `ifr::formset_spans` (Task 1), `values::varstore_map_formset` (Task 1).
- Produces: `pub fn form_hijack::locate_form_attribution(pkg: &[u8], form_id: u16) -> Option<(usize, HijackFormSpan)>` — (formset-индекс первого формсета с формой, спан формы); первый-match, паритет `locate_form`.
- Produces: `QuestionTarget { ..., formset_idx: usize }`; `check_question_slots(schema, pkg, formset_idx, pending, extra_varstores)`; `preflight_question_splice(..., formset_idx, ...)`; `check_rsrc_question_splice(pe, formset_idx, form_id, pos, ops_len)`; `splice_question_ops_into_resource(pe, formset_idx, form_id, pos, ops)` — все private.

- [ ] **Step 1: Failing fixture + tests** (фикстуры в `question_add_fixtures`; константа `FORMSET2_GUID` — новой строкой рядом с существующей `FORMSET_GUID`)

```rust
    pub(crate) const FORMSET2_GUID: &str = "B1B2C3D4-E5F6-7890-ABCD-EF1234567890";

    pub(crate) fn two_formset_question_add_pkg() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        let g1 = Guid::from_str(FORMSET_GUID).unwrap();
        b.emit_form_set(&g1, 1, 1, &[]);
        b.emit_var_store(1, &g1, 0x100, "Setup");
        b.emit_form(10019, 1);
        b.emit_one_of(0x01A3, 0x01A4, 0x3B, 1, 0x3A, 0, 1);
        b.emit_one_of_option(4, 0x30, TYPE_NUM_SIZE_8, 0, 1);
        b.emit_one_of_option(3, 0x00, TYPE_NUM_SIZE_8, 1, 1);
        b.emit_end();
        b.emit_end();
        b.emit_end();
        let g2 = Guid::from_str(FORMSET2_GUID).unwrap();
        b.emit_form_set(&g2, 2, 2, &[]);
        b.emit_var_store(2, &g2, 0x80, "Setup2");
        b.emit_form(10020, 2);
        b.emit_one_of(0x01A5, 0x01A6, 0x55, 2, 0x40, 0, 1);
        b.emit_one_of_option(5, 0x00, TYPE_NUM_SIZE_8, 0, 1);
        b.emit_end();
        b.emit_end();
        b.emit_end();
        let ifr = b.build();
        let len = 4 + ifr.len() as u32;
        let mut pkg = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend_from_slice(&ifr);
        pkg
    }

    pub(crate) fn two_formset_question_add_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = two_formset_question_add_pkg();
        let mut spf_body = question_add_spf_body_for(&pkg);
        let skeleton = spf::append_page_skeleton(&mut spf_body, 0x68, 10020, 2, 2, 0);
        spf::register_page_slot(&mut spf_body, skeleton).expect("page-table gap free");
        let new_len = spf_body.len() - spf::container_start(&spf_body).unwrap();
        spf::bump_container_length(&mut spf_body, new_len);
        let blob = hii_list_blob(&[&pkg, &string_package_bytes()]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &pe),
        );
        let sd = sd_file_direct(&spf_body);
        (flash_with_files(vec![setup, sd]), pkg, spf_body)
    }
```

(Plan-fix 2026-10-01: голый `question_add_spf_body_for` даёт $SPF с единственной
страницей form 10019 — `plan_spf_append` требует страницу с form_id целевой
формы (`page_slot.ok_or(NotFound)`), оба новых теста целют 10020 и без
второй страницы зелёными стать не могут, независимо от threading. Страница
для 10020 регистрируется штатным тулкитом `add_page`: `append_page_skeleton`
+ `register_page_slot` (слот пишется в zero-gap после таблицы страниц) +
`bump_container_length`. Проверено пробой: plan для 10020 = Ok(page_slot=1,
page_offset=0x178), план 10019 не меняется.)

Тесты — в `mod question_add_tests` (рядом с `question_add_schema` :5180; хелпер `pkg_of` :5227):

```rust
        fn question_add_schema_10020(qid: u16, voff: u16) -> schema::QuestionAddSchema {
            schema::QuestionAddSchema {
                form_id: 10020,
                prompt: "Serial Console".into(),
                help: "Serial console help".into(),
                question_id: qid,
                var_store_id: 2,
                var_offset: voff,
                size: 1,
                options: vec![
                    schema::QuestionAddOption {
                        text: "Disabled".into(),
                        value: 0,
                        default: None,
                    },
                    schema::QuestionAddOption {
                        text: "Enabled".into(),
                        value: 1,
                        default: Some(schema::DefaultClass::Optimized),
                    },
                ],
                defaults: None,
                insert_before: None,
            }
        }

        #[test]
        fn check_question_add_scopes_varstore_checks_to_owning_formset() {
            let (flash, _, _) = two_formset_question_add_flash_image();
            let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let target2 = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10020";
            let mut foreign = question_add_schema_10020(0x77, 0x10);
            foreign.var_store_id = 1;
            let err = check_question_add(&img, target2, &[foreign], &[]).unwrap_err();
            assert!(
                matches!(err, HiiError::InvalidSchema(ref m) if m.contains("var store id 0x1 is not declared")),
                "id 1 декларирован только в соседнем fs1, не в формсете-владельце fs2, got {err:?}"
            );
            let vs = schema::VarStoreSchema {
                id: 2,
                guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 0x40,
                name: "Dup".into(),
                var_type: schema::VarStoreType::Buffer,
                attributes: 7,
            };
            let err = check_question_add(&img, target2, &[], &[vs]).unwrap_err();
            assert!(
                matches!(err, HiiError::InvalidSchema(ref m) if m.contains("varstore id 0x2 already exists")),
                "id 2 декларирован в формсете-владельце (fs2), got {err:?}"
            );
            let mut undeclared = question_add_schema_10020(0x78, 0x11);
            undeclared.var_store_id = 9;
            let err = check_question_add(&img, target2, &[undeclared], &[]).unwrap_err();
            assert!(
                matches!(err, HiiError::InvalidSchema(ref m) if m.contains("var store id 0x9 is not declared")),
                "id 9 не декларирован в fs2 (в fs1 только id 1), got {err:?}"
            );
        }

        #[test]
        fn add_question_targets_form_in_second_formset() {
            let (flash, _, _) = two_formset_question_add_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let target2 = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10020";
            let res = add_question(&mut img, target2, &question_add_schema_10020(0x77, 0x10)).unwrap();
            assert_eq!(res.question_id, 0x77);
            let pkg = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
            let (fs2_start, fs2_end) = crate::hii::ifr::formset_spans(&pkg).unwrap()[1];
            let in_fs2 = crate::hii::form_hijack::locate_questions(&pkg, 10020)
                .iter()
                .any(|&(off, qid)| qid == 0x77 && (fs2_start..fs2_end).contains(&off));
            assert!(in_fs2, "вопрос 0x77 в форме 10020 формсета #1");
            assert!(crate::hii::form_hijack::locate_questions(&pkg, 10019)
                .iter()
                .all(|&(_, qid)| qid != 0x77));
        }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p uefi-engine scopes_varstore_checks add_question_targets_form`
Expected: FAIL — `..._scopes_...`: assert-падение на `foreign`-кейсе с `NotFound` (preflight `locate_insert_at(pkg, 0, 10020)` с hardcoded formset 0 — формы 10020 в формсете #0 нет; после threading ожидание сменится на InvalidSchema «var store id 0x1 is not declared»); `add_question_targets_...`: NotFound от splice с hardcoded formset 0 (формы 10020 в формсете #0 нет). (Plan-fix 2026-10-01 №1: прежнее ожидание «InvalidSchema already exists на foreign-кейсе» было недостижимо — foreign-вызов передаёт `varstores=&[]`, коллизионный блок до preflight не срабатывает. Plan-fix 2026-10-01 №2: фикстура была без закрывающих FORM_SET END — незакрытый fs1 делал `form_span(pkg, 0, 10020)` ложно успешным (форма 10020 «внутри» формсета #0), `formset_spans` на таком пакете даёт None; каждый блок формсета закрывается тремя END: ONE_OF, FORM, FORM_SET.)

- [ ] **Step 3: Implement `locate_form_attribution`** (form_hijack.rs, над `locate_form`; наверху — `use super::ifr::formset_spans;`)

(Plan-fix 2026-10-01 №3: атрибуция в `resolve_question_target` ужесточает контракт для пакетов с незакрытым FORM_SET — `formset_spans` fail-closed → NotFound, спека §5 «честное ужесточение». Legacy-фикстура `package_with_form` (form_hijack test_fixtures, потребитель — `hijack_flash_image`/`hijack_test_flash`, rpc-тесты question add) не закрывала FORM_SET: 3 scoped-op, 2 END. Фикстура добирает закрывающий `b.emit_end()` в хвост — смещения существующих ops не меняются, `$SPF`-записи (считаются по `locate_questions`) не сдвигаются.)

```rust
/// Формсет-владелец формы + её спан: первый формсет, содержащий form_id
/// (первый-match, паритет locate_form). Спека formset-ordinal-followups §2.
pub fn locate_form_attribution(pkg: &[u8], form_id: u16) -> Option<(usize, HijackFormSpan)> {
    let span = locate_form(pkg, form_id)?;
    let idx = formset_spans(pkg)?
        .iter()
        .position(|&(s, e)| s <= span.form_op && span.form_op < e)?;
    Some((idx, span))
}
```

- [ ] **Step 4: Implement threading в mod.rs**

`QuestionTarget` (:1636-1642) — добавить поле:

```rust
struct QuestionTarget {
    target: crate::types::Target,
    form_id: u16,
    bare_channel: bool,
    formset_idx: usize,
    pkg: Vec<u8>,
    span: form_hijack::HijackFormSpan,
}
```

`resolve_question_target` (:1665-1673) — атрибуция вместо слепого locate_form:

```rust
    let pkg = question_forms_package(&image.root, &target, bare_channel)?.to_vec();
    let (formset_idx, span) =
        form_hijack::locate_form_attribution(&pkg, form_id).ok_or(HiiError::NotFound)?;
    Ok(QuestionTarget {
        target,
        form_id,
        bare_channel,
        formset_idx,
        pkg,
        span,
    })
```

`check_question_slots` (:1676) — параметр `formset_idx: usize` (после `pkg`), declared_size (:1719-1728):

```rust
    let declared_size = values::varstore_map_formset(pkg, formset_idx)
        .ok_or(HiiError::InvalidIfr)?
        .iter()
        .find(|v| v.id == schema.var_store_id)
        .map(|v| u32::from(v.size))
        .or_else(|| {
            extra_varstores
                .iter()
                .find(|v| v.id == schema.var_store_id)
                .map(|v| u32::from(v.size))
        })
```

(далее без изменений).

`preflight_question_splice` (:1605) — параметр `formset_idx: usize` (после `form_id`); :1617:

```rust
        return ifr::locate_insert_at(&node.body, formset_idx, form_id, pos).map(|_| ());
```

и вызов :1632 (resource-ветка preflight):

```rust
    check_rsrc_question_splice(&post_strings, formset_idx, form_id, pos, ops_len)?;
```

`check_rsrc_question_splice` (:1310) — параметр `formset_idx: usize` (после `pe`); :1320:

```rust
    ifr::locate_insert_at(pkg, formset_idx, form_id, pos)?;
```

`splice_question_ops_into_resource` (:1350) — параметр `formset_idx: usize` (после `pe`); :1358:

```rust
    let res = ifr::splice_question_ops(&mut pkg, formset_idx, form_id, pos, ops)?;
```

`add_question`: вызов `check_question_slots` :1776 → `check_question_slots(schema, &qt.pkg, qt.formset_idx, &[], &[])?;`; `preflight_question_splice` :1781-1789 — вставить `qt.formset_idx,` после `qt.form_id,`; splice :1822-1826:

```rust
        if qt.bare_channel {
            ifr::splice_question_ops(&mut node.body, qt.formset_idx, qt.form_id, pos, &ops)?
        } else {
            splice_question_ops_into_resource(&mut node.body, qt.formset_idx, qt.form_id, pos, &ops)?
        }
```

`check_question_add`: петля :1875-1891 → scoped-карта (Plan-fix 2026-10-01: без `let node = find_item(...)` — lookup в новом блоке не читается, `unused_variables` валит clippy-гейт; `qt.pkg` уже несёт байты того же узла):

```rust
    {
        let existing = values::varstore_map_formset(&qt.pkg, qt.formset_idx)
            .ok_or(HiiError::InvalidIfr)?;
        for vs in varstores {
            if existing.iter().any(|m| m.id == vs.id) {
                return Err(HiiError::InvalidSchema(format!(
                    "varstore id {:#x} already exists in the formset",
                    vs.id
                )));
            }
        }
    }
```

declared-check :1911-1914:

```rust
        let declared = varstores.iter().any(|v| v.id == schema.var_store_id)
            || values::varstore_map_formset(&qt.pkg, qt.formset_idx)
                .map(|m| m.iter().any(|v| v.id == schema.var_store_id))
                .unwrap_or(false);
```

(`unwrap_or(false)` здесь — не канал чтения, а отказ: None-карта → «не декларирован» → InvalidSchema ниже, громко.)

Вызов `check_question_slots` :1921 → `check_question_slots(schema, &qt.pkg, qt.formset_idx, &pending, varstores)?;`; вызов `preflight_question_splice` :1925-1933 — вставить `qt.formset_idx,` после `qt.form_id,`.

Сигнатурно-обязательные вызовы (Plan-fix 2026-10-01: в исходном плане не перечислены, но сигнатуры меняются — без правок крейт не соберётся): `add_ref` — preflight :2180 вставить `qt.formset_idx,` после `qt.form_id,`; splice :2216-2218 — bare-ветка `ifr::splice_question_ops(&mut node.body, qt.formset_idx, qt.form_id, pos, &ops)?`, resource-ветка `splice_question_ops_into_resource(&mut node.body, qt.formset_idx, qt.form_id, pos, &ops)?`; `check_ref_add` — preflight :2263 вставить `qt.formset_idx,` после `qt.form_id,`; тест `preflight_bare_channel_bad_anchor_is_invalid_schema_with_listing` :5610 — аргумент `0` (bare-фикстура одно-формсетная) после `10019`.

- [ ] **Step 5: Run — verify pass + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — новые тесты зелёные; все существующие question-add/add_question-тесты (одно-формсетные, idx=0) без регрессий.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/form_hijack.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): locate_form_attribution — question-add проверки и вставка вопросов в формсет-владелец формы (formset-ordinal-followups §2.1)"
```

---

### Task 4: `add_varstores` — атрибуция + splice в пролог формсета-владельца

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs`: `locate_formset_prelude_end` :337 (+idx), `splice_varstore_ops` :353 (+idx); их тесты :1650-1690
- Modify: `crates/uefi-engine/src/hii/mod.rs`: `splice_varstore_ops_into_resource` :1380 (+idx, :1388/:1410), `add_varstores` :1948; тесты :5266-5270 (сигнатуры), новый тест
**Interfaces:**
- Consumes: `locate_form_attribution` (Task 3), `formset_spans` (Task 1), фикстура `two_formset_question_add_flash_image` (Task 3).
- Produces: `pub fn ifr::splice_varstore_ops(package: &mut Vec<u8>, ops: &[u8], formset_idx: usize) -> Result<(usize, usize), HiiError>` (сигнатура меняется; потребитель один — add_varstores + тесты); `pub(crate) fn ifr::locate_formset_prelude_end(package: &[u8], formset_idx: usize) -> Option<usize>` — первый IFR_FORM_OP внутри спана idx (форм у формсета нет → None, formless-контракт сохранён); `fn splice_varstore_ops_into_resource(pe, ops, formset_idx)`.
- Поведение `add_varstores(image, "TARGET#FORM", ...)`: форма FORM обязана существовать в writable-пакете → иначе `HiiError::NotFound` (ужесточение: раньше декларации молча вставлялись в первый формсет); TARGET без `#FORM` → InvalidItemId из parse_item_id (form_id там — u16, не Option; компонента # обязательна на уровне парсинга).

- [ ] **Step 1: Failing tests** (ifr.rs tests — рядом с `splice_varstore_ops_inserts_before_first_form` :1650; двухформсетный ifr собери inline, как в тестах formset_spans Task 1; `varstore_bytes()` :1074 уже есть)

```rust
#[test]
fn splice_varstore_ops_targets_given_formset_prelude() {
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
    let mut pkg = package(&ifr);
    let ops = varstore_bytes();
    let fs2_form = 4 + (23 + 6 + 2) + 23;
    let (at, delta) = splice_varstore_ops(&mut pkg, &ops, 1).unwrap();
    assert_eq!(at, fs2_form, "вставка перед первой формой ВТОРОГО формсета");
    assert_eq!(delta, ops.len());
    assert_eq!(&pkg[at..at + ops.len()], &ops[..]);
}
```

Мод.rs tests — в `mod question_add_tests` (рядом с `add_varstores_declares_efi_varstore_and_shifts_spf_records` :5296):

```rust
        #[test]
        fn add_varstores_declares_in_owning_formset_prelude() {
            let (flash, _, _) = two_formset_question_add_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let target2 = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10020";
            let vs = schema::VarStoreSchema {
                id: 5,
                guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 16,
                name: "NewV".into(),
                var_type: schema::VarStoreType::Buffer,
                attributes: 7,
            };
            add_varstores(&mut img, target2, std::slice::from_ref(&vs)).unwrap();
            let pkg = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
            let fs2 = values::varstore_map_formset(&pkg, 1).unwrap();
            assert!(fs2.iter().any(|m| m.id == 5 && m.name == "NewV"));
            let fs1 = values::varstore_map_formset(&pkg, 0).unwrap();
            assert!(
                fs1.iter().all(|m| m.id != 5),
                "декларация не должна попадать в пролог формсета #0, got {fs1:?}"
            );
        }

        #[test]
        fn add_varstores_unknown_form_is_not_found() {
            let (flash, _, _) = two_formset_question_add_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let err = add_varstores(
                &mut img,
                "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#40000",
                &[schema::VarStoreSchema {
                    id: 5,
                    guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                    size: 16,
                    name: "NewV".into(),
                    var_type: schema::VarStoreType::Buffer,
                    attributes: 7,
                }],
            )
            .unwrap_err();
            assert!(matches!(err, HiiError::NotFound), "got {err:?}");
        }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p uefi-engine splice_varstore_ops_targets add_varstores_declares_in add_varstores_unknown`
Expected: FAIL — сигнатуры старые (компиляция тестов упадёт на `splice_varstore_ops(&mut pkg, &ops, 1)`); после реализации Step 3 — зелёные.

- [ ] **Step 3: Implement ifr.rs** (заменить :337-367)

```rust
/// Конец пролога формсета formset_idx = offset первого IFR_FORM_OP его
/// спана (точка вставки formset-уровневых деклараций). Форм у формсета
/// нет → None (formless-контракт). Спека formset-unlock §3 U3,
/// formset-ordinal-followups §2.
pub(crate) fn locate_formset_prelude_end(package: &[u8], formset_idx: usize) -> Option<usize> {
    let (start, end) = formset_spans(package)?.get(formset_idx).copied()?;
    let mut first: Option<usize> = None;
    walk_statements(package, |op, off, _len, _| {
        if op == IFR_FORM_OP && first.is_none() && (start..end).contains(&off) {
            first = Some(off);
        }
    });
    first
}

/// Вставка formset-уровневых ops в пролог (до первой формы) формсета
/// formset_idx с обновлением u24-длины пакета. НЕ проверяет коллизии id —
/// это уровень add_varstores. Спека formset-unlock §3 U3,
/// formset-ordinal-followups §2.
pub fn splice_varstore_ops(
    package: &mut Vec<u8>,
    ops: &[u8],
    formset_idx: usize,
) -> Result<(usize, usize), HiiError> {
    if ops.is_empty() {
        return Err(HiiError::InvalidSchema("empty ops".into()));
    }
    let Some(insert_at) = locate_formset_prelude_end(package, formset_idx) else {
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

Существующие ifr-тесты `splice_varstore_ops_inserts_before_first_form` / `splice_varstore_ops_rejects_empty_ops_and_formless_package` (:1650, :1670) — дополнить вызовы третьим аргументом `0`.

- [ ] **Step 4: Implement mod.rs**

`splice_varstore_ops_into_resource` (:1380-1424) — параметр `formset_idx: usize` (после `pe`); :1388 → `ifr::locate_formset_prelude_end(pkg, formset_idx).ok_or(HiiError::NotFound)?;`; :1410 → `ifr::splice_varstore_ops(&mut pkg, ops, formset_idx)?;`.

`add_varstores` (:1948-2026) — заменить блок :1956 (form_id больше не отбрасывается) и блоки :1973-1995 (валидация + pkg_before):

```rust
    let (target, form_id, _qid) = parse_item_id(item_id)?;
```

(form_id — u16, не Option: parse_item_id гарантирует наличие `#FORM`, иначе InvalidItemId; отдельная `.ok_or`-строка не нужна и не компилируется)

после сборки `ops` и проверки node/subtype (:1973-1977) — атрибуция и scoped-валидация вместо петли по всем рангам:

```rust
    let pkg_before = form_package_ranges(node)
        .into_iter()
        .next()
        .map(|(s, l)| node.body[s..s + l].to_vec())
        .ok_or(HiiError::NotASetupItem)?;
    let formset_idx =
        form_hijack::locate_form_attribution(&pkg_before, form_id).ok_or(HiiError::NotFound)?.0;
    if let Some(existing) = values::varstore_map_formset(&pkg_before, formset_idx) {
        for vs in varstores {
            if existing.iter().any(|m| m.id == vs.id) {
                return Err(HiiError::InvalidSchema(format!(
                    "varstore id {:#x} already exists in the formset",
                    vs.id
                )));
            }
        }
    } else {
        return Err(HiiError::InvalidIfr);
    }
```

splice (:1996-2004):

```rust
    let (insert_at, delta) = {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.subtype == EFI_SECTION_RAW {
            ifr::splice_varstore_ops(&mut node.body, &ops, formset_idx)?
        } else {
            splice_varstore_ops_into_resource(&mut node.body, &ops, formset_idx)?
        }
    };
```

Старый дубль вычисления `pkg_before` (:1991-1995) удалить (теперь считается до валидации). Rustdoc `add_varstores` (:1944-1946) дополнить строкой: «Декларации встают в пролог формсета-владельца целевой формы TARGET#FORM (formset-ordinal-followups §2.1); формы нет → NotFound.»

Тест `verify_spliced_snapshot_accepts_exact_splice_and_rejects_divergence` (:5266, :5270) — вызовы получают `, 0`.

- [ ] **Step 5: Run — verify pass + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — включая `add_varstores_declares_efi_varstore_and_shifts_spf_records` (одно-формсетная фикстура: атрибуция формы 10019 → idx 0, поведение то же) и `add_varstores_error_order_contract`.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/ifr.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): add_varstores — атрибуция формсета целевой формы, декларации в пролог владельца (formset-ordinal-followups §2.1)"
```

---

### Task 5: collect_forms — writable по индексу + dedup + фикстуры multi/dup resource-записей

**Files:**
- Modify: `crates/uefi-engine/src/hii/pe_resource.rs` (новые `#[cfg(test)]` синтезаторы рядом с `synth_hii_pe` :738)
- Modify: `crates/uefi-engine/src/hii/forms.rs` (PE32-ветка `walk_sections` :115-132; тесты :856-904)
**Interfaces:**
- Produces (test-only): `pub(crate) fn pe_resource::synth_hii_pe_multi(type_name: &str, blobs: &[&[u8]]) -> Vec<u8>` — N различных HII-resource-записей; `pub(crate) fn pe_resource::synth_hii_pe_dup(type_name: &str, blob: &[u8]) -> Vec<u8>` — две записи с идентичным (off,len).
- Поведение: writable-канал = первая запись ПО ИНДЕКСУ; диапазон (off,len), уже слитый, пропускается (дубли не рождают ни ordinal, ни вторых строк).

- [ ] **Step 1: Failing tests** (forms.rs tests; ifr-байты — по образцу `collect_forms_ordinal_pe32_only_first_resource_forms_pkg` :856-895; хелперы `package`/`string_pkg`/`hii_list_raw`/`mk_node`/`img_of` есть)

```rust
    #[test]
    fn collect_forms_ordinal_pe32_second_resource_entry_is_none() {
        let g1: [u8; 16] = [1; 16];
        let g3: [u8; 16] = [3; 16];
        let mut ifr1 = Vec::new();
        ifr1.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
        ifr1.extend_from_slice(&g1);
        ifr1.extend_from_slice(&[1, 0, 0, 0, 0]);
        ifr1.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 1, 0]);
        ifr1.extend_from_slice(&[IFR_END_OP, 2]);
        let mut ifr2 = Vec::new();
        ifr2.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
        ifr2.extend_from_slice(&g3);
        ifr2.extend_from_slice(&[3, 0, 0, 0, 0]);
        ifr2.extend_from_slice(&[IFR_FORM_OP, 6, 3, 0, 3, 0]);
        ifr2.extend_from_slice(&[IFR_END_OP, 2]);
        let list1 = hii_list_raw(&package(&ifr1), &string_pkg());
        let list2 = hii_list_raw(&package(&ifr2), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe_multi("HII", &[&list1, &list2]);
        let pe_sec = mk_node(None, FfsType::Section, EFI_SECTION_PE32, pe, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![pe_sec],
        );
        let image = img_of(file);
        let forms = collect_forms_with_ordinals(&image);
        assert_eq!(forms.len(), 2, "по одной форме на запись: got {forms:?}");
        assert_eq!(
            forms.iter().find(|(f, _)| f.form_id_ifr == 1).unwrap().1,
            Some(0)
        );
        assert_eq!(
            forms.iter().find(|(f, _)| f.form_id_ifr == 3).unwrap().1,
            None
        );
    }

    #[test]
    fn collect_forms_ignores_duplicate_resource_ranges() {
        let g1: [u8; 16] = [1; 16];
        let mut ifr1 = Vec::new();
        ifr1.extend_from_slice(&[IFR_FORM_SET_OP, 23 | 0x80]);
        ifr1.extend_from_slice(&g1);
        ifr1.extend_from_slice(&[1, 0, 0, 0, 0]);
        ifr1.extend_from_slice(&[IFR_FORM_OP, 6, 1, 0, 1, 0]);
        ifr1.extend_from_slice(&[IFR_END_OP, 2]);
        let list = hii_list_raw(&package(&ifr1), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe_dup("HII", &list);
        let pe_sec = mk_node(None, FfsType::Section, EFI_SECTION_PE32, pe, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![pe_sec],
        );
        let image = img_of(file);
        let forms = collect_forms_with_ordinals(&image);
        assert_eq!(forms.len(), 1, "дубль-диапазон не удваивает строки");
        assert_eq!(forms[0].1, Some(0));
    }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p uefi-engine collect_forms_ordinal_pe32_second collect_forms_ignores_duplicate`
Expected: FAIL — «cannot find function `synth_hii_pe_multi`».

- [ ] **Step 3: Implement синтезаторы** (pe_resource.rs, рядом с `synth_hii_pe` :738-793; общий PE-хвост вынеси в приватную fn `pe_with_rsrc(rsrc: Vec<u8>) -> Vec<u8>` с телом хвоста :769-792, а `synth_hii_pe` пере-использует её)

```rust
#[cfg(test)]
fn pe_with_rsrc(rsrc: Vec<u8>) -> Vec<u8> {
    let rsrc_rva: u32 = 0x1000;
    let size_of_image = (rsrc_rva + rsrc.len() as u32).next_multiple_of(0x1000);
    let mut pe = vec![0u8; 0x170];
    pe[0] = b'M';
    pe[1] = b'Z';
    pe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    pe[0x40..0x44].copy_from_slice(b"PE\0\0");
    pe[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
    pe[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
    pe[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
    pe[0x58..0x5a].copy_from_slice(&0x20bu16.to_le_bytes());
    pe[0x78..0x7c].copy_from_slice(&0x1000u32.to_le_bytes());
    pe[0x7c..0x80].copy_from_slice(&16u32.to_le_bytes());
    pe[0x90..0x94].copy_from_slice(&size_of_image.to_le_bytes());
    pe[0xc4..0xc8].copy_from_slice(&16u32.to_le_bytes());
    pe[0xd8..0xdc].copy_from_slice(&rsrc_rva.to_le_bytes());
    pe[0xdc..0xe0].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x148..0x14f].copy_from_slice(b".rsrc\0\0");
    pe[0x150..0x154].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x154..0x158].copy_from_slice(&rsrc_rva.to_le_bytes());
    pe[0x158..0x15c].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x15c..0x160].copy_from_slice(&0x170u32.to_le_bytes());
    pe[0x16c..0x170].copy_from_slice(&0x4000_0040u32.to_le_bytes());
    pe.extend_from_slice(&rsrc);
    pe
}

#[cfg(test)]
pub(crate) fn synth_hii_pe_multi(type_name: &str, blobs: &[&[u8]]) -> Vec<u8> {
    let rsrc_rva: u32 = 0x1000;
    let name_at = 0x40 + 8 * blobs.len();
    let mut name_end = name_at + 2 + 2 * type_name.encode_utf16().count();
    name_end = name_end.next_multiple_of(8);
    let data_offs: Vec<usize> = (0..blobs.len()).map(|i| name_end + 16 * i).collect();
    let mut blob_offs: Vec<usize> = Vec::with_capacity(blobs.len());
    let mut cur = name_end + 16 * blobs.len();
    for b in blobs {
        blob_offs.push(cur);
        cur = (cur + b.len()).next_multiple_of(16);
    }
    let mut rsrc = Vec::new();
    rsrc.extend_from_slice(&rsrc_dir_header(1, 0));
    rsrc.extend_from_slice(&(0x8000_0000u32 | name_at as u32).to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0018u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 1));
    rsrc.extend_from_slice(&1u32.to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0030u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, blobs.len() as u16));
    for &d in &data_offs {
        rsrc.extend_from_slice(&0x409u32.to_le_bytes());
        rsrc.extend_from_slice(&(d as u32).to_le_bytes());
    }
    while rsrc.len() < name_at {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(&(type_name.len() as u16).to_le_bytes());
    for u in type_name.encode_utf16() {
        rsrc.extend_from_slice(&u.to_le_bytes());
    }
    while rsrc.len() < name_end {
        rsrc.push(0);
    }
    for (i, b) in blobs.iter().enumerate() {
        rsrc.extend_from_slice(&(rsrc_rva + blob_offs[i] as u32).to_le_bytes());
        rsrc.extend_from_slice(&(b.len() as u32).to_le_bytes());
        rsrc.extend_from_slice(&0u32.to_le_bytes());
        rsrc.extend_from_slice(&0u32.to_le_bytes());
    }
    for b in blobs {
        rsrc.extend_from_slice(b);
        while rsrc.len() % 16 != 0 {
            rsrc.push(0);
        }
    }
    pe_with_rsrc(rsrc)
}

#[cfg(test)]
pub(crate) fn synth_hii_pe_dup(type_name: &str, blob: &[u8]) -> Vec<u8> {
    let rsrc_rva: u32 = 0x1000;
    let name_at = 0x40 + 8 * 2;
    let mut name_end = name_at + 2 + 2 * type_name.encode_utf16().count();
    name_end = name_end.next_multiple_of(8);
    let data_at = name_end;
    let blobs_at = data_at + 16 * 2;
    let mut rsrc = Vec::new();
    rsrc.extend_from_slice(&rsrc_dir_header(1, 0));
    rsrc.extend_from_slice(&(0x8000_0000u32 | name_at as u32).to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0018u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 1));
    rsrc.extend_from_slice(&1u32.to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0030u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 2));
    rsrc.extend_from_slice(&0x409u32.to_le_bytes());
    rsrc.extend_from_slice(&(data_at as u32).to_le_bytes());
    rsrc.extend_from_slice(&0x40Au32.to_le_bytes());
    rsrc.extend_from_slice(&((data_at + 16) as u32).to_le_bytes());
    while rsrc.len() < name_at {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(&(type_name.len() as u16).to_le_bytes());
    for u in type_name.encode_utf16() {
        rsrc.extend_from_slice(&u.to_le_bytes());
    }
    while rsrc.len() < name_end {
        rsrc.push(0);
    }
    for _ in 0..2 {
        rsrc.extend_from_slice(&(rsrc_rva + blobs_at as u32).to_le_bytes());
        rsrc.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        rsrc.extend_from_slice(&0u32.to_le_bytes());
        rsrc.extend_from_slice(&0u32.to_le_bytes());
    }
    while rsrc.len() < blobs_at {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(blob);
    while rsrc.len() % 16 != 0 {
        rsrc.push(0);
    }
    pe_with_rsrc(rsrc)
}
```

(`synth_hii_pe` рефакторится на `pe_with_rsrc`: его rsrc-строительство остаётся, хвост :769-792 заменяется на `pe_with_rsrc(rsrc)` — поведение идентично, существующие тесты PE-фикстур это пиннят.)

- [ ] **Step 4: Run — tests still fail on dedup case (fixture ok, semantics not yet)**

Run: `cargo test -p uefi-engine collect_forms_ordinal_pe32_second collect_forms_ignores_duplicate`
Expected: `..._second_resource_entry_is_none` PASS — существующая семантика уже даёт None для второй записи с другим (off,len) (новая фикстура пиннит контракт TODO-пункта 3); `..._ignores_duplicate` FAIL — дубль-диапазон даёт ДВЕ строки с Some(0) у обеих (дефект TODO-пункта 2).

- [ ] **Step 5: Implement writable-детект + dedup** (forms.rs :115-132, PE32-ветка `walk_sections`)

```rust
        } else if child.subtype == EFI_SECTION_PE32 {
            let ranges = hii_resource_ranges(&child.body);
            let mut drained: Vec<(usize, usize)> = Vec::new();
            for (i, &(off, len)) in ranges.iter().enumerate() {
                if drained.contains(&(off, len)) {
                    continue;
                }
                drained.push((off, len));
                let Some(blob) = child.body.get(off..off + len) else {
                    continue;
                };
                let Some(list) = parse_package_list(blob) else {
                    continue;
                };
                drain_list_packages(&list, &target, titles, found, i == 0);
            }
            for pkg in bare_form_packages(&child.body, &ranges) {
                if let Some(sets) = parse_form_package_sets(pkg) {
                    for (_, fs) in sets {
                        found.push((target.clone(), fs, None));
                    }
                }
            }
        }
```

(разница с текущим: `enumerate` + `i == 0` вместо `ranges.first()`-по-значению; `(off,len)` маркируется слитым ДО parse-continue, чтобы нераспарсенный дубль тоже не разбирался дважды.)

- [ ] **Step 6: Run — verify pass + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — включая `collect_forms_ordinal_pe32_only_first_resource_forms_pkg` (одна запись: i==0 → writable, семантика та же) и остальные ordinal-тесты.

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/src/hii/pe_resource.rs crates/uefi-engine/src/hii/forms.rs
git commit -m "fix(engine): collect_forms writable по индексу resource-записи + dedup дублей; фикстуры multi/dup (formset-ordinal-followups §3)"
```

---

### Task 6: CLI `print_forms` — TSV-сборка в чистых функциях

**Files:**
- Modify: `crates/uefi-cli/src/output.rs` (`print_forms` :179-213; рядом с `ordinal_cell` :172)
- Test: там же, `mod tests` (рядом с `ordinal_cell_formats_ordinal_and_dash` :1245)

**Interfaces:**
- Produces (private): `fn forms_tsv_header() -> &'static str`, `fn forms_tsv_row(f: &FormInfo) -> String`, `fn forms_text_row(f: &FormInfo) -> String`; `print_forms` только печатает.

- [ ] **Step 1: Failing tests**

```rust
    #[test]
    fn forms_tsv_header_and_rows_carry_ordinal_column() {
        assert_eq!(
            forms_tsv_header(),
            "form_id\tformset_guid\tform_id_ifr\ttitle\tvisible\tn"
        );
        let f = uefi_proto::FormInfo {
            form_id: "T".into(),
            formset_guid: "G".into(),
            form_id_ifr: 1,
            title: "t".into(),
            visible: true,
            formset_ordinal: Some(1),
        };
        assert_eq!(forms_tsv_row(&f), "T\tG\t1\tt\ttrue\t#1");
        let none = uefi_proto::FormInfo {
            formset_ordinal: None,
            ..f.clone()
        };
        assert_eq!(forms_tsv_row(&none), "T\tG\t1\tt\ttrue\t—");
        assert_eq!(forms_text_row(&f), "T\tG\t1\tt\tvisible=true\t#1");
    }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p uefi-cli forms_tsv`
Expected: FAIL «cannot find function».

- [ ] **Step 3: Implement** (output.rs; `print_forms` переписывается на хелперы)

```rust
fn forms_tsv_header() -> &'static str {
    "form_id\tformset_guid\tform_id_ifr\ttitle\tvisible\tn"
}

fn forms_tsv_row(f: &FormInfo) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        f.form_id,
        f.formset_guid,
        f.form_id_ifr,
        f.title,
        f.visible,
        ordinal_cell(f.formset_ordinal)
    )
}

fn forms_text_row(f: &FormInfo) -> String {
    format!(
        "{}\t{}\t{}\t{}\tvisible={}\t{}",
        f.form_id,
        f.formset_guid,
        f.form_id_ifr,
        f.title,
        f.visible,
        ordinal_cell(f.formset_ordinal)
    )
}

pub fn print_forms(forms: &[FormInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(forms).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("{}", forms_tsv_header());
            for f in forms {
                println!("{}", forms_tsv_row(f));
            }
        }
        OutputFormat::Text => {
            for f in forms {
                println!("{}", forms_text_row(f));
            }
        }
    }
}
```

- [ ] **Step 4: Run — verify pass + clippy**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-cli/src/output.rs
git commit -m "refactor(cli): print_forms — TSV/Text-сборка в чистых функциях с прямыми тестами (formset-ordinal-followups §3)"
```

---

### Task 7: TODO-закрытие + roadmap + финальные гейты

**Files:**
- Modify: `TODO.md` (секция «Отложенное финального ревью formset-ordinal (2026-10-01)» :4753-4777)
- Modify: `roadmap.md` (после блока мини-цикла formset-ordinal :527)

**Interfaces:** нет (docs).

- [ ] **Step 1: TODO.md** — каждую из 4 записей секции перевести в закрытое состояние (по образцу закрытия записи formset-ordinal в этом же файле :558-561): отметить `[x]`, дописать строку-резолюцию. Итоговый вид записей:

```markdown
* [x] **класс: limitation: `hii varstore list` игнорирует formset-ординал в
  `TARGET[#n]`** —
  Закрыто циклом formset-ordinal-followups (2026-10-01): list_varstores
  скоупится по #n (varstore_map_formset), write-валидация/вставка тоже
  (validate_form_varstores, check_question_add/add_varstores, splice в
  пролог формсета-владельца); спека
  2026-10-01-formset-ordinal-followups-design.md.
* [x] **класс: limitation: PE с дублирующимися resource-записями ...** —
  Закрыто циклом formset-ordinal-followups (2026-10-01): writable по
  индексу записи + dedup диапазона (synth_hii_pe_dup-тест).
* [x] **uefi-engine: явная фикстура «второй resource-entry → ordinal
  None» отсутствует** —
  Закрыто циклом formset-ordinal-followups (2026-10-01):
  synth_hii_pe_multi + тест collect_forms_ordinal_pe32_second_resource_entry_is_none.
* [x] **uefi-cli: print_forms — обвязка колонки `n` без прямого теста** —
  Закрыто циклом formset-ordinal-followups (2026-10-01): forms_tsv_header/
  forms_tsv_row/forms_text_row extraction с прямыми unit-тестами.
```

(Тексты первых строк записей сохранить как есть; резолюции дописать в конец каждой записи. Заголовок секции оставить.)

- [ ] **Step 2: roadmap.md** — после блока «Мини-цикл formset-ordinal …» добавить блок:

```markdown
## Мини-цикл formset-ordinal-followups — формсет-скоуп varstore-семантики (2026-10-01)

`docs/superpowers/specs/2026-10-01-formset-ordinal-followups-design.md` +
план `docs/superpowers/plans/2026-10-01-formset-ordinal-followups.md`
(7 задач, TDD). Реализовано: `ifr::formset_spans` +
`values::varstore_map_formset` (единый источник границ FORM_SET),
`list_varstores TARGET#n` чтит ординал; write-путь скоупится по целевому
формсету: `validate_form_varstores` (form add),
`check_question_add`/`add_varstores`/вставка вопросов и деклараций —
формсет-владелец формы (`locate_form_attribution`), splice_varstore_ops с
formset_idx; collect_forms: writable по индексу resource-записи + dedup
дублей (фикстуры synth_hii_pe_multi/dup); CLI print_forms-форматтер с
прямыми тестами. Закрывает 4 записи TODO-секции «Отложенное финального
ревью formset-ordinal».
```

- [ ] **Step 3: Финальные гейты**

```bash
cargo test --all
cargo clippy --all --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: все PASS/чисто. WebUI/TUI не менялись — `npm run check` не требуется (proto не трогался), но при сомнениях: `cd webui && npm run check`.

- [ ] **Step 4: Commit**

```bash
git add TODO.md roadmap.md
git commit -m "docs: закрытие TODO formset-ordinal-followups + roadmap (formset-ordinal-followups §6)"
```

---

## Notes for executor

- Task 1 Step 9: тест кладётся в тот же тестовый модуль mod.rs, где живут `list_varstores_returns_formset_declarations` и хелперы `section_bytes`/`ffs_file_bytes`/`flash_with_files` (модуль фикстур question_add); вызывай хелперы напрямую — путь `crate::hii::mod_tests_helpers::` в коде теста выше условный, поправь на фактический scope. Если хелперы недоступны из нужного модуля — docs-коммит с фиксацией плана (Global Constraints).
- Task 3: `question_add_spf_body_for` (:2499) сам считает offset'ы вопросов по pkg — работает с любым pkg, включая двухформсетный (проверяется тестом).
- Порядок задач строгий: Task 3/4 зависят от Task 1 (formset_spans/varstore_map_formset) и Task 3 (locate_form_attribution, фикстура).
- Если тест падает не по описанной причине — систематический дебаг (прочитай ошибку → код → минимальная правка), не ослабляй ожидания.
