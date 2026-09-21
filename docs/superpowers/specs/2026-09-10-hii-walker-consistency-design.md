# Спека мини-цикла hii-walker-consistency (пачка 1: консистентность hii-читателей)

Дата: 2026-09-10. Источник: ревизия TODO «миноры engine+cli» (пачка 1), код-аудит
`ifr.rs`/`gates.rs`/`values.rs`/`mod.rs` от 2026-09-10. Реализация — отдельная ветка
`fix/hii-walker-consistency`, слияние через PR.

## 1. Цель

Привести легаси-читатели `hii/` к консистентному состоянию: механика обхода IFR
единая, `set_item_visibility` перестаёт молча возвращать успех без эффекта,
печатаемые смещения гейтов/флипов имеют один задокументированный контракт.
Мини-цикл из «пачки 1»; следующий за ним — cli-polish (пачка 3).

## 2. Пересмотр диагнозов TODO (аудит 2026-09-10)

Два пункта пачки при аудите кода получили иную причину, чем записано в TODO.
Спека фиксирует пересмотр; TODO закрывается с правильной причиной при реализации
(дух rule-11: история показывает, что выяснилось).

| Пункт TODO | Реальность в коде | Действие цикла |
|---|---|---|
| «quirk 0x8a не перенесён в легаси-walker'ы» | Маскирование `len & 0x7F` есть во всех walker'ах `ifr.rs` с первого коммита (`ff1a5ab`). No-op `set_item_visibility` на setup-модуле HNX99TF имеет две реальные причины: (а) **семантическая** — страница скрыта suppress'ом вокруг REF в родительской форме (форма 10002 Advanced, E12/E24), а `find_form_suppress_scope` ищет только скоуп вокруг самой формы; REF-гейты покрывает gates-слой (`Wraps::Ref`); (б) **механическая** — `find_suppress_if_scopes` считает глубину только по вложенным SUPPRESS_IF, а END любого scoped-опа декрементирует → AMI-quirk операнды с END-терминаторами (`0f9d554`) и вложенные FORM/GRAYOUT ломают баланс. | Починить механику (§3.1); семантику НЕ расширять (§4), но no-op сделать явной ошибкой (§3.2) |
| «walker игнорирует заявленную длину пакета в header bytes 0..2» | Bounds `(4, plen.min(len))` есть во всех walker'ах `ifr.rs` (`find_form_suppress_scope`, `collect_form_string_ids`, `locate_formset_insert_points`, `locate_form_end`, `parse_form_package`), кроме `find_suppress_if_scopes` — ходит до `body.len()`. | Закрыть вместе с §3.1 |
| `question_storage_width` читает qflags@+12 | Подтверждено: `gates.rs:276` читает `body[i+12]` (question-flags), numeric size-flags лежат на `@+13` (`values.rs:305,355` читают правильно). | §3.4 |
| «напечатанные смещения флипов нестабильны ±4» | Подтверждено как дизайн-дефект: базы смешаны — `UnlockOutcome.applied` печатает body-абсолют с меткой `pkg+` (`mod.rs:372`, `flip_text(0, …)`), ошибка `plan_gates` — pkg-относительный (`gates.rs:319`), `gates_list` — `start + pkg-rel` с меткой `pkg+` (`mod.rs:279,281`). Evidence E15: grayout-флип напечатан `pkg+0x9705`, байт изменён `@dec+0x9709`; suppress-скоуп `@pkg+0x8fa0` против `@dec+0x8FA4`; suppress-флип `pkg+0x8fae` — точный. | §3.3 |
| «недостающие edge-тесты» / «decoder допускает один мусорный байт» | Актуально. | §3.6 |
| «scope_balance в pub(crate)» | Дублируется в тестах `mod.rs:3634` и `real_image.rs:4162`. | §3.5 |

Отдельная фиксация знания (не пункт TODO): `ifr::unsuppress` переписывает структуру
скоупа — «пустой скоуп + выражение сдвинуто за END», т.е. производит ровно класс
байтов, который E11 доказал падающим AMITSE на setup-модуле. На Platform-формсете
(901) этот класс работал (E7/E8). Hardware-валидный путь для setup-модуля — флипы
литералов (unlock, E12-класс). Закрепляется rustdoc-контрактом (§3.7), НЕ удалением
`unsuppress`: Platform-канал жив (real-image тест 901).

## 3. Решения (дизайн)

### 3.1 Механика `find_suppress_if_scopes`

Переписать обход зеркально `find_form_suppress_scope`/gates-walker'у:

- обход opcode-aligned (`i += length`), не побайтовый `i += 1` — устраняет ложные
  срабатывания на `0x0A` внутри payload чужих опкодов;
- глубина: +1 на любой оп с scope-битом (`byte[1] & 0x80`), −1 на END (`0x29`) —
  AMI-quirk операнды (scoped-операнд + собственный END) и вложенные FORM/GRAYOUT
  перестают ломать баланс;
- bounds: `(4, plen.min(body.len()))` через общий `package_bounds`;
- контракт возврата не меняется: все скоупы пакета, no-discriminator путь
  `set_item_visibility` берёт первый.

### 3.2 Поведение `set_item_visibility` — честные ошибки вместо тихого Ok

- `visible=true`, скоуп вокруг формы не найден ни в одном пакете/канале →
  `Err(HiiError::NoSuppressScope)` — новый вариант, текст вида «form has no
  suppress-if scope of its own; REF-parent gates are the unlock op's domain»;
  RPC-маппинг → `not_found`;
- `visible=false` → сегодня всегда тихий Ok (кода сокрытия не существует) →
  `Err(HiiError::HidingUnsupported)` — новый вариант; RPC → `invalid_argument`;
- оба варианта добавляются в `hii_error_status` (`rpc/server.rs`);
- потребители: CLI `hii form set-visibility` и gateway `/set-visibility` получают
  внятные ошибки вместо ложного успеха; их тесты (вкл. mock-сервер) обновляются.

### 3.3 Контракт печатаемых смещений — единая точка истины

- Определение: **все печатаемые смещения — от начала form-пакета, включая его
  4-байтовый заголовок (u24 length + kind); метка `pkg+`**.
- Один хелпер форматирования в `hii/mod.rs` (приёмник `flip_text`); все четыре
  места печати (`GateInfo.flip`, `GateInfo.scope_offset`, `UnlockOutcome.applied`,
  текст ошибки `plan_gates`) проходят через него с pkg-относительным значением.
- В `unlock` для `applied` флипы сейчас агрегируются в body-абсолют — перевести в
  pkg-относительный (диапазон пакета, из которого флип, известен на момент сбора);
  `apply_flips` продолжает работать по body-абсолюту внутри мутации (внутреннее,
  не печатаемое представление).
- `GateInfo.scope_offset` (поле proto, u32) — pkg-относительное значение по тому
  же определению;
- пятый потребитель хелпера — `HijackResult.unlock_flips` (`form_hijack.rs`) —
  уже печатает pkg-относительные флипы (планируются на слайсе пакета);
  переводится на общий хелпер без смены вывода.

### 3.4 `question_storage_width` — numeric size-flags @+13

- Для `IFR_NUMERIC_OP` читать `body[i+13] & IFR_NUMERIC_SIZE` (как `values.rs`),
  не `body[i+12]` (question-flags);
- `IFR_CHECKBOX_OP` → 1, qid@+6 — без изменений;
- width-guard EqIdVal в `plan_flip` начинает читать правильную ширину вопроса.

### 3.5 `scope_balance` → общий хелпер

Хелпер баланса скоупов IFR поднимается в `hii::ifr` как `pub` (уточнение при
планировании 2026-09-10: `real_image.rs` — интеграционный тест, внешний крейт,
`pub(crate)` ему недоступен); локальные копии в тестах `mod.rs` и
`real_image.rs` удаляются (инвариант железо-доказан раундом 8 дуги
setup-new-page).

### 3.6 Edge-тесты

- `find_form_suppress_scope`/`find_suppress_if_scopes`/`parse_form_package`:
  stray END на пустом стеке, `length < 2`, короткий IfrFormSet (`length` 2..23),
  nested-suppress: внешний скоуп покрывает внутренний, возвращается только
  внешний (outermost-only — контракт «не меняется» из §3.1);
- `decode_expr`: тест-фиксация «ровно один мусорный байт после валидного префикса
  операндов допустим» (текущее поведение, END-quirk) — до появления новых quirk'ов.

### 3.7 Rustdoc-контракты (правило 8 AGENTS.md, смягчено 2026-09-10)

На функциях (не постфактум, а шагами задач плана — см. §8):

- `find_form_suppress_scope` — «ищет только suppress-скоуп, оборачивающий саму
  форму (E7/E8, Platform 901); гейты вокруг REF в родительской форме не ищет —
  территория gates/unlock (E12)»;
- `unsuppress` — «структурный rewrite: пустой скоуп + выражение за END;
  валидирован только на Platform-формсете; для setup-модуля класс E11-фатален —
  hardware-валидный путь: unlock (флипы литералов)»;
- `find_suppress_if_scopes` — контракт обхода §3.1;
- `question_storage_width` — «numeric size-flags @+13 (r-efi IfrNumeric)»;
- хелпер смещений §3.3 — определение базы `pkg+`.

### 3.8 `package_bounds` → общий хелпер

`package_bounds` поднимается в `hii::ifr` как `pub(crate)`; приватный дубль в
`gates.rs` удаляется, вызовы переключаются.

## 4. Не-цели

- REF-гейты в `set_item_visibility` / пересадка op на gates-слой — отдельный
  кандидат в TODO (меняет класс мутации существующего RPC);
- TRUE→FALSE-флипы, каскадный form-unlock, SIBT-унификация reader/writer;
- error-precedence NotWritable/NotFound в `resolve_*` — пачка 3 (cli-polish);
- bounds-guard'ы `plan_flip` на hand-constructed `Gate` (нет источников).

## 5. Тесты и гейты

Unit (синтетика):

- §3.1: скоуп с вложенным FORM/GRAYOUT внутри закрывается на своём END;
  scoped-операнды с END не ломают глубину; tail за пределами plen не читается;
  «0x0A» внутри payload не матчится;
- §3.2: `NoSuppressScope` на форме без собственного скоупа; `HidingUnsupported`
  на `visible=false`;
- §3.3: мульти-пакетная фикстура (≥2 form-пакета в теле секции) — захардкоженные
  ожидаемые числа `pkg+0x…` для flip/scope/applied/ошибки;
- §3.4: numeric width 2/4/8 читается корректно (SIZE_2/4/8 флаги);
- §3.6: перечень edge-случаев.

Real-image (`#[ignore]`, HNX99TF):

- регресс: Platform 901 visibility round-trip остаётся зелёным;
- новый: `set_item_visibility …#10029` на setup-модуле → `NoSuppressScope`
  (фиксирует исходную жалобу «no-op»);
- новый: ассерт «байт по напечатанному смещению == from-значению флипа» для
  `unlock`/`gates_list` (обе поля GateInfo);
- полный прогон ignore-набора (актуальное количество — по ветке на момент
  старта, на 2026-09-10 это 23+) — обязательный гейт цикла.

## 6. Потребители и миграция

- CLI: `hii form set-visibility` — тексты новых ошибок (тексты приходят из
  RPC-статуса, код CLI не меняется);
- gateway: `/set-visibility` — теперь 404/400 вместо 200-пусто (маппинг
  tonic→HTTP уже есть в `error.rs`);
- аудит при планировании 2026-09-10: ни один тест потребителей (CLI/gateway/TUI
  mock-серверы) не фиксирует текущий тихий Ok — вызовов `visible=false` в тестах
  нет, mock-хендлеры возвращают только успех; обновления тестов потребителей не
  требуется, новые варианты покрываются unit-тестом `hii_error_status`
  (`rpc/server.rs`);
- proto не меняется (новых полей нет; значения GateInfo.flip/scope_offset меняют
  смысл базы — задокументировать в release-note PR).

## 7. Риски

- `find_suppress_if_scopes` на живых образах может начать находить скоупы там,
  где раньше ломался/промахивался — полный ignore-прогон обязателен;
- изменение RPC-поведения «не найдено» — обновление тестов трёх потребителей
  (CLI, gateway, mock) входит в цикл;
- ±4-фикс меняет пользовательский вывод `gates`/`unlock` — скрипты владельца,
  парсившие `pkg+` (E15/E29), получат консистентную pkg-базу (желательно
  перепроверить руками на живом прогоне).

## 8. Процесс

- Ветка `fix/hii-walker-consistency` от master; слияние PR (правило: коммиты по
  шагам плана, docs-фиксы дефектов плана — отдельным коммитом ДО реализации);
- rustdoc-контракты §3.7 включаются в шаги задач плана явно (не отдельной
  финальной задачей);
- при закрытии цикла актуализировать TODO.md: закрыть «quirk 0x8a», «walker
  игнорирует заявленную длину», `question_storage_width`, «смещения ±4»,
  `scope_balance`, edge-тесты, «один мусорный байт» — с формулировками пересмотра
  из §2; добавить кандидат «пересадка set_item_visibility на gates-слой» (§4).

## Аддендум (2026-09-21): третий канал форм — FREEFORM_SUBTYPE_GUID (0x18)

Контекст: TODO «Setup-формы 226D2IL лежат в FREEFORM_SUBTYPE_GUID» (находка
разбора 2026-09-21). AMI Setup-файл 899407D7-99FE-43D8-9A21-79EC328CAC21 на
226D2IL3.30/.50 разложен иначе, чем на C275: COMPRESSION ctype=0 → PE32 без
ресурсов + секция 0x18 (subTypeGuid `97E409E6-4CC1-11D9-81F6-…`, 382 483 б),
тело которой — HII-список: `[GUID 16][u32 = счётчик пакетов][пакеты]` **без
END-терминатора, ровно до последнего байта** (probe 2026-09-21 на 226D2IL3.30:
1 строковый пакет на 3833 строки + 7 форм-пакетов = 85 форм; титулы «Main»,
«Above 4G Decoding» резолвятся из него же — кросс-файловый резолв не нужен).
Строгий `parse_package_list` (PE32-ресурсы) требует END — на этом блобе
возвращает None; его контракт не меняется.

Решения:

1. `package_list::parse_package_list_exact(bytes)` — tolerant-вариант: цепочка
   пакетов обязана потребить буфер ровно до конца, END-терминатор не требуется
   (если есть — допустим последним пакетом). Exact-конец и есть защита от
   false-positive на нерелевантных 0x18-секциях.
2. `forms::walk_sections` — ветка 0x18: exact-список → PACKAGE_FORMS в found,
   PACKAGE_STRINGS в titles; цикл дрейняет общий с PE32-веткой (хелпер, без
   дублирования).
3. `form_package_ranges` — ветка 0x18: диапазоны форм-пакетов exact-списка.
   Каскад без правок потребителей: `list_questions`, `find_question_map`
   (`set_value`, `question_info`), `form_export`, `cross_formset`, gates.

Таргет-грамматика: `{file-guid}:0x18:{idx}` — `find_item` generic по
subtype+index, round-trip фиксируется тестом. Пользовательский эффект: Forms
View / `hii questions` / `hii set-value` на asr1 (226D2IL) — контракт
tui-forms-view не меняется, TUI-код не трогается.

Не-цели: мутации 0x18-списка (`hii form add`), правка строгого парсера,
кросс-файловый резолв титулов.

Гейты: unit (exact-парсер: exact-end без END → Some, обрыв/мусор → None,
END последним → Some; walk 0x18 c титулами; ranges 0x18; таргет round-trip);
real-image 226D2IL3.30/.50: 85 форм от `899407D7…:0x18:0`, титулы непустые;
C275-регресс: формы из .rsrc без задвоения (0x18 там нет).

### Дополнение к аддендуму (2026-09-21, живой прогон Forms View asr1)

Два дефекта, найденных живым прогоном TUI на 226D2IL, закрыты тем же
принципом «канал во всех обходчиках»:

1. **ref_tree::collect_edges** (шестой обходчик) не знал 0x18 → REF-рёбра
   формсетов 0x18-канала не собирались, Forms View показывал E14F04FA (42
   формы) плоским списком. Фикс: ветка exact-списка, юнит tree_pkg-в-0x18.
2. **Строковый fallback**: формсеты без родных строк (D37BCD57 в 91B4D9C1
   PE32/bare, 77F2EA2F в 10989AA4 — titled=0) получили fallback из
   **крупнейшего строкового пула образа** (`image_string_fallback`,
   questions.rs): AMI централизует setup-строки в одном пакете (3833 строк
   в 0x18-блобе 899407D7), а сиды уникальны только внутри списка — пулы не
   мёржатся (коллизии: sid4 = «Disabled»/«Rescan Device»/«Version %x…»),
   поэтому берётся один самый большой. Приоритет: per-file пул → fallback.
   Применён в collect_forms (титулы форм) и prompt_texts (промпты вопросов:
   list_questions / question_info / set_value / export_form).
   Оговорка: для формсетов с родным пулом в образеfallback не активен;
   для чужих сидов возможны неточные тексты (см. TODO про string-id
   коллизии) — лучше непусто-и-примерно, чем пусто, до появления
   formset→pool связки.
