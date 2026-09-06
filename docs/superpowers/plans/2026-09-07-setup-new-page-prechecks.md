# Setup New Page NP-P — пре-чеки движка (ref-вопрос, регистрация страницы, гейты E36/E37) Implementation Plan

> 2026-09-07, ветка `setup-new-page`. Входы: спека
> `docs/superpowers/specs/2026-09-07-setup-new-page-design.md` (§5 P1–P5,
> аддендум §10), журнал NP-R `docs/reports/2026-09-07-setup-page-recon.md`
> (§3 рецепт E37, §5 выходы R). Критерий готовности: гейты np1/np2 зелёные
> на LIVE-образе, кандидаты E36/E37 собраны живым движком и отвалидированы
> (fv_audit + зонный дифф), pack-отчёт на ревью. Прошивка и вердикт —
> фаза NP-E, только владелец.

## Вход (уже готово, не делаем повторно)

| Механизм | Статус | Где |
|---|---|---|
| REF-эмиттер (FormId u16@+13, total 15) | готов, совпадает со стоком | `ifr_builder.rs` `emit_ref`; уже используется `formset_add.rs:381` |
| сплайн вопросов в живую форму (bare + PE32-resource) | готов (S3) | `ifr::splice_question_ops`, `splice_question_ops_into_resource` |
| строковые аппенды (оба канала) | готов (S3, железо E31) | `string_pack::add_strings[_to_resource]` |
| $SPF-примитивы: клон страницы, слот, длина | готов (S3, железо E31) | `spf.rs` `clone_page_with_controls`/`repoint_page_slot`/`bump_container_length` |
| $SPF-план по форме (страница формсета по form_id) | готов (S3) | `mod.rs` `plan_spf_append` |
| `hii form add` (форма + строки в живой формсет) | готов (E16) | `form_add.rs` |
| доп. факты NP-R | B@+0x12=слот родителя; seq=свой слот; marker=1; u18 — константа; зазор 4 нуля ровно до page[0]; cnt=0-страницы = ровно 0x20 байт | журнал §1/§3 |

Ключевая развязка порядка: `plan_spf_append` требует существующую страницу
формы → **`page add` обязан выполняться ДО `question add` на новой форме**
(после регистрации страницы S3-конвейер `question add` работает без правок).

## Global Constraints

- BIOS-образы не коммитятся; `refs/` read-only; JSON-фикстуры тестов —
  можно (`crates/uefi-engine/tests/data/serial/`, прецедент `s3_questions.json`).
- TDD-порядок; module-first rule; комментариев в коде нет (кроме `file:line`).
- После каждой задачи: `env -u http_proxy -u https_proxy -u HTTP_PROXY
  -u HTTPS_PROXY -u ALL_PROXY -u all_proxy cargo test -p <crate>` и
  `cargo clippy -p <crate> -- -D warnings`; `cargo fmt --all -- --check`
  в конце каждой задачи с правками кода.
- **E9-правило**: никаких сдвигов внутри $SPF — только аппенды в конец,
  занятие нулевого слота @0x64+4·count (с проверкой зазора), бамп
  count@0x60 и длины; внутри IFR-пакета сдвиг — только точкой вставки с
  компенсирующим фиксапом ifr-полей записей (`fixup_selected_record_ifr_offsets`).
- goto-вопросы БЕЗ $SPF-записей/пул-контролов/клона страницы — сток-сигнал
  NP-R (25/31 без записей); $SPF-длина при ref-add не растёт.
- Дефекты плана — отдельный `docs:`-коммит ДО реализации (правило-11).
- Ветка `setup-new-page`; push на dsevosty после каждого коммита.

## Схема данных (контракт между задачами)

LIVE-числа (NP-R §3): форма-цель **0x2775** (10101, свободна), REF-вопрос
**q0x210** (528, свободен), вопросы **q0x258/q0x259** на varstore 1,
офсеты **0x72/0x73** (занято до 0x71), родитель — форма 10019 (слот 8).

| элемент | id | строки | привязка |
|---|---|---|---|
| REF (goto) на 10019 | q0x210 | prompt `UEFIPatcher Setup`, help `UEFIPatcher serial console settings` | FormId 0x2775 |
| форма-цель | 0x2775 | title `UEFIPatcher Serial Settings` | varstores: [] (varstore 1 уже объявлен в формсете — НЕ переизлучать) |
| вопрос A | q0x258 | prompt `Serial Console`, help `Serial console output enable (demo)` | one_of [0 `Disabled`, 1 `Enabled`], vs 1 @0x72, default optimized 0 |
| вопрос B | q0x259 | prompt `Verbose Boot`, help `Verbose boot messages (demo)` | one_of [0 `Off`, 1 `On`], vs 1 @0x73, default optimized 0 |
| страница $SPF | слот 188 | title-id ← строка формы | клон-скелет страницы 10019: marker=1 (от родителя), fid=0x2775, seq=188, **B=8**, u18 от родителя, **cnt=0** |

Порядок сборки кандидатов (гейты Task 5 повторяют его в точности):

```
E36 = LIVE → вставка тройки s4 (SerialIo-донор, TerminalDxe, glue v2)
        → hii form add  (форма 0x2775 + q0x258/q0x259 + строки)
        → hii question add (refs: q0x210 на 10019)
E37 = E36-флоу + hii page add (после form add, до question add:
        parent 10019 → скелет в слоте 188, count 188→189)
```

Фикстуры: `tests/data/serial/np_form.json` (FormSetSchema: 1 форма, 2
one_of, varstores []), `tests/data/serial/np_ref.json`
(`{"refs":[{form_id:10101, prompt:…, help:…, question_id:528}]}`).

---

### Task 1: schema.rs — refs в question-add схеме

**Files:**
- Modify: `crates/uefi-engine/src/hii/schema.rs`

**Interfaces:**
- `pub struct QuestionAddRefSchema { pub form_id: u16, pub prompt: String, pub help: String, pub question_id: u16 }`
  (`#[serde(deny_unknown_fields)]` — varstore-полей нет принципиально).
- `QuestionAddList` становится: `questions: Vec<QuestionAddSchema>` →
  `#[serde(default)] pub questions: Vec<QuestionAddSchema>` +
  `#[serde(default)] pub refs: Vec<QuestionAddRefSchema>`; пустота BOTH →
  ошибка `InvalidSchema` (существующее поведение «questions must not be
  empty» обобщается). Обратная совместимость: `s3_questions.json` и гейты
  s3/s4 не меняются.

- [ ] **Step 1: failing-тесты** (в конец mod tests schema.rs, по образцу
  `parse_question_add_schema_*`): (1) parse refs-only
  `{"refs":[{"form_id":10101,"prompt":"P","help":"H","question_id":528}]}`;
  (2) parse mixed (questions + refs, оба списка на месте); (3) оба пустых
  `{"questions":[],"refs":[]}` → InvalidSchema; (4) unknown-поле в ref
  (`var_offset`) → unknown field; (5) legacy `{"questions":[…]}` без
  `refs` — парсится, refs пуст.
- [ ] **Step 2:** `cargo test -p uefi-engine schema` — падают.
- [ ] **Step 3:** Реализация (struct + default-поля + обобщённая проверка
  пустоты в `parse_question_add_schema`).
- [ ] **Step 4:** Зелёные + clippy + fmt.
- [ ] **Step 5:** Commit: `feat(uefi-engine): question-add schema v2 — optional refs (goto) list, questions now optional`.

### Task 2: mod.rs — add_ref: REF-вопрос в живую форму без $SPF-аппендов

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs`

**Interfaces:**
- `fn build_ref_ops(schema: &schema::QuestionAddRefSchema, prompt_id: u16, help_id: u16) -> Vec<u8>` —
  `IfrBuilder::emit_ref(prompt_id, help_id, qid, 0, 0, form_id)` + `emit_end`.
- `fn check_ref_slots(schema: &schema::QuestionAddRefSchema, pkg: &[u8], pending_qids: &[u16]) -> Result<(), HiiError>` —
  только уникальность qid (существующий в формсете `values::scan_question_slots`
  + уже заявленные в том же запросе); varstore-проверки НЕ применяются
  (у REF нет varstore-привязки); дубли form_id цели допустимы (сток:
  q139–q148 — self-goto).
- `pub struct AddRefResult { pub question_id: u16, pub string_ids: HashMap<String, u16> }`.
- `pub fn add_ref(image: &mut Image, item_id: &str, schema: &schema::QuestionAddRefSchema) -> Result<AddRefResult, HiiError>` —
  флоу зеркалит `add_question` (mod.rs:1168): `resolve_question_target` →
  `plan_spf_append` (страница исходной формы существует — даёт
  selected_records для фиксапов) → `check_ref_slots` → preflight (строки
  prompt/help + длина ops; переиспользовать `preflight_question_splice`,
  вынеся сборку ops в замыкание/enum — минимальная правка) → строковый
  аппенд (оба канала, как в add_question) → сплайн (bare/resource) →
  **фиксап-_only**: `spf::fixup_selected_record_ifr_offsets(node.body,
  &plan.selected_records, insert_at, delta)` — БЕЗ
  append/клона/repoint/бампа (длина контейнера не меняется) → mark
  rebuild обоих путей.
- `pub fn check_ref_add(image: &Image, item_id: &str, refs: &[schema::QuestionAddRefSchema], question_qids: &[u16]) -> Result<(), HiiError>` —
  префлайт-близнец (по образцу `check_question_add`).

- [ ] **Step 1: failing-тесты** (mod tests, фикстуры
  `question_add_fixtures` — синт-образ уже содержит формы 10019/10020 и
  $SPF-контейнер): (1) ref вставляется перед END формы 10019: в байтах
  пакета REF-оп total 15, `FormId u16@+13 == 10020` (фикстура-цель),
  qid@+6, prompt@2/help@4 = appended id; (2) $SPF после add_ref: count не
  изменился, длина контейнера не изменилась, новых записей/контролов нет,
  ifr-поля записей выше точки вставки сдвинуты на delta (фиксап работает);
  (3) `check_ref_slots`: дубликат qid существующего вопроса → InvalidSchema,
  дубликат в запросе → InvalidSchema; (4) read-only образ → NotWritable;
  (5) цель на несуществующую форму IFR проходит (проверка цели — на
  стороне вызывающего/гейта: сток допускает dangling? — НЕТ: проверять
  не будем, E16-урок — форма добавляется form add'ом до/после; фиксируем
  контрактом сборки).
- [ ] **Step 2:** `cargo test -p uefi-engine add_ref` — падают.
- [ ] **Step 3:** Реализация; при необходимости `apply_spf_question`
  рефакторится фиксом-хелпером `apply_spf_ifr_fixup(body, plan, insert_at,
  delta)`, вызываемым обоими путями (правка без изменения поведения
  существующих тестов).
- [ ] **Step 4:** Зелёные + clippy + fmt.
- [ ] **Step 5:** Commit: `feat(uefi-engine): add_ref — goto-question splice with ifr-offset fixups only (no $SPF appends)`.

### Task 3: spf.rs — append_page_skeleton + register_page_slot

**Files:**
- Modify: `crates/uefi-engine/src/hii/spf.rs`

**Interfaces** (чистые функции, офсеты контейнерные от `$SPF`):
- `pub fn append_page_skeleton(body: &mut Vec<u8>, template: usize, form_id: u16, title_id: u16, seq: u16, parent_slot: u16) -> usize` —
  копирует у шаблона 8-байтовый нулевой префикс, marker@+8 и u32@+0x18
  (плато-константы NP-R), патчит form-id@+0xA, title-id@+0xE, seq@+0x10,
  B@+0x12=parent_slot, cnt@+0x1C=0; **размер ровно 0x20** — без списка и
  хвостового слота (прецедент LIVE: cnt=0-страницы 10009/10003 —
  ровно 0x20); аппенд в конец; возвращает офсет скелета.
- `pub fn register_page_slot(body: &mut [u8], skeleton_offset: usize) -> Option<usize>` —
  читает count@sig+0x60, слот=base+0x64+4·count; если u32 по адресу
  слота НЕ 0 (зазор занят — E9-страховка) → None; иначе пишет
  skeleton_offset, count += 1; возвращает индекс нового слота.
  `bump_container_length` — существующий, вызывается op-слоем.

- [ ] **Step 1: failing-тесты** (расширить `synth_container`: таблица из
  2 страниц + валидный 4-байтовый зазор нулей после неё — как LIVE
  0x354→0x358; отдельный контейнер-без-зазора): (1) skeleton: длина тела
  +0x20, префикс маркера/u18 от шаблона, патчи fid/title/seq/B, cnt=0,
  всё до аппенда байт-в-байт нетронуто; (2) register: слот count пишет
  офсет, count+1, остальные байты нетронуты; (3) register при занятом
  зазоре → None и байты не тронуты; (4) round-trip: scan слотов после
  register находит новую страницу как валидную (8-нуль префикс, fid).
- [ ] **Step 2:** `cargo test -p uefi-engine spf::` — падают.
- [ ] **Step 3:** Реализация.
- [ ] **Step 4:** Зелёные + clippy + fmt.
- [ ] **Step 5:** Commit: `feat(uefi-engine): spf page skeleton + zero-slot registration (count bump, occupied-gap guard)`.

### Task 4: op `hii page add` — движок + proto + RPC + CLI

**Files:**
- Modify: `crates/uefi-engine/src/hii/schema.rs`, `crates/uefi-engine/src/hii/mod.rs`,
  `crates/uefi-proto/proto/engine.proto`, `crates/uefi-engine/src/rpc/server.rs`,
  `crates/uefi-cli/src/client.rs`, `crates/uefi-cli/src/commands/hii.rs`,
  `crates/uefi-cli/src/main.rs`, `crates/uefi-cli/src/output.rs`

**Interfaces:**
- `pub struct PageAddSchema { pub form_id: u16, pub title: String }`
  (deny_unknown_fields); `pub fn parse_page_add_schema(json: &str) -> Result<PageAddSchema, HiiError>`
  (title непустой).
- `pub struct AddPageResult { pub form_id: u16, pub slot: usize, pub page_offset: usize, pub title_string_id: u16 }`.
- `pub fn add_page(image: &mut Image, item_id: &str, schema: &schema::PageAddSchema) -> Result<AddPageResult, HiiError>` —
  item_id = target форм-пакета + `#<parent_form_id>` (дискриминатор как у
  question add): `resolve_question_target` (родитель) → `plan_spf_append`
  (даёт page_offset/page_slot родителя) → проверка, что страница
  `schema.form_id` ещё не зарегистрирована в таблице $SPF (скан слотов
  по fid@+0xA → повторная регистрация = InvalidSchema; IFR-форма к
  этому моменту уже есть — порядок сборки: form add → page add) →
  строковый аппенд `[title]` (оба канала) → `append_page_skeleton(body,
  plan.page_offset, form_id, title_id, seq=count, parent_slot=plan.page_slot)` →
  `register_page_slot` (None → `HiiError::InvalidIfr`… нет — новый вариант
  `HiiError::SpfSlotOccupied`? — не плодить: `InvalidSchema("page table
  gap occupied")`) → `bump_container_length` → mark rebuild (setup +
  setupdata). seq = count ДО бампа (индекс нового слота).
- proto: `rpc HiiPageAdd(HiiPageAddRequest) returns (HiiPageAddResponse);`
  `message HiiPageAddRequest { string target=1; string schema_json=2; }`,
  `message HiiPageAddResponse { uint32 form_id=1; uint32 slot=2; uint32 page_offset=3; uint32 title_string_id=4; }`.
- RPC-сервер: parse → write-lock → `hii::add_page` (статус-маппинг по
  образцу `hii_question_add`, server.rs:944).
- refs-wiring (закрывает пробел, найден pre-flight сканом): RPC
  `HiiQuestionAdd` обязан обрабатывать и `.refs` — иначе сборка
  кандидата E36 командой `uefi-cli hii question add --file np_ref.json`
  (прецедент E31) молча no-op'ает (handler итерирует только
  `schema.questions`, server.rs:952-960). После
  `check_question_add` — `check_ref_add(img, target, &schema.refs,
  &question_qids)` (qid всех вопросов того же запроса), затем `add_ref`
  на каждый ref; `HiiQuestionAddResponse` расширяется полем
  `repeated HiiQuestionAddOutcome refs = 2` (у ref нет
  spf_record_offset — отдельное поле вместо семантики «0 = нет»);
  CLI-команда `hii question add` не меняется (JSON уже идёт насквозь),
  output.rs печатает refs-outcomes тем же принтером.
- CLI: `uefi-cli hii page add <target> --file <schema.json> [--sock …] [--format …]`
  (парсинг/регистрация по образцу `question_add`, main.rs:442);
  `print_page_add` в output.rs (text + json smoke-тест).

- [ ] **Step 1: failing-тесты**: движок (фикстуры question_add_fixtures):
  (1) add_page: count+1, слот содержит офсет скелета, скелет
  fid/title/seq/B/marker/u18/cnt=0, родительская страница байт-в-байт
  нетронута, длина контейнера выросла на 0x20, header-регион-офсет
  последней длины обновлён; (2) round-trip `build_image` → re-parse →
  повторный add_page того же form_id → ошибка (страница уже
  зарегистрирована в таблице $SPF → InvalidSchema; сам round-trip
  доказывает, что страница пережила rebuild); (3) пустой title → InvalidSchema. RPC: статус-тест по
  образцу `question_add_status` (server.rs:1630) + тест refs-обработки
  в `hii_question_add` (refs-only schema → outcome в `refs`, вопросы
  не задеты). CLI: parse-тест
  `parse_hii_page_add_args` + print-smoke.
- [ ] **Step 2:** Тесты падают (proto-поля ещё не сгенерированы —
  proto-правка в этом же шаге после записи failing-тестов, генерация
  через обычный build).
- [ ] **Step 3:** Реализация: schema → движок → proto → rpc → cli → output.
- [ ] **Step 4:** `cargo test -p uefi-proto -p uefi-engine -p uefi-cli`
  + clippy всех трёх + fmt.
- [ ] **Step 5:** Commit: `feat: hii page add — $SPF page registration op (engine+rpc+cli), skeleton from parent page clone`.

### Task 5: real-image гейты np1 (E36) / np2 (E37)

**Files:**
- Add: `crates/uefi-engine/tests/data/serial/np_form.json`, `crates/uefi-engine/tests/data/serial/np_ref.json`
- Modify: `crates/uefi-engine/tests/real_image.rs`

**Инварианты np1 (E36 = тройка s4 + form add + refs add):**
- переиспользовать хелперы гейта s4c (вставка тройки в FV1-хвост,
  q512/q513); новые шаги — `hii::form_add` (np_form.json, target
  Setup-формсета) и `hii::add_ref` (np_ref.json);
- байтовый дифф против LIVE — только 3 зоны: FV1-хвост, слот Setup,
  слот SetupData;
- $SPF: count == 188, container_len не вырос, записей/контролов столько
  же (фиксапы ifr-полей допустимы — проверяются отдельно: записи выше
  точки вставки смещены на delta);
- IFR round-trip: форма 0x2775 присутствует, REF на 10019 → FormId@+13
  == 0x2775, qid 0x210; q0x258/q0x259 — round-trip значений
  (values-механика как в s3-гейте);
- save → open → save байт-стабилен.

**Инварианты np2 (E37 = np1 + page add):**
- $SPF: count 188→**189**, slot[188] == офсет скелета; скелет:
  marker==marker(родителя), fid==0x2775, title-id == appended,
  seq==188, B==8 (слот родителя — сверить с plan), cnt==0, префикс
  8 нулей; слоты 0..187 и тела всех существующих страниц байт-в-байт;
  длина контейнера выросла ровно на 0x20 (+ bump-поле);
- вопрос add (S3-механика) на форме 0x2775 теперь находит страницу
  (план по form_id) — smoke через публичный `check_question_add` с
  throwaway-вопросом (qid 0x25A, vs 1 @0x74, one_of 2 опции): Ok(())
  после page add (`plan_spf_append` вызывается внутри — резолв
  доказан), в np1 без страницы тот же вызов → NotFound; мутаций нет
  (вопросы уже в IFR через form add; $SPF-запись = E38-косметика);
- дифф-зоны и round-trip — как np1.

- [ ] **Step 1:** фикстуры np_form.json/np_ref.json (данные §«Схема
  данных») + failing-гейты `real_image_ops_insert_serial_np1` /
  `real_image_ops_insert_serial_np2` (`#[ignore]`, LIVE-путь — константы
  из real_image.rs).
- [ ] **Step 2:** `cargo test -p uefi-engine --test real_image -- --ignored`
  (LIVE-файл присутствует) — падают.
- [ ] **Step 3:** Реализация гейтов (гейты не правят движок; падение =
  дефект Tasks 1–4 → правило-11 при расхождении с планом).
- [ ] **Step 4:** Оба зелёные + полная серия `--ignored` не регрессирует
  (31+2) + clippy + fmt.
- [ ] **Step 5:** Commit: `test(uefi-engine): np1/np2 real-image gates — E36 goto-only and E37 page-registration replay on LIVE`.

### Task 6: кандидаты E36/E37 + pack-отчёт + статусы

**Files:**
- Add: `docs/reports/2026-09-07-setup-np-e36-e37-pack.md`
- Modify: `roadmap.md` (NP-P закрыта, NP-E — кандидаты готовы к прошиву), при необходимости `TODO.md`

- [ ] **Step 1:** Сборка живым движком (`/tmp/setup-np/`, UEFIPATCHER_DATA
  там же, `rm -f` сокета перед стартом; команды — идентичны гейту): E36 →
  sha256, размер; E37 → sha256. Постоянные копии `~/E36/`, `~/E37/`
  (вне git), sha сверить после копирования.
- [ ] **Step 2:** Независимая валидация: `hack/fv_audit.py` (FV1 216→219,
  used/free_tail дельты), зонный дифф против LIVE (вне зон 0),
  повтор ParseImage живым движком.
- [ ] **Step 3:** Pack-отчёт: состав каждого кандидата (таблица стартов
  FV1-хвоста), инварианты (сводка гейтов), протокол приёмки с деревом
  исходов E36 (a–d по спеке §6) и E37, ветка отката (E32
  `~/E32/E32-candidate-8def2850.bin`, E5C88C6F), инструкция прошивки
  владельцу (прецедент E31-pack §5).
- [ ] **Step 4:** roadmap/TODO-статусы; Commit:
  `docs(np-p): E36/E37 candidates packed — goto-only discriminator + page registration; arc ready for NP-E flash`;
  push dsevosty.

## Self-Review (выполнен при написании)

- Порядок зависимостей: schema → движок ref → spf-примитивы → op page
  add → гейты → pack; ни одна задача не требует ещё не созданного.
- Все новые офсеты/поля — из NP-R (доказано на двух образах); ни одного
  хардкода, не покрытого журналом или константами spf.rs.
- Существующие тесты/фикстуры не меняются (кроме расширения
  synth_container и обобщения пустоты в QuestionAddList — поведение
  legacy-JSON сохранено).
- Гейты не флешат: вся железная часть — NP-E (владелец).

## Риски (из NP-R, не блокеры)

- **Занятый зазор слота** (register → None): на LIVE/MNX подтверждён
  свободным; страховка — явная ошибка op, не тихая порча.
- **zero-scan vs count**: слот+бамп валидны в обеих семантиках (журнал
  §1); остаточный риск — валидация TSE длин регионов → исход E37-(d),
  откат на E32.
- **goto без fallback-рендера** (0/89 стока): ожидаемый исход E36 — (b);
  это план (E37 готов в том же пакете), не авария.
- **PE32-resource рост** (обе правки Setup-модуля): механика
  `try_grow_rsrc_tail` доказана E16/E30/E31; P4-запас LZMA-слота
  проверяется гейтом np2 неявно (сборка либо влезает, либо падает
  до прошивки).
