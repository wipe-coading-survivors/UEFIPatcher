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
