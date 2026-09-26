# Спека: hii-errors-cleanup — путающие ошибки HII + диагностика сокета (класс C)

Дата: 2026-09-26. Ветка реализации: `hii-errors-cleanup` (спека и её
ревью-правки — docs-коммитами в master по паттерну проекта; код — только
в ветке).

Первоисточники: TODO.md поз. 1322 (set_item_visibility: error-precedence),
2890 (set_value: NotWritable до парсинга item_id), 2143 (plan_gates
неидемпотентен), 1387 (non-HII канал: Ok-пусто вместо NotASetupItem), 2997
(form-level unlock не каскадирует — подсказка), 3013 (заметка-факт о списке
qid сценария B), 2919 (hii_set_value flush и на no-op), 1374 (слабый
предикат `gates.len() <= 1`), 2884 (третий дубль QuestionInfo-литерала),
1369 (клон образа ради session_id), 3510/3563 (tracing-паритет), 1409
(transport error без сокета/источника). Классификация «класс C: путающие
ошибки» — триаж-сессия владельца 2026-09-25..26 (класс A закрыт
hii-read-truth PR #26, класс B — hii-write-guard PR #27).

## Контекст и проблема

Класс C — ошибки честные по байтам (данные не портятся, вывод не врёт), но
путающие: неверный порядок ошибок прячет опечатку за режимом, повтор
идемпотентной по смыслу операции падает, «нечего делать» неотличимо от
«не тот объект», а диагностика соединения не сообщает, куда стучался
клиент. Каждая из них уже стоила времени в живых сессиях (E15/E27 —
идемпотентность; эксплуатация 2026-09-19 — молчащий сокет).

## Решения

### 1. Единый контракт порядка ошибок (TODO:1322, 2890)

Статус-кво: `resolve_writable_path` (hii/mod.rs:189-214) проверяет
`ImageMode::Write` **до** резолва цели; `set_value` (mod.rs:988-990) —
режим **до** парсинга item_id. В Read-режиме любая опечатка в target
невидима за NotWritable.

Контракт (единый для `set_item_visibility`, `set_value`, `unlock`):

1. malformed item_id → новый вариант `HiiError::InvalidItemId(String)`
   → RPC `invalid_argument`, текст «malformed item_id …: expected
   `<target>[#<form>[:<qid>]]`»;
2. цель не резолвится (`find_item`/`find_item_path`) → `NotFound`;
3. `ImageMode != Write` → `NotWritable` (текст variant'а уже подсказывает
   reopen в write);
4. далее — барьер `MutationBehindCompression` и семантика (как сегодня).

Реализация: выделить общий хелпер резолва (parse → find → mode → barrier),
заменяющий `resolve_writable_path` во всех трёх call-сайтах; `parse_item_id`
(mod.rs:171-187) вместо `.map_err(|_| HiiError::NotFound)` возвращает
`InvalidItemId` (сегмент, на котором parse упал). `hii_error_status`
(rpc/server.rs:54) расширяется веткой InvalidItemId → invalid_argument;
`hii_error_status_ctx` (server.rs:73) его не трогает (ctx-обогащение —
только NotFound). gates_list (read-only) переходит на parse с InvalidItemId,
резолв и дискриминацию §3 — без mode-гейта.

### 2. Идемпотентность unlock: аудит-факт + warn (TODO:2143)

Аудит кода 2026-09-26: **повторный unlock уже не падает** — own-фаза
(mod.rs:436) и кросс-фаза (mod.rs:537) обе используют
`plan_gates_skip_unlocked` (gates.rs:401), hijack — тоже
(form_hijack.rs:158); no-op не флашит диск (server.rs:1078, тест
`hii_unlock_noop_keeps_artifact_untouched`). Это сделали дуга
formset-unlock (2026-09-12) и hii-write-guard; запись TODO:2143 устарела.

Остаток по решению владельца: **`tracing::warn!` при skip вскрытого
гейта** — в own- и кросс-фазах unlock для каждого найденного гейта с
unlocked-выражением (предикат EqConst `a≠b` / EqIdVal `0xFFFF` — вынести
в pub(crate) рядом с `plan_gates_skip_unlocked`, gates.rs:395-396)
логировать
`warn!(offset = pkg+…, expr, "gate already unlocked — skipped")`. Warn —
не Err: идемпотентность сохраняется. TODO:2143 закрывается с пометкой
«реализовано formset-unlock/write-guard, warn добавлен этим циклом».

`GateExpressionUnsupported` остаётся только для неподдерживаемых классов
выражений (составные q61-гейты, TRUE — см. «Вне скоупа»).

### 3. Дискриминация non-HII цели (TODO:1387)

Статус-кво: gates_list на Section без form-пакетов возвращает Ok с пустым
списком (mod.rs:371-377), unlock — Ok без мутации. Расхождение со спекой
unlock-op §5 (NotASetupItem).

Контракт: Section-цель, у которой **read-селектор пуст**
(`form_package_ranges_read`, mod.rs:240 — resource+bare+RAW+0x18 все
каналы) → `NotASetupItem` в `gates_list` и в own-фазе `unlock`
(маппинг в invalid_argument уже есть). Форм-пакеты есть, гейтов нет →
Ok с пустым списком (легитимный ответ «нечего вскрывать»).

Смежный случай без изменения семантики: bare-only PE32 (EDK2) — read-селектор
не пуст, мутационный (`form_package_ranges`, mod.rs:219) пуст → unlock
возвращает Ok без мутации как сегодня (известное ограничение TODO:397
«bare-канал не мутабелен»); добавить `tracing::warn!`
«form packages visible via read-only bare channel; bare channel is not
mutable» — молчаливый no-op становится видимым в логе.

### 4. Подсказка остаточных гейтов при form-level unlock (TODO:2997, 3013)

Семантика не меняется: form-таргет вскрывает хаб-REF/обёртку формы,
построчные гейты вопросов — отдельными `question unlock` (E25/E27-паттерн).

CLI: `print_unlock` (uefi-cli/src/output.rs) при form-таргете
(item_id с `#<form>` без `:qid`) печатает после applied-флипов строку:

```
note: form-level unlock does not unlock per-question gates;
  list: hii question gates <item_id>, unlock: hii question unlock <target>#<form>:<qid>
```

TUI: `:hii … unlock` дописывает ту же note в `status_msg` при
form-таргете.

**Отступление от согласованного эскиза:** число оставшихся гейтов N не
считаем — точный подсчёт требует перебора вопросов формы (новый walker
или N вызовов find_gates); по YAGNI печать безусловна для form-таргета.
При живом запросе (владелец споткнётся о недостаток подсказки) — расширить
счётчиком за счёт `list_questions` + per-question walk.

Детализация текста note (план): для form-таргета `item_id ==
<target>#<form>`, поэтому печатаются конкретные значения —
`list: hii question gates {item_id}, unlock: hii question unlock
{item_id}:<qid>` (плейсхолдер `<qid>` остаётся: qid неизвестен).
TUI — та же note одной строкой в status_msg.

TODO:3013 (скорректированный список qid сценария B) закрывается
docs-коммитом как зафиксированный факт — код не меняется.

### 5. Условный flush в hii_set_value (TODO:2919)

`set_value` умеет no-op Ok (идемпотентность: тесты
`set_value_repeated_is_noop_report`, `set_value_partial_noop_reports_only_
real_changes` — applied несёт только реальные изменения), а хендлер
`hii_set_value` (server.rs:1136) флашит безусловно. Фикс: условный flush
`!outcome.applied.is_empty()` — в точности зеркало hii_unlock
(server.rs:1078); поле `applied: Vec<String>` у ValueOutcome уже есть
(mod.rs:882-886). Серия
«no-op RPC не должен писать на диск» закрыта для всех HII-мутаторов.

### 6. Сопутствующая механика

- **TODO:1374** — тест обрезанного пакета в gates.rs ассертит
  `gates.len() <= 1`: захардкодить точное ожидание (1 гейт —
  suppress-ref формы 10029: walker emit'ит гейт на statement-опе
  до обрезанного хвоста, under-walk невозможен; фикс плана
  2026-09-26 по аудиту find_gates/emit_gates/package_bounds)
  вместо слабого предиката.
- **TODO:2884** — tests-mod `mock_question()` в uefi-cli/src/output.rs
  расширить DefaultEntry; удалить третий инлайн-литерал QuestionInfo.
- **TODO:1369** — `hii_unlock` (server.rs:1069) берёт `get_or_load_image`
  (клон всего образа) ради `session_id` для touch. Фикс: session_id из
  лёгкого чтения `images.lock().get(id).map(|i| i.session_id.clone())`
  (образ гарантированно в кэше — мутация только что шла через этот же
  lock); get_or_load_image из хендлера убрать.
- **TODO:3510/3563** — tracing-паритет: `hii_list_questions` без
  info-строки (server.rs:1106-1119), `hii_list_forms`/`hii_list_strings`/
  `hii_form_tree` — сверить и выровнять с соседями (image_id, target/
  form_id, count, «hii … listed»).

### 7. Диагностика transport-ошибки клиента (TODO:1409)

Статус-кво: `resolve_sock` (uefi-common/src/state.rs:54) — приоритет
cli > env (`UEFIPATCHER_SOCK`) > state-файл > default; при недоступном
сокете uefi-cli падает с голым `transport error (RPC_INTERNAL)` без пути.
Эксплуатационный кейс 2026-09-19: state-файл пинил устаревший sock_path,
клиент молча стучался в мёртвый сокет при живом движке на новом дефолте.

Реализация:

- uefi-common: `pub enum SockSource { CliArg, Env, StateFile, Default }`;
  `pub fn resolve_sock_with_source(cli_sock: Option<&str>, state: &State)
  -> (PathBuf, SockSource)` — вся логика приоритета переезжает сюда;
  `resolve_sock` становится делегирующей обёрткой (call-сайты engine/
  gateway не меняются).
- uefi-cli `connect` (uefi-cli/src/client.rs:29): при transport-ошибке
  текст включает резолвнутый путь и источник:
  `transport error: cannot connect to <path> (source: <sock source>)`.

TUI-коннект — вне скоупа (симметричная правка при следующем касании
TUI-клиента).

## Вне скоупа

- **TODO:1364** (precedence HidingUnsupported/NotASetupItem в
  set_item_visibility) — отложено до пересадки op на gates-слой (TODO:1348):
  смена класса ошибки дважды за два цикла — потеря диагностики.
- **Каскад form-unlock на вопросы** — семантика не меняется, только
  подсказка (§4).
- **TRUE→FALSE класс флипов** (TODO:1302/3895) — живой прецедент есть
  (скрытый GOTO→Chipset на 450x), но это новый класс мутации, а не
  диагностика; кандидат в собственный мини-цикл.
- **bare-канал мутабельность** (TODO:397) — осознанное ограничение;
  этим циклом только warn (§3).

## Тестирование

- **C1**: параметризованный набор на каждый op: Read+malformed →
  InvalidItemId; Read+unknown target → NotFound; Read+valid → NotWritable;
  Write+unknown → NotFound; Write+malformed → InvalidItemId. RPC-маппинг:
  `hii_error_status` ветка InvalidItemId.
- **C2**: предикат unlocked-выражения (pub(crate), §2) — юнит на все
  классы выражений; повторный unlock → Ok, applied пуст (тест
  `unlock_skips_already_unlocked_gate` уже есть — не дублировать);
  warn-строка — тривиальный format, покрытие код-ревью.
- **C3**: Section без form-пакетов → NotASetupItem (gates_list + unlock);
  форма без гейтов → Ok пусто; bare-only PE32 → Ok + warn.
- **C4**: CLI e2e на мок-сервере — note-строка присутствует при
  form-таргете, отсутствует при question-таргете; TUI — status_msg.
- **C5**: set_value no-op → артефакт не тронут (mtime/байты — по образцу
  `hii_unlock_noop_keeps_artifact_untouched`); gates-тест с точным
  ожиданием; mock_question без третьего литерала (компиляция + ассерты).
- **L1409**: unit на приоритет источников (кли > env > state > default —
  уже есть, расширить проверкой SockSource); e2e на мёртвом сокете —
  текст ошибки содержит путь и source.
- Real-image: новых гейтов нет (байтоизменяющего поведения нет) —
  smoke-прогон живого образа после цикла: unlock повторный, gates на
  не-HII, set_value идемпотентный, save round-trip.

## Риски

- **InvalidItemId — новый клиентский код ошибки** (invalid_argument вместо
  not_found при malformed): CLI-тесты, ассертящие RPC_NOT_FOUND для
  «несуществующего item_id», разделяются на malformed/not-found кейсы;
  TUI-вывод ошибки меняется только для мусорного ввода (улучшение).
- **NotASetupItem на ранее-Ok-пустых ответах** (C3): клиенты, молча
  переживавшие пустой список, получат ошибку на не-HII целях — это
  задумано (грабли звенят); TUI Forms-панель на не-HII образе показывает
  пусто через list-forms (не gates) — не задет.
- **Убийство get_or_load_image из hii_unlock** (TODO:1369): если образ
  теоретически не в кэше — session_id None → touch пропускается (уже
  `let _`); деградации нет.
