# hii-write-guard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Гарды пишущего пути HII (класс B): checked-арифметика string-id (B1), bounds-guard'ы план-фазы гейтов (B2), snapshot-rollback атомарность apply-фаз (B3), единый резолвер .rsrc-секции с отказом при неоднозначности (B4) + docs-балансировка TODO.

**Architecture:** Пять точечных независимых фиксов пишущего пути плюс два docs-коммита (актуализация TODO до кода — в master; закрытие строк — в конце цикла). B1 меняет сигнатуры `scan_sibt`/`add_strings_to_body` и добавляет вариант `HiiError::StringIdExhausted`; B3 добавляет переиспользуемый хелпер `with_rollback` (deepcopy `FfsNode`) на две точки входа; B2 переводит `plan_flip` на `Result<Option<_>, String>`; B4 вводит `resolve_rsrc_section` и переводит на него все три потребителя max-предиката.

**Tech Stack:** Rust workspace (edition 2024), object 0.39 (PE-парсинг, только leaf-резолюция), tonic (Status::resource_exhausted), tracing + tracing-test (warn-семантика reader-близнеца).

**Spec:** `docs/superpowers/specs/2026-09-25-hii-write-guard-design.md` (коммит `6646830`)

## Global Constraints

- **Ветка `hii-write-guard`** — весь код только в ней (от master). Task 1 и Task 7 — docs-коммиты (Task 1 в master ДО ветки; Task 7 на ветке — уйдёт в master вместе с мерджем).
- TDD строго: (1) failing-тест → (2) `cargo test` red → (3) реализация → (4) green → (5) commit.
- После каждой задачи: `cargo test -p uefi-engine`, `cargo clippy -p uefi-engine -- -D warnings`, `cargo fmt --all -- --check`.
- Комментарии в коде — только rustdoc `///`-контракты на pub/pub(crate)-функциях (что делает / чего НЕ делает + `спека hii-write-guard §N`); narration-комментарии нет.
- Вся новая арифметика границ — checked (`checked_add` / `is_none_or` / `get`); усечение/переполнение = Err (writer) или warn + stop walk (reader), без паник.
- Валидные назначаемые string-id: `1..=0xFFFE` (0 — NULL, 0xFFFF — EFI_STRING_ID_INVALID).
- Real-гейты (`#[ignore]`) в обычном прогоне не запускаются; явный запуск — Task 7.
- Дефекты плана/спеки, найденные в шаге — отдельный docs-коммит ДО реализации (AGENTS.md правило 11).
- Один коммит на задачу; сообщения — из плана.
- Номера TODO-строк ниже — по текущему master; спека ссылается на сдвинутые номера (428→`:442`, 431→`:445`, 1319→`:1333`, 3240→`:3257`, 3744→`:3767`, 2946→`:2963`, 2952→`:2969`) — при смещении искать по заголовку записи.

## Контекст для исполнителя (файлы и точки)

- `crates/uefi-engine/src/hii/string_pack.rs` — `add_strings` (`:30`), `add_strings_to_body` (`:46`), `AddStringsToResourceError` (`:60`), `add_strings_to_resource` (`:66`, вызов `add_strings_to_body` на `:83`), `string_info_offset` (`:164`), `scan_sibt` (`:179`, ~15 `wrapping_add`), тест-хелперы `make_string_package`/`make_image`/`res_list`/`pkg` (`:308-533`).
- `crates/uefi-engine/src/hii/mod.rs` — `HiiError` (`:33`, `IdOccupied` на `:45`); 4 match-потребителя `AddStringsToResourceError`: `preflight_question_splice` (`:1452`), `add_question` (`:1625`), `add_ref` (`:2019`), `add_page` (`:2174`); `gate_info` (`:316-318`, срез + `plan_flip`); `unlock` (`:401`, own-фаза уже восстанавливает body через `mem::take`); `apply_cross_formset_gates` (`:491-549`, план/apply чередуются по сайту — без отката); `node_at`/`node_at_mut` (`:931-945`); тест-фикстуры `question_add_fixtures` (`:2215`, `question_add_bare_flash_image` `:2388`).
- `crates/uefi-engine/src/hii/gates.rs` — `Gate` (`:53`), `plan_flip` (`:308-332`, индексирование без проверок), `plan_gates` (`:334`), `plan_gates_skip_unlocked` (`:366`), `apply_flips` (`:393`, уже guarded — НЕ трогаем); Err-сообщения планеров берут срез `&body[gate.expr_offset..gate.expr_end.min(body.len())]` (`:340`, `:375`).
- `crates/uefi-engine/src/hii/strings.rs` — `parse_string_package` walk (`:37-231`), `push` (`:241`, `wrapping_add`), SKIP-руки (`:181-193`, `wrapping_add`), мёртвый let-else в STRINGS_SCSU/SCSU_FONT (`:73-84` и `:95-107`), тест-хелпер `make_pkg` (`:458`), паттерн `#[tracing_test::traced_test]` (`:840`).
- `crates/uefi-engine/src/hii/pe_resource.rs` — `push_leaf` → `file.section_table().pe_file_range_at(rva)` (`:117`, object-канал, НЕ трогаем); `rsrc_grow_plan` max-предикат first-match (`:289-300`); `rsrc_raw_end` (`:629-651`) / `rsrc_virt_end` (`:653-674`) — тот же max-предикат; вызовы: `plan_rsrc_blob_growth_for` (`:511-512`), `append_plan_for` (`:604`); тест-хелперы `pkg`/`string_pkg`/`list`/`LIST_GUID`/`le_u32`/`synth_hii_pe` (`:699-754` и выше в `mod tests`).
- `crates/uefi-engine/src/hii/form_add.rs:131`, `form_hijack.rs:252`, `formset_add.rs:93` — остальные match-потребители `AddStringsToResourceError`.
- `crates/uefi-engine/src/hii/cross_formset.rs` — `find_cross_gates` (`:20`, порядок сайтов = порядок обхода дерева), фикстуры `cross_fixtures` (`donor_pkg` `:166`, `target_pkg` `:239`, `two_file_image_with` `:287`, `mk_node` `:256`).
- `crates/uefi-engine/src/rpc/server.rs` — `hii_error_status` (`:54-70`, catch-all `_ => internal`), `hii_question_add` (`:1145-1209`, check-фаза `:1161-1170`, apply `:1171-1200`), тесты: `hii_error_status_maps_id_occupied` (`:1553`), `question_add_status` (`:2207`), `hii_question_add_list_failure_applies_nothing` (`:2294` — паттерн «build_image == flash после отказа»).
- `crates/uefi-engine/src/types.rs:148` — `FfsNode` already `Clone` (deepcopy = snapshot).
- `crates/uefi-engine/src/builder/mod.rs` — `build_image(&Image) -> Result<Vec<u8>, BuilderError>`.
- object 0.39 семантика (референс, не зависимость): `SectionTable::pe_file_range` = `(raw_ptr, min(vsize, raw))`; `pe_file_range_at(rva)` содержит адрес ⇔ `rva - va < min(vsize, raw)`. Отсюда min-зеркало в B4.
- `TODO.md` — записи `:442` (string-id), `:445` (.rsrc), `:1333` (plan bounds), `:3257` (question_add откат), `:2963`/`:2969` (insert_strings_at_ids*), `:3767` (кросс-фаза unlock), `:4382` (мёртвый let-else).

---

### Task 1: Docs №0 — актуализация TODO (master)

**Files:**
- Modify: `TODO.md` (записи `:442`, `:445`, `:1333`, `:3257`, `:3767`, `:2963`, `:2969`)

**Interfaces:** — (чистая docs-правка, код не трогаем)

- [ ] **Step 1: Пометить пять записей класса B «в цикле»**

В каждую из пяти записей дописать строкой ниже (с тем же отступом продолжения записи) приведённый текст. Полный путь спеки: `docs/superpowers/specs/2026-09-25-hii-write-guard-design.md`.

`:442` (`string_pack: исчерпание string-id 0xFFFF`):
```markdown
  В цикле hii-write-guard (спека
  docs/superpowers/specs/2026-09-25-hii-write-guard-design.md §1 B1).
```

`:445` (`pe_resource: неоднозначный выбор .rsrc-секции`):
```markdown
  В цикле hii-write-guard (спека
  docs/superpowers/specs/2026-09-25-hii-write-guard-design.md §4 B4).
```

`:1333` (`hii/gates: plan_flip/plan_gates/apply_flips без bounds-guard'ов`):
```markdown
  В цикле hii-write-guard (спека
  docs/superpowers/specs/2026-09-25-hii-write-guard-design.md §2 B2;
  apply_flips уже guarded — правится только план-фаза).
```

`:3257` (`rpc/hii_question_add: нет откката при сбое середины цикла`):
```markdown
  В цикле hii-write-guard (спека
  docs/superpowers/specs/2026-09-25-hii-write-guard-design.md §3 B3;
  решение владельца — snapshot-rollback, не plan-all-then-apply).
```

`:3767` (`uefi-engine [minor]: кросс-фаза unlock между донорами не атомарна`):
```markdown
  В цикле hii-write-guard (спека
  docs/superpowers/specs/2026-09-25-hii-write-guard-design.md §3 B3;
  решение владельца — snapshot-rollback, не двухпроходный план).
```

- [ ] **Step 2: Закрыть устаревшие 2946/2952 (insert_strings_at_ids*)**

Запись `:2963` (`string_pack: shrink-путь insert_strings_at_ids_in_resource`) — заменить целиком на:
```markdown
* [x] **string_pack: shrink-путь `insert_strings_at_ids_in_resource`** —
  функции insert_strings_at_ids* удалены из кода; живой остаток один:
  `plan_rsrc_blob_growth` возвращает None, если после HII-blob следует
  любой leaf ресурса. Переформулировано в refusal-семантику: отказ роста
  безопасен (невалидный рост не применяется), задокументирован как
  known-behavior (спека hii-write-guard §6). Закрыто docs-коммитом
  цикла hii-write-guard.
```

Запись `:2969` (`string_pack: span-арифметика insert_strings_at_ids`) — заменить `[ ]` на `[x]` и дописать в конец:
```markdown
  Закрыто: устаревшее — функции insert_strings_at_ids* удалены из кода;
  родной пункт об исчерпании 0xFFFF живёт отдельной записью выше
  (цикл hii-write-guard §1 B1).
```

- [ ] **Step 3: Проверить формат и закоммитить**

```bash
cargo fmt --all -- --check
git add TODO.md
git commit -m "docs(todo): hii-write-guard — устаревшие 2946/2952 закрыты, 428/1319/3240/431/3744 помечены в цикл"
```

Expected: fmt без изменений (docs-only), коммит в master.

---

### Task 2: B1(writer) — string-id exhaustion: checked-арифметика + `HiiError::StringIdExhausted`

**Files:**
- Create: ветка `hii-write-guard`
- Modify: `crates/uefi-engine/src/hii/string_pack.rs` (`scan_sibt` `:179`, `add_strings_to_body` `:46`, `add_strings` `:30`, `AddStringsToResourceError` `:60`, `add_strings_to_resource` `:83`, существующие тесты `:367-403`)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`HiiError` `:45`, потребители `:1452`, `:1625`, `:2019`, `:2174`)
- Modify: `crates/uefi-engine/src/hii/form_add.rs:131`, `form_hijack.rs:252`, `formset_add.rs:93`
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_error_status` `:54`, тесты `:1553`)
- Test: те же файлы (unit-тесты в `mod tests`)

**Interfaces:**
- Consumes: `make_string_package`/`make_image`/`res_list`/`pkg` (string_pack tests), `scan_sibt` (private), `string_info_offset` (private).
- Produces:
  - `fn scan_sibt(body: &[u8], start: usize) -> Option<(u16, usize)>` — None = walk существующего пакета переполнил u16.
  - `fn add_strings_to_body(body: &mut Vec<u8>, strings: &[String]) -> Result<HashMap<String, u16>, StringIdExhausted>`, где `struct StringIdExhausted { next: u16, requested: usize }` (private в string_pack.rs). Err ⇒ тело пакета байт-в-байт не тронуто.
  - `HiiError::StringIdExhausted { next: u16 }` (Display: `string id space exhausted at {next}`).
  - `AddStringsToResourceError::IdExhausted { next: u16 }`.
  - RPC: `StringIdExhausted` → `Status::resource_exhausted`.

- [ ] **Step 1: Создать ветку**

```bash
git checkout -b hii-write-guard
```

- [ ] **Step 2: Написать failing-тесты (string_pack.rs, в `mod tests`)**

```rust
    #[test]
    fn add_strings_to_body_refuses_when_ids_exceed_0xfffe() {
        let mut pkg = make_string_package(&[]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0xFD, 0xFF]);
        let snapshot = pkg.clone();
        let err = add_strings_to_body(&mut pkg, &["a".to_string(), "b".to_string()])
            .unwrap_err();
        assert_eq!(err.next, 0xFFFE);
        assert_eq!(err.requested, 2);
        assert_eq!(pkg, snapshot, "отказ исчерпания не должен трогать пакет");
    }

    #[test]
    fn add_strings_to_body_allows_the_last_valid_id() {
        let mut pkg = make_string_package(&[]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0xFD, 0xFF]);
        let mapping = add_strings_to_body(&mut pkg, &["only".to_string()]).unwrap();
        assert_eq!(mapping["only"], 0xFFFE);
        assert_eq!(*pkg.last().unwrap(), SIBT_END);
    }

    #[test]
    fn add_strings_to_body_refuses_at_invalid_0xffff_next() {
        let mut pkg = make_string_package(&[]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0xFE, 0xFF]);
        let snapshot = pkg.clone();
        assert!(add_strings_to_body(&mut pkg, &["x".to_string()]).is_err());
        assert_eq!(pkg, snapshot);
    }

    #[test]
    fn add_strings_to_body_empty_slice_is_always_ok() {
        let mut pkg = make_string_package(&[]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0xFE, 0xFF]);
        let snapshot = pkg.clone();
        let mapping = add_strings_to_body(&mut pkg, &[]).unwrap();
        assert!(mapping.is_empty());
        assert_eq!(pkg, snapshot);
    }

    #[test]
    fn scan_sibt_returns_none_on_skip2_id_overflow() {
        let mut pkg = make_string_package(&[]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0xFF, 0xFF]);
        assert!(scan_sibt(&pkg, string_info_offset(&pkg)).is_none());
    }

    #[test]
    fn add_strings_maps_exhaustion_to_hii_error() {
        let mut pkg = make_string_package(&[]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0xFE, 0xFF]);
        let mut image = make_image(pkg);
        assert!(matches!(
            add_strings(&mut image, None, &["x".to_string()]),
            Err(HiiError::StringIdExhausted { next: 0xFFFF })
        ));
    }

    #[test]
    fn add_strings_to_resource_maps_exhaustion() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let mut sp = make_string_package(&[]);
        let end = sp.len() - 1;
        sp.splice(end..end, [SIBT_SKIP2, 0xFE, 0xFF]);
        let len = sp.len() as u32;
        sp[0] = (len & 0xFF) as u8;
        sp[1] = ((len >> 8) & 0xFF) as u8;
        sp[2] = ((len >> 16) & 0xFF) as u8;
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let blob = res_list(&g, &[&form, &sp]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let snapshot = pe.clone();
        assert!(matches!(
            add_strings_to_resource(&mut pe, &["x".to_string()]),
            Err(AddStringsToResourceError::IdExhausted { next: 0xFFFF })
        ));
        assert_eq!(pe, snapshot);
    }
```

(u24 пересчитывается после splice: resource-канал парсит пакет по заявленной длине, bare-каналу хватает `declared_len_sane` с заявленной ≤ фактической — там пересчёт не нужен.)

- [ ] **Step 3: Red**

```bash
cargo test -p uefi-engine string_pack
```
Expected: FAIL — `StringIdExhausted`/`IdExhausted` не существуют (ошибка компиляции), `scan_sibt` возвращает кортеж, не Option.

- [ ] **Step 4: Реализовать string_pack.rs**

4a. `scan_sibt` — заменить целиком (все `wrapping_add` → `checked_add`, `?` = отказ):

```rust
/// Walk SIBT-блока существующего пакета до SIBT_END: возвращает
/// (next_id, позицию END). None = обход переполняет u16 id-пространство —
/// данные пакета не согласованы с id-пространством, писать в него нельзя.
/// Спека hii-write-guard §1 B1.
fn scan_sibt(body: &[u8], start: usize) -> Option<(u16, usize)> {
    let mut pos = start;
    let mut next_id: u16 = 1;
    while pos < body.len() {
        match body[pos] {
            SIBT_END => return Some((next_id, pos)),
            SIBT_STRING_SCSU => {
                next_id = next_id.checked_add(1)?;
                pos = skip_scsu(body, pos + 1);
            }
            SIBT_STRING_SCSU_FONT => {
                next_id = next_id.checked_add(1)?;
                pos = skip_scsu(body, pos + 2);
            }
            SIBT_STRINGS_SCSU => {
                let (ids, p) = read_u16_count(body, pos + 1);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.checked_add(1)?;
                    pos = skip_scsu(body, pos);
                }
            }
            SIBT_STRINGS_SCSU_FONT => {
                let (ids, p) = read_u16_count(body, pos + 2);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.checked_add(1)?;
                    pos = skip_scsu(body, pos);
                }
            }
            SIBT_STRING_UCS2 => {
                next_id = next_id.checked_add(1)?;
                pos = skip_ucs2(body, pos + 1);
            }
            SIBT_STRING_UCS2_FONT => {
                next_id = next_id.checked_add(1)?;
                pos = skip_ucs2(body, pos + 2);
            }
            SIBT_STRINGS_UCS2 => {
                let (ids, p) = read_u16_count(body, pos + 1);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.checked_add(1)?;
                    pos = skip_ucs2(body, pos);
                }
            }
            SIBT_STRINGS_UCS2_FONT => {
                let (ids, p) = read_u16_count(body, pos + 2);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.checked_add(1)?;
                    pos = skip_ucs2(body, pos);
                }
            }
            SIBT_DUPLICATE => {
                next_id = next_id.checked_add(1)?;
                pos += 1 + 2;
            }
            SIBT_SKIP2 => {
                let (c, p) = read_u16_count(body, pos + 1);
                next_id = next_id.checked_add(c)?;
                pos = p;
            }
            SIBT_SKIP1 => {
                let count = body.get(pos + 1).copied().unwrap_or(0);
                next_id = next_id.checked_add(u16::from(count))?;
                pos += 2;
            }
            _ => break,
        }
    }
    Some((next_id, body.len()))
}
```

4b. `StringIdExhausted` + новая `add_strings_to_body` (заменить `:46-58`):

```rust
/// Отказ выдачи string-id: next — недоступный следующий id,
/// requested — сколько строк просили. Спека hii-write-guard §1 B1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StringIdExhausted {
    next: u16,
    requested: usize,
}

/// Добавляет строки в тело string-пакета, назначая последовательные id.
/// Контракт: id выдаются только из 1..=0xFFFE; при исчерпании или
/// нечитаемом next_id существующего пакета — Err, тело пакета не тронуто.
/// Пустой срез — байт-в-байт no-op (u24 не пересчитывается).
/// Спека hii-write-guard §1 B1.
fn add_strings_to_body(
    body: &mut Vec<u8>,
    strings: &[String],
) -> Result<HashMap<String, u16>, StringIdExhausted> {
    if strings.is_empty() {
        return Ok(HashMap::new());
    }
    let sibt_start = string_info_offset(body);
    let (mut next_id, mut end_pos) =
        scan_sibt(body, sibt_start).ok_or(StringIdExhausted {
            next: 0xFFFF,
            requested: strings.len(),
        })?;
    if !strings.is_empty() {
        let span = u16::try_from(strings.len() - 1)
            .ok()
            .and_then(|n| next_id.checked_add(n));
        match span {
            Some(last) if last <= 0xFFFE => {}
            _ => {
                return Err(StringIdExhausted {
                    next: next_id,
                    requested: strings.len(),
                })
            }
        }
    }
    let mut mapping = HashMap::new();
    for s in strings {
        let new_id = next_id;
        next_id += 1;
        mapping.insert(s.clone(), new_id);
        end_pos = append_scsu_string(body, end_pos, s);
    }
    update_package_length(body);
    Ok(mapping)
}
```

4c. `add_strings` (`:41`) — маппинг ошибки:

```rust
    let mapping = add_strings_to_body(&mut node.body, strings)
        .map_err(|e| HiiError::StringIdExhausted { next: e.next })?;
```

4d. `AddStringsToResourceError` (`:60-64`) — новый вариант:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddStringsToResourceError {
    NotFound,
    GrowthUnsupported,
    IdExhausted { next: u16 },
}
```

и вызов в `add_strings_to_resource` (`:83`):

```rust
    let mapping = add_strings_to_body(&mut grown, strings)
        .map_err(|e| AddStringsToResourceError::IdExhausted { next: e.next })?;
```

4e. Существующие прямые вызовы `add_strings_to_body` в тестах (`:367-403`, четыре штуки: `add_strings_to_body_assigns_sequential_ids`, `add_strings_to_body_writes_scsu_block_before_end`, `add_strings_to_body_updates_length`, `add_strings_skips_skip_blocks_in_id_counting`) — ко всем дописать `.unwrap()` (Result нельзя оставить неиспользованным: `-D warnings`).

- [ ] **Step 5: Реализовать HiiError + потребителей**

5a. `crates/uefi-engine/src/hii/mod.rs`, после `IdOccupied(u16)` (`:45-46`):

```rust
    #[error("string id space exhausted at {next}")]
    StringIdExhausted { next: u16 },
```

5b. В каждый из 7 match-потребителей `AddStringsToResourceError` добавить руку (позиция — после `GrowthUnsupported`):

`mod.rs:1452` (preflight), `mod.rs:1625` (add_question), `mod.rs:2019` (add_ref), `mod.rs:2174` (add_page), `form_add.rs:131`, `form_hijack.rs:252` — форма `match ... { Ok(ids) => ids, Err(...) => return Err(...) }`:

```rust
            Err(string_pack::AddStringsToResourceError::IdExhausted { next }) => {
                return Err(HiiError::StringIdExhausted { next });
            }
```

`formset_add.rs:93` — форма `.map_err(|e| match e { ... })`:

```rust
                    string_pack::AddStringsToResourceError::IdExhausted { next } => {
                        HiiError::StringIdExhausted { next }
                    }
```

- [ ] **Step 6: Green (engine)**

```bash
cargo test -p uefi-engine string_pack
cargo test -p uefi-engine
```
Expected: PASS (новые + все существующие add_strings_* — id-выдача на валидных пакетах не изменилась).

- [ ] **Step 7: Failing-тест RPC (rpc/server.rs, `mod tests`, рядом с `hii_error_status_maps_id_occupied` `:1553`)**

```rust
    #[test]
    fn hii_error_status_maps_string_id_exhausted() {
        let st = hii_error_status(crate::hii::HiiError::StringIdExhausted { next: 0xFFFE });
        assert_eq!(st.code(), tonic::Code::ResourceExhausted);
        assert!(st.message().contains("exhausted at 65534"));
    }
```

- [ ] **Step 8: Red → реализация → Green**

```bash
cargo test -p uefi-engine hii_error_status_maps_string_id
```
Expected: FAIL (`_ => internal` ловит новый вариант).

В `hii_error_status` (`:54-70`) добавить руку перед catch-all:

```rust
        crate::hii::HiiError::StringIdExhausted { .. } => {
            Status::resource_exhausted(e.to_string())
        }
```

```bash
cargo test -p uefi-engine hii_error_status_maps_string_id
```
Expected: PASS.

- [ ] **Step 9: Гейты + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/string_pack.rs crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/form_add.rs crates/uefi-engine/src/hii/form_hijack.rs crates/uefi-engine/src/hii/formset_add.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): hii-write-guard B1 — checked string-id arithmetic, HiiError::StringIdExhausted, resource_exhausted RPC"
```

---

### Task 3: B1(reader) — толерантный близнец в parse_string_package + drive-by TODO:4382

**Files:**
- Modify: `crates/uefi-engine/src/hii/strings.rs` (walk `:49-229`, `push` `:241`, SKIP-руки `:181-193`)
- Modify: `TODO.md` (запись `:4382`)
- Test: `crates/uefi-engine/src/hii/strings.rs` (`mod tests`)

**Interfaces:**
- Consumes: `parse_string_package` walk, `make_pkg` (`:458`), `#[tracing_test::traced_test]`.
- Produces: поведение reader'а — переполнение id-пространства = `tracing::warn!("string id space exhausted; stopping string parse")` + останов walk, `Some(ParsedStringPackage)` без записей с id 0; публичный API не меняется.

- [ ] **Step 1: Failing-тесты (strings.rs, `mod tests`)**

```rust
    #[test]
    fn parse_stops_on_skip2_id_overflow_without_id_zero() {
        let pkg = make_pkg("en", &[SIBT_SKIP2, 0xFF, 0xFF, SIBT_END]);
        let parsed = parse_string_package(&pkg).unwrap();
        assert!(parsed.strings.is_empty());
        assert!(parsed.strings.iter().all(|(id, _)| *id != 0));
    }

    #[test]
    fn parse_stops_when_string_block_would_pass_0xfffe() {
        let mut sibt = vec![SIBT_SKIP2, 0xFE, 0xFF];
        sibt.push(SIBT_STRING_SCSU);
        sibt.extend_from_slice(b"past");
        sibt.push(0x00);
        sibt.push(SIBT_END);
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert!(
            parsed.strings.iter().all(|(id, _)| *id != 0xFFFF),
            "блок на невалидном id 0xFFFF не листится"
        );
    }

    #[test]
    fn parse_survives_skip_exactly_to_0xffff_then_end() {
        let pkg = make_pkg("en", &[SIBT_SKIP2, 0xFE, 0xFF, SIBT_END]);
        let parsed = parse_string_package(&pkg).unwrap();
        assert!(parsed.strings.is_empty());
    }
```

Плюс warn-семантика (стиль A2 hii-read-truth):

```rust
    #[tracing_test::traced_test]
    #[test]
    fn parse_warns_on_id_exhaustion() {
        let pkg = make_pkg("en", &[SIBT_SKIP2, 0xFF, 0xFF, SIBT_END]);
        let _ = parse_string_package(&pkg);
        assert!(logs_contain("string id space exhausted"));
    }
```

(если тесты без `SIBT_*` в скоупе — константы уже в файле на `:12-26`).

- [ ] **Step 2: Red**

```bash
cargo test -p uefi-engine strings::tests::parse_
```
Expected: FAIL — `parse_stops_on_skip2_id_overflow...` видит wrap (id 0 записей может не быть, но walk молча продолжается; строгое утверждение — warn-тест падает, SKIP-wrap не останавливается).

- [ ] **Step 3: Реализация (walk `parse_string_package`)**

3a. Guard в начале цикла `while pos < body.len()` (`:49-50`), перед `match`:

```rust
    'outer: while pos < body.len() {
        if body[pos] != SIBT_END && next_id == 0xFFFF {
            tracing::warn!("string id space exhausted; stopping string parse");
            break;
        }
        match body[pos] {
```

(существующая метка `'outer:` остаётся; guard гасит string-производящие руки на входе в итерацию — но STRINGS-руки кладут `count` id за одну итерацию, поэтому дополнительно 3d.)

3b. SKIP-руки (`:181-193`) — checked:

```rust
            SIBT_SKIP2 => {
                let Some((count, p)) = read_u16(body, pos + 1) else {
                    warn_truncated_block(body[pos]);
                    break;
                };
                match next_id.checked_add(count) {
                    Some(n) => next_id = n,
                    None => {
                        tracing::warn!("string id space exhausted; stopping string parse");
                        break;
                    }
                }
                pos = p;
            }
            SIBT_SKIP1 => {
                let count = body.get(pos + 1).copied().unwrap_or(0);
                match next_id.checked_add(u16::from(count)) {
                    Some(n) => next_id = n,
                    None => {
                        tracing::warn!("string id space exhausted; stopping string parse");
                        break;
                    }
                }
                pos += 2;
            }
```

3c. Drive-by TODO:4382 — в руках `SIBT_STRINGS_SCSU` (`:73-84`) и `SIBT_STRINGS_SCSU_FONT` (`:95-107`) мёртвый let-else после guard `if p >= body.len()` заменить:

```rust
                    let (text, np) = read_scsu(body, p).expect("p < body.len() checked above");
```

(UCS2-руки НЕ трогать — их let-else жив: хвостовой байт.)

3d. Исчерпание внутри STRINGS-блоков (находка ревью Task 3): `push` (`:251`) заменить на

```rust
/// Записывает строку под next_id и инкрементирует его. false = id-пространство
/// исчерпано (next_id == 0xFFFF): запись не создана, инкремента нет.
/// Спека hii-write-guard §1 B1.
fn push(
    strings: &mut Vec<(u16, String)>,
    by_id: &mut HashMap<u16, String>,
    next_id: &mut u16,
    text: String,
) -> bool {
    if *next_id == 0xFFFF {
        return false;
    }
    by_id.insert(*next_id, text.clone());
    strings.push((*next_id, text));
    *next_id += 1;
    true
}
```

и во всех 9 call-сайтах (`:61`-`:176`, все внутри walk с меткой `'outer`):

```rust
                if !push(&mut strings, &mut by_id, &mut next_id, text) {
                    tracing::warn!("string id space exhausted; stopping string parse");
                    break 'outer;
                }
```

Тест (Step 1 дополнение):

```rust
    #[test]
    fn parse_stops_inside_strings_block_at_0xffff() {
        let mut sibt = vec![SIBT_SKIP2, 0xFD, 0xFF];
        sibt.push(SIBT_STRINGS_SCSU);
        sibt.extend_from_slice(&3u16.to_le_bytes());
        sibt.extend_from_slice(b"aa\x00bb\x00cc\x00");
        sibt.push(SIBT_END);
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1);
        assert_eq!(parsed.strings[0], (0xFFFE, "aa".to_string()));
    }
```

- [ ] **Step 4: Green**

```bash
cargo test -p uefi-engine strings
```
Expected: PASS (новые + регресс всех parse-тестов).

- [ ] **Step 5: Закрыть TODO:4382**

Запись `TODO.md:4382` (`hii/strings: мёртвый let-else в STRINGS_SCSU/SCSU_FONT телах`) — `[ ]` → `[x]`, дописать в конец:

```markdown
  Закрыто: цикл hii-write-guard Task 3 (reader-близнец B1) — let-else
  заменён на expect с инвариантом guard'а.
```

- [ ] **Step 6: Гейты + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/strings.rs TODO.md
git commit -m "feat(uefi-engine): hii-write-guard B1 reader — warn+stop на исчерпании string-id, мёртвый let-else убран (TODO:4382)"
```

---

### Task 4: B3 — атомарность apply-фаз (snapshot-rollback)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (новый `with_rollback` рядом с `node_at` `:931`; `apply_cross_formset_gates` `:491-549`; тесты)
- Modify: `crates/uefi-engine/src/hii/cross_formset.rs` (фикстуры: `donor_true_expr_pkg`, `DONOR2_FILE`, `three_file_image` в `cross_fixtures`)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_question_add` `:1156-1201`; тесты)
- Test: те же файлы

**Interfaces:**
- Consumes: `FfsNode: Clone` (types.rs `:148`), `crate::builder::build_image`, фикстуры `question_add_fixtures` (mod.rs `:2215`), `form_hijack::test_fixtures` (`FILE_GUID`, `ffs_file_bytes`, `file_sections`, `flash_with_files`, `section_bytes`), Err-путь B1 (`HiiError::StringIdExhausted` из Task 2).
- Produces:
  - `pub(crate) fn with_rollback<T>(image: &mut Image, f: impl FnOnce(&mut Image) -> Result<T, HiiError>) -> Result<T, HiiError>` — при Err дерево байт-идентично состоянию до вызова.
  - `apply_cross_formset_gates`: тот же контракт атомарности (rustdoc).
  - RPC `hii_question_add`: apply-фаза (varstores → questions → refs) целиком под rollback; при Err — исходная ошибка, частичных outcomes нет.
  - Фикстуры `cross_fixtures::{donor_true_expr_pkg, three_file_image}` (pub(crate), для будущих циклов).

- [ ] **Step 1: Failing-тест кросс-фазы (mod.rs, `mod tests`, рядом с `unlock_flips_cross_formset_gate_in_donor_section` `:3726`)**

```rust
    #[test]
    fn unlock_cross_phase_failure_rolls_back_all_donors() {
        let mut image = cross_formset::cross_fixtures::three_file_image(
            cross_formset::cross_fixtures::donor_pkg(),
            cross_formset::cross_fixtures::donor_true_expr_pkg(),
            cross_formset::cross_fixtures::target_pkg(),
        );
        let debug_before = format!("{:?}", image.root);
        let bytes_before = crate::builder::build_image(&image).unwrap();
        let err = unlock(&mut image, "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1").unwrap_err();
        assert!(matches!(err, HiiError::GateExpressionUnsupported(_)));
        assert_eq!(
            format!("{:?}", image.root),
            debug_before,
            "кросс-фаза с поздним отказом должна откатить и раннего донора: ни флипов, ни rebuild-меток"
        );
        assert_eq!(crate::builder::build_image(&image).unwrap(), bytes_before);
    }
```

- [ ] **Step 2: Failing-тест RPC (rpc/server.rs, `mod tests`, рядом с `hii_question_add_list_failure_applies_nothing` `:2294`)**

Хелпер + тест:

```rust
    fn near_exhausted_string_package(next_id: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, crate::hii::string_pack::PACKAGE_STRINGS]);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.push(0x21);
        buf.extend_from_slice(&(next_id - 1).to_le_bytes());
        buf.push(0x00);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    #[tokio::test]
    async fn hii_question_add_rolls_back_apply_phase_failure() {
        use crate::hii::form_hijack::test_fixtures::{
            FILE_GUID, ffs_file_bytes, file_sections, flash_with_files, section_bytes,
        };
        use crate::hii::question_add_fixtures::{
            question_add_forms_pkg, question_add_spf_body_for, sd_file_direct,
        };
        let pkg = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&pkg);
        let setup = ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(crate::ffs::EFI_SECTION_RAW, &pkg),
                section_bytes(
                    crate::ffs::EFI_SECTION_RAW,
                    &near_exhausted_string_package(0xFFFB),
                ),
            ]),
        );
        let sd = sd_file_direct(&spf_body);
        let flash = flash_with_files(vec![setup, sd]);
        let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let debug_before = format!("{:?}", img.root);
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let images = Arc::new(Mutex::new(HashMap::from([("i".to_string(), img)])));
        let server = EngineServer {
            sm,
            images: images.clone(),
            data_dir: td.path().to_path_buf(),
        };
        const TARGET: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#10019";
        const TWO_QUESTIONS: &str = r#"{"questions": [
            {"form_id": 10019, "prompt": "A", "help": "AH", "question_id": 512,
             "var_store_id": 1, "var_offset": 128, "size": 1,
             "options": [{"text": "Off", "value": 0}, {"text": "On", "value": 1, "default": "optimized"}]},
            {"form_id": 10019, "prompt": "B", "help": "BH", "question_id": 513,
             "var_store_id": 1, "var_offset": 129, "size": 1,
             "options": [{"text": "Off2", "value": 0}, {"text": "On2", "value": 1, "default": "optimized"}]}
        ]}"#;
        let st = server
            .hii_question_add(Request::new(HiiQuestionAddRequest {
                image_id: "i".into(),
                target: TARGET.into(),
                schema_json: TWO_QUESTIONS.into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(st.code(), tonic::Code::ResourceExhausted);
        let img_ref = images.lock().await;
        let img_ref = img_ref.get("i").unwrap();
        assert_eq!(
            crate::builder::build_image(img_ref).unwrap(),
            flash,
            "apply-фаза с серединным отказом должна откатить дерево байт-в-байт"
        );
        assert_eq!(format!("{:?}", img_ref.root), debug_before);
    }
```

(1-й вопрос получает id 0xFFFB..0xFFFE — валидно; 2-й упирается в next=0xFFFF → Err B1 → триггер атомарности.)

- [ ] **Step 3: Red**

```bash
cargo test -p uefi-engine unlock_cross_phase_failure_rolls_back_all_donors
cargo test -p uefi-engine hii_question_add_rolls_back_apply_phase_failure
```
Expected: FAIL — фикстур `three_file_image`/`donor_true_expr_pkg` нет (ошибка компиляции); без них (если исполнитель добавит только тесты) — частичная мутация первого донора остаётся / RPC оставляет полусостояние.

- [ ] **Step 4: Фикстуры (cross_formset.rs, `cross_fixtures`; после `two_file_image_with` `:287`)**

```rust
    /// Донор с не-E12 выражением (TRUE) вокруг REF3 → RC_SET#1:
    /// plan_flip даёт Ok(None) → кросс-фаза падает GateExpressionUnsupported.
    /// Фикстура атомарности B3 (спека hii-write-guard §3).
    pub(crate) fn donor_true_expr_pkg() -> Vec<u8> {
        let mut ifr = opcode(IFR_FORM_SET_OP, true, &[0u8; 21]);
        ifr.extend(opcode(
            IFR_FORM_OP,
            true,
            &[10001u16.to_le_bytes(), 20u16.to_le_bytes()].concat(),
        ));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(vec![r_efi::hii::IFR_TRUE_OP, 0x02]);
        ifr.extend(ref3(1));
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        package(&ifr)
    }

    pub(crate) const DONOR2_FILE: &str = "A22A2A2A-1212-4C4C-9A9A-2E2E2E2E2E2E";

    /// Образ с двумя донорами и таргетом (порядок обхода = порядок детей):
    /// донор1 флипаемый, донор2 нет. Фикстура атомарности B3.
    pub(crate) fn three_file_image(donor1: Vec<u8>, donor2: Vec<u8>, target: Vec<u8>) -> Image {
        let mk_file = |guid: &str, body: Vec<u8>| {
            mk_node(
                Some(Guid::from_str(guid).unwrap()),
                FfsType::File,
                0x07,
                vec![],
                vec![mk_node(None, FfsType::Section, 0x19, body, vec![])],
            )
        };
        let volume = mk_node(
            None,
            FfsType::Volume,
            0,
            vec![],
            vec![
                mk_file(SETUP_FILE, donor1),
                mk_file(DONOR2_FILE, donor2),
                mk_file(TARGET_FILE, target),
            ],
        );
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }
```

- [ ] **Step 5: Хелпер `with_rollback` (mod.rs, рядом с `node_at` `:931`)**

```rust
/// Apply-фаза с snapshot-rollback: при Err дерево байт-идентично
/// состоянию до вызова (deepcopy FfsNode до первой мутации).
/// Спека hii-write-guard §3 B3.
pub(crate) fn with_rollback<T>(
    image: &mut Image,
    f: impl FnOnce(&mut Image) -> Result<T, HiiError>,
) -> Result<T, HiiError> {
    let snapshot = image.root.clone();
    match f(image) {
        Ok(v) => Ok(v),
        Err(e) => {
            image.root = snapshot;
            Err(e)
        }
    }
}
```

- [ ] **Step 6: `apply_cross_formset_gates` атомарна (mod.rs `:491-549`)**

Заменить тело после `let sites = cross_formset::find_cross_gates(...)` (`:515`) — цикл по сайтам обернуть в rollback (ранние `return Ok(false)` до снапшота не трогаем):

```rust
    let sites = cross_formset::find_cross_gates(image, &skip_path, &gt_cross);
    let snapshot = image.root.clone();
    let mut any = false;
    let result = (|| -> Result<(), HiiError> {
        for site in &sites {
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
        Ok(())
    })();
    match result {
        Ok(()) => Ok(any),
        Err(e) => {
            image.root = snapshot;
            Err(e)
        }
    }
```

И обновить rustdoc функции (`:486-490`), дописав строку: `/// Атомарна: при Err дерево байт-идентично состоянию до вызова (snapshot-rollback; спека hii-write-guard §3 B3).`

- [ ] **Step 7: RPC `hii_question_add` под rollback (rpc/server.rs `:1156-1201`)**

Блок `let mut images = self.images.lock().await;` переписать: check-фазы как были, apply — одним замыканием (отложенная инициализация `outcomes`/`ref_outcomes` — без пустых Vec, иначе `unused_assignments` под `-D warnings`):

```rust
        let (outcomes, ref_outcomes);
        {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            crate::hii::check_question_add(
                img_slot,
                &r.target,
                &schema.questions,
                &schema.varstores,
            )
            .map_err(|e| hii_error_status_ctx(e, &r.target))?;
            let question_qids: Vec<u16> = schema.questions.iter().map(|q| q.question_id).collect();
            crate::hii::check_ref_add(img_slot, &r.target, &schema.refs, &question_qids)
                .map_err(|e| hii_error_status_ctx(e, &r.target))?;
            let applied = crate::hii::with_rollback(img_slot, |img| {
                let mut outcomes = Vec::with_capacity(schema.questions.len());
                let mut ref_outcomes = Vec::with_capacity(schema.refs.len());
                if !schema.varstores.is_empty() {
                    crate::hii::add_varstores(img, &r.target, &schema.varstores)?;
                }
                for q in &schema.questions {
                    let result = crate::hii::add_question(img, &r.target, q)?;
                    outcomes.push(HiiQuestionAddOutcome {
                        question_id: u32::from(result.question_id),
                        string_ids: result
                            .string_ids
                            .into_iter()
                            .map(|(k, v)| (k, u32::from(v)))
                            .collect(),
                        spf_record_offset: result.spf_record_offset as u32,
                    });
                }
                for rf in &schema.refs {
                    let result = crate::hii::add_ref(img, &r.target, rf)?;
                    ref_outcomes.push(HiiQuestionAddOutcome {
                        question_id: u32::from(result.question_id),
                        string_ids: result
                            .string_ids
                            .into_iter()
                            .map(|(k, v)| (k, u32::from(v)))
                            .collect(),
                        spf_record_offset: 0,
                    });
                }
                Ok((outcomes, ref_outcomes))
            })
            .map_err(|e| hii_error_status_ctx(e, &r.target))?;
            (outcomes, ref_outcomes) = applied;
        }
```

(`flush_image` остаётся после блока — на Err-пути не выполняется.)

Добавить rustdoc-контракт над методом `hii_question_add` (перед `#[tracing::instrument]`):

```rust
    /// Apply-фаза атомарна: при Err дерево байт-идентично состоянию до
    /// вызова (snapshot-rollback; спека hii-write-guard §3 B3).
```

- [ ] **Step 8: Green + регресс**

```bash
cargo test -p uefi-engine unlock_cross_phase_failure_rolls_back_all_donors
cargo test -p uefi-engine hii_question_add_rolls_back_apply_phase_failure
cargo test -p uefi-engine
```
Expected: PASS, включая happy-path (`hii_question_add_processes_refs_only_schema`, `unlock_flips_cross_formset_gate_in_donor_section`, `hii_question_add_list_failure_applies_nothing`).

- [ ] **Step 9: Гейты + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/cross_formset.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): hii-write-guard B3 — snapshot-rollback apply-фаз question_add и кросс-unlock"
```

- [ ] **Step 10 (финальное ревью B3): `unlock` атомарна целиком — own-фаза + кросс-фаза под общим rollback**

Находка финального ревью: own-фаза `unlock` (`:416-470`) мутирует дерево (флипы + rebuild-метки) ДО снапшота `apply_cross_formset_gates` (`:471`, снапшот на входе функции). Если own-гейты применились, а кросс-донор упал — Err с частичной own-мутацией в дереве. Спека §3:146 («Err = ничего не применено») этого не допускает.

10a. Failing-тест (mod.rs, `mod tests`, рядом с `unlock_cross_phase_failure_rolls_back_all_donors`): образ `three_file_image(donor_pkg(), donor_true_expr_pkg(), target_with_own_gate_pkg())` — таргет с собственным флипаемым гейтом на вопросе (иначе own-фаза ничего не мутирует и окно не тестируется); `unlock(...)` → `Err(GateExpressionUnsupported)`; дерево (debug) и `build_image`-байты == до вызова. При необходимости — новая фикстура `cross_fixtures::target_with_own_gate_pkg()` (по образцу `target_pkg`, + suppress-if EqConst на вопросе).

10b. Реализация: тело `unlock` после `resolve_writable_path` (`:408`) обернуть в `with_rollback(image, |image| { ... })` (хелпер Step 5; снапшот до первой мутации); rustdoc `unlock` дополнить строкой атомарности (как у `apply_cross_formset_gates`). Внутренний снапшот `apply_cross_formset_gates` остаётся (самостоятельный контракт функции).

Сопутствующий дефект (выявлен при реализации Step 10): тест `unlock_marks_own_rebuild_when_cross_donor_behind_compression` (Task 4, ранний шаг) кодифицировал старую семантику частичной мутации («own-флипы применены до отказа донора», rebuild-метка на own-пути) — противоречит §3:146 при новом контракте целостной атомарности. Тест обновить: `Err(MutationBehindCompression)` сохраняется, но own-тело байт-идентично исходному и `action == NoAction`; переименовать в `unlock_rolls_back_own_phase_when_cross_donor_behind_compression`.

10c. Гейты + коммит:

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git commit -m "fix(uefi-engine): hii-write-guard B3 — unlock атомарна целиком: own-фаза и кросс-фаза под общим snapshot-rollback (финальное ревью)"
```


---

### Task 5: B2 — bounds-guard'ы план-фазы гейтов

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs` (`plan_flip` `:308-332`, `plan_gates` `:334`, `plan_gates_skip_unlocked` `:366`, тесты — в т.ч. `:1022`)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`gate_info` `:316-318`; тест)
- Test: те же файлы

**Interfaces:**
- Consumes: `Gate`/`GateKind`/`Wraps`/`GateExpr` (gates.rs), `pkg_off` (mod.rs `:303`), фикстуры `vendor_ifr`/`package`/`find_gates` (gates.rs tests).
- Produces:
  - `pub(crate) fn plan_flip(body: &[u8], gate: &Gate) -> Result<Option<PlannedFlip>, String>` — Err = границы пакета нарушены (`"gate bounds out of package at pkg+{expr_offset:#x}"`), Ok(None) = «не hardware-validated класс» (без изменений семантики).
  - `plan_gates`/`plan_gates_skip_unlocked` пробрасывают Err; Err-сообщения берут регион через `body.get(..)`.
  - `gate_info` (mod.rs) на out-of-bounds Gate деградирует: `flippable: false`, пустой регион (без паники).

- [ ] **Step 1: Failing-тесты (gates.rs, `mod tests`)**

```rust
    fn hand_gate(expr_offset: usize, expr_end: usize, expr: GateExpr) -> Gate {
        Gate {
            kind: GateKind::Suppress,
            wraps: Wraps::Form { form_id: 901 },
            scope_offset: 4,
            expr_offset,
            expr_end,
            expr,
        }
    }

    #[test]
    fn plan_flip_errs_when_expr_offset_beyond_body() {
        let pkg = package(&vendor_ifr());
        let gate = hand_gate(pkg.len() + 10, pkg.len() + 40, GateExpr::EqConst { a: 1, b: 1 });
        let err = plan_flip(&pkg, &gate).unwrap_err();
        assert!(err.contains("gate bounds out of package"), "{err}");
        assert!(plan_gates(&pkg, &[gate]).is_err());
    }

    #[test]
    fn plan_flip_errs_when_first_len_pushes_literal_out() {
        let mut pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        let gate = gates[0].clone();
        pkg[gate.expr_offset + 1] = 0x7F;
        let err = plan_flip(&pkg, &gate).unwrap_err();
        assert!(err.contains("gate bounds out of package"), "{err}");
    }

    #[test]
    fn plan_flip_errs_when_eq_id_val_literal_out() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        let body = &pkg[..gates[0].expr_offset + 4];
        let err = plan_flip(body, &gates[0]).unwrap_err();
        assert!(err.contains("gate bounds out of package"), "{err}");
        assert!(plan_gates(body, &gates).is_err());
    }

    #[test]
    fn plan_flip_errs_when_region_inverted() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        let mut gate = gates[0].clone();
        gate.expr_end = gate.expr_offset;
        let err = plan_flip(&pkg, &gate).unwrap_err();
        assert!(err.contains("gate bounds out of package"), "{err}");
    }
```

Тест деградации gate_info (mod.rs, `mod tests`):

```rust
    #[test]
    fn gate_info_degrades_out_of_bounds_gate_to_unflippable() {
        let pkg = vec![0u8; 8];
        let gate = gates::Gate {
            kind: gates::GateKind::Suppress,
            wraps: gates::Wraps::Form { form_id: 901 },
            scope_offset: 0,
            expr_offset: 100,
            expr_end: 104,
            expr: gates::GateExpr::EqConst { a: 1, b: 1 },
        };
        let gi = gate_info(&pkg, &gate);
        assert!(!gi.flippable);
    }
```

- [ ] **Step 2: Red**

```bash
cargo test -p uefi-engine plan_flip_errs
cargo test -p uefi-engine gate_info_degrades
```
Expected: FAIL — текущий `plan_flip` паникует (`index out of bounds`) или тест не компилируется (сигнатура Option).

- [ ] **Step 3: Реализация gates.rs**

3a. `plan_flip` — заменить целиком (`:308-332`):

```rust
/// План флипа одного гейта. Ok(None) — выражение не hardware-validated
/// класс (EqConst с a!=b, EqIdVal c 0xFFFF, True, Other). Err — границы
/// гейта выходят за тело пакета: не паникует ни на каком Gate, в т.ч.
/// сконструированном вручную. Спека hii-write-guard §2 B2.
pub(crate) fn plan_flip(body: &[u8], gate: &Gate) -> Result<Option<PlannedFlip>, String> {
    let bounds_err = || {
        format!(
            "gate bounds out of package at {}",
            super::pkg_off(gate.expr_offset)
        )
    };
    if gate
        .expr_offset
        .checked_add(2)
        .is_none_or(|e| e > gate.expr_end.min(body.len()))
    {
        return Err(bounds_err());
    }
    match gate.expr {
        GateExpr::EqConst { a, b } if a == b => {
            let first_len = usize::from(body[gate.expr_offset + 1] & 0x7F);
            let Some(offset) = gate
                .expr_offset
                .checked_add(first_len)
                .and_then(|o| o.checked_add(2))
            else {
                return Err(bounds_err());
            };
            let Some(&from) = body.get(offset) else {
                return Err(bounds_err());
            };
            Ok(Some(PlannedFlip {
                offset,
                from: vec![from],
                to: vec![from.wrapping_add(1)],
            }))
        }
        GateExpr::EqIdVal { question_id, value } if value != 0xFFFF => {
            if question_storage_width(body, question_id).is_some_and(|w| w > 1) {
                return Ok(None);
            }
            if gate
                .expr_offset
                .checked_add(6)
                .is_none_or(|e| e > body.len())
            {
                return Err(bounds_err());
            }
            Ok(Some(PlannedFlip {
                offset: gate.expr_offset + 4,
                from: value.to_le_bytes().to_vec(),
                to: 0xFFFFu16.to_le_bytes().to_vec(),
            }))
        }
        _ => Ok(None),
    }
}
```

3b. `plan_gates` (`:334-356`) и `plan_gates_skip_unlocked` (`:366-391`) — в каждой заменить match-руку None-ветки и проброс Err; регион через `get`:

```rust
        match plan_flip(body, gate) {
            Ok(Some(flip)) => flips.push(flip),
            Ok(None) => {
                let region = body
                    .get(gate.expr_offset..gate.expr_end.min(body.len()))
                    .unwrap_or(&[]);
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
            }
            Err(e) => return Err(e),
        }
```

3c. Существующий тест `:1022` (`plan_eq_id_val_with_ffff_value_is_not_a_flip`): `assert!(plan_flip(&pkg, &gates[0]).is_none());` → `assert_eq!(plan_flip(&pkg, &gates[0]), Ok(None));`.

3d. `gate_info` (mod.rs `:316-318`):

```rust
    let region = pkg
        .get(gate.expr_offset..gate.expr_end.min(pkg.len()))
        .unwrap_or(&[]);
    let flip = gates::plan_flip(pkg, gate).unwrap_or(None);
```

и rustdoc на `gate_info`: `/// Out-of-bounds Gate деградирует в flippable=false (read-only дисплей; спека hii-write-guard §2 B2).`

- [ ] **Step 4: Green + регресс pinned-офсетов**

```bash
cargo test -p uefi-engine gates
cargo test -p uefi-engine
```
Expected: PASS. Регресс pinned-офсетов: `plan_eq_const_flip_targets_second_operand_lsb`, `plan_eq_id_val_flip_rewrites_value_to_unreachable`, `unlock_resource_channel_changes_exactly_flip_bytes`, `gates_and_unlock_print_pkg_relative_offsets_in_second_package` — офсеты флипов не изменились.

- [ ] **Step 5: Гейты + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/gates.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): hii-write-guard B2 — bounds-guard'ы plan_flip/plan_gates, gate_info без паник"
```

---

### Task 6: B4 — единый резолвер .rsrc-секции с отказом при неоднозначности

**Files:**
- Modify: `crates/uefi-engine/src/hii/pe_resource.rs` (новый `RsrcSpan` + `resolve_rsrc_section`; `rsrc_grow_plan` `:289-300`; `rsrc_raw_end` `:629-651`; `rsrc_virt_end` `:653-674`; вызовы `:511-512`, `:604`; тесты)
- Test: тот же файл

**Interfaces:**
- Consumes: `PeFile64`/`PeFile32`/`PeFile`/`ImageNtHeaders`, `LittleEndian`, `read_le_u32`/`read_le_u16` (`:440-450`), тест-хелперы `pkg`/`string_pkg`/`list`/`LIST_GUID`/`synth_hii_pe`.
- Produces:
  - `struct RsrcSpan { header_off: usize, va: u32, vsize: u32, raw_size: u32, raw_ptr: u32 }` (private).
  - `fn resolve_rsrc_section<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>, pe: &[u8]) -> Option<RsrcSpan>` — max-множество (`rsrc_rva - va < max(vsize, raw)`) строго одноэлементно, иначе `warn "ambiguous .rsrc section mapping"` + None; min-зеркало object (`rsrc_rva - va < min(vsize, raw)`) при непустоте не должно выбирать другую секцию (расхождение каналов → warn + None); пустое min-множество (rva в slack-хвосте) — легально.
  - `rsrc_raw_end`/`rsrc_virt_end` получают параметр `pe: &[u8]` и идут через резолвер.
  - `push_leaf` (`:117`) остаётся на object — каналы теперь согласованы или операция отказана.

Примечание исполнителю: min-множество ⊆ max-множество (min ≤ max), поэтому после max-уникальности min-расхождение формально недостижимо; проверка сохранена как spec-mandated зеркало object-семантики (защита от будущих расхождений предикатов). Тесты 1 и 2 оба попадают в refusal неоднозначности — это ожидаемо.

- [ ] **Step 1: Failing-тесты (pe_resource.rs, `mod tests`)**

Хелперы (рядом с существующими):

```rust
    fn section_hdr(name: &str, vsize: u32, va: u32, raw: u32, raw_ptr: u32) -> [u8; 40] {
        let mut h = [0u8; 40];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[8..12].copy_from_slice(&vsize.to_le_bytes());
        h[12..16].copy_from_slice(&va.to_le_bytes());
        h[16..20].copy_from_slice(&raw.to_le_bytes());
        h[20..24].copy_from_slice(&raw_ptr.to_le_bytes());
        h
    }

    fn pe_with_second_section(base: Vec<u8>, hdr: &[u8; 40]) -> Vec<u8> {
        let mut out = Vec::with_capacity(base.len() + 40);
        out.extend_from_slice(&base[..0x170]);
        out.extend_from_slice(hdr);
        out.extend_from_slice(&base[0x170..]);
        out[0x46..0x48].copy_from_slice(&2u16.to_le_bytes());
        let rsrc_len = (base.len() - 0x170) as u32;
        out[0x158..0x15c].copy_from_slice(&rsrc_len.to_le_bytes());
        out[0x15c..0x160].copy_from_slice(&0x198u32.to_le_bytes());
        out
    }

    fn hii_base_pe() -> Vec<u8> {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let blob = list(&g, &[&string_pkg(&["first"])]);
        synth_hii_pe("HII", &blob)
    }
```

Тесты:

```rust
    #[test]
    fn resolve_rsrc_section_refuses_overlapping_candidates() {
        let pe = pe_with_second_section(
            hii_base_pe(),
            &section_hdr(".foo", 0x100, 0x1000, 0x100, 0x198),
        );
        let file = PeFile64::parse(&pe[..]).unwrap();
        assert!(resolve_rsrc_section(&file, &pe).is_none());
        assert!(rsrc_raw_end(&file, &pe).is_none());
        assert!(rsrc_virt_end(&file, &pe).is_none());
        assert!(plan_rsrc_blob_growth(&pe, 8).is_none());
    }

    #[test]
    fn resolve_rsrc_section_refuses_neighbor_vsize_overlap() {
        let mut base = hii_base_pe();
        let vsize = (base.len() - 0x170) as u32 + 0x2000;
        base[0x150..0x154].copy_from_slice(&vsize.to_le_bytes());
        base[0xd8..0xdc].copy_from_slice(&0x2800u32.to_le_bytes());
        let pe = pe_with_second_section(base, &section_hdr(".bar", 0x200, 0x2700, 0x200, 0x198));
        let file = PeFile64::parse(&pe[..]).unwrap();
        assert!(resolve_rsrc_section(&file, &pe).is_none());
        assert!(rsrc_raw_end(&file, &pe).is_none());
    }

    #[test]
    fn resolve_rsrc_section_allows_slack_tail_rva() {
        let mut base = hii_base_pe();
        let raw_len = (base.len() - 0x170) as u32;
        base[0x150..0x154].copy_from_slice(&(raw_len + 0x800).to_le_bytes());
        base[0xd8..0xdc].copy_from_slice(&(0x1000 + raw_len).to_le_bytes());
        let file = PeFile64::parse(&base[..]).unwrap();
        let span = resolve_rsrc_section(&file, &base).unwrap();
        assert_eq!(span.raw_ptr, 0x170);
        assert_eq!(span.raw_size, raw_len);
        assert_eq!(rsrc_raw_end(&file, &base), Some(0x170 + u64::from(raw_len)));
        assert_eq!(
            rsrc_virt_end(&file, &base),
            Some(0x1000 + u64::from(raw_len) + 0x800)
        );
    }
```

- [ ] **Step 2: Red**

```bash
cargo test -p uefi-engine resolve_rsrc_section
```
Expected: FAIL — `resolve_rsrc_section` не существует (ошибка компиляции).

- [ ] **Step 3: Реализация**

3a. Резолвер (разместить перед `rsrc_raw_end` `:629`):

```rust
struct RsrcSpan {
    header_off: usize,
    va: u32,
    vsize: u32,
    raw_size: u32,
    raw_ptr: u32,
}

/// Секция-носитель .rsrc одним резолвером для всех каналов мутации
/// (rsrc_grow_plan / rsrc_raw_end / rsrc_virt_end). max-множество
/// (rsrc_rva - va < max(vsize, raw), сегодняшний предикат — покрывает
/// vsize>raw slack, сужать нельзя) обязано быть строго одноэлементным;
/// min-зеркало object::pe_file_range_at (rsrc_rva - va < min(vsize, raw))
/// при непустоте не должно выбирать другую секцию. Любая неоднозначность —
/// warn + None, не first-match. push_leaf остаётся на object: каналы либо
/// согласованы, либо операция отказана. Спека hii-write-guard §4 B4.
fn resolve_rsrc_section<Pe: ImageNtHeaders>(
    file: &PeFile<'_, Pe>,
    pe: &[u8],
) -> Option<RsrcSpan> {
    let rsrc_dir = file
        .data_directories()
        .get(object::pe::IMAGE_DIRECTORY_ENTRY_RESOURCE)?;
    let (rsrc_rva, _) = rsrc_dir.address_range();
    if rsrc_rva == 0 {
        return None;
    }
    let pe_off = read_le_u32(pe, 0x3c)? as usize;
    let opt_off = pe_off.checked_add(24)?;
    let opt_size = read_le_u16(pe, pe_off.checked_add(20)?)? as usize;
    let table_off = opt_off.checked_add(opt_size)?;
    let sections: Vec<(usize, u32, u32, u32, u32)> = file
        .section_table()
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            (
                table_off + idx * 40,
                s.virtual_address.get(LittleEndian),
                s.virtual_size.get(LittleEndian),
                s.size_of_raw_data.get(LittleEndian),
                s.pointer_to_raw_data.get(LittleEndian),
            )
        })
        .collect();
    let in_span = |va: u32, size: u32| {
        rsrc_rva.checked_sub(va).is_some_and(|off| off < size)
    };
    let max_set: Vec<usize> = sections
        .iter()
        .enumerate()
        .filter(|(_, s)| in_span(s.1, s.2.max(s.3)))
        .map(|(i, _)| i)
        .collect();
    if max_set.len() != 1 {
        tracing::warn!(candidates = max_set.len(), "ambiguous .rsrc section mapping");
        return None;
    }
    let chosen = max_set[0];
    let min_set: Vec<usize> = sections
        .iter()
        .enumerate()
        .filter(|(_, s)| in_span(s.1, s.2.min(s.3)))
        .map(|(i, _)| i)
        .collect();
    if let Some(&m) = min_set.first()
        && m != chosen
    {
        tracing::warn!("rsrc section mapping diverges from object min-semantics");
        return None;
    }
    let (header_off, va, vsize, raw_size, raw_ptr) = sections[chosen];
    Some(RsrcSpan {
        header_off,
        va,
        vsize,
        raw_size,
        raw_ptr,
    })
}
```

3b. `rsrc_grow_plan` — заменить first-match выбор секции (строки `:289-300`, от `let mut rsrc_idx = None;` по `let (section_off, rsrc_va, rsrc_vsize, rsrc_raw_size, rsrc_raw_ptr) = sections[rsrc_idx];`) на:

```rust
    let rsrc = resolve_rsrc_section(file, pe)?;
    let rsrc_idx = sections
        .iter()
        .position(|s| s.0 == rsrc.header_off)?;
    let (section_off, rsrc_va, rsrc_vsize, rsrc_raw_size, rsrc_raw_ptr) =
        (rsrc.header_off, rsrc.va, rsrc.vsize, rsrc.raw_size, rsrc.raw_ptr);
```

(остальной код функции не меняется — `sections` и `rsrc_idx` остаются в употреблении ниже по функции).

3c. `rsrc_raw_end` (`:629-651`) и `rsrc_virt_end` (`:653-674`) — заменить целиком:

```rust
fn rsrc_raw_end<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>, pe: &[u8]) -> Option<u64> {
    let s = resolve_rsrc_section(file, pe)?;
    u64::from(s.raw_ptr).checked_add(u64::from(s.raw_size))
}

fn rsrc_virt_end<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>, pe: &[u8]) -> Option<u64> {
    let s = resolve_rsrc_section(file, pe)?;
    u64::from(s.va).checked_add(u64::from(s.vsize))
}
```

3d. Вызовы: `plan_rsrc_blob_growth_for` (`:511-512`) и `append_plan_for` (`:604`) — добавить `pe`:

```rust
    let raw_end = rsrc_raw_end(file, pe)?;
    let virt_end = rsrc_virt_end(file, pe)?;
```

и в `append_plan_for` (`:604`):

```rust
    let insert_tail = usize::try_from(rsrc_raw_end(file, pe)?)
```

- [ ] **Step 4: Green + регресс**

```bash
cargo test -p uefi-engine resolve_rsrc_section
cargo test -p uefi-engine pe_resource
cargo test -p uefi-engine
```
Expected: PASS — включая все существующие grow/append/reloc-тесты (однозначные образы резолвера не замечают).

- [ ] **Step 5: Гейты + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/pe_resource.rs
git commit -m "feat(uefi-engine): hii-write-guard B4 — resolve_rsrc_section: отказ при неоднозначной .rsrc-секции"
```

---

### Task 7: Финальные гейты + закрывающий docs-коммит

**Files:**
- Modify: `TODO.md` (закрытие `:442`, `:445`, `:1333`, `:3257`, `:3767`)

**Interfaces:** — (проверки и docs)

- [ ] **Step 1: Полный гейт с --all-targets (урок TODO:3443)**

```bash
cargo test -p uefi-engine --all-targets && cargo clippy -p uefi-engine --all-targets -- -D warnings && cargo fmt --all -- --check
```
Expected: PASS без warnings.

- [ ] **Step 2: Real-image регрессия (если образ есть)**

```bash
ls ../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin 2>/dev/null && cargo test -p uefi-engine --test real_image -- --ignored || echo "fw absent — skip (помечено в журнале цикла)"
```
Expected: PASS — все resource/strings гейты зелёные (однозначный .rsrc-случай и валидные id-пространства реальных образов изменений не замечают; HNX-счётчики форм/строк из цикла hii-read-truth не сдвинулись).

- [ ] **Step 3: Smoke полного цикла на живом образе (если образ есть)**

```bash
cargo test -p uefi-engine --test real_image -- --ignored --nocapture 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 4: Закрывающий docs-коммит**

Собрать номера коммитов задач:

```bash
git log --oneline master..hii-write-guard
```

Закрыть пять записей TODO (`[ ]` → `[x]`, каждая — допиской с хешем своего коммита; пример для `:442`):

```markdown
  Закрыто: `<hash B1-коммита>` — цикл hii-write-guard (спека ..., §1 B1).
```

Аналогично: `:445` → хеш B4-коммита (§4), `:1333` → хеш B2-коммита (§2), `:3257` и `:3767` → хеш B3-коммита (§3). Полный путь спеки в каждой дописке: `docs/superpowers/specs/2026-09-25-hii-write-guard-design.md`.

```bash
git add TODO.md
git commit -m "docs(todo): hii-write-guard — закрыть 428/1319/3240/431/3744 (класс B, коммиты цикла)"
```

- [ ] **Step 5: Итог ветки**

```bash
cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check
git log --oneline master..hii-write-guard
```
Expected: 6 коммитов на ветке (5 code: B1-writer, B1-reader, B3, B2, B4 + 1 docs-close; docs №0 из Task 1 остался в master), все гейты зелёные. Ветка готова к мерджу (решение о PR — за владельцем).
