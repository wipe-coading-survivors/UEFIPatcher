# hii-errors-cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Класс C — путающие ошибки HII: единый порядок ошибок (InvalidItemId/NotFound/NotWritable), warn на skip вскрытых гейтов и bare-only no-op, NotASetupItem для non-HII целей, условный flush set_value, подсказка при form-level unlock, диагностика transport-ошибки клиента (путь+источник сокета) + сопутствующие minors.

**Architecture:** Точечные фиксы в трёх слоях: uefi-engine (порядок ошибок + дискриминация + warn), rpc/server (RPC-маппинг InvalidItemId, условный flush, де-клон hii_unlock, tracing-паритет), клиенты uefi-cli/uefi-tui/uefi-common (note-подсказка, SockSource). Два docs-коммита в master ДО ветки (фикс дефекта спеки + актуализация TODO), закрытие TODO — последним коммитом ветки. Побайтового поведения живых образов НЕ меняем (кроме уже покрытых идемпотентных no-op путей).

**Tech Stack:** Rust workspace (edition 2024), thiserror, tonic (invalid_argument), tracing + tracing-test 0.2, assert_cmd (e2e).

**Spec:** `docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md` (коммит `6be015e`)

## Global Constraints

- **Ветка `hii-errors-cleanup`** — весь код только в ней (от master). Task 1–2 — docs-коммиты в master ДО ветки; Task 14 — docs-коммит на ветке (уйдёт в master вместе с мерджем).
- TDD строго: (1) failing-тест → (2) `cargo test` red → (3) реализация → (4) green → (5) commit.
- После каждой задачи: `cargo test -p <crate>`, `cargo clippy -p <crate> -- -D warnings`, `cargo fmt --all -- --check`.
- Комментарии в коде — только rustdoc `///`-контракты на pub/pub(crate)-функциях (что делает / чего НЕ делает + `спека hii-errors-cleanup §N`); narration-комментарии нет.
- Warn — не Err: идемпотентность unlock/set_value сохраняется во всех новых ветках.
- Real-гейты (`#[ignore]`) в обычном прогоне не запускаются; smoke живого образа — Task 14.
- Дефекты плана/спеки, найденные в шаге — отдельный docs-коммит ДО реализации (AGENTS.md правило 11).
- Один коммит на задачу; сообщения — из плана.
- Номера TODO-строк — по master на 2026-09-26 (TODO.md 4418 строк); при смещении искать по заголовку записи: `:1322` set_item_visibility precedence, `:1369` hii_unlock клон ради touch, `:1374` gates.len() <= 1, `:1387` non-HII Ok-пусто, `:1409` transport error без сокета, `:2143` plan_gates неидемпотентен, `:2884` третий QuestionInfo-литерал, `:2890` set_value NotWritable до парсинга, `:2919` set_value flush на no-op, `:2997` form-unlock не каскадирует, `:3013` qid сценария B, `:3510` list_questions без info, `:3563` form_tree асимметрия.

## Контекст для исполнителя (файлы и точки)

- `crates/uefi-engine/src/hii/mod.rs` — `HiiError` (`:33-73`); `set_item_visibility` (`:75-143`, parse-ветка `:81-94`, голый target `:90`); `parse_item_id` (`:172-187`, все `.ok_or(HiiError::NotFound)`); `resolve_writable_path` (`:189-214`, mode-чек `:193-195` ДО find `:196-197`); `form_package_ranges` (`:219`, мутационный селектор); `form_package_ranges_read` (`:240`, read-селектор, суперсет); `gates_list` (`:359-397`, read-only); `unlock` (`:409-486`, own-фаза `:420-471`, вызов plan_gates_skip_unlocked `:436`); `apply_cross_formset_gates` (`:503-571`, plan `:537`); `find_question_map` (`:691-719`, без mode-чека); `ValueOutcome.applied` (`:881-886`); `set_value` (`:986-1060`, mode-чек `:988-990` ДО parse `:991`); `expr_text` (`:275-291`); `pkg_off` (`:305`). Тесты: фикстуры `g_opcode`/`g_form`/`g_ref`/`g_one_of`/`g_end`/`forms_pkg`/`vendor_forms_pkg` (`:3164-3257`), `vendor_image_with` (`:3259-3272`, mode=Write), `VENDOR_FORM_ITEM`/`VENDOR_QUESTION_ITEM` (`:3285-3286`), gates_list-тесты (`:3330-3389`), unlock-тесты (`:3729-3789`, `unlock_skips_already_unlocked_gate` `:3763`), set_value-тесты (`:4703-4753`).
- `crates/uefi-engine/src/hii/gates.rs` — `GateExpr` (`:40-51`), `find_gates` (`:151-197`, emit на statement-опе — гейты рождаются ДО хвоста пакета), `emit_gates` (`:199-272`), `plan_flip` (`:312-364`), `is_unlocked_expr` (`:393-399`, приватный), `plan_gates_skip_unlocked` (`:401-429`). Тесты: фикстуры `opcode`/`form`/`ref_op`/`vendor_ifr` (`:461-537`), `FORM_GATE_TARGET` (`:561`), `find_gates_stops_gracefully_on_truncated_package` (`:885-891`, слабый `<= 1`).
- `crates/uefi-engine/src/hii/ifr.rs` — `package_bounds` (`:17-24`, `(4, plen.min(len))` — обрезка хвоста не двигает начало).
- `crates/uefi-engine/src/rpc/server.rs` — `hii_error_status` (`:54-71`), `hii_error_status_ctx` (`:73-78`, ctx-обогащение только NotFound); `hii_list_forms` (`:810-819`), `hii_list_strings` (`:849-858`), `hii_gates_list` (`:1053-1064`, образец info-строки `:1062`), `hii_unlock` (`:1067-1087`, get_or_load_image `:1069`, условный flush `:1078-1080`), `hii_list_questions` (`:1106-1119`, без info), `hii_set_value` (`:1122-1144`, безусловный flush `:1136`). Тесты: `hii_error_status_maps_*` (`:1537-1595`), `amibcp_450x_path` (`:2772-2778`), `hii_unlock_noop_keeps_artifact_untouched` (`:2782-2827`, `#[ignore]`), `fv_image_with_two_files` (`:2850-2867`), `image_upload_roundtrip` (`:2870-2893`, поля ImageUploadRequest: session_id/data/mode/name).
- `crates/uefi-cli/src/output.rs` — `print_unlock` (`:298-317`); tests-mod `mock_question` (`:963-998`), инлайн-литералы QuestionInfo (`:1019` — тест defaults, НЕ трогать; `:1063` — третий дубль, удалить).
- `crates/uefi-cli/src/client.rs` — `connect` (`:29-46`).
- `crates/uefi-cli/tests/e2e.rs` — хелпер `cli()` (`:9-13`), `hii_gates_and_unlock_output_content` (`:245-287`).
- `crates/uefi-cli/tests/mock_server.rs` — mock `hii_unlock` (`:332-352`), `mock_question` (`:481`).
- `crates/uefi-common/src/state.rs` — `resolve_sock` (`:54-65`), тесты приоритетов (`:184-216`, паттерн `env_guard`/`lock_guard`).
- `crates/uefi-tui/src/commands.rs` — unlock-arm `:hii` (`:1090-1115`), префилл-хелперы (`:1993-2028`).
- `crates/uefi-tui/tests/tui_integration.rs` — `hii_verbs_visibility_setvalue_unlock` (`:339-399`, unlock form-таргета `:379-382`).
- Противо-пример warn-теста: `crates/uefi-engine/src/hii/strings.rs:884` (`#[tracing_test::traced_test]` + `logs_contain`), tracing-test 0.2 уже в dev-deps uefi-engine.
- Фикстура bare-пакета: `crates/uefi-engine/tests/fixtures/hii_rk3588_bare_form.bin` (include_bytes-паттерн `pe_resource.rs:882`).

## Дефекты спеки, найденные при разведке плана (закрываются Task 1)

1. **§6 TODO:1374 «0 гейтов» — неверно.** `find_gates` emit'ит гейт в момент statement-опа внутри скоупа (`gates.rs:181`): suppress-ref гейт формы 10029 рождается на `ref_op` задолго до обрезанного хвоста (`package_bounds` = `(4, plen.min(len))`, `ifr.rs:17-24` — обрезка трёх хвостовых байт END не влияет). Точное ожидание теста — **1 гейт**, не 0.
2. **§4 текст note** содержит схемы `<item_id>`/`<target>#<form>:<qid>`: для form-таргета `item_id == <target>#<form>`, поэтому в реализации подставляем конкретный item_id: `list: hii question gates {item_id}, unlock: hii question unlock {item_id}:<qid>` (плейсхолдер `<qid>` остаётся — qid неизвестен). TUI — та же note одной строкой в status_msg.

---

### Task 1: Docs №0 — фикс дефекта спеки §6 (master)

**Files:**
- Modify: `docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md` (§6, строки 133-135)

**Interfaces:** — (чистая docs-правка, код не трогаем)

- [ ] **Step 1: Исправить ожидание обрезанного пакета**

Заменить в §6 строку:

```markdown
- **TODO:1374** — тест обрезанного пакета в gates.rs ассертит
  `gates.len() <= 1`: захардкодить точное ожидание (0 гейтов,
  under-walk-невозможность) вместо слабого предиката.
```

на:

```markdown
- **TODO:1374** — тест обрезанного пакета в gates.rs ассертит
  `gates.len() <= 1`: захардкодить точное ожидание (1 гейт —
  suppress-ref формы 10029: walker emit'ит гейт на statement-опе
  до обрезанного хвоста, under-walk невозможен; фикс плана
  2026-09-26 по аудиту find_gates/emit_gates/package_bounds)
  вместо слабого предиката.
```

- [ ] **Step 2: Зафиксировать подстановку item_id в note §4**

После абзаца «Отступление от согласованного эскиза» (строки 111-115) дописать:

```markdown
Детализация текста note (план): для form-таргета `item_id ==
<target>#<form>`, поэтому печатаются конкретные значения —
`list: hii question gates {item_id}, unlock: hii question unlock
{item_id}:<qid>` (плейсхолдер `<qid>` остаётся: qid неизвестен).
TUI — та же note одной строкой в status_msg.
```

- [ ] **Step 3: Коммит**

```bash
git add docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md
git commit -m "docs(spec): fix hii-errors-cleanup §6 (точное ожидание обрезанного пакета — 1 гейт, не 0) и §4 (подстановка item_id в note)"
```

---

### Task 2: Docs №1 — актуализация TODO (master)

**Files:**
- Modify: `TODO.md` (записи `:2143`, `:3013`, `:1322`, `:2890`, `:1387`, `:2997`, `:2919`, `:1374`, `:2884`, `:1369`, `:3510`, `:3563`, `:1409`)

**Interfaces:** — (чистая docs-правка)

- [ ] **Step 1: Закрыть устаревшую 3013 (факт зафиксирован, код не меняется)**

Запись `:3013` — заменить `[ ]` на `[x]` и дописать в конец записи:

```markdown
  Закрыто: факт зафиксирован спекой hii-errors-cleanup §4
  (docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md);
  код не меняется — docs-коммит цикла.
```

- [ ] **Step 2: Скорректировать устаревшее утверждение 2143 и пометить в цикл**

Запись `:2143` (`plan_gates неидемпотентен`) — после существующего текста дописать (не закрывая):

```markdown
  Аудит 2026-09-26: повторный unlock уже не падает — own/кросс-фазы
  и hijack используют plan_gates_skip_unlocked (сделано циклами
  formset-unlock и hii-write-guard). Остаток — tracing::warn! при
  skip вскрытого гейта; в цикле hii-errors-cleanup (спека
  docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md §2).
```

- [ ] **Step 3: Пометить остальные 10 записей «в цикле»**

В каждую из записей `:1322`, `:2890`, `:1387`, `:2997`, `:2919`, `:1374`, `:2884`, `:1369`, `:3510`, `:3563`, `:1409` дописать строкой ниже (с тем же отступом продолжения записи), с указанием своего параграфа спеки:

```markdown
  В цикле hii-errors-cleanup (спека
  docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md
  §<N: 1322→1, 2890→1, 1387→3, 2997→4, 2919→5, 1374→6, 2884→6, 1369→6, 3510/3563→6, 1409→7>).
```

- [ ] **Step 4: Коммит**

```bash
git add TODO.md
git commit -m "docs(todo): hii-errors-cleanup — 3013 закрыт (факт), 2143 скорректирован (уже не падает), 11 позиций помечены в цикл"
```

---

### Task 3: Engine — `HiiError::InvalidItemId` + parse_item_id (спека §1)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`HiiError` `:33-73`, `parse_item_id` `:172-187`)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_error_status` `:54-71`, тесты `:1537`)
- Test: `crates/uefi-engine/src/hii/mod.rs` (tests-mod), `crates/uefi-engine/src/rpc/server.rs` (tests-mod)

**Interfaces:**
- Consumes: `parse_item_id` сигнатура не меняется: `(item_id: &str) -> Result<(crate::types::Target, u16, Option<u16>), HiiError>`.
- Produces: `HiiError::InvalidItemId(String)` (payload — сегмент, на котором parse упал: весь item_id если нет `#`; `form[:qid]`-часть если не спарсился form/qid; target-часть если не спарсился target). RPC-маппинг: invalid_argument. Все parse_item_id-потребители (`set_item_visibility`, `gates_list`, `unlock`, `question_info`, `set_value`, `list_questions`, `form_export.rs:72`, `form_hijack.rs:85`) автоматически получают новый класс ошибки.

- [ ] **Step 1: Failing-тесты на parse_item_id**

В tests-mod mod.rs заменить тест `parse_item_id_rejects_garbage` (`:3301-3306`) на:

```rust
    #[test]
    fn parse_item_id_rejects_garbage() {
        assert!(matches!(
            parse_item_id("no-discriminator"),
            Err(HiiError::InvalidItemId(_))
        ));
        assert!(matches!(
            parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#nope"),
            Err(HiiError::InvalidItemId(_))
        ));
        assert!(matches!(
            parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:zz"),
            Err(HiiError::InvalidItemId(_))
        ));
        let err =
            parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#nope").unwrap_err();
        assert!(
            err.to_string()
                .contains("expected `<target>[#<form>[:<qid>]]`")
        );
    }
```

- [ ] **Step 2: Запустить, убедиться в падении**

Run: `cargo test -p uefi-engine parse_item_id`
Expected: FAIL — `InvalidItemId` не существует (E0433/E0599 компиляция tests).

- [ ] **Step 3: Реализация — вариант enum + parse_item_id**

В `HiiError` после варианта `NotFound` (`:35-36`) вставить:

```rust
    #[error("malformed item_id {0}: expected `<target>[#<form>[:<qid>]]`")]
    InvalidItemId(String),
```

`parse_item_id` (`:172-187`) целиком заменить на:

```rust
pub(crate) fn parse_item_id(
    item_id: &str,
) -> Result<(crate::types::Target, u16, Option<u16>), HiiError> {
    let (target_str, disc) = item_id
        .rsplit_once('#')
        .ok_or_else(|| HiiError::InvalidItemId(item_id.to_string()))?;
    let (form_str, qid_str) = match disc.split_once(':') {
        Some((f, q)) => (f, Some(q)),
        None => (disc, None),
    };
    let form_id = form_str
        .parse::<u16>()
        .map_err(|_| HiiError::InvalidItemId(disc.to_string()))?;
    let question_id = match qid_str {
        Some(q) => Some(
            parse_u16_loose(q).ok_or_else(|| HiiError::InvalidItemId(disc.to_string()))?,
        ),
        None => None,
    };
    let target = crate::parser::target::parse_target(target_str)
        .map_err(|_| HiiError::InvalidItemId(target_str.to_string()))?;
    Ok((target, form_id, question_id))
}
```

- [ ] **Step 4: Тест зелёный**

Run: `cargo test -p uefi-engine parse_item_id`
Expected: PASS. (Красный каскад: `set_item_visibility_rejects_malformed_discriminator` ассертил NotFound на `#notanumber` — обновить на InvalidItemId в этом же шаге. Дефект сноски «таких тестов нет» найден при реализации, фикс плана 2026-09-26.)

- [ ] **Step 5: Failing-тест RPC-маппинга**

В tests-mod server.rs рядом с `hii_error_status_maps_preconditions_not_found_and_bad_target` (`:1537`) добавить:

```rust
    #[test]
    fn hii_error_status_maps_invalid_item_id() {
        let st = hii_error_status(crate::hii::HiiError::InvalidItemId("x#y".into()));
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        assert!(st.message().contains("malformed item_id"));
    }

    #[test]
    fn hii_error_status_ctx_does_not_enrich_invalid_item_id() {
        let st = hii_error_status_ctx(crate::hii::HiiError::InvalidItemId("x#y".into()), "0#99");
        assert_eq!(st.code(), tonic::Code::InvalidArgument);
        assert!(
            !st.message().contains("0#99"),
            "ctx-обогащение — только NotFound (спека §1)"
        );
    }
```

- [ ] **Step 6: RED → реализация → GREEN**

Запустить (`cargo test -p uefi-engine hii_error_status` — red), затем в `hii_error_status` (`:59-61`) ветку invalid_argument расширить:

```rust
        crate::hii::HiiError::NotASetupItem
        | crate::hii::HiiError::InvalidItemId(_)
        | crate::hii::HiiError::InvalidSchema(_)
        | crate::hii::HiiError::HidingUnsupported => Status::invalid_argument(e.to_string()),
```

`hii_error_status_ctx` не менять. Run: `cargo test -p uefi-engine hii_error_status`
Expected: PASS (2 теста).

- [ ] **Step 7: Полный прогон + коммит**

```bash
cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/mod.rs crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): HiiError::InvalidItemId — parse_item_id различает мусорный item_id (RPC invalid_argument)"
```

---

### Task 4: Engine — единый порядок ошибок: malformed → NotFound → NotWritable (спека §1, TODO:1322/2890)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`resolve_writable_path` `:189-214`, `set_value` `:986-995`, `set_item_visibility` `:81-94`)
- Test: `crates/uefi-engine/src/hii/mod.rs` (tests-mod, после `gates_list_unknown_target_is_not_found` `:3383-3389`)

**Interfaces:**
- Consumes: `HiiError::InvalidItemId` из Task 3.
- Produces: контракт порядка для всех ops: (1) malformed → `InvalidItemId`; (2) цель не резолвится → `NotFound`; (3) `ImageMode != Write` → `NotWritable`; (4) барьер `MutationBehindCompression` и семантика — как сегодня. `resolve_writable_path` сигнатура не меняется; reorder внутри автоматически распространяется на все её call-сайты (`:94`, `:411`, `:532`, `:1633`, `:1763`, `:1831`, `:2033`, `:2114`, `:2198`).

- [ ] **Step 1: Failing-тесты — матрица 5 кейсов × 3 ops**

В tests-mod mod.rs добавить:

```rust
    const MALFORMED_ITEM: &str = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#nope";
    const UNKNOWN_TARGET_ITEM: &str = "00000000-0000-0000-0000-000000000001:0x19:0#10029";

    fn read_mode_image() -> Image {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        image.mode = ImageMode::Read;
        image
    }

    fn set_value_read_mode_image() -> Image {
        let mut image = image_with_nvar_stores();
        image.mode = ImageMode::Read;
        image
    }

    #[test]
    fn unlock_error_order_contract() {
        let mut r = read_mode_image();
        assert!(matches!(unlock(&mut r, MALFORMED_ITEM), Err(HiiError::InvalidItemId(_))));
        assert!(matches!(unlock(&mut r, UNKNOWN_TARGET_ITEM), Err(HiiError::NotFound)));
        assert!(matches!(unlock(&mut r, VENDOR_FORM_ITEM), Err(HiiError::NotWritable)));
        let mut w = vendor_image_with(0x19, vendor_forms_pkg());
        assert!(matches!(unlock(&mut w, UNKNOWN_TARGET_ITEM), Err(HiiError::NotFound)));
        assert!(matches!(unlock(&mut w, MALFORMED_ITEM), Err(HiiError::InvalidItemId(_))));
    }

    #[test]
    fn set_value_error_order_contract() {
        let mut r = set_value_read_mode_image();
        assert!(matches!(
            set_value(&mut r, MALFORMED_ITEM, 1),
            Err(HiiError::InvalidItemId(_))
        ));
        assert!(matches!(
            set_value(&mut r, UNKNOWN_TARGET_ITEM, 1),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            set_value(&mut r, VENDOR_QUESTION_ITEM, 1),
            Err(HiiError::NotWritable)
        ));
        let mut w = image_with_nvar_stores();
        assert!(matches!(
            set_value(&mut w, UNKNOWN_TARGET_ITEM, 1),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            set_value(&mut w, MALFORMED_ITEM, 1),
            Err(HiiError::InvalidItemId(_))
        ));
    }

    #[test]
    fn set_item_visibility_error_order_contract() {
        let mut r = read_mode_image();
        assert!(matches!(
            set_item_visibility(&mut r, MALFORMED_ITEM, true),
            Err(HiiError::InvalidItemId(_))
        ));
        assert!(matches!(
            set_item_visibility(&mut r, UNKNOWN_TARGET_ITEM, true),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            set_item_visibility(&mut r, VENDOR_FORM_ITEM, true),
            Err(HiiError::NotWritable)
        ));
        let mut w = vendor_image_with(0x19, vendor_forms_pkg());
        assert!(matches!(
            set_item_visibility(&mut w, UNKNOWN_TARGET_ITEM, true),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            set_item_visibility(&mut w, MALFORMED_ITEM, true),
            Err(HiiError::InvalidItemId(_))
        ));
    }
```

> Дефект-фикс 2026-09-26: Read+NotWritable кейс `set_value_error_order_contract` изначально использовал `read_mode_image()` (plain `vendor_forms_pkg`), где вопрос 0x3B принципиально не резолвится: `g_one_of` даёт 12-байтный statement (payload 10 + заголовок 2), а `values::find_question` требует `len >= 13` — `find_question_map` вернул бы NotFound раньше mode-чека. Кейсы set_value переведены на `image_with_nvar_stores()` (вопрос резолвится, см. `question_info_reports_4g_like_question`); unlock/visibility остаются на plain-фикстуре — VENDOR_FORM_ITEM там резолвится.

- [ ] **Step 2: RED**

Run: `cargo test -p uefi-engine error_order_contract`
Expected: FAIL — Read+unknown даёт NotWritable (mode-чек раньше find), set_value Read+malformed даёт NotWritable.

- [ ] **Step 3: Реализация — reorder resolve_writable_path**

`resolve_writable_path` (`:189-214`): перенести find до mode-чека:

```rust
fn resolve_writable_path(
    image: &Image,
    target: &crate::types::Target,
) -> Result<Vec<usize>, HiiError> {
    let path =
        crate::parser::target::find_item_path(&image.root, target).ok_or(HiiError::NotFound)?;
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
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
```

- [ ] **Step 4: Реализация — set_value: mode после find**

В `set_value` (`:986-995`) удалить mode-чек `:988-990` и вставить его после `find_question_map`:

```rust
pub fn set_value(image: &mut Image, item_id: &str, value: u64) -> Result<ValueOutcome, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let Some(question_id) = question_id else {
        return Err(HiiError::NotFound);
    };
    let (map, texts) = find_question_map(image, &target, form_id, question_id)?;
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let info = question_info_proto(image, form_id, &map, &texts);
```

- [ ] **Step 5: Реализация — set_item_visibility: голый target тоже InvalidItemId**

В `set_item_visibility` (`:89-92`) ветку без `#` заменить на:

```rust
        None => (
            crate::parser::target::parse_target(item_id)
                .map_err(|_| HiiError::InvalidItemId(item_id.to_string()))?,
            None,
        ),
```

- [ ] **Step 6: GREEN + полный прогон**

Run: `cargo test -p uefi-engine`
Expected: PASS. Возможные красные: тесты, ожидавшие NotWritable при Read+unknown target — в mod.rs таких нет (`unlock`/`set_value`/`visibility` NotWritable-тесты используют валидные item'ы).

- [ ] **Step 7: Коммит**

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): единый порядок ошибок HII-мутаторов — InvalidItemId → NotFound → NotWritable (TODO:1322, 2890)"
```

---

### Task 5: Engine — warn при skip вскрытого гейта + pub(crate) предикат (спека §2, TODO:2143)

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs` (`is_unlocked_expr` `:393-399`)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (own-фаза unlock `:430-453`, кросс-фаза `:530-543`)
- Test: `crates/uefi-engine/src/hii/gates.rs` (tests-mod)

**Interfaces:**
- Produces: `pub(crate) fn is_unlocked_expr(expr: &GateExpr) -> bool` — EqConst `a≠b` или EqIdVal `value == 0xFFFF`. Warn `gate already unlocked — skipped` с полями `offset` (pkg+…) и `expr` (текст выражения) — только лог, без изменения семантики.

- [ ] **Step 1: Failing-тест предиката**

В tests-mod gates.rs рядом с `plan_gates_skip_unlocked_passes_already_unlocked` (`:1242`) добавить:

```rust
    #[test]
    fn is_unlocked_expr_covers_all_expression_classes() {
        assert!(is_unlocked_expr(&GateExpr::EqConst { a: 1, b: 2 }));
        assert!(!is_unlocked_expr(&GateExpr::EqConst { a: 1, b: 1 }));
        assert!(is_unlocked_expr(&GateExpr::EqIdVal {
            question_id: 0x9A,
            value: 0xFFFF
        }));
        assert!(!is_unlocked_expr(&GateExpr::EqIdVal {
            question_id: 0x9A,
            value: 1
        }));
        assert!(!is_unlocked_expr(&GateExpr::True));
        assert!(!is_unlocked_expr(&GateExpr::Other));
    }
```

- [ ] **Step 2: тест → pub(crate)**

Run: `cargo test -p uefi-engine is_unlocked_expr` → PASS сразу: tests-mod инлайн в gates.rs (descendant-модуль видит private), RED по приватности здесь не наблюдается. Приватность ломается только на sibling-потребителе — warn-циклы mod.rs из Step 3/4 не скомпилируются без pub(crate); pub(crate) вводится именно под них. В gates.rs заменить `fn is_unlocked_expr` на:

```rust
/// Аппаратно-вскрытое выражение гейта: EqConst с a≠b (константа сдвинута)
/// или EqIdVal со значением 0xFFFF. Спека hii-errors-cleanup §2.
pub(crate) fn is_unlocked_expr(expr: &GateExpr) -> bool {
    match expr {
        GateExpr::EqConst { a, b } => a != b,
        GateExpr::EqIdVal { value, .. } => *value == 0xFFFF,
        _ => false,
    }
}
```

Run: `cargo test -p uefi-engine is_unlocked_expr` → PASS.

- [ ] **Step 3: Warn в own-фазе unlock**

В `unlock` (mod.rs `:430-436`) между `if found.is_empty() { continue; }` и `match gates::plan_gates_skip_unlocked(...)` вставить:

```rust
                for gate in &found {
                    if gates::is_unlocked_expr(&gate.expr) {
                        let region = pkg
                            .get(gate.expr_offset..gate.expr_end.min(pkg.len()))
                            .unwrap_or(&[]);
                        tracing::warn!(
                            offset = %pkg_off(gate.scope_offset),
                            expr = %expr_text(&gate.expr, region),
                            "gate already unlocked — skipped"
                        );
                    }
                }
```

- [ ] **Step 4: Warn в кросс-фазе**

В `apply_cross_formset_gates` (mod.rs `:537`) перед `let flips = gates::plan_gates_skip_unlocked(&pkg, &site.gates)` вставить тот же цикл:

```rust
            for gate in &site.gates {
                if gates::is_unlocked_expr(&gate.expr) {
                    let region = pkg
                        .get(gate.expr_offset..gate.expr_end.min(pkg.len()))
                        .unwrap_or(&[]);
                    tracing::warn!(
                        offset = %pkg_off(gate.scope_offset),
                        expr = %expr_text(&gate.expr, region),
                        "gate already unlocked — skipped"
                    );
                }
            }
```

- [ ] **Step 5: GREEN + полный прогон + коммит**

Run: `cargo test -p uefi-engine`
Expected: PASS — `unlock_skips_already_unlocked_gate` (`:3763`) зелёный (warn не меняет исход), `unlock_flips_cross_formset_gate_in_donor_section` (`:3792`) зелёный.

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/gates.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): unlock предупреждает warn'ом о skip вскрытых гейтов (pub(crate) is_unlocked_expr, TODO:2143)"
```

---

### Task 6: Engine — NotASetupItem для non-HII цели + warn bare-only (спека §3, TODO:1387)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`gates_list` `:359-377`, own-фаза `unlock` `:420-430`)
- Test: `crates/uefi-engine/src/hii/mod.rs` (tests-mod)

**Interfaces:**
- Consumes: `form_package_ranges_read` (`:240`) — read-селектор (resource+bare+RAW+0x18), `form_package_ranges` (`:219`) — мутационный.
- Produces: контракт §3 — Section-цель с пустым read-селектором → `NotASetupItem` в `gates_list` и own-фазе `unlock`; форм-пакеты есть, гейтов нет → Ok пусто; bare-only PE32 (read непуст, мутационный пуст) → Ok без мутации + warn `bare channel is not mutable`.

- [ ] **Step 1: Failing-тесты**

В tests-mod mod.rs добавить (после `gates_list_unknown_target_is_not_found`):

```rust
    #[test]
    fn gates_list_non_hii_section_is_not_a_setup_item() {
        let mut image = vendor_image_with(0x19, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        image.mode = ImageMode::Read;
        assert!(matches!(
            gates_list(&image, VENDOR_FORM_ITEM),
            Err(HiiError::NotASetupItem)
        ));
    }

    #[test]
    fn gates_list_form_without_gates_is_ok_empty() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        image.mode = ImageMode::Read;
        let gates = gates_list(&image, "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10002")
            .unwrap();
        assert!(gates.is_empty(), "форма без гейтов — легитимный пустой ответ");
    }

    #[test]
    fn unlock_non_hii_section_is_not_a_setup_item() {
        let mut image = vendor_image_with(0x19, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(matches!(
            unlock(&mut image, VENDOR_FORM_ITEM),
            Err(HiiError::NotASetupItem)
        ));
    }

    #[test]
    fn gates_list_bare_only_pe32_is_ok() {
        let mut body = vec![0x11u8; 64];
        body.extend_from_slice(RK3588_BARE_FORM);
        body.extend_from_slice(&[0x22; 32]);
        let mut image = vendor_image_with(0x10, body);
        image.mode = ImageMode::Read;
        assert!(
            gates_list(&image, "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029").is_ok(),
            "bare-канал читается — пустой Ok, не ошибка"
        );
    }

    #[tracing_test::traced_test]
    fn unlock_bare_only_pe32_warns_and_noops() {
        let mut body = vec![0x11u8; 64];
        body.extend_from_slice(RK3588_BARE_FORM);
        body.extend_from_slice(&[0x22; 32]);
        let expected = body.clone();
        let mut image = vendor_image_with(0x10, body);
        let outcome =
            unlock(&mut image, "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029").unwrap();
        assert!(outcome.applied.is_empty());
        assert_eq!(
            image.root.children[0].children[0].children[0].body, expected,
            "bare-канал не мутабелен — байты нетронуты"
        );
        assert!(logs_contain("bare channel is not mutable"));
    }
```

И рядом с константами `VENDOR_FORM_ITEM` (`:3285`) добавить фикстуру:

```rust
    const RK3588_BARE_FORM: &[u8] =
        include_bytes!("../../tests/fixtures/hii_rk3588_bare_form.bin");
```

- [ ] **Step 2: RED**

Run: `cargo test -p uefi-engine gates_list_non_hii unlock_non_hii bare_only`
Expected: FAIL — non-HII сейчас Ok-пусто; traced-тест без warn-строки.

- [ ] **Step 3: Реализация — gates_list**

В `gates_list` (`:363-365`) после проверки `node.node_type != FfsType::Section` добавить:

```rust
    if form_package_ranges_read(node).is_empty() {
        return Err(HiiError::NotASetupItem);
    }
```

- [ ] **Step 4: Реализация — own-фаза unlock**

В own-фазе `unlock` (`:423-426`) после `if node.node_type != FfsType::Section { return Err(HiiError::NotASetupItem); }` и до `let ranges = form_package_ranges(node);` вставить:

```rust
            if form_package_ranges_read(node).is_empty() {
                return Err(HiiError::NotASetupItem);
            }
            let ranges = form_package_ranges(node);
            if ranges.is_empty() {
                tracing::warn!(
                    "form packages visible via read-only bare channel; bare channel is not mutable"
                );
            }
```

(существующую строку `let ranges = form_package_ranges(node);` заменить приведённым блоком).

- [ ] **Step 5: GREEN + полный прогон**

Run: `cargo test -p uefi-engine`
Expected: PASS. Существующие тесты не задеты: `unlock_no_gates_is_ok_empty` (`:3730`, read-селектор непуст), `gates_list_works_behind_non_recompressable_wrapper` (`:3351`, read-селектор непуст), `gates_list_bare_channel_reports_form_and_question_gates` (`:3330`, form-пакет в RAW).

- [ ] **Step 6: Коммит**

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(uefi-engine): non-HII цель — NotASetupItem в gates_list/unlock; warn на bare-only PE32 no-op (TODO:1387)"
```

---

### Task 7: RPC — условный flush в hii_set_value (спека §5, TODO:2919)

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_set_value` `:1136`)
- Test: `crates/uefi-engine/src/rpc/server.rs` (tests-mod, рядом с `hii_unlock_noop_keeps_artifact_untouched` `:2782`)

**Interfaces:**
- Consumes: `ValueOutcome.applied: Vec<String>` (mod.rs `:881-886`), образец условного flush `hii_unlock` (`:1078-1080`).
- Produces: серия «no-op RPC не должен писать на диск» закрыта для всех HII-мутаторов.

- [ ] **Step 1: Failing-тест (#[ignore], живой AMI-образ)**

В tests-mod server.rs добавить:

```rust
    #[tokio::test]
    #[ignore = "requires real AMI image under refs/amibcp/ (gitignored)"]
    async fn hii_set_value_noop_keeps_artifact_untouched() {
        let (td, mut client) = setup().await;
        let orig = amibcp_450x_path();
        let orig_bytes = std::fs::read(&orig).unwrap();
        let session = create_session(&mut client).await;
        let opened = client
            .image_open(ImageOpenRequest {
                session_id: session.clone(),
                path: orig.to_string_lossy().to_string(),
                mode: ImageMode::Write as i32,
                name: "450x".into(),
            })
            .await
            .unwrap()
            .into_inner();
        let target = "abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0";
        let qs = client
            .hii_list_questions(HiiListQuestionsRequest {
                image_id: opened.image_id.clone(),
                target: target.into(),
                form_id: 1,
            })
            .await
            .unwrap()
            .into_inner()
            .questions;
        let seeded = qs
            .iter()
            .find(|q| q.seed_value.is_some() || q.ifr_default.is_some())
            .expect("на форме #1 AMI-образа есть вопрос с seed/default");
        let value = seeded.seed_value.or(seeded.ifr_default).unwrap();
        let item = format!("{target}#1:{:#x}", seeded.question_id);
        let first = client
            .hii_set_value(HiiSetValueRequest {
                image_id: opened.image_id.clone(),
                item_id: item.clone(),
                value,
            })
            .await
            .unwrap()
            .into_inner();
        let img_path = td
            .path()
            .join("sessions")
            .join(&session)
            .join("images")
            .join(format!("{}.bin", opened.image_id));
        let before = std::fs::read(&img_path).unwrap();
        let mtime_before = std::fs::metadata(&img_path).unwrap().modified().unwrap();
        let second = client
            .hii_set_value(HiiSetValueRequest {
                image_id: opened.image_id.clone(),
                item_id: item.clone(),
                value,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(
            second.applied_flips.is_empty(),
            "повтор того же value — no-op"
        );
        let after = std::fs::read(&img_path).unwrap();
        assert_eq!(after.len(), before.len());
        assert_eq!(
            after, before,
            "no-op set_value не должен переписывать артефакт"
        );
        assert_eq!(
            std::fs::metadata(&img_path).unwrap().modified().unwrap(),
            mtime_before,
            "mtime не тронут — flush пропущен"
        );
        if first.applied_flips.is_empty() {
            assert_eq!(after, orig_bytes);
        }
        let _ = client
            .session_destroy(SessionDestroyRequest {
                session_id: session,
            })
            .await;
    }
```

- [ ] **Step 2: RED (подтверждение на живом образе при smoke-прогоне)**

Run: `cargo test -p uefi-engine hii_set_value_noop -- --ignored`
Expected (при наличии образа): FAIL — mtime изменился (безусловный flush). Если образа нет — шаг пропускается, red-статус подтверждается ревью строки `:1136`.

- [ ] **Step 3: Реализация**

В `hii_set_value` (`:1136`) безусловный `self.flush_image(&r.image_id).await?;` заменить на:

```rust
        if !outcome.applied.is_empty() {
            self.flush_image(&r.image_id).await?;
        }
```

- [ ] **Step 4: GREEN + полный прогон + коммит**

Run: `cargo test -p uefi-engine && cargo test -p uefi-engine hii_set_value_noop -- --ignored` (второй — при наличии образа)
Expected: PASS.

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(uefi-engine): hii_set_value flush только при реальных изменениях — зеркало hii_unlock (TODO:2919)"
```

---

### Task 8: RPC — hii_unlock без get_or_load_image-клона (спека §6, TODO:1369)

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_unlock` `:1067-1087`)
- Test: `crates/uefi-engine/src/rpc/server.rs` (tests-mod)

**Interfaces:**
- Produces: `hii_unlock` берёт `session_id` из images-слота в том же lock, что и мутация; холодный кэш (движок перезапущен, образ только на диске) → `not_found "image not found"` — санкционировано спекой (§Риски).

- [ ] **Step 1: Failing-тест (хендлер-путь без get_or_load)**

В tests-mod server.rs рядом с `image_upload_roundtrip` (`:2870`) добавить:

```rust
    #[tokio::test]
    async fn hii_unlock_malformed_item_is_invalid_argument() {
        let (_td, mut client) = setup().await;
        let session = create_session(&mut client).await;
        let opened = client
            .image_upload(tonic::Request::new(ImageUploadRequest {
                session_id: session.clone(),
                data: fv_image_with_two_files(),
                mode: 0,
                name: "up.bin".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        let err = client
            .hii_unlock(HiiUnlockRequest {
                image_id: opened.image_id.clone(),
                item_id: "0#nope".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("malformed item_id"));
        let _ = client
            .session_destroy(SessionDestroyRequest {
                session_id: session,
            })
            .await;
    }
```

- [ ] **Step 2: Запустить — должен быть ЗЕЛЁНЫМ до правки**

Run: `cargo test -p uefi-engine hii_unlock_malformed`
Expected: PASS (InvalidItemId из Task 3; тест фиксирует хендлер-путь `images.get_mut` и защищает рефакторинг Step 3 от регресса). Если RED — остановиться и разобраться (не реализовывать вслепую).

- [ ] **Step 3: Реализация**

Хендлер `hii_unlock` (`:1067-1087`) целиком заменить на:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn hii_unlock(&self, req: Request<HiiUnlockRequest>) -> RpcResult<HiiUnlockResponse> {
        let r = req.into_inner();
        let (outcome, session_id) = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let session_id = img_slot.session_id.clone();
            let outcome = crate::hii::unlock(img_slot, &r.item_id)
                .map_err(|e| hii_error_status_ctx(e, &r.item_id))?;
            (outcome, session_id)
        };
        if !outcome.applied.is_empty() {
            self.flush_image(&r.image_id).await?;
        }
        let _ = self.sm.touch(&session_id);
        tracing::info!(image_id = %r.image_id, item_id = %r.item_id, flips = outcome.applied.len(), "hii unlock");
        Ok(Response::new(HiiUnlockResponse {
            gates: outcome.gates,
            applied_flips: outcome.applied,
        }))
    }
```

- [ ] **Step 4: GREEN + полный прогон + коммит**

Run: `cargo test -p uefi-engine`
Expected: PASS (включая `hii_unlock_noop_keeps_artifact_untouched` при явном `--ignored` прогоне на живом образе).

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(uefi-engine): hii_unlock без get_or_load_image-клона — session_id из images-слота (TODO:1369)"
```

---

### Task 9: RPC — tracing-паритет list-хендлеров (спека §6, TODO:3510/3563)

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`hii_list_questions` `:1117-1118`, `hii_list_forms` `:817-818`, `hii_list_strings` `:856-857`)

**Interfaces:** — (только info-строки; образец — `hii_gates_list` `:1062`, `hii_list_varstores` `:831`)

- [ ] **Step 1: Добавить info-строки**

`hii_list_questions`, перед `Ok(Response::new(...))`:

```rust
        tracing::info!(image_id = %r.image_id, target = %r.target, form_id = r.form_id, count = questions.len(), "hii questions listed");
```

`hii_list_forms`:

```rust
        tracing::info!(image_id = %r.image_id, count = forms.len(), "hii forms listed");
```

`hii_list_strings`:

```rust
        tracing::info!(image_id = %r.image_id, count = strings.len(), "hii strings listed");
```

(`hii_form_tree` `:844` уже паритетен — не трогать.)

- [ ] **Step 2: Прогон + коммит**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check`
Expected: PASS (паритет подтверждается ревью, авто-теста нет — по спеке).

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "chore(uefi-engine): tracing-паритет hii_list_questions/list_forms/list_strings (TODO:3510, 3563)"
```

---

### Task 10: Engine — точное ожидание обрезанного пакета (спека §6, TODO:1374)

**Files:**
- Modify: `crates/uefi-engine/src/hii/gates.rs` (тест `find_gates_stops_gracefully_on_truncated_package` `:885-891`)

**Interfaces:** — (тест-фикс; обоснование «1, не 0» — фикс спеки Task 1: emit на statement-опе `gates.rs:181` до обрезанного хвоста)

- [ ] **Step 1: Захардкодить ожидание**

Тест заменить на:

```rust
    #[test]
    fn find_gates_stops_gracefully_on_truncated_package() {
        let mut pkg = package(&vendor_ifr());
        pkg.truncate(pkg.len() - 3);
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert_eq!(
            gates.len(),
            1,
            "обрезка хвостовых END не прячет ранний suppress-ref гейт: \
             walker emit'ит его на statement-опе (REF) до обрезанного хвоста"
        );
    }
```

- [ ] **Step 2: Прогон + коммит**

Run: `cargo test -p uefi-engine find_gates_stops_gracefully`
Expected: PASS — если RED (фактически 0), остановиться: это противоречит аудиту find_gates/emit_gates/package_bounds — разобраться, при подтверждении дефекта оформить docs-коммит по правилу 11 и исправить ожидание.

```bash
cargo clippy -p uefi-engine -- -D warnings && cargo fmt --all -- --check
git add crates/uefi-engine/src/hii/gates.rs
git commit -m "test(uefi-engine): точное ожидание обрезанного пакета в find_gates — 1 гейт (TODO:1374)"
```

---

### Task 11: CLI — mock_question с DefaultEntry, третий литерал удалён (спека §6, TODO:2884)

**Files:**
- Modify: `crates/uefi-cli/src/output.rs` (tests-mod `mock_question` `:963-998`, тест `:1061-1086`)

**Interfaces:** — (тест-фикстура; второй литерал `:1019` — сам тест defaults-печати, остаётся)

- [ ] **Step 1: Расширить фикстуру**

В `mock_question` (`:994`) заменить `defaults: vec![],` на:

```rust
            defaults: vec![DefaultEntry {
                default_id: 0,
                r#type: 0,
                value: 1,
            }],
```

- [ ] **Step 2: Удалить третий литерал**

Тест `question_info_text_keeps_sid_fallback_when_text_empty` (`:1061-1086`) заменить на:

```rust
    #[test]
    fn question_info_text_keeps_sid_fallback_when_text_empty() {
        let mut q = mock_question();
        q.varstore = None;
        q.options = vec![OptionEntry {
            string_id: 9,
            value: 2,
            flags: 0x00,
            ..Default::default()
        }];
        let text = question_info_text(&q);
        assert!(text.contains("value = 2 (string 9, flags 0x0)"));
    }
```

- [ ] **Step 3: Прогон + коммит**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings && cargo fmt --all -- --check`
Expected: PASS — smoke-тесты `mock_question`-потребителей (`question_info_print_smoke`, `question_info_no_options_print_smoke`, `set_value_print_smoke`) печатают лишнюю default-строку без ассертов против.

```bash
git add crates/uefi-cli/src/output.rs
git commit -m "test(uefi-cli): mock_question расширен DefaultEntry — третий инлайн-литерал QuestionInfo удалён (TODO:2884)"
```

---

### Task 12: Common+CLI — SockSource: transport-ошибка называет путь и источник (спека §7, TODO:1409)

**Files:**
- Modify: `crates/uefi-common/src/state.rs` (`resolve_sock` `:54-65`, тесты `:184-216`)
- Modify: `crates/uefi-cli/src/client.rs` (`connect` `:29-46`)
- Test: `crates/uefi-common/src/state.rs` (tests-mod), `crates/uefi-cli/tests/e2e.rs`

**Interfaces:**
- Produces: `pub enum SockSource { CliArg, Env, StateFile, Default }` (Display: `cli arg` / `env UEFIPATCHER_SOCK` / `state file` / `default`); `pub fn resolve_sock_with_source(cli_sock: Option<&str>, state: &State) -> (PathBuf, SockSource)`; `resolve_sock` — делегирующая обёртка (call-сайты engine/gateway/TUI/session.rs не меняются). Текст ошибки клиента: `transport error: cannot connect to <path> (source: <sock source>): <cause>`.

- [ ] **Step 1: Failing-тест resolve_sock_with_source**

В tests-mod state.rs рядом с `resolve_sock_priority_*` добавить:

```rust
    #[test]
    fn resolve_sock_with_source_reports_source() {
        let _g = lock_guard();
        let st = State {
            sock_path: Some("/from-state".into()),
            ..Default::default()
        };
        let env = env_guard("UEFIPATCHER_SOCK", "/from-env");
        assert_eq!(
            resolve_sock_with_source(Some("/from-cli"), &st),
            (PathBuf::from("/from-cli"), SockSource::CliArg)
        );
        assert_eq!(
            resolve_sock_with_source(None, &st),
            (PathBuf::from("/from-env"), SockSource::Env)
        );
        drop(env);
        assert_eq!(
            resolve_sock_with_source(None, &st),
            (PathBuf::from("/from-state"), SockSource::StateFile)
        );
        assert_eq!(
            resolve_sock_with_source(None, &State::default()),
            (default_sock(), SockSource::Default)
        );
    }
```

- [ ] **Step 2: RED → реализация**

Run: `cargo test -p uefi-common resolve_sock_with_source` → FAIL (нет типов). В state.rs заменить `resolve_sock` (`:54-65`) на:

```rust
/// Источник резолва сокета — для диагностики transport-ошибок клиента.
/// Спека hii-errors-cleanup §7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SockSource {
    CliArg,
    Env,
    StateFile,
    Default,
}

impl std::fmt::Display for SockSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SockSource::CliArg => "cli arg",
            SockSource::Env => "env UEFIPATCHER_SOCK",
            SockSource::StateFile => "state file",
            SockSource::Default => "default",
        })
    }
}

/// Полная логика приоритета сокета (cli > env > state > default) вместе с
/// источником. Спека hii-errors-cleanup §7.
pub fn resolve_sock_with_source(cli_sock: Option<&str>, state: &State) -> (PathBuf, SockSource) {
    if let Some(s) = cli_sock {
        return (PathBuf::from(s), SockSource::CliArg);
    }
    if let Ok(env) = std::env::var("UEFIPATCHER_SOCK") {
        return (PathBuf::from(env), SockSource::Env);
    }
    if let Some(s) = &state.sock_path {
        return (PathBuf::from(s), SockSource::StateFile);
    }
    (default_sock(), SockSource::Default)
}

pub fn resolve_sock(cli_sock: Option<&str>, state: &State) -> PathBuf {
    resolve_sock_with_source(cli_sock, state).0
}
```

Run: `cargo test -p uefi-common` → PASS (существующие `resolve_sock_priority_*` зелёные — обёртка сохраняет поведение).

- [ ] **Step 3: Failing e2e на мёртвом сокете**

В `crates/uefi-cli/tests/e2e.rs` добавить:

```rust
#[test]
fn dead_socket_transport_error_names_path_and_source() {
    let td = TempDir::new().unwrap();
    let cwd = td.path();
    let dead = td.path().join("dead.sock");
    Command::cargo_bin("uefi-cli")
        .unwrap()
        .current_dir(cwd)
        .args([
            "--sock",
            dead.to_str().unwrap(),
            "session",
            "init",
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "transport error: cannot connect to",
        ))
        .stderr(predicates::str::contains(dead.display().to_string()))
        .stderr(predicates::str::contains("(source: cli arg)"));
}
```

- [ ] **Step 4: RED → реализация client.rs**

Run: `cargo test -p uefi-cli --test e2e dead_socket` → FAIL (голый `transport error (RPC_INTERNAL)`). В `Client::connect` (`:29-46`) заменить резолв и error-маппинг:

```rust
    pub async fn connect(cli_sock: Option<&str>, state: State) -> Result<Self, AppError> {
        let (sock, source) = resolve_sock_with_source(cli_sock, &state);
        let sock_display = sock.display().to_string();
        let sock_str = sock_display.clone();
        let channel = Endpoint::try_from("http://localhost")
            .map_err(|e| AppError::new(ErrKind::IoError, e.to_string()))?
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                let s = sock_str.clone();
                async move {
                    Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
                }
            }))
            .await
            .map_err(|e| {
                AppError::new(
                    ErrKind::RpcInternal,
                    format!(
                        "transport error: cannot connect to {sock_display} (source: {source}): {e}"
                    ),
                )
            })?;
        Ok(Self {
            inner: EngineServiceClient::new(channel),
            state,
        })
    }
```

Импорт `resolve_sock` в client.rs (`:8`) заменить на `resolve_sock_with_source`.

- [ ] **Step 5: GREEN + полный прогон + коммит**

Run: `cargo test -p uefi-common -p uefi-cli && cargo clippy -p uefi-common -p uefi-cli -- -D warnings && cargo fmt --all -- --check`
Expected: PASS.

```bash
git add crates/uefi-common/src/state.rs crates/uefi-cli/src/client.rs crates/uefi-cli/tests/e2e.rs
git commit -m "feat(uefi-cli): transport error называет сокет и источник резолва (SockSource, TODO:1409)"
```

---

### Task 13: CLI+TUI — note при form-level unlock (спека §4, TODO:2997)

**Files:**
- Modify: `crates/uefi-cli/src/output.rs` (`print_unlock` `:298-317`)
- Modify: `crates/uefi-tui/src/commands.rs` (unlock-arm `:1090-1115`)
- Test: `crates/uefi-cli/tests/e2e.rs` (`hii_gates_and_unlock_output_content` `:245-287`), `crates/uefi-tui/tests/tui_integration.rs` (`hii_verbs_visibility_setvalue_unlock` `:339-399`)

**Interfaces:**
- Produces: form-таргет = item_id с `#` и без `:` в дискриминаторе. CLI (Text/Tsv): после applied-флипов две строки note (Json не трогаем). TUI: та же note одной строкой дописывается в `status_msg`.

- [ ] **Step 1: Failing e2e — note при form-таргете, нет при question-таргете**

В `hii_gates_and_unlock_output_content` (`:265-270`) существующий блок question-unlock дополнить негативным ассертом и добавить form-unlock:

```rust
    cli(&sock, cwd)
        .args(["hii", "question", "unlock", "0#10029:0x3B"])
        .assert()
        .success()
        .stdout(predicates::str::contains("grayout"))
        .stdout(predicates::str::contains("applied pkg+0xdd1"))
        .stdout(predicates::str::contains("form-level unlock").not());

    cli(&sock, cwd)
        .args(["hii", "form", "unlock", "0#10029"])
        .assert()
        .success()
        .stdout(predicates::str::contains("applied pkg+0xdd1"))
        .stdout(predicates::str::contains(
            "note: form-level unlock does not unlock per-question gates;",
        ))
        .stdout(predicates::str::contains(
            "list: hii question gates 0#10029, unlock: hii question unlock 0#10029:<qid>",
        ));
```

- [ ] **Step 2: RED → реализация print_unlock**

Run: `cargo test -p uefi-cli --test e2e hii_gates_and_unlock` → FAIL. В output.rs над `print_unlock` добавить хелпер:

```rust
/// Form-таргет: item_id с `#` и без `:qid` в дискриминаторе — признак
/// form-level unlock (спека hii-errors-cleanup §4).
fn is_form_level_item(item_id: &str) -> bool {
    item_id
        .rsplit_once('#')
        .is_some_and(|(_, disc)| !disc.contains(':'))
}
```

В `print_unlock` (`:307-315`) ветка `_ =>` после цикла `for f in applied`:

```rust
        _ => {
            print_gates(item_id, gates, format);
            if applied.is_empty() {
                println!("nothing to unlock for {item_id}");
            }
            for f in applied {
                println!("applied {f}");
            }
            if is_form_level_item(item_id) {
                println!("note: form-level unlock does not unlock per-question gates;");
                println!(
                    "  list: hii question gates {item_id}, unlock: hii question unlock {item_id}:<qid>"
                );
            }
        }
```

Run: `cargo test -p uefi-cli --test e2e hii_gates_and_unlock` → PASS.

- [ ] **Step 3: Failing TUI-тест**

В `hii_verbs_visibility_setvalue_unlock` (`:379-382`) заменить блок unlock на:

```rust
    uefi_tui::commands::execute_command(&mut app, &format!("hii unlock {item}"), &mut client)
        .await
        .unwrap();
    assert!(app.status_msg.contains("unlock"));
    assert!(
        app.status_msg.contains("form-level unlock"),
        "form-таргет: подсказка про построчные гейты"
    );

    let qitem = "11111111-2222-3333-4444-555555555555:0x19:0#10001:0x210";
    uefi_tui::commands::execute_command(&mut app, &format!("hii unlock {qitem}"), &mut client)
        .await
        .unwrap();
    assert!(
        !app.status_msg.contains("form-level unlock"),
        "question-таргет: note не печатается"
    );
```

- [ ] **Step 4: RED → реализация TUI**

Run: `cargo test -p uefi-tui --test tui_integration hii_verbs` → FAIL (нет note). В commands.rs рядом с префилл-хелперами (`:1993`) добавить:

```rust
/// Form-таргет: item_id с `#` и без `:qid` в дискриминаторе — признак
/// form-level unlock (спека hii-errors-cleanup §4).
fn is_form_level_item(item_id: &str) -> bool {
    item_id
        .rsplit_once('#')
        .is_some_and(|(_, disc)| !disc.contains(':'))
}
```

В unlock-arm (`:1106-1113`) после присвоения `app.status_msg`:

```rust
                    if is_form_level_item(&item) {
                        app.status_msg.push_str(
                            "; note: form-level unlock does not unlock per-question gates \
                             (list: hii question gates, unlock: hii question unlock <item>#<form>:<qid>)",
                        );
                    }
```

- [ ] **Step 5: GREEN + полный прогон + коммит**

Run: `cargo test -p uefi-cli -p uefi-tui && cargo clippy -p uefi-cli -p uefi-tui -- -D warnings && cargo fmt --all -- --check`
Expected: PASS.

```bash
git add crates/uefi-cli/src/output.rs crates/uefi-tui/src/commands.rs crates/uefi-cli/tests/e2e.rs crates/uefi-tui/tests/tui_integration.rs
git commit -m "feat(uefi-cli,uefi-tui): note о построчных гейтах при form-level unlock (TODO:2997)"
```

---

### Task 14: Docs — закрытие TODO + real-image smoke (ветка)

**Files:**
- Modify: `TODO.md` (записи `:1322`, `:1369`, `:1374`, `:1387`, `:1409`, `:2143`, `:2884`, `:2890`, `:2919`, `:2997`, `:3510`, `:3563`)

**Interfaces:** — (docs + ручной smoke)

- [ ] **Step 1: Закрыть записи**

Каждую из записей `:1322`, `:2890`, `:1387`, `:2997`, `:2919`, `:1374`, `:2884`, `:1369`, `:3510`, `:3563`, `:1409`: `[ ]` → `[x]`, в конец дописать `Закрыто: цикл hii-errors-cleanup (спека docs/superpowers/specs/2026-09-26-hii-errors-cleanup-design.md §<соответствующий>).` Запись `:2143` — `[ ]` → `[x]` с текстом `Закрыто: реализовано formset-unlock/write-guard (падение), warn добавлен циклом hii-errors-cleanup §2.`

- [ ] **Step 2: Real-image smoke (живой образ, вручную)**

При наличии `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` (или AMI-образа) прогнать:

```bash
cargo test -p uefi-engine --test real_image -- --ignored
cargo test -p uefi-engine hii_unlock_noop hii_set_value_noop -- --ignored
```

Чек-лист спеки (Тестирование): повторный unlock → Ok applied пуст; gates на не-HII цели → NotASetupItem; set_value идемпотентный → applied пуст, артефакт не тронут; save round-trip — длина/байты. Новых байтоизменяющих гейтов нет.

- [ ] **Step 3: Финальный прогон всего workspace + коммит**

```bash
cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check
git add TODO.md
git commit -m "docs(todo): hii-errors-cleanup — цикл закрыт, 12 позиций [x]"
```

---

## Self-Review (выполнен при написании)

- **Покрытие спеки:** §1 → Task 3+4; §2 → Task 5; §3 → Task 6; §4 → Task 13 (+3013 в Task 2); §5 → Task 7; §6 (1374/2884/1369/3510/3563) → Task 10/11/8/9; §7 → Task 12; Тестирование C1→Task 4, C2→Task 5, C3→Task 6, C4→Task 13, C5→Task 7/10/11, L1409→Task 12, real-image→Task 14. Вне скоупа (1364, TRUE→FALSE, bare-мутабельность, TUI-коннект) — задач нет, верно.
- **Placeholder-скан:** код во всех шагах полный; TBD нет.
- **Консистентность типов:** `InvalidItemId(String)` одинаково в Task 3/4/8; `resolve_sock_with_source -> (PathBuf, SockSource)` в Task 12; `is_form_level_item(&str) -> bool` в Task 13 (CLI и TUI — свои копии, крейты не зависят друг от друга); `is_unlocked_expr` pub(crate) в Task 5, потребляется mod.rs.
