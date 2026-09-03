# Спека: мини-цикл «unlock-op» — unlock вопросов/страниц через флипы литералов в гейтящих IFR-выражениях

> Дизайн-документ, 2026-09-03. Статус: решения брейнсторма согласованы с
> пользователем 2026-09-03: **u2 — строго E12-классы флипов** (расширение
> опкод-флипами TRUE→FALSE — отложено), **u3 — полное удаление UPG/PRC**
> (теория фальсифицирована на E8, не «вынести опцией»), **API — два RPC**
> (`HiiGatesList` чтение + `HiiUnlock` правка) и четыре CLI-команды
> (`hii form|question gates|unlock`). Источник: секция TODO
> «Мини-цикл „unlock-op“ (engine)» и отчёт
> `docs/reports/2026-09-02-hw-validation-hnx99tf.md` §10–12 (E10–E12).
> Эталон: `refs/amibcp/e12-above4g-dataflip.bin` (sha256 `506f068b…`,
> вне репо) — побайтовые координаты флипов записаны в §3 спеки.

## 1. Контекст и проблема

Аппаратная валидация 2026-09-03 (E12, `§12.3` отчёта) доказала на живой
плате HNX99TF конвейер «парсинг → адресная правка → liblzma-рекомпрессия
в слот → прошивка»: скрытая страница PCI (форма 10029 «PCI Subsystem
Settings») и вопрос «Above 4G Decoding» (qid 0x003B) раскрыты флипами
**ровно 3 байт** декомпрессированного контента, значения сохраняются в
NVRAM. Единственный HW-валидированный класс правок setup-модуля —
**флипы данных внутри гейтящих выражений, без структурных изменений**.

Вендорская блокировка двухслойная (`§11.1`, `§11.3`):

1. **suppress страницы** — REF→10029 (в хаб-форме 10002 «Advanced»)
   обёрнут `SUPPRESS_IF(UINT64(1) == UINT64(1))` — всегда-истинное
   выражение, пункт меню скрыт;
2. **grayout каждого вопроса** — личный `GRAYOUT_IF(EQ_ID_VAL(0x009A == 1))`,
   где 0x009A — невидимый NUMERIC-мастер-переключатель.

Движок сегодня этого не умеет: `set_item_visibility` (`hii/mod.rs:56`)
умеет только опустошать suppress-скоуп вокруг **формы** (класс E7/E8,
валидирован только на формсет-модуле ABBCE13D; на setup-модуле 899407D7
этот класс вешает AMITSE — E11), не видит REF-гейты и question-grayout'ы
вообще, а его walker'ы (`find_form_suppress_scope`) ломаются на
вендорском quirk'е «scope-бит на операнде выражения» (`[45 8a …]`,
`ifr.rs:98` трактует любой 0x80 как открытие скоупа и не находит END).

Дополнительно: авто-патч UPG/PRC-токенов в `set_item_visibility`
(цикл 2026-09-02) построен на теории «нет токена → пустая подпись»,
фальсифицированной на E8 (аддендум §9 отчёта): вставка токенов ничего
не меняет, `x-UEFI-AMI` — артефакт AMI-тулинга. Мёртвый механизм не
просто бесполезен — он **блокирует валидные операции** (отказ
`PrcPatchUnsupported` при невозможности роста .rsrc).

## 2. Цели и не-цели

**Цели (u1–u5 из TODO):**

1. **u1 — карта гейтинга (только чтение)**: для цели — форма
   (`<target>#<form_id_ifr>`) или вопрос (`<target>#<form_id_ifr>:<qid>`) —
   найти все объемлющие `SUPPRESS_IF`/`GRAYOUT_IF`-скоупы (вокруг самой
   формы, вокруг любого `REF`→form_id, вокруг вопроса), декодировать
   выражения, пометить сводимость к безопасному классу и показать
   планируемый флип. Работает на Read-образе.
2. **u2 — операция unlock (правка)**: применить флипы литералов на месте.
   Инварианты: длины IFR-пакета/списка/PE **неизменны**, опкоды и
   смещения нетронуты; отказ всей цели с диагностикой, если хоть один
   объемлющий гейт не сводится к распознанному классу; запрет структурных
   правок (пустые скоупы/stray — E11, relocate — E9) — архитектурно:
   флип меняет только байты литералов.
3. **u3 — удаление UPG/PRC-механики**: `plan_prc_entries`, UPG-ветка и
   ошибка `PrcPatchUnsupported` из `set_item_visibility`; тесты
   фальсифицированного поведения удаляются. Механика
   `string_pack::insert_strings_at_ids*` остаётся (используется
   `form_add`/`formset_add`).
4. **u4 — real-image gate**: unlock страницы PCI + «Above 4G Decoding»
   на HNX99TF; контроль — байт-диф form-пакета против эталона E12
   (ровно 3 байта, §3), slot-fit (раскладка файла Setup неизменна),
   round-trip.
5. **u5 — CLI**: `hii form gates/unlock`, `hii question gates/unlock`;
   вывод — найденные скоупы, декодированные выражения, применённые
   флипы; smoke на живом образе.

**Не-цели:**

- Флип опкода `TRUE→FALSE` (0x46→0x47) — тот же класс длины, приём
  комьюнити, но нашим железом не валидирован; при появлении живого
  прецедента — одной строкой в `plan_flip` (TODO).
- Правка легаси-walker'ов `ifr.rs` (`find_suppress_if_scopes`,
  `find_form_suppress_scope`, quirk 0x8a) — их путь (`set-visibility`)
  не меняется помимо u3; отдельный TODO.
- Bare-канал вопрос-гейтов в PE вне 'HII'-ресурсов, TUI/Gateway/WebUI
  обёртки, relocate вопросов, SDP-патчинг, NVRAM-модели — вне цикла.
- HW-прошивка E13 — по желанию пользователя, после ревью дифа.

## 3. Верификационные данные

Из отчёта §11–12 и TODO (координаты — побайтово сверены пробниками):

- **Флипы E12** (pkg = FORM-пакет Setup-модуля): `pkg+0x67A: 01→02`
  (второй операнд `UINT64(1)` suppress'а REF→10029, `1==1` → `1==2`);
  `pkg+0xDD1..0xDD2: 01 01→FF FF` (литерал `EQ_ID_VAL(0x009A==1)` →
  `==0xFFFF`, недостижимо для 1-байтового значения). Итог: диф dec =
  ровно 3 байта.
- **Slot-fit**: recompressed поток Setup-секции `0x63BF ≤ 0x6491`
  (lc2/pb0); наш свип стартует с pb0/lc0 и может выбрать другую точку —
  ассерт `≤ слота` (сохранение раскладки файла), не равенство потока.
- **Layout выражений** (r-efi 7.0): `IFR_UINT64_OP=0x45` (len 10,
  payload u64), `IFR_EQUAL_OP=0x2F`, `IFR_EQ_ID_VAL_OP=0x12` (len 6:
  qid@+2, val@+4), `IFR_TRUE_OP=0x46`; question-header: prompt@+2/
  help@+4/qid@+6/flags@+12; `IFR_REF_OP=0x0F` len 15, form_id@+13
  (IfrOpHeader 2 + IfrQuestionHeader 11 + FormId 2 — IfrRef, не IfrRef2);
  NUMERIC width = `1 << (flags@+12 & IFR_NUMERIC_SIZE)`, CHECKBOX = 1.
- **Vendor quirk**: первый операнд выражения может нести scope-бит
  (`[45 8a …][45 0a …][2f 02]`) — выражение нельзя обходить стеком
  скоупов; только линейно по длинам.
- **Двухслойная схема**: REF→10029 @pkg+0x686 в форме 10002; grayout
  qid 0x003B в форме 10029; мастер 0x009A — невидимый NUMERIC.
- Setup-модуль HNX99TF: FFS `899407D7-99FE-43D8-9A21-79EC328CAC21`
  (PE32 за LZMA-GUIDed, HII в 'HII'-ресурсах).

## 4. Дизайн

### 4.1 Новый модуль `hii/gates.rs`

**Модель:**

```rust
pub enum GateKind { Suppress, Grayout }
pub enum Wraps {
    Form { form_id: u16 },
    Ref { form_id: u16, host_form_id: u16 },   // REF в host-форме ведёт на form_id
    Question { form_id: u16, question_id: u16 },
}
pub enum GateExpr {
    EqConst { a: u64, b: u64 },                // EQUAL(UINT64(a), UINT64(b))
    EqIdVal { question_id: u16, value: u16 },  // EQ_ID_VAL(qid, val)
    True,
    Other,                                     // не распознано
}
pub struct Gate { kind, wraps, scope_offset, expr_offset, expr_end, expr }
pub struct GateTarget { form_id: u16, question_id: Option<u16> }
```

**Декодер** `decode_expr(region: &[u8]) -> GateExpr`: линейный обход
региона по длинам (`len = byte & 0x7F`, scope-бит игнорируется);
распознавание ровно трёх форм: `[UINT64, UINT64, EQUAL]`,
`[EQ_ID_VAL]`, `[TRUE]`; всё прочее (лишние операнды, неизвестные
опкоды, обрыв) — `Other`.

**Walker** `find_gates(body, target) -> Vec<Gate>` — грамматик-
осведомлённый обход form-пакета (start=4 при `is_form_package`, до
`min(plen, len)`):

- стек фреймов `{op, offset, expr_end, form_id}`; END — pop; scope-бит
  на **statement'ах** — push;
- пока внутренний открытый фрейм — гейт и в нём ещё не было statement'а,
  текущий опкод принадлежит **выражению**: обход линейно по длине,
  **без** push (это и есть обход quirk'а 0x8a);
- первый statement внутри гейта фиксирует `expr_end` (регион выражения
  `[scope_offset+2, expr_end)`); statement-множество для границы —
  FORM/SUBTITLE/TEXT/REF/вопросы (ONE_OF, CHECKBOX, NUMERIC, PASSWORD,
  ORDERED_LIST, STRING, DATE, TIME, ACTION)/ONE_OF_OPTION/DEFAULT;
- матчинг цели: FORM с `form_id == target` (режим формы); REF с
  `form_id@+13 == target` (режим формы, host = текущая форма из стека);
  вопрос с `qid@+6 == target.question_id` внутри формы
  `target.form_id` (режим вопроса). Каждому матчу — по гейту на
  **каждый** объемлющий 0x0A/0x19-фрейм (внутренний первым);
- при `len < 2`/выходе за границы — остановка обхода (без паники),
  зеркально существующим walker'ам.

**Ширина хранилища** `question_storage_width(body, qid) -> Option<u8>`:
линейный поиск вопроса; CHECKBOX → 1; NUMERIC (len ≥ 13) →
`1 << (flags & IFR_NUMERIC_SIZE)`; прочее/не найден → `None`.

**Флипы (строго E12):**

- `EqConst{a, b}` при `a == b` — LSB payload второго UINT64
  (`expr_offset + len_first + 2`): `from.wrapping_add(1)` — байт-в-байт
  E12 `1→2`; гарантия `a != b` после флипа (исходные байты равны, один
  изменён). `a != b` — гейт уже false, флипа нет;
- `EqIdVal{qid, val}` — val (2 байта @ `expr_offset+4`) → `0xFFFF`;
  guard: если `question_storage_width` нашла вопрос и ширина > 1 байта —
  флип запрещён (0xFFFF достижим);
- `True`/`Other` — флипа нет (отчёт «не сводится», для TRUE — отдельный
  TODO-класс).

`plan_gates(body, gates) -> Result<Vec<PlannedFlip>, String>`: все гейты
цели должны иметь флип, иначе `Err` с диагностикой (kind, `pkg+`offset,
hex региона). `apply_flips(&mut [u8], flips) -> Result<(), String>`:
двухфазно — проверка предусловий (вхождение `from`-байт) по всем флипам,
затем запись; длины не меняются in-place.

### 4.2 Грамматика item_id

`<target>#<form_id_ifr>` — режим формы; `<target>#<form_id_ifr>:<qid>` —
режим вопроса; `qid` — u16 (десятичный или `0x`-hex). Единый
`parse_item_id(item_id) -> (Target, form_id, Option<qid>)` в `hii/mod.rs`;
`set_item_visibility` переиспользует его и отвергает `qid`-суффикс
(`NotFound`) — прежняя семантика сохранена.

### 4.3 Интеграция `hii/mod.rs`

- `gates_list(&Image, item_id) -> Result<Vec<uefi_proto::GateInfo>, HiiError>`
  — только чтение: без Write-гейта и без compression-гейта (PE32-тела
  доступны и за нерекомпрессируемыми обёртками).
- `unlock(&mut Image, item_id) -> Result<UnlockOutcome, HiiError>`,
  `UnlockOutcome { gates: Vec<GateInfo>, applied: Vec<String> }` —
  гейты `ImageMode::Write` + предков-compression (как у
  `set_item_visibility`); сбор гейтов по всем FORM-пакетам секции-цели
  (bare: `is_form_package(body)`; resource:
  `pe_resource_form_packages`); пустой список гейтов — успех с пустым
  `applied` (нечего раскрывать); непустой — атомарно plan→apply
  (в `std::mem::take`-теле секции, длина тела инвариантна), каскад
  `mark_rebuild_to_root_by_path`.
- Общий хелпер `resolve_writable_path` (Write + compression-предки)
  выделяется из `set_item_visibility`; `form_package_ranges(&FfsNode)`
  выбирает канал (условия как у `set_item_visibility`: RAW ||
  `is_form_package` → bare; PE32 → resource).
- Отображение `Gate` → proto `GateInfo`: kind/wraps-строки, form_id,
  host_form_id, question_id, декодированное выражение (`"1 == 1"`,
  `"0x009A == 0x0001"`, `"[45 8a 01 …]"` для Other), `flippable`,
  человекочитаемый флип (`"pkg+0x67a: 01 -> 02"`), `scope_offset`.

Новая ошибка:

```rust
#[error("gating expression not reducible to a hardware-validated flip: {0}")]
GateExpressionUnsupported(String),
```

### 4.4 u3 — удаление UPG/PRC

Из `hii/mod.rs`: `plan_prc_entries`, `PRC_TOKEN_LANGUAGE`,
`PRC_NAME_PREFIX`, UPG-блоки PE32-ветки `set_item_visibility`
(pre-check probe + вставка), вариант `HiiError::PrcPatchUnsupported`.
Из `rpc/server.rs`: arm `hii_error_status` + строки его теста. Тесты:
удалить `set_item_visibility_patches_prc_tokens_for_unhidden_form`,
`set_item_visibility_prc_growth_failure_leaves_image_untouched`,
`real_image_unhide_patches_prc_tokens`; убрать UPG-ассерт из
`real_image_unhide_rebuild_keeps_layout`; хелперы, ставшие orphan'ами
(`token_sibt`, `prc_blob`, `form_901_suppressed` — по факту использования),
удалить. `set_item_visibility_no_token_package_is_noop_unhide`,
`string_pack::insert_strings_at_ids*` — остаются.

### 4.5 Proto + RPC

```proto
rpc HiiGatesList(HiiGatesListRequest) returns (HiiGatesListResponse);
rpc HiiUnlock(HiiUnlockRequest)     returns (HiiUnlockResponse);

message HiiGatesListRequest  { string image_id = 1; string item_id = 2; }
message GateInfo {
  string gate_kind = 1;    // "suppress" | "grayout"
  string wraps = 2;        // "form" | "ref" | "question"
  uint32 form_id = 3;      // целевая форма
  uint32 host_form_id = 4; // форма, где живёт гейтнутый statement (REF-хаб / форма вопроса)
  uint32 question_id = 5;  // 0 для form/ref-гейтов
  string expression = 6;   // декодированное выражение или hex-дамп
  bool   flippable = 7;
  string flip = 8;         // "pkg+0x67a: 01 -> 02" или ""
  uint32 scope_offset = 9; // offset гейт-опкода в FORM-пакете
}
message HiiGatesListResponse { repeated GateInfo gates = 1; }
message HiiUnlockRequest     { string image_id = 1; string item_id = 2; }
message HiiUnlockResponse    { repeated GateInfo gates = 1; repeated string applied_flips = 2; }
```

`build.rs`: `.message_attribute("engine.GateInfo", "#[derive(serde::Serialize)]")`
(иначе JSON-вывод CLI не сработает — сериализация перечисляется явно).
Handlers зеркалят `hii_list_forms` (чтение) и `hii_set_form_visibility`
(мутация + `flush_image`). Маппинг: `GateExpressionUnsupported` →
`failed_precondition`; прочие — существующий `hii_error_status`.
Mock-сервер uefi-cli получает заглушки в той же задаче, что и proto
(иначе крейт не соберётся).

### 4.6 CLI

```
uefi-cli hii form gates    <item_id>      # чтение, Read-образ ок
uefi-cli hii form unlock   <item_id>      # мутация
uefi-cli hii question gates  <item_id>    # item_id = <target>#<form>:<qid>
uefi-cli hii question unlock <item_id>
```

`client.rs`: `hii_gates_list(image_id, item_id)`, `hii_unlock(image_id,
item_id) -> (Vec<GateInfo>, Vec<String>)`. `output.rs`:
`print_gates`/`print_unlock` в трёх форматах (JSON — serde GateInfo;
TSV — колонки; text — по строке на гейт + `applied:`-строки флипов,
пустой список — «no gates»). Dispatch в `main.rs`: `HiiFormCmd::Gates/
Unlock`, новый noun `HiiCmd::Question(HiiQuestionCmd::Gates/Unlock)`.

## 5. Обработка ошибок

| Ситуация | Ошибка | RPC-статус |
|---|---|---|
| выражение гейта не сводится к E12-классу (в т.ч. width > 1 у EQ_ID_VAL) | `HiiError::GateExpressionUnsupported(String)` | failed_precondition |
| предусловие apply (байты `from` не совпали) | `GateExpressionUnsupported` | failed_precondition |
| цель/дискриминатор не парсится или не найдена | `HiiError::NotFound` | not_found (существующий) |
| цель не секция / не HII-канал | `HiiError::NotASetupItem` | invalid_argument (существующий) |
| unlock в Read-режиме | `HiiError::NotWritable` | failed_precondition (существующий) |
| unlock за нерекомпрессируемой обёрткой | `HiiError::MutationBehindCompression` | failed_precondition (существующий) |
| гейтов нет | не ошибка: `gates=[]`, `applied=[]` | — |

## 6. Тестирование и приёмка

**Синтетика (TDD, по задачам):** декодер (три класса + quirk 0x8a + обрыв
+ чужие формы); walker (REF-гейт, form-гейт, question-grayout, вложенные
гейты, чужая форма, пустой результат, обрезанный пакет); флипы (байт-точ-
ность E12 на фикстуре, width-guard NUMERIC-2б, атомарность plan,
двухфазность apply, длина инвариантна, повторный plan после флипа —
пуст); интеграция (bare/resource-каналы, NotWritable,
MutationBehindCompression, LZMA-обёртка разрешена, no-gates — Ok,
unflippable — отказ без мутации, каскад Rebuild); CLI (parse, mock e2e
с content-ассертами stdout).

**Real-image `#[ignore]`** (`tests/real_image.rs`):
`real_image_hii_unlock_matches_e12` — (1) gates_list формы 10029 → ровно
1 гейт `suppress/ref` с `"1 == 1"`, flippable; (2) gates_list вопроса
`:0x3B` → ровно 1 `grayout/question` с `"0x009A == 0x0001"`; (3) unlock
оба → build_image: длина образа и байты вне файла Setup неизменны;
(4) диф FORM-пакета против оригинала — ровно `[(0x67A,1,2), (0xDD1,1,FF),
(0xDD2,1,FF)]`; (5) extent файла Setup и длина guided-тела неизменны
(slot-fit); (6) re-parse: гейты теперь `"1 == 2"` / `"0x009A == 0xFFFF"`
и не flippable; collect_forms видит 10029. Регрессии:
`real_image_hii_form_visibility_round_trip`,
`real_image_unhide_rebuild_keeps_layout` (без UPG-ассерта),
full-flash round-trip/repatch — зелёные.

**u5 smoke (живой образ, вручную):** `hii form list` до/после unlock —
цель в списке; `hii question gates` показывает гейт и флип. HW-кандидат
E13 — по желанию, после ревью дифа.

## 7. Риски и открытые вопросы

- **Базис pkg-офсетов**: `0x67A`/`0xDD1..DD2` считаются от байтов
  FORM-пакета (тело пакета, включая 4-байтовый заголовок). Если реальный
  диф окажется смещён — это план-дефект (AGENTS.md п.11): отдельный
  docs-коммит с уточнением базиса по пробнику
  `refs/amibcp/probes/suppress_probe.py`, затем правка теста.
- **Свип энкодера** может выбрать параметры ≠ lc2/pb0 — ассерт
  сохранения раскладки (extent/guided-len), а не равенства потока E12.
- **Чужие вендоры**: незнакомые выражения → честный отказ (by design);
  практика покажет, какие классы добавлять (TRUE→FALSE — первый кандидат).
- **Width-guard**: мастер-вопрос не найден или не NUMERIC/CHECKBOX →
  флип разрешён (E11-кейс: 0x009A — NUMERIC, находится); риск только для
  2+байтовых мастеров — не встречены, задокументировано в отчёте ошибки.
- **Несколько REF→form_id** в пакете: все флипаются атомарно (единообразно
  с правилом «все объемлющие гейты»).

## 8. Влияние на компоненты

- **uefi-engine**: новый `hii/gates.rs`; `hii/mod.rs` (gates_list/unlock,
  parse_item_id, resolve_writable_path, u3-удаление); `rpc/server.rs`
  (2 handler'а, маппинг); `tests/real_image.rs` (новый тест, u3-правки).
- **uefi-proto**: `engine.proto` (2 RPC + 4 message), `build.rs` (serde
  GateInfo).
- **uefi-cli**: `client.rs`, `commands/hii.rs`, `main.rs` (clap),
  `output.rs`, `tests/{mock_server,cli_integration,e2e}.rs`.
- **TUI/Gateway/WebUI**: не затронуты.

## Отложенное (не в этом цикле)

- Флип `TRUE→FALSE` — при живом прецеденте (одна arm в `plan_flip`).
- Quirk 0x8a в легаси-walker'ах `ifr.rs` (путь `set-visibility`):
  `find_form_suppress_scope` молча возвращает None на setup-модуле —
  отдельный TODO (клинически: set-visibility на 899407D7 — no-op).
- `hii form list`: REF-гейтнутые формы показываются `visible=true`
  (гейт живёт в хаб-форме) — при подключении TUI/WebUI расширить
  FormInfo полем «gated» на базе `find_gates`.
- E13: HW-прошивка движкового кандидата (чеклист E11/E12) — по желанию
  пользователя.
