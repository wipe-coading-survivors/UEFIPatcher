# Спека: мини-цикл `ref-guard-true-flip` — flip константного TRUE, валидация цели add_ref

Дата: 2026-10-01. Статус: реализовано (ветка feat/ref-guard-true-flip, план-ревью APPROVE после одного FIX-FIRST раунда); гейты зелёные; живой гейт владельца (450x, вкладка Chipset) — pending.

Закрывает TODO: 4027 (класс «suppress_if TRUE» не флипается), 4038 (add_ref не валидирует цель REF).

## Контекст

Две живые болячки 450x (HXE99TF), зафиксированные разведкой live-гейта 2026-09-12:

1. Жёстко скрытые GOTO/вопросы с константным выражением `true` (`EFI_IFR_TRUE_OP`, одиночный опкод) не снимаются `unlock`: `decode_expr` даёт `GateExpr::True`, но у `plan_flip` нет для него ветки → `Ok(None)` → «expression is not a hardware-validated flip class». На 450x так скрыт GOTO→Chipset (10008) в корневой форме 10000 — вкладка Chipset отсутствует в живом баре именно из-за этого.
2. `add_ref`/`check_ref_add` не проверяют существование цели: plain REF (без `formset_guid`) на отсутствующую в формсете-владельце форму и REF3 на несуществующие формсет/форму вставляются молча — висячий GOTO (в TUI Forms View виден как «! … dangling REF target», в живом Setup — мёртвый пункт меню). Прецедент: два GOTO на форму 1 вставились без ошибки и повисли. Паритет-разрыв: import-precheck (envelope) цель уже валидирует, движковый add — нет.

## §1 Третий flip-класс: ConstantTrue (gates.rs)

### Декодирование

`GateExpr` получает вариант `False` — одиночный `IFR_FALSE_OP` в expr-регионе, симметрично `True` (r-efi: TRUE=0x46, FALSE=0x47, оба len 2). Нужен для идемпотентности после флипа: повторный `unlock` на уже-открытом гейте должен skip'нуть его через `is_unlocked_expr`, а не упасть «not a flip class» (параллель с EqConst{a≠b}).

### plan_flip

Новый arm `GateExpr::True` → `PlannedFlip { offset: gate.expr_offset, from: [IFR_TRUE_OP], to: [IFR_FALSE_OP] }`: побайтовая замена опкода, length-preserving. Входной инвариант границ уже проверен (`expr_offset+2 ≤ expr_end` — TRUE занимает ровно 2 байта с начала региона; ручной Gate с выходом за тело пакета по-прежнему Err «gate bounds out of package»).

### is_unlocked_expr / expr_text

- `is_unlocked_expr(False) = true` (гейт уже открыт); `True` остаётся locked.
- `expr_text(False) = "false"`.

Обратный флип FALSE→TRUE НЕ строим — принцип «unlock только открывает» (как EqConst после флипа не флипается обратно).

### Поверхности без изменений

`gate_info.flippable`/`flip`-текст, `unlock` (own+cross фазы), CLI/TUI/RPC — едят новый класс автоматически; proto/WebUI не трогаем. flip-текст: `pkg+0xNNN: 46 -> 47`.

### Тесты (unit, gates.rs)

- `decode_expr([(FALSE,_)]) == False`; True-регион не путается с False.
- `plan_flip` True-гейта → flip `{expr_offset, [0x46], [0x47]}`; `plan_gates` ок.
- apply → повторный find_gates даёт `False`, `plan_gates_skip_unlocked` пропускает, `plan_gates` (строгий) — Err (уже-открытый, параллель `plan_after_flip_refuses_second_unlock`).
- `is_unlocked_expr(False) == true`, `(True) == false`.
- hand-gate c `True` и expr_offset за телом → Err bounds.
- Интеграционный (mod.rs): фикстура suppress_if TRUE → GOTO; `gates_list` flippable=true; `unlock` применяет, форма видима, повторный unlock — no-op без ошибки.

## §2 Валидация цели REF (add_ref/check_ref_add)

### Хелпер

`validate_ref_target(image: &Image, qt: &QuestionTarget, schema: &QuestionAddRefSchema) -> Result<(), HiiError>`:

- **plain REF** (`formset_guid: None`): цель обязана существовать в формсете-владельце родительской формы — `parse_form_package_sets(&qt.pkg)` → `sets[qt.formset_idx].forms` содержит `form_id == schema.form_id`. Suppressed-формы — валидные цели (REF на скрытую форму легитимен; её снимают unlock'ом — см. §1). Отказ: `InvalidSchema("ref target form {:#x} not declared in owning formset {guid} (have: {sorted form_ids})")`.
- **REF3** (`Some(gs)`): GUID парсится рано (невалидная строка — InvalidSchema, как в build_ref_ops сегодня); глобальная карта `forms::collect_forms(image)`: пара `(formset_guid, form_id_ifr)`. Read-only каналы — валидные цели (GOTO — навигация, не мутация цели). Отказы:
  - формсет есть, формы нет: `InvalidSchema("ref target form {:#x} not found in formset {guid} (have: {form_ids})")`;
  - формсета нет: `InvalidSchema("ref target formset {guid} not found in image (have: {formset_guids})")`.

### Точки вызова

- `add_ref`: после `resolve_writable_path`, до `plan_spf_fixup`/`check_ref_slots`/строк — отказ до любой мутации (принцип plan-all-then-apply).
- `check_ref_add`: в цикле по refs, тем же хелпером (precheck-паритет с apply).

Порядок ошибок: InvalidItemId/NotFound (резолв item_id) → InvalidSchema цели → InvalidSchema коллизий qid (check_ref_slots).

### Тест-инверсия

`add_ref_allows_dangling_destination_form` (mod.rs:6394) инвертируется в `add_ref_rejects_dangling_destination_form`: InvalidSchema, дерево байт-идентично (rollback-инвариант).

### Новые тесты (mod.rs, existing fixtures)

- intra: существующая форма того же формсета → Ok; форма соседнего формсета пакета → InvalidSchema «not declared in owning formset».
- REF3: существующие формсет+форма → Ok; формсет не найден → InvalidSchema со списком формсетов; форма не найдена → InvalidSchema со списком форм формсета.
- suppressed-цель → Ok.
- check_ref_add: dangling → InvalidSchema (без мутации); валидная цель → Ok.
- add_ref: до строки-добавления (string_pack не вызван — фикстура без string-пакета не даёт StringPackageNotFound раньше InvalidSchema цели).

## Гейты

`cargo test --all`; `cargo clippy --all --all-targets -- -D warnings`; `cargo fmt --all -- --check`. Живой гейт владельца (rd450x/HXE99TF, опционально): (1) unlock GOTO 10008 в форме 10000 → вкладка Chipset в баре; (2) add_ref на несуществующую форму → отказ.

## За рамками

- Обратный флип FALSE→TRUE (re-hide) и флип прочих `Other`-выражений.
- Валидация question_id у REF3-цели (REF с точностью до вопроса) — движок эмитит form-level REF.
- Валидация целей у уже существующих в образе REF (только новые вставки).
- Кросс-валидация «формсет-цель не в read-only» — не требуется (см. §2).
