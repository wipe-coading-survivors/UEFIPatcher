# Спека: positional-insert — позиционная вставка REF/вопроса (IntelRCSetup топ-уровнем бара)

Дата: 2026-09-21. Ветка реализации: `positional-insert` (спека и её
ревью-правки — docs-коммитами в master по паттерну проекта; код — только
в ветке). Первоисточники: TODO.md:3896 (**uefi-engine: позиционная
вставка REF/вопроса — IntelRCSetup топ-уровнем бара Setup**),
вердикт-аддендум §7 спеки formset-unlock
(`2026-09-12-formset-unlock-design.md`: рабочая схема REF3 в Advanced
10002 — вложенный пункт; топ-уровень бара требует позиционной вставки +
qid 0), брейншторм владельца 2026-09-21 (якорь goto_form_id+
question_id; подход «anchor внутри splice»).

## Контекст и проблема

`add_ref`/`add_question` вставляют стейтмент перед END целевой формы —
всегда в конец (`splice_question_ops` → `locate_form_end`,
`hii/ifr.rs:312`/`:368`). Для обычных форм этого достаточно, но пункт
бара корневого Setup обязан стоять **между** существующими GOTO-детьми:
цель владельца — вкладка «IntelRCSetup» между «Advanced» и
«Server Mgmt» (Main|Advanced|IntelRCSetup|Server Mgmt|Security|Boot|
Save&Exit). Вставка в конец даёт пункт ПОСЛЕ «Save & Exit».

Живая геометрия (вердикт §7, образ `refs/amibcp/450x — копия.bin`):
бар AMITSE строится из **qid-0 GOTO-детей корневой формы 10000**
формсета `899407D7-…` — 6 шт: 10001 Main, 10002 Advanced, 10008 Chipset
(suppress_if TRUE, TODO:3850), 10009 Security, 10010 Boot, 10012
Save&Exit; «Server Mgmt» дорисовывается TSE извне (IPMI-приложение).
REF3 с ненулевым qid вкладкой не принимается — топ-уровень бара
требует qid 0. Пункт TODO «форма 1» неточен: корневая форма бара —
10000 (живые байты, §7); форма 1 — цель IntelRCSetup-стороны.

> Примечание: TODO:3896 упоминает «вариант спеки §7 сейчас целится в
> Chipset 10008» — это закрытый внедрённый пункт REF3 в Advanced 10002
> (вложенный); настоящая дуга — топ-уровневый бар.

Что уже готово к переиспользованию:

- `$SPF`-сдвиг уже пороговый: `fixup_selected_record_ifr_offsets`
  (`hii/spf.rs:263`) двигает только записи с `ifr_offset >= threshold`;
  `apply_spf_ifr_fixup` (`hii/mod.rs:1323`) зовётся с фактическим
  `insert_at` — середина формы для него не новый случай.
- REF-цели различает готовый `ref_variant::parse_ref`
  (`hii/ref_variant.rs:26`): REF/REF2/REF3 → `RefTarget` c `form_id`.
- Resource-канал (`splice_question_ops_into_resource`, `hii/mod.rs:1152`)
  и bare-канал сходятся в один `ifr::splice_question_ops` — позиция
  достаточно параметризовать в одном месте.
- RPC `HiiQuestionAddRequest.schema_json` — схема течёт строкой
  (`crates/uefi-proto/proto/engine.proto:324`): новое JSON-поле доступно
  CLI/TUI без изменений протокола.

## Решения владельца (брейншторм 2026-09-21)

- **Якорь**: `insert_before` c `goto_form_id` (навигационный: первый
  REF-стейтмент формы, ведущий на форму — покрывает кейс бара, где все
  qid 0) или `question_id` (первый вопрос с данным qid — для обычных
  форм). Нет поля = конец формы (текущее поведение). Направление
  «before» только; сырой offset-якорь — нет (хрупко, YAGNI).
- **Подход A — anchor внутри splice**: `splice_question_ops` получает
  позицию enum'ом и сам резолвит офсет якоря по свежему пакету в момент
  вставки. Один код-путь; офсет не течёт между слоями (этап string-pack
  до splice меняет тело узла — координаты, вычисленные заранее,
  невалидны). Отдельная функция или низкоуровневый offset-API — нет
  (дублирование u24-фиксапа / риск рассинхрона).
- **Карве-аут qid 0 для refs**: qid 0 — навигационный (без NVRAM-
  хранилища), дублирование легитимно по живому образцу (6 qid-0 GOTO в
  форме 10000). Уточнение от 2026-09-21 (найдено при реализации Task 4):
  `check_ref_slots` сканирует слоты через `scan_question_slots` →
  `is_question_op`, который НЕ включает `IFR_REF_OP` — барные GOTO
  слот-чек никогда не видел, единственный qid-0 REF в запросе проходит
  и без карве-аута. Карве-аут load-bearing для `pending_qids`-ветки
  (несколько qid-0 REF в одном запросе — кейс «две вкладки») и остаётся
  явным правилом; для storage-вопросов `add_question` запрет qid 0
  остаётся (qid адресует `$SPF`-запись).
- **Live-гейт** — за владельцем; primary-якорь `goto_form_id: 10008`
  (= сразу после Advanced, визуально «между Advanced и Server Mgmt»;
  Chipset suppressed и не рисуется; если TODO:3850 позже снимется,
  порядок Main|Advanced|IntelRCSetup|Chipset|… — владелец хочет
  IntelRCSetup сразу после Advanced).

## Решение

### 1. JSON-контракт (`hii/schema.rs`)

`QuestionAddRefSchema` (`:255`) и `QuestionAddSchema` (`:225`) получают
опциональное поле:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub insert_before: Option<InsertBefore>,
```

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertBefore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goto_form_id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<u16>,
}
```

`parse_question_add_schema` валидирует: задан **ровно один** из двух
ключей (оба/ни одного → `InvalidSchema` с перечнем полученного).
Кейс владельца:

```json
{ "form_id": 1, "prompt": "IntelRCSetup",
  "help": "Intel RC Setup Configuration",
  "question_id": 0,
  "formset_guid": "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
  "insert_before": { "goto_form_id": 10008 } }
```

(10008 = 0x2718 — GOTO→Chipset в корневой форме 10000; вставка перед ним
= сразу после GOTO→Advanced 10002.)

### 2. ifr.rs — позиция внутри splice

Новый публичный enum + параметризованная сигнатура:

```rust
pub enum InsertPos {
    End,
    BeforeGoto(u16),
    BeforeQuestion(u16),
}

pub fn splice_question_ops(
    package: &mut Vec<u8>,
    formset_idx: usize,
    form_id: u16,
    pos: InsertPos,
    ops: &[u8],
) -> Result<(usize, usize), HiiError>
```

Резолв позиции `locate_insert_at(pkg, formset_idx, form_id, pos)`:

- `End` → `locate_form_end` (как сейчас);
- `BeforeGoto(f)` → обход стейтментов **внутри целевой формы**: границы
  даёт лёгкая реструктуризация `locate_form_end` (`hii/ifr.rs:368`) в
  `form_span` (start = найденный `IFR_FORM_OP`, end = текущий возврат),
  `locate_form_end` остаётся тонкой обёрткой на время миграции (§3
  переводит его последних вызывателей на `locate_insert_at`; после этого
  обёртка удаляется — мёртвый pub(crate)-код не проходит clippy
  `-D warnings`; уточнение от 2026-09-21). Первый стейтмент с opcode
  `IFR_REF_OP`, у которого `ref_variant::parse_ref` возвращает цель с
  `form_id == f` (варианты `Form`/`FormQuestion`/`Formset` —
  formset_guid цели игнорируется, внутри формы бара цели уникальны по
  form_id; `Dynamic` не матчится — цель статически неразрешима).
  Возврат — offset начала стейтмента;
- **Подъём к scope-блоку**: если якорь-стейтмент лежит внутри
  scope-блока (scope-бит длины; практически — gate-опкоды
  SUPPRESS_IF/GRAY_OUT_IF/DISABLED_IF, реализация поднимает по любому
  opener'у), insert_at
  поднимается к началу самого внешнего охватывающего блока внутри
  формы. Живой кейс: GOTO→10008 в форме 10000 закрыт `suppress_if TRUE`
  (`@pkg+0x6da`, вердикт §7) — вставка «перед 10008» обязана встать
  перед suppress-блоком (видимый пункт сразу после Advanced), а не
  внутрь него (иначе новый REF унаследует невидимость якоря).
  Следствие для якорь-подбора в тестах/живых схемах: якорь внутри
  gate-блока вставляется перед блоком — сосед за вставкой это
  gate-опкод, не сам якорь (подтверждено на HNX: первый $SPF-рекорд
  формы 10019 — CHECKBOX внутри SUPPRESS_IF; уточнение от 2026-09-21).
- `BeforeQuestion(q)` → первый question-op (`values::is_question_op`)
  внутри формы с QuestionId == q (QuestionId — u16@+6 question-header,
  как в `spf_record_resolves`, `hii/mod.rs:1239`).

Якорь не найден → `InvalidSchema` со списком доступных якорей формы
(REF-цели `form_id → offset`, qid'ы) — по образцу диагностики
TODO:3861 (валидация цели add_ref). После резолва — тот же splice +
u24-фиксап, что и сейчас. Все существующие вызовы мигрируют на
`InsertPos::End` — поведение байт-в-байт не меняется.

### 3. mod.rs — проброс и проверки

- `add_ref` (`:1925`) / `add_question` (`:1544`): конвертация
  `schema.insert_before` → `InsertPos` (None = `End`), проброс в оба
  канала: bare — `ifr::splice_question_ops(…, pos, …)`; resource —
  `splice_question_ops_into_resource` получает `pos` и передаёт внутрь
  (после роста .rsrc splice идёт по свежей копии пакета — координаты
  якоря считаются в момент вставки).
- `preflight_question_splice` (`:1406`) и `check_rsrc_question_splice`
  (`:1113`): вместо проверки «форма найдена» (`locate_form_end`) —
  резолв полной позиции `locate_insert_at` (якорь проверяется до
  мутации; message листинга доступных целей идентичен runtime-ошибке).
- **Карве-аут qid 0**: `check_ref_slots` (`:1897`) — оба чека (слоты и
  `pending_qids`) при `schema.question_id == 0` в refs-ветке
  пропускаются (навигационный qid; load-bearing — `pending_qids`:
  несколько qid-0 REF в одном запросе; слот-чек REF'ов и так не видит —
  `is_question_op` без `IFR_REF_OP`, уточнение от 2026-09-21;
  `check_question_slots` `:1475` не меняется). Для `QuestionAddSchema` (storage-вопросы)
  — новое явное правило `qid 0 → InvalidSchema`: QuestionId 0 в IFR
  означает «не-вопрос», `$SPF`-запись по нему не адресуема (сегодня
  отвергалось лишь случайно — коллизией, если в форме был свой qid-0
  слот).
- `$SPF`: без изменений кода — `apply_spf_ifr_fixup`/`plan_spf_append`
  уже вызываются с фактическим `insert_at`/`delta`. Новые тесты
  фиксируют порог: записи с `ifr_offset >= insert_at` (вопросы после
  якоря) сдвигаются, до якоря — нет.

### 4. Поверхности (RPC/CLI/TUI)

Протокол не меняется (`schema_json` — строка). CLI `hii question add` и
TUI `:hii question add TARGET#FORM FILE` получают поле автоматически.
Обновляется help-текст `:hii` (упоминание `insert_before`) — без
отдельной интерактивной TUI-механики.

## Границы (out of scope)

- `insert_after` — before покрывает все позиции (после X = перед
  следующим; конец = нет поля).
- Сырой offset-якорь — отклонён (хрупко, валиден для одной ревизии
  образа).
- Класс ConstantTrue (TRUE→FALSE флип, раскрытие Chipset) — TODO:3850.
- Валидация существования цели REF — TODO:3861 (диагностический формат
  листинга переиспользуется, операция отдельная).
- IFR-удаление, TUI-клавиша вставки, экспорт позиций в form-export
  JSON — не запрашивалось.

## Acceptance

1. Синтетика (uefi-engine): вставка перед каждым из трёх GOTO синтетической
   формы (`BeforeGoto`) даёт ожидаемый порядок стейтментов;
   `BeforeQuestion` — перед целевым вопросом; якорь GOTO внутри
   suppress_if-блока → вставка перед блоком, не внутрь (новый
   стейтмент на depth 0 формы); `End` — регрессия
   байт-в-байт; `insert_before` с обоими/ни одним ключом →
   `InvalidSchema`; несуществующий якорь → `InvalidSchema` c листингом
   доступных REF-целей/qid формы; qid 0 в refs проходит мимо
   коллизия-чека, qid 0 в questions → `InvalidSchema`.
2. Каналы: bare и resource дают одинаковую позицию (re-parse после
   вставки — новый стейтмент между якорем-соседями); `$SPF`-порог:
   записи после `insert_at` сдвинуты, до — нет.
3. Регрессия: существующие тесты question-add/ref/spf не падают
   (сигнатурная миграция на `InsertPos::End` механическая).
4. Real-гейт `#[ignore]` (`real_amibcp_450x_positional_insert`, по
   образцу `real_amibcp_450x_formset_unlock`): REF3 qid 0
   `insert_before {goto_form_id: 10008}` в форму 10000, билд, re-parse:
   порядок REF-стейтментов формы 10000 по offset =
   …, 10002, NEW(→IntelRCSetup#1), 10008, 10009, 10010, 10012, END;
   рост образа 0 байт (16 MiB сохранён — .rsrc-рост поглощён слотом
   FV, как в вердикте §7).
5. Владельческий live-гейт (сценарий ниже) — вердикт-аддендум в §Live.

## Live-гейт (владелец; сценарий подготовлен планом)

1. Чистая копия `refs/amibcp/450x — копия.bin`, write-режим.
2. `:hii question add 899407D7-…:0x10:0#10000 <refs-schema.json>` —
   schema из §1 (REF3 qid 0, formset EC87D643-…, форма 1, prompt
   «IntelRCSetup», `insert_before {goto_form_id: 10008}`).
3. `:image save` → прошивка → SOL: бар Setup содержит вкладку
   «IntelRCSetup» между «Advanced» и «Server Mgmt»; Enter открывает
   корень IntelRCSetup (дерево как в вердикте §7). F9 после флеша
   обязателен (AGENTS.md).
4. Развилки: вкладка не отрисовалась с якорем 10008 → повтор с
   `goto_form_id: 10009` (позиция TSE-вкладки «Server Mgmt» — живой
   эксперимент; обе позиции даёт один и тот же механизм). Qid-0 REF3
   вкладкой не принят → вердикт «топ-уровень бара недостижим через IFR
   на этом TSE», кейс закрывается лучшим из доступных (вложенный пункт
   Advanced уже работает, §7).

Вердикт: (заполняется после гейта владельца).
