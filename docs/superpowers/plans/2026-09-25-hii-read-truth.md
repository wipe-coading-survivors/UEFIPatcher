# hii-read-truth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Честность читающего пути HII (класс A): SIBT_EXT-блоки, усечённые SIBT-чтения, form_id>0xFFFF, `StringInfo.source`, read/write-сплит селектора рангов форм + docs-балансировка TODO.

**Architecture:** Пять независимых точечных фиксов читающего пути (`parse_string_package`, RPC-хендлер, proto+collect_strings, селектор рангов) плюс один docs-коммит. Мутационный путь не расширяется: bare-канал в мутациях остаётся вырезанным (read/write-сплит `form_package_ranges` / `form_package_ranges_read`), готовя класс B.

**Tech Stack:** Rust workspace (edition 2024), prost/tonic (engine.proto), tracing + tracing-test (warn-семантика), ratatui (TUI).

**Spec:** `docs/superpowers/specs/2026-09-25-hii-read-truth-design.md` (коммит `921e139`)

## Global Constraints

- **Ветка `hii-read-truth`** — весь код только в ней (от master). Task 1 — docs-коммит прямо в master (AGENTS-паттерн: docs отдельно от кода).
- TDD строго: (1) failing-тест → (2) `cargo test` red → (3) реализация → (4) green → (5) commit.
- После каждой задачи: `cargo test -p <затронутый крейт>`, `cargo clippy -p <крейт> -- -D warnings`, `cargo fmt --all -- --check`.
- Комментарии в коде — только rustdoc `///`-контракты на pub/pub(crate)-функциях (что делает / чего НЕ делает + `спека hii-read-truth §N`); narration-комментарии нет.
- SIBT-константы EXT — свои в strings.rs (`r-efi` SIBT-констант не содержит, как и существующие `SIBT_*`). Writer (`string_pack.rs`) НЕ трогаем — `SibtBlockUnsupported` уже честно отказывает; унификация reader/writer (TODO:173) вне скоупа.
- Вся новая арифметика границ — checked (`checked_add` / `saturating_sub` / `get`); усечение = warn + stop walk, без паник.
- Real-гейты (`#[ignore]`) в обычном прогоне не запускаются; явный запуск — Task 10.
- **Ломающее изменение вывода (объявить владельцу в Task 6):** TSV-заголовок `hii string list` становится `language\tstring_id\tsource\ttext` — внешние TSV-скрипты увидят новую колонку.
- TODO:169/183/3463/1668/3475 закрываются в своих задачах с коммит-ссылками; TODO:177/202 — Task 1 (master).
- Дефекты плана/спеки, найденные в шаге — отдельный docs-коммит ДО реализации (AGENTS.md правило 11).
- Один коммит на задачу; сообщения — из плана.

## Контекст для исполнителя (файлы и точки)

- `crates/uefi-engine/src/hii/strings.rs` — `parse_string_package` (`:34`), walk по SIBT (`:46-154`), `read_u16` (`:181`, fallback `(0, body.len())` — дефект A2), `read_scsu` (`:199`), `read_ucs2` (`:209`), `push` (`:158`), `collect_strings` (`:230`), `StringPackageRef`/`StringPackageChannel` (`:244-257`), `walk_string_packages` (`:265`, несёт `file_guid` + `channel`), тест-хелпер `make_pkg` (`:335`).
- `crates/uefi-engine/src/hii/mod.rs` — `form_package_ranges` (`:214`), `pe_resource_form_packages` (`:143`), `gates_list` own-петля (`:338`), `unlock` (`:388`), `own_formset_guid` (`:449`), `find_question_map` (`:660`), `list_questions` (`:750`); тест-хелперы: `g_form`/`g_end`/`g_one_of` (`:3094-3118`), `forms_pkg` (`:3120`), `vendor_forms_pkg` (`:3140`), `vendor_image_with` (`:3164`), `vendor_hii_blob` (`:3179`), `value_forms_pkg` (`:3875`, форма 10029 + one_of qid 0x3B + varstore «Setup»).
- `crates/uefi-engine/src/hii/pe_resource.rs` — `bare_form_packages` (`:134`, валидация кандидата: u24-len + `PACKAGE_FORMS` + `IFR_FORM_SET_OP` + `plen >= 24` + `parse_form_package`; exclude = ранги resource-блобов).
- `crates/uefi-engine/src/hii/form_export.rs` — `find_form_package` (`:90-100`), импорт `:5`.
- `crates/uefi-engine/src/hii/cross_formset.rs:83` — остаётся на mutation-рангах (не трогаем).
- `crates/uefi-engine/src/rpc/server.rs` — `hii_list_questions` (`:1105-1115`, усечение `r.form_id as u16` на `:1111`), тест-паттерн `form_add_status` (`:1819`, EngineServer + images map), `hii_error_status`-семейство (`:54-77`).
- `crates/uefi-proto/proto/engine.proto` — `message StringInfo` (`:201-205`); `crates/uefi-proto/build.rs:6` — serde-derive на StringInfo (json получает поле автоматически).
- `crates/uefi-cli/src/output.rs` — `print_strings` (`:195-213`).
- `crates/uefi-cli/src/commands/hii.rs:38` — `question_list` (form_id парсится как u32 — усечение происходит в engine).
- `crates/uefi-cli/tests/mock_server.rs` — `hii_list_questions` (`:360`), `hii_list_strings` (`:241`, литерал StringInfo `:246`).
- `crates/uefi-cli/tests/e2e.rs` — `hii_list_output_content` (`:68`), `hii_question_info_and_set_value_output_content` (`:97`).
- `crates/uefi-tui/src/ui/forms.rs` — `render_strings` (`:164-194`), тест-хелперы `row_of`/`panel_text` (`:256-267`), литерал StringInfo `:489`.
- `crates/uefi-tui/src/app.rs` — литералы StringInfo `:1344`, `:1349`, `:1523`.
- `crates/uefi-engine/tests/real_image.rs` — `fw_path()`/`load_fw()` (`:11-22`, env `UEFIPATCHER_TEST_FW`), `amibcp_path()` (`:24`), гейт `real_image_hii_forms_and_strings` (`:1022`, печатает счётчики форм/строк).
- Фикстура `crates/uefi-engine/tests/fixtures/hii_rk3588_bare_form.bin` — bare form-пакет rk3588 (паттерн использования: forms.rs `:440`).
- `TODO.md` — записи 169 (`:169`), 177 (`:176`), 183 (`:183`), 202 (`:202`), 1668 (`:1668`), 3463 (`:3463`), 3475 (`:3475`).
- Реальные образы (gitignored): `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, `refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img`, `refs/amibcp/450x — копия.bin`.
- edk2-референс EXT-layout: `../refs/edk2/MdePkg/Include/Uefi/UefiInternalFormRepresentation.h:365-399` — Header(u8 opcode) + BlockType2(u8) + Length(u8/u16/u32 LE); размер блока = 3/4/6 + Length.

---

### Task 1: A6 — docs-балансировка TODO (master)

**Files:**
- Modify: `TODO.md` (записи `:176` и `:202`)

**Interfaces:** — (чистая docs-правка, код не трогаем)

- [ ] **Step 1: Закрыть TODO:177 (found-флаг) как устаревшее**

В записи `* [ ] **hii/strings: walk-сигнал «found» = ...` (строка ~176) заменить `[ ]` на `[x]` и дописать в конец записи (после «...поправить found-флагом при следующем касании файла.»):

```markdown
  Закрыто: устаревшее — полный обход всех string-пакетов уже реализован
  (`883ca14`), walk не останавливается на первом непустом (hii-read-truth
  A6, спека docs/superpowers/specs/2026-09-25-hii-read-truth-design.md §6).
```

- [ ] **Step 2: Закрыть TODO:202 (cross-file титулы) как устаревшее/known-behavior**

В записи `* [ ] **hii/forms: глобальная карта титулов из первого string-package...` (строка ~202) заменить `[ ]` на `[x]` и дописать в конец записи (после «...если проявится — scoping карты per-file.»):

```markdown
  Закрыто: per-file scoping реализовано (forms.rs:36, тест
  `collect_forms_titles_are_scoped_to_file`); остаток — fallback-эвристика
  «крупнейший пул образа» (questions.rs:87–94) — known-behavior, живых
  ложных титулов не зафиксировано; при первом живом ложном срабатывании —
  отдельный пункт на признак `title_source` в FormInfo (hii-read-truth A6,
  спека §6).
```

- [ ] **Step 3: Проверить формат**

```bash
cargo fmt --all -- --check
```
Expected: без изменений (docs-only).

- [ ] **Step 4: Commit (в master, ветку НЕ создавать)**

```bash
git add TODO.md
git commit -m "docs(todo): hii-read-truth A6 — закрыть устаревшие 177 (found-флаг) и 202 (cross-file титулы)"
```

---

### Task 2: A1 — SIBT_EXT1/2/4 в parse_string_package

**Files:**
- Create: ветка `hii-read-truth`
- Modify: `crates/uefi-engine/src/hii/strings.rs` (константы `:23`, arms walk `:148`, новый хелпер, tests)
- Modify: `TODO.md` (запись `:169`)

**Interfaces:**
- Consumes: `parse_string_package` walk (strings.rs `:46`), `make_pkg` тест-хелпер.
- Produces: EXT-блоки (0x30–0x32) скипаются по `Length` без движения `next_id`; усечённый EXT → warn `"truncated SIBT_EXT block; stopping string parse"` + останов walk. Публичного API не меняет.

- [ ] **Step 1: Создать ветку**

```bash
git checkout -b hii-read-truth
```

- [ ] **Step 2: Зафиксировать базу счётчиков HNX (если образ есть)**

```bash
cargo test -p uefi-engine --test real_image -- --ignored real_image_hii_forms_and_strings --nocapture 2>&1 | grep "real_image hii"
```

Expected: PASS + строка вида `real_image hii: N forms across ...; M strings across ...`. Записать N/M — сравнение в Task 10 (сдвиг = ложное срабатывание новых веток, §7 регресс-обоснование). Образа нет — пропустить шаг, пометить в журнале цикла.

- [ ] **Step 3: Написать failing-тесты** (в конец `mod tests` strings.rs)

```rust
    #[test]
    fn parse_ext1_between_strings_skips_extended_data() {
        let sibt = [
            SIBT_STRING_SCSU, b'H', b'i', 0,
            0x30, 0x99, 0x02, 0xAA, 0xBB,
            SIBT_STRING_SCSU, b'B', b'y', b'e', 0,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 2, "строки по обе стороны EXT видны");
        assert_eq!(parsed.strings[0], (1, "Hi".to_string()));
        assert_eq!(parsed.strings[1], (2, "Bye".to_string()));
    }

    #[test]
    fn parse_ext2_and_ext4_blocks_skip() {
        let mut sibt = vec![SIBT_STRING_SCSU, b'A', 0];
        sibt.extend_from_slice(&[0x31, 0x77, 0x01, 0x00, 0xCC]);
        sibt.extend_from_slice(&[SIBT_STRING_SCSU, b'B', 0]);
        sibt.extend_from_slice(&[0x32, 0x77, 0x00, 0x00, 0x00, 0x00]);
        sibt.extend_from_slice(&[SIBT_STRING_SCSU, b'C', 0, SIBT_END]);
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 3);
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (2, "B".to_string()));
        assert_eq!(parsed.strings[2], (3, "C".to_string()));
    }

    #[tracing_test::traced_test]
    #[test]
    fn parse_truncated_ext_stops_walk_without_fake_strings() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            0x30, 0x99, 0x09, 0x01, 0x02,
            SIBT_STRING_SCSU, b'Z', 0,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1, "строки до EXT возвращены, после — нет");
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert!(logs_contain("truncated SIBT_EXT block"));
    }
```

(Length=0x09: EXT-контракт требует 9 байт extended-данных, в теле остаётся 6 — реальное усечение; Length=0x05 умещался бы ровно в остаток и усечением не был бы.)

- [ ] **Step 4: Запустить — убедиться в падении**

```bash
cargo test -p uefi-engine hii::strings
```
Expected: FAIL — `parse_ext1_between_strings_skips_extended_data` и `parse_ext2_and_ext4_blocks_skip` дают `strings.len() == 1` (EXT → unknown-opcode, walk остановлен); `parse_truncated_ext_stops_walk_without_fake_strings` — walk и так останавливается на unknown-opcode (`strings.len() == 1`), красным является только `logs_contain("truncated SIBT_EXT block")` (лога нет).

- [ ] **Step 5: Реализовать** (strings.rs)

Константы (после `const SIBT_SKIP1`):

```rust
const SIBT_EXT1: u8 = 0x30;
const SIBT_EXT2: u8 = 0x31;
const SIBT_EXT4: u8 = 0x32;
```

Хелпер (рядом с `read_u16`):

```rust
/// Смещение следующего SIBT_EXT-блока: header = opcode + BlockType2 +
/// Length(len_width LE); total = pos + 2 + len_width + Length. None —
/// заголовок или Length выходят за тело (усечённый EXT). Спека
/// hii-read-truth §1 (edk2 UefiInternalFormRepresentation.h:365–399).
fn sibt_ext_next(body: &[u8], pos: usize, len_width: usize) -> Option<usize> {
    let field = pos.checked_add(2)?;
    let mut length = 0usize;
    for i in 0..len_width {
        length |= (body.get(field + i).copied()? as usize) << (8 * i);
    }
    let next = pos.checked_add(2 + len_width)?.checked_add(length)?;
    (next <= body.len()).then_some(next)
}
```

Arms в walk (вставить перед `other =>`):

```rust
            SIBT_EXT1 => match sibt_ext_next(body, pos, 1) {
                Some(next) => pos = next,
                None => {
                    tracing::warn!(opcode = body[pos], "truncated SIBT_EXT block; stopping string parse");
                    break;
                }
            },
            SIBT_EXT2 => match sibt_ext_next(body, pos, 2) {
                Some(next) => pos = next,
                None => {
                    tracing::warn!(opcode = body[pos], "truncated SIBT_EXT block; stopping string parse");
                    break;
                }
            },
            SIBT_EXT4 => match sibt_ext_next(body, pos, 4) {
                Some(next) => pos = next,
                None => {
                    tracing::warn!(opcode = body[pos], "truncated SIBT_EXT block; stopping string parse");
                    break;
                }
            },
```

EXT не двигает `next_id` и не добавляет строк — семантика §1.

- [ ] **Step 6: Запустить — зелено**

```bash
cargo test -p uefi-engine hii::strings
```
Expected: PASS (включая все старые тесты).

- [ ] **Step 7: Закрыть TODO:169**

В записи `* [ ] **hii/strings: SIBT_EXT1/2/4 (0x30–0x32) не обрабатываются** —` заменить `[ ]` на `[x]`, дописать в конец записи:

```markdown
  Закрыто: hii-read-truth A1 (ветка `hii-read-truth`) — EXT-блоки
  скипаются по Length без движения next_id; усечённый EXT = warn+stop.
```

- [ ] **Step 8: Полный чек крейта**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
```

- [ ] **Step 9: Commit**

```bash
git add crates/uefi-engine/src/hii/strings.rs TODO.md
git commit -m "fix(hii): A1 — SIBT_EXT1/2/4: skip extended data в parse_string_package (+TODO:169)"
```

---

### Task 3: A2 — усечённый count и фальшивые пустые записи

**Files:**
- Modify: `crates/uefi-engine/src/hii/strings.rs` (`read_u16` `:181`, `read_scsu` `:199`, `read_ucs2` `:209`, все call-sites walk, tests)
- Modify: `TODO.md` (запись `:183`)

**Interfaces:**
- Consumes: walk из Task 2.
- Produces: внутренние хелперы `read_u16/read_scsu/read_ucs2 -> Option<(T, usize)>` (None = тело пакета кончилось); warn `"truncated SIBT block (<opcode>); stopping string parse"` + `break 'outer` для всех потребителей внутри walk (STRINGS_* count, DUPLICATE ref_id, SKIP2 count, одиночные STRING_*/FONT). Публичного API не меняет. Deliberate non-goal: строка без терминатора до конца тела (SCSU ≥1 байта без NUL / UCS2 ≥2 байтов без 00 00) по-прежнему принимается — специфицировано только «меньше минимального завершимого объёма» (§2).

- [ ] **Step 1: Написать failing-тесты** (в конец `mod tests` strings.rs)

```rust
    #[test]
    fn parse_truncated_duplicate_does_not_fabricate_empty_string() {
        let sibt = [SIBT_STRING_SCSU, b'A', 0, SIBT_DUPLICATE];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1, "усечённый DUPLICATE не даёт пустую запись");
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
    }

    #[test]
    fn parse_trailing_scsu_opcode_is_truncation_not_empty_string() {
        let sibt = [SIBT_STRING_SCSU, b'A', 0, SIBT_STRING_SCSU];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1, "0 байтов до NUL = усечение");
    }

    #[test]
    fn parse_trailing_ucs2_byte_is_truncation_not_empty_string() {
        let sibt = [SIBT_STRING_SCSU, b'A', 0, SIBT_STRING_UCS2, 0x41];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1, "<2 байтов до терминатора = усечение");
    }

    #[test]
    fn parse_valid_empty_strings_remain() {
        let sibt = [
            SIBT_STRING_SCSU, b'A', 0,
            SIBT_STRING_SCSU, 0,
            SIBT_STRING_UCS2, 0, 0,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 3, "валидные пустые строки остаются");
        assert_eq!(parsed.strings[1], (2, String::new()));
        assert_eq!(parsed.strings[2], (3, String::new()));
    }

    #[tracing_test::traced_test]
    #[test]
    fn parse_truncated_strings_count_warns_instead_of_silence() {
        let sibt = [SIBT_STRING_SCSU, b'A', 0, SIBT_STRINGS_SCSU];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1, "строки до усечения возвращены");
        assert!(logs_contain("truncated SIBT block"), "без молчаливого count=0");
    }

    #[tracing_test::traced_test]
    #[test]
    fn parse_truncated_skip2_warns_instead_of_silence() {
        let sibt = [SIBT_STRING_SCSU, b'A', 0, SIBT_SKIP2];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1);
        assert!(logs_contain("truncated SIBT block"));
    }
```

- [ ] **Step 2: Запустить — красно**

```bash
cargo test -p uefi-engine hii::strings
```
Expected: FAIL — `..._duplicate_...` и два `trailing_*` дают `strings.len() == 2` (фальшивая пустая запись); два `traced_test` — нет лога. `parse_valid_empty_strings_remain` — уже зелено (пиннинг).

- [ ] **Step 3: Реализовать** (strings.rs)

Хелпер warn (рядом с `push`):

```rust
fn warn_truncated_block(opcode: u8) {
    tracing::warn!(
        opcode = opcode,
        "truncated SIBT block ({:#04x}); stopping string parse",
        opcode
    );
}
```

`read_u16` → Option (замена целиком):

```rust
fn read_u16(body: &[u8], pos: usize) -> Option<(u16, usize)> {
    if pos + 2 > body.len() {
        return None;
    }
    Some((u16::from_le_bytes([body[pos], body[pos + 1]]), pos + 2))
}
```

`read_scsu` → Option (замена целиком):

```rust
fn read_scsu(body: &[u8], start: usize) -> Option<(String, usize)> {
    if start >= body.len() {
        return None;
    }
    let mut i = start;
    while i < body.len() && body[i] != 0 {
        i += 1;
    }
    let text = String::from_utf8_lossy(&body[start..i]).into_owned();
    Some((text, if i < body.len() { i + 1 } else { body.len() }))
}
```

`read_ucs2` → Option (заменить первую строку тела и вернуть кортеж в `Some`):

```rust
fn read_ucs2(body: &[u8], start: usize) -> Option<(String, usize)> {
    if body.len().saturating_sub(start) < 2 {
        return None;
    }
    let mut i = start;
    while i + 1 < body.len() && !(body[i] == 0 && body[i + 1] == 0) {
        i += 2;
    }
    let units: Vec<u16> = body[start..i]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units);
    Some((
        text,
        if i + 1 < body.len() {
            i + 2
        } else {
            body.len()
        },
    ))
}
```

Call-sites walk — единая семантика (все `let ... = read_...(...)` получают `let-else`):

Одиночные STRING_*/FONT (4 arm'а, шаблон на примере SCSU; FONT-варианты отличаются смещением `pos + 2` и своим опкодом):

```rust
            SIBT_STRING_SCSU => {
                let Some((text, p)) = read_scsu(body, pos + 1) else {
                    warn_truncated_block(body[pos]);
                    break;
                };
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
```

STRINGS_* (4 arm'а; замена чтения count и чтения строки в теле цикла; FONT-варианты читают count на `pos + 2`, UCS2-варианты зовут `read_ucs2` — остальное идентично):

```rust
            SIBT_STRINGS_SCSU => {
                let Some((count, mut p)) = read_u16(body, pos + 1) else {
                    warn_truncated_block(body[pos]);
                    break;
                };
                for _ in 0..count {
                    if p >= body.len() {
                        tracing::warn!(opcode = body[pos], "truncated SIBT_STRINGS block; stopping string parse");
                        break 'outer;
                    }
                    let Some((text, np)) = read_scsu(body, p) else {
                        warn_truncated_block(body[pos]);
                        break 'outer;
                    };
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
```

DUPLICATE:

```rust
            SIBT_DUPLICATE => {
                let Some((ref_id, _)) = read_u16(body, pos + 1) else {
                    warn_truncated_block(body[pos]);
                    break;
                };
                let text = by_id.get(&ref_id).cloned().unwrap_or_default();
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos += 1 + 2;
            }
```

SKIP2:

```rust
            SIBT_SKIP2 => {
                let Some((count, p)) = read_u16(body, pos + 1) else {
                    warn_truncated_block(body[pos]);
                    break;
                };
                next_id = next_id.wrapping_add(count);
                pos = p;
            }
```

SKIP1 не трогать (усечение = тихий no-op без фабрикации данных — §2 не требует).

- [ ] **Step 4: Запустить — зелено (весь крейт)**

```bash
cargo test -p uefi-engine
```
Expected: PASS. Контроль регресса: `parse_truncated_font_opcode_does_not_panic` (0 строк, `first()` → None), `parse_truncated_strings_scsu_block_returns_only_real_strings` (внутренний guard срабатывает раньше), `parse_truncated_language_falls_back_empty` — все зелены.

- [ ] **Step 5: Закрыть TODO:183**

В записи `* [ ] **hii/strings: обрезанный u16-count STRINGS_\* блока молча даёт count=0** —` заменить `[ ]` на `[x]`, дописать:

```markdown
  Закрыто: hii-read-truth A2 (ветка `hii-read-truth`) — усечённые
  u16-чтения (count/ref_id) и хвостовые <минимального объёма тела строки =
  warn `truncated SIBT block` + stop; фальшивых пустых записей нет.
```

- [ ] **Step 6: Чек + Commit**

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/strings.rs TODO.md
git commit -m "fix(hii): A2 — усечённые SIBT-чтения: warn+stop вместо молчаливых пустых строк (+TODO:183)"
```

---

### Task 4: A3 — form_id u32→u16 в hii_list_questions

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (handler `:1105-1115`, tests)
- Modify: `crates/uefi-cli/tests/mock_server.rs` (`hii_list_questions` `:360`)
- Modify: `crates/uefi-cli/tests/e2e.rs` (`hii_question_info_and_set_value_output_content` `:97`)
- Modify: `TODO.md` (запись `:3463`)

**Interfaces:**
- Consumes: `HiiListQuestionsRequest { image_id, target, form_id: u32 }`, тест-паттерн `form_add_status` (server.rs `:1819`).
- Produces: form_id > 0xFFFF → `Status::invalid_argument("form_id out of range: {n}")` ДО вызова `list_questions`; пустой список «успехом» больше не маскирует опечатку.

- [ ] **Step 1: Написать failing-тест** (server.rs, `mod tests`, рядом с `hii_form_add_*`)

```rust
    async fn list_questions_status(img: Image, target: &str, form_id: u32) -> Status {
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
            .hii_list_questions(Request::new(HiiListQuestionsRequest {
                image_id: "i".into(),
                target: target.into(),
                form_id,
            }))
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn hii_list_questions_rejects_form_id_above_u16() {
        let data = form_add_bare_flash();
        let img = crate::parser::image::parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let st = list_questions_status(
            img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1",
            0x1_0000,
        )
        .await;
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        assert!(st.message().contains("form_id out of range: 65536"));
    }
```

- [ ] **Step 2: Запустить — красно**

```bash
cargo test -p uefi-engine hii_list_questions_rejects_form_id_above_u16
```
Expected: FAIL — паника на `.unwrap_err()` (handler возвращает `Ok` с пустым списком: 0x10000 усекается до 0).

- [ ] **Step 3: Реализовать guard в handler** (server.rs `hii_list_questions`, после `let img = ...`)

```rust
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let form_id = u16::try_from(r.form_id).map_err(|_| {
            Status::invalid_argument(format!("form_id out of range: {}", r.form_id))
        })?;
        let questions = crate::hii::list_questions(&img, &r.target, form_id)
            .map_err(|e| hii_error_status_ctx(e, &r.target))?;
```

- [ ] **Step 4: Зелено**

```bash
cargo test -p uefi-engine rpc
```
Expected: PASS.

- [ ] **Step 5: e2e — мок возвращает ту же ошибку** (mock_server.rs `hii_list_questions`)

```rust
    async fn hii_list_questions(
        &self,
        req: Request<HiiListQuestionsRequest>,
    ) -> Result<Response<HiiListQuestionsResponse>, Status> {
        let r = req.get_ref();
        if r.form_id > u16::MAX as u32 {
            return Err(Status::invalid_argument(format!(
                "form_id out of range: {}",
                r.form_id
            )));
        }
        self.journal.lock().await.push("HiiListQuestions".into());
        Ok(Response::new(HiiListQuestionsResponse {
```

(остальное тело мока без изменений)

- [ ] **Step 6: e2e-тест** (e2e.rs, в `hii_question_info_and_set_value_output_content`, после блока `hii question set-value`)

```rust
    cli(&sock, cwd)
        .args(["hii", "question", "list", "0:0x19:0#65536"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("form_id out of range: 65536"));
```

- [ ] **Step 7: Прогнать e2e**

```bash
cargo test -p uefi-cli
```
Expected: PASS (новый e2e зелёный: ненулевой exit + контекст в stderr).

- [ ] **Step 8: Закрыть TODO:3463**

В записи `* [ ] **uefi-engine: HiiListQuestions молча режет form_id u32→u16** —` заменить `[ ]` на `[x]`, дописать:

```markdown
  Закрыто: hii-read-truth A3 (ветка `hii-read-truth`) — u16::try_from +
  invalid_argument "form_id out of range: {n}" в handler'е до вызова
  list_questions.
```

- [ ] **Step 9: Чек + Commit**

```bash
cargo clippy -p uefi-engine -p uefi-cli -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/rpc/server.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-cli/tests/e2e.rs TODO.md
git commit -m "fix(rpc): A3 — HiiListQuestions form_id>0xFFFF → invalid_argument, e2e (+TODO:3463)"
```

---

### Task 5: A4 — StringInfo.source: proto + engine

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (`message StringInfo` `:201`)
- Modify: `crates/uefi-engine/src/hii/strings.rs` (`collect_strings` `:230`, `StringPackageChannel` `:244`, tests)
- Modify: `crates/uefi-cli/tests/mock_server.rs` (литерал StringInfo `:246`)
- Modify: `crates/uefi-tui/src/app.rs` (литералы `:1344`, `:1349`, `:1523`)
- Modify: `crates/uefi-tui/src/ui/forms.rs` (литерал `:489`)

**Interfaces:**
- Consumes: `StringPackageRef { file_guid: Option<Guid>, channel: StringPackageChannel, ... }` (strings.rs `:251`), `crate::types::guid_to_upper_string`.
- Produces: `uefi_proto::StringInfo { language: String, string_id: u32, text: String, source: String }` (proto3-additive, поле 4); source = `"<UPPER-FFS-GUID>/res" | "<UPPER-FFS-GUID>/bare" | "res" | "bare"`. Задачи 6-7 читают поле.

- [ ] **Step 1: Proto-поле** (engine.proto)

```proto
message StringInfo {
  string language = 1;
  uint32 string_id = 2;
  string text = 3;
  string source = 4;   // NEW: "<UPPER-FFS-GUID>/res" | "<UPPER-FFS-GUID>/bare" | "res" | "bare"
}
```

```bash
cargo build -p uefi-proto
```

- [ ] **Step 2: Восстановить компиляцию workspace** — во ВСЕ литералы StringInfo добавить `source: String::new()`:

`collect_strings` (strings.rs `:234`):

```rust
            out.push(StringInfo {
                language: pkg.language.clone(),
                string_id: sid as u32,
                text,
                source: String::new(),
            });
```

`mock_server.rs:246`, `app.rs:1344/:1349/:1523`, `ui/forms.rs:489` — в каждый литерал добавить строку `source: String::new(),`.

```bash
cargo test -p uefi-engine -p uefi-cli -p uefi-tui
```
Expected: PASS — поведение не изменилось (compile-restore).

- [ ] **Step 3: Failing-тесты source** (strings.rs tests)

В `collect_strings_returns_bare_package_strings` (файл без guid) добавить:

```rust
        assert_eq!(out[0].source, "bare", "файл-владелец неизвестен — голый канал");
```

В `collect_strings_from_pe_resources` добавить:

```rust
        assert_eq!(out[0].source, "res");
```

В `collect_strings_traverses_all_packages_and_files` (ga = bare-владелец, gb = res-владелец) добавить:

```rust
        assert!(strings.iter().all(|s| matches!(
            s.source.as_str(),
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5/bare"
                | "ABBCE13D-E25A-4D9F-A1F9-2F7710786892/res"
        )));
```

- [ ] **Step 4: Красно**

```bash
cargo test -p uefi-engine hii::strings
```
Expected: FAIL — source пустой.

- [ ] **Step 5: Реализовать** (strings.rs)

Метод канала (у `enum StringPackageChannel`):

```rust
impl StringPackageChannel {
    fn suffix(&self) -> &'static str {
        match self {
            StringPackageChannel::Bare => "bare",
            StringPackageChannel::Resource => "res",
        }
    }
}
```

`collect_strings` (замена целиком):

```rust
/// Строки всех string-пакетов образа; source = владелец + канал
/// (`GUID/res|bare`, без владельца — голый канал). Идентификаторы
/// уникальны только внутри package-list — source делает коллизии
/// различимыми. Спека hii-read-truth §4.
pub fn collect_strings(image: &Image) -> Vec<StringInfo> {
    let mut out = Vec::new();
    for pkg in collect_string_packages(image) {
        let source = match &pkg.file_guid {
            Some(g) => format!(
                "{}/{}",
                crate::types::guid_to_upper_string(g),
                pkg.channel.suffix()
            ),
            None => pkg.channel.suffix().to_string(),
        };
        for (sid, text) in pkg.strings {
            out.push(StringInfo {
                language: pkg.language.clone(),
                string_id: sid as u32,
                text,
                source: source.clone(),
            });
        }
    }
    out
}
```

- [ ] **Step 6: Зелено (весь workspace)**

```bash
cargo test -p uefi-engine -p uefi-cli -p uefi-tui
```
Expected: PASS.

- [ ] **Step 7: Чек + Commit**

```bash
cargo clippy -p uefi-proto -p uefi-engine -p uefi-cli -p uefi-tui -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-proto/proto/engine.proto crates/uefi-engine/src/hii/strings.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-tui/src/app.rs crates/uefi-tui/src/ui/forms.rs
git commit -m "feat(proto): A4 — StringInfo.source: GUID владельца + канал (engine)"
```

---

### Task 6: A4 — CLI: source в hii string list

**Владельцу объявлено (§9):** TSV-заголовок `hii string list` меняется на `language\tstring_id\tsource\ttext` — внешние TSV-скрипты увидят новую колонку.

**Files:**
- Modify: `crates/uefi-cli/src/output.rs` (`print_strings` `:195-213`)
- Modify: `crates/uefi-cli/tests/mock_server.rs` (литерал StringInfo — осмысленный source)
- Modify: `crates/uefi-cli/tests/e2e.rs` (`hii_list_output_content` `:68`)
- Modify: `TODO.md` (запись `:1668`)

**Interfaces:**
- Consumes: `StringInfo.source` (Task 5).
- Produces: text `[lang] id source: text`; TSV-заголовок `language\tstring_id\tsource\ttext`; json-поле `source` (serde-derive уже на месте).

- [ ] **Step 1: Failing e2e** (e2e.rs, в `hii_list_output_content`, после существующего блока `hii string list`)

```rust
    cli(&sock, cwd)
        .args(["--format", "tsv", "hii", "string", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("language\tstring_id\tsource\ttext"))
        .stdout(predicates::str::contains(
            "eng\t1\t899407D7-99FE-43D8-9A21-79EC328CAC21/res\tHello",
        ));
    cli(&sock, cwd)
        .args(["--format", "json", "hii", "string", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"source\""));
```

- [ ] **Step 2: Мок — осмысленный source** (mock_server.rs литерал `:246`)

```rust
            strings: vec![StringInfo {
                language: "eng".into(),
                string_id: 1,
                text: "Hello".into(),
                source: "899407D7-99FE-43D8-9A21-79EC328CAC21/res".into(),
            }],
```

- [ ] **Step 3: Красно**

```bash
cargo test -p uefi-cli hii_list_output_content
```
Expected: FAIL — старый TSV-заголовок без `source`.

- [ ] **Step 4: Реализовать print_strings** (output.rs, замена `:195-213`)

```rust
pub fn print_strings(strings: &[StringInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(strings).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("language\tstring_id\tsource\ttext");
            for s in strings {
                println!("{}\t{}\t{}\t{}", s.language, s.string_id, s.source, s.text);
            }
        }
        OutputFormat::Text => {
            for s in strings {
                println!("[{}] {} {}: {}", s.language, s.string_id, s.source, s.text);
            }
        }
    }
}
```

- [ ] **Step 5: Зелено**

```bash
cargo test -p uefi-cli
```
Expected: PASS (в т.ч. старый `contains("Hello")` в text-режиме).

- [ ] **Step 6: Закрыть TODO:1668**

В записи `* [ ] **мелочь: string-id коллизии между списками пакетов** —` заменить `[ ]` на `[x]`, дописать:

```markdown
  Закрыто: hii-read-truth A4 (ветка `hii-read-truth`) — StringInfo.source
  (GUID владельца + канал) в `hii string list` text/tsv/json; одинаковые
  id из разных package-list'ов различимы.
```

- [ ] **Step 7: Чек + Commit**

```bash
cargo clippy -p uefi-cli -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-cli/src/output.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-cli/tests/e2e.rs TODO.md
git commit -m "feat(cli): A4 — source в hii string list (text/tsv/json) (+TODO:1668)"
```

---

### Task 7: A4 — TUI: source выбранной строки в strings-браузере

**Files:**
- Modify: `crates/uefi-tui/src/ui/forms.rs` (`render_strings` `:164-194`, tests: осмысленные source в литерале `:489`, новый тест)
- Modify: `crates/uefi-tui/src/app.rs` (литералы `:1344/:1349` — осмысленные source)

**Interfaces:**
- Consumes: `StringInfo.source` (Task 5), `app.forms.strings_cursor`.
- Produces: source выбранной строки в титуле панели Strings (`Strings [...] — <source>` / `Strings (filter: X) — <source>`); без новой колонки списка (§4).

- [ ] **Step 1: Failing-тест** (ui/forms.rs, `mod tests`)

```rust
    #[test]
    fn strings_browser_shows_selected_source_in_title() {
        let mut app = crate::app::App::new();
        app.forms.show_strings = true;
        app.forms.strings = vec![uefi_proto::StringInfo {
            language: "eng".into(),
            string_id: 1,
            text: "Hello".into(),
            source: "899407D7-99FE-43D8-9A21-79EC328CAC21/res".into(),
        }];
        app.forms.strings_cursor = 0;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(
            text.contains("899407D7-99FE-43D8-9A21-79EC328CAC21/res"),
            "source выбранной строки в титуле"
        );
    }
```

- [ ] **Step 2: Красно**

```bash
cargo test -p uefi-tui strings_browser_shows_selected_source_in_title
```
Expected: FAIL — титула с source нет.

- [ ] **Step 3: Реализовать** (render_strings, заменить блок title `:171-175` и передачу в Block)

```rust
    let mut title = if app.forms.strings_filter.is_empty() {
        "Strings".to_string()
    } else {
        format!("Strings (filter: {})", app.forms.strings_filter)
    };
    if let Some(s) = app
        .forms
        .strings
        .get(app.forms.strings_cursor)
        .filter(|s| !s.source.is_empty())
    {
        title.push_str(&format!(" — {}", s.source));
    }
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray));
```

- [ ] **Step 4: Осмысленные source в тестовых литералах** (не влияют на asserts, для самодокументирования): `app.rs:1344` и `:1349` → `source: "899407D7-99FE-43D8-9A21-79EC328CAC21/res".into()`; `app.rs:1523` (map, 40 элементов) → `source: "res".into()`; `ui/forms.rs:489` (map, 30 элементов) → `source: "res".into()`.

- [ ] **Step 5: Зелено**

```bash
cargo test -p uefi-tui
```
Expected: PASS.

- [ ] **Step 6: Чек + Commit**

```bash
cargo clippy -p uefi-tui -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-tui/src/ui/forms.rs crates/uefi-tui/src/app.rs
git commit -m "feat(tui): A4 — source выбранной строки в титуле strings-браузера"
```

---

### Task 8: A5 — bare_form_package_ranges + form_package_ranges_read (read/write-сплит)

**Files:**
- Modify: `crates/uefi-engine/src/hii/pe_resource.rs` (`bare_form_packages` `:134` — делегирует новому ranges; новый `bare_form_package_ranges`)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (rustdoc на `form_package_ranges` `:214`; новый `form_package_ranges_read` рядом; tests)

**Interfaces:**
- Consumes: валидацию кандидата bare-скана (pe_resource.rs `:134-154`), `pe_resource_form_packages` (mod.rs `:143`), `hii_resource_ranges` (pe_resource.rs, pub).
- Produces: `pub fn bare_form_package_ranges(pe: &[u8], exclude: &[(usize, usize)]) -> Vec<(usize, usize)>` (pe_resource.rs); `pub(crate) fn form_package_ranges_read(node: &FfsNode) -> Vec<(usize, usize)>` (mod.rs) — суперсет мутационного: PE32-ветка = resource-ранги + bare-ранги (exclude = resource-блобы). Task 9 переключает потребителей.

- [ ] **Step 1: Failing-тесты** (mod.rs, `mod tests`, после `form_package_ranges_ignores_non_hii_0x18_body`)

```rust
    #[test]
    fn form_package_ranges_read_includes_bare_and_mutation_does_not() {
        let mut body = vec![0x44u8; 16];
        body.extend(value_forms_pkg());
        let image = vendor_image_with(0x10, body);
        let node = &image.root.children[0].children[0].children[0];
        assert!(
            form_package_ranges(node).is_empty(),
            "мутационный селектор bare не включает (пиннинг, TODO:383)"
        );
        let read = form_package_ranges_read(node);
        assert_eq!(read.len(), 1, "read-селектор видит bare-пакет");
        let (start, len) = read[0];
        assert_eq!(&node.body[start..start + len], &value_forms_pkg()[..]);
    }

    #[test]
    fn form_package_ranges_read_does_not_duplicate_resource_packages() {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &vendor_hii_blob());
        let image = vendor_image_with(0x10, pe);
        let node = &image.root.children[0].children[0].children[0];
        let mutation = form_package_ranges(node);
        let read = form_package_ranges_read(node);
        assert!(!mutation.is_empty());
        assert_eq!(read, mutation, "resource-пакеты не задваиваются (exclude)");
    }

    #[test]
    fn form_package_ranges_read_matches_mutation_on_raw_and_freeform() {
        let raw_image = vendor_image_with(0x19, vendor_forms_pkg());
        let raw_node = &raw_image.root.children[0].children[0].children[0];
        assert_eq!(form_package_ranges_read(raw_node), form_package_ranges(raw_node));

        let pkg1 = value_forms_pkg();
        let pkg2 = forms_pkg([g_form(9), g_end(), g_end()].concat());
        let list_guid = Guid::try_parse("97E409E6-4CC1-11D9-81F6-000000000000").unwrap();
        let mut body = list_guid.to_bytes().to_vec();
        body.extend_from_slice(&2u32.to_le_bytes());
        body.extend_from_slice(&pkg1);
        body.extend_from_slice(&pkg2);
        let ff_image = vendor_image_with(0x18, body);
        let ff_node = &ff_image.root.children[0].children[0].children[0];
        assert_eq!(form_package_ranges_read(ff_node), form_package_ranges(ff_node));
        assert_eq!(form_package_ranges_read(ff_node).len(), 2);
    }
```

- [ ] **Step 2: Красно (compile error — функция не существует)**

```bash
cargo test -p uefi-engine form_package_ranges_read
```
Expected: FAIL — `cannot find function form_package_ranges_read`.

- [ ] **Step 3: Реализовать ranges-вариацию + DRY** (pe_resource.rs)

Новая функция над `bare_form_packages` (сохранить существующий `bare_form_packages` с той же сигнатурой, тело — делегирование):

```rust
/// Ранги (off, len) bare form-пакетов в теле PE32: та же валидация
/// кандидата, что и канал срезов (`bare_form_packages`); exclude — ранги
/// resource-блобов, чтобы не задваивать пакеты, достигнутые обоими
/// каналами. Только читающий путь (спека hii-read-truth §5).
pub fn bare_form_package_ranges(pe: &[u8], exclude: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 5 <= pe.len() {
        let plen = pe[pos] as usize | (pe[pos + 1] as usize) << 8 | (pe[pos + 2] as usize) << 16;
        if pe[pos + 3] == PACKAGE_FORMS
            && pe[pos + 4] == IFR_FORM_SET_OP
            && plen >= 24
            && pos + plen <= pe.len()
        {
            let covered = exclude.iter().any(|&(o, l)| o <= pos && pos < o + l);
            if !covered && parse_form_package(&pe[pos..pos + plen]).is_some() {
                out.push((pos, plen));
            }
            pos += plen;
        } else {
            pos += 1;
        }
    }
    out
}

pub fn bare_form_packages<'a>(pe: &'a [u8], exclude: &[(usize, usize)]) -> Vec<&'a [u8]> {
    bare_form_package_ranges(pe, exclude)
        .into_iter()
        .map(|(off, len)| &pe[off..off + len])
        .collect()
}
```

- [ ] **Step 4: Реализовать read-селектор + rustdoc контракт** (mod.rs, над `form_package_ranges`)

Rustdoc на `form_package_ranges` (точный текст §5):

```rust
/// Ранги form-пакетов для мутаций и их пре-чеков; bare-канал не входит —
/// мутации bare не аппаратно-валидированы (TODO:383); чтение —
/// `form_package_ranges_read`. Спека hii-read-truth §5.
```

Новая функция следом:

```rust
/// Суперсет мутационного селектора для читающего пути (list-questions /
/// question-info / gates-list own / form-export): PE32-ветка =
/// resource-ранги + bare-ранги (exclude = resource-блобы, без дублей);
/// RAW/0x18-ветви идентичны мутационному. Мутации сюда НЕ переключать.
/// Спека hii-read-truth §5.
pub(crate) fn form_package_ranges_read(node: &FfsNode) -> Vec<(usize, usize)> {
    if node.subtype == EFI_SECTION_RAW || ifr::is_form_package(&node.body) {
        if ifr::is_form_package(&node.body) {
            vec![(0, node.body.len())]
        } else {
            vec![]
        }
    } else if node.subtype == EFI_SECTION_PE32 {
        let mut out = pe_resource_form_packages(&node.body).unwrap_or_default();
        let blobs = crate::hii::pe_resource::hii_resource_ranges(&node.body);
        out.extend(crate::hii::pe_resource::bare_form_package_ranges(
            &node.body,
            &blobs,
        ));
        out
    } else if node.subtype == EFI_SECTION_FREEFORM_SUBTYPE_GUID {
        freeform_form_package_ranges(&node.body).unwrap_or_default()
    } else {
        vec![]
    }
}
```

- [ ] **Step 5: Зелено (весь крейт — задет общий bare-скан)**

```bash
cargo test -p uefi-engine
```
Expected: PASS (включая forms.rs `collect_forms_sees_bare_form_package_in_pe_body` — поведение `bare_form_packages` не изменилось).

- [ ] **Step 6: Чек + Commit**

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/pe_resource.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(hii): A5 — form_package_ranges_read: read/write-сплит селектора рангов (bare в чтении)"
```

---

### Task 9: A5 — переключение читающих потребителей на read-селектор

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`gates_list` own-петля `:338`, `find_question_map` `:660`, `list_questions` `:750`; tests)
- Modify: `crates/uefi-engine/src/hii/form_export.rs` (импорт `:5`, `find_form_package` `:91`)
- Modify: `TODO.md` (запись `:3475`)

**Interfaces:**
- Consumes: `form_package_ranges_read` (Task 8).
- Produces: bare-таргеты отдают вопросы/гейты/exchange; мутации (`unlock` `:388`, `own_formset_guid` `:449`, `set_item_visibility`, `form_add` `:1676/:1779/:1791/:1808`, `cross_formset.rs:83`) остаются на `form_package_ranges` — НЕ трогать. `set_value` не задет: его мутация — NVAR-флипы, read-карта (find_question_map) только даёт varstore/offset/width (§5).

- [ ] **Step 1: Failing-тест** (mod.rs, `mod tests`, рядом с тестами Task 8)

```rust
    #[test]
    fn list_questions_and_question_info_see_bare_form_package_in_pe_body() {
        let mut body = vec![0x44u8; 16];
        body.extend(value_forms_pkg());
        let image = vendor_image_with(0x10, body);
        let target = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0";
        let questions = list_questions(&image, target, 10029).unwrap();
        assert!(!questions.is_empty(), "bare-таргет отдаёт вопросы (было пусто)");
        assert_eq!(questions[0].question_id, 0x3B);
        let item = format!("{target}#10029:0x3B");
        let info = question_info(&image, &item).unwrap();
        assert_eq!(info.question_id, 0x3B, "question_info резолвится по bare (было NotFound)");
    }
```

- [ ] **Step 2: Красно**

```bash
cargo test -p uefi-engine list_questions_and_question_info_see_bare
```
Expected: FAIL — `list_questions` возвращает пустой vec (unwrap Ok, questions пуст).

- [ ] **Step 3: Переключить 4 потребителя** (только имя функции в заголовке петли)

- mod.rs `:338` (gates_list own-петля): `for (start, len) in form_package_ranges_read(node) {`
- mod.rs `:660` (find_question_map): `for (start, len) in form_package_ranges_read(node) {`
- mod.rs `:750` (list_questions): `for (start, len) in form_package_ranges_read(node) {`
- form_export.rs `:5`: `use super::{HiiError, form_package_ranges_read, ifr, parse_item_id, schema, values};` и `:91`: `for (start, len) in form_package_ranges_read(node) {`

- [ ] **Step 4: Зелено (весь крейт)**

```bash
cargo test -p uefi-engine
```
Expected: PASS — существующие тесты gates/unlock/form_export (RAW/resource-таргеты) не задеты: read-селектор на их данных идентичен mutation.

- [ ] **Step 5: Закрыть TODO:3475**

В записи `* [ ] **uefi-engine: bare PE form-пакеты дают пустой список вопросов** —` заменить `[ ]` на `[x]`, дописать:

```markdown
  Закрыто: hii-read-truth A5 (ветка `hii-read-truth`) — read/write-сплит:
  list-questions/question-info/gates-list(own)/form-export на
  form_package_ranges_read (bare включён); мутации на mutation-селекторе.
```

- [ ] **Step 6: Чек + Commit**

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/hii/form_export.rs TODO.md
git commit -m "fix(hii): A5 — читающие потребители на read-селектор: bare-вопросы видны (+TODO:3475)"
```

---

### Task 10: Real-image гейты (HNX source / rk3588 bare / 450x коллизии)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новые `#[ignore]`-гейты, рядом с `real_image_hii_forms_and_strings` `:1022`)

**Interfaces:**
- Consumes: `collect_strings` (source, Task 5), `collect_forms`/`list_questions`/`question_info` (Task 9), `fw_path()`/`load_fw()`/`amibcp_path()` (`:11-28`).
- Produces: гейты §7.1–7.3. Проверка счётчиков HNX = ручное сравнение с базой из Task 2 Step 2.

- [ ] **Step 1: Гейт §7.1 — source у строк HNX** (real_image.rs)

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_strings_source_tagged() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let strings = uefi_engine::hii::strings::collect_strings(&img);
    assert!(!strings.is_empty(), "expected strings in real image");
    for s in &strings {
        let chan = s
            .source
            .rsplit_once('/')
            .map(|(_, c)| c)
            .unwrap_or(s.source.as_str());
        assert!(
            chan == "res" || chan == "bare",
            "source channel must be res|bare, got {}",
            s.source
        );
    }
    assert!(
        strings.iter().any(|s| s.source.contains('/')),
        "expected file-owned strings (GUID-prefixed source) on HNX"
    );
    let with_guid = strings.iter().filter(|s| s.source.contains('/')).count();
    eprintln!(
        "real_image hii source: {with_guid}/{} strings file-owned",
        strings.len()
    );
}
```

- [ ] **Step 2: Гейт §7.2 — bare-вопросы rk3588** (real_image.rs)

```rust
#[test]
#[ignore = "requires rk3588 image; run with UEFIPATCHER_TEST_FW=refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img"]
fn real_image_rk3588_bare_questions_resolve() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "rk", "s1").expect("parse_image");
    let forms = uefi_engine::hii::forms::collect_forms(&img);
    assert!(!forms.is_empty(), "rk3588: expected bare forms");
    let mut forms_with_questions = 0usize;
    let mut info_resolved = 0usize;
    for f in &forms {
        let Ok(qs) = uefi_engine::hii::list_questions(&img, &f.form_id, f.form_id_ifr as u16) else {
            continue;
        };
        if qs.is_empty() {
            continue;
        }
        forms_with_questions += 1;
        let item = format!("{}#{}:{:#x}", f.form_id, f.form_id_ifr, qs[0].question_id);
        if uefi_engine::hii::question_info(&img, &item).is_ok() {
            info_resolved += 1;
        }
    }
    assert!(
        forms_with_questions > 0,
        "bare targets must expose questions (was NotFound/empty pre-fix)"
    );
    assert!(info_resolved > 0, "question_info must resolve on bare targets");
    eprintln!(
        "rk3588: {forms_with_questions}/{} forms with questions, {info_resolved} question_info resolved",
        forms.len()
    );
}
```

- [ ] **Step 3: Гейт §7.3 (опциональный) — коллизии string_id 450x** (real_image.rs)

```rust
#[test]
#[ignore = "requires external real AMI image under refs/amibcp/ (gitignored)"]
fn real_amibcp_450x_string_id_sources_distinguishable() {
    let data = std::fs::read(amibcp_path()).unwrap();
    let img = parse_image(&data, ImageMode::Read, "s1", "s2").unwrap();
    let strings = uefi_engine::hii::strings::collect_strings(&img);
    for sid in [3u32, 4u32] {
        let sources: std::collections::HashSet<&str> = strings
            .iter()
            .filter(|s| s.string_id == sid)
            .map(|s| s.source.as_str())
            .collect();
        assert!(
            sources.len() >= 2,
            "string_id {sid} expected in >=2 package lists (UiApp vs Setup), got {sources:?}"
        );
    }
}
```

- [ ] **Step 4: Компиляция + обычный прогон**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
```
Expected: PASS (гейты `#[ignore]` не запускаются).

- [ ] **Step 5: Явный запуск гейтов**

```bash
cargo test -p uefi-engine --test real_image -- --ignored real_image_hii --nocapture
UEFIPATCHER_TEST_FW=refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img \
  cargo test -p uefi-engine --test real_image -- --ignored real_image_rk3588_bare_questions_resolve --nocapture
cargo test -p uefi-engine --test real_image -- --ignored real_amibcp_450x_string_id_sources_distinguishable --nocapture
```
Expected: все PASS. Сверить счётчики `real_image_hii_forms_and_strings` с базой из Task 2 Step 2: **сдвиг = ложное срабатывание новых веток** (EXT на живых образах не встречается, §7) — при сдвиге остановиться и разбираться (systematic-debugging), НЕ коммитировать.

- [ ] **Step 6: Финальный полный чек workspace**

```bash
cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check
```

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(hii): real-гейты read-truth — source на HNX, bare-вопросы rk3588, 450x коллизии"
```

---

## Completion

Ветка `hii-read-truth` готова: все задачи зелены, real-гейты пройдены (HNX счётчики не сдвинуты). Дальше — superpowers:finishing-a-development-branch (merge/PR решение владельца). Known-behavior, зафиксированный спекой §5/§6 и не требующий кода: асимметрия gates_list (донорские REF-гейты на bare-цель не видны), unlock на bare — молчаливое «0 gates» до Б-цикла, `title_source` — до живого прецедента.
