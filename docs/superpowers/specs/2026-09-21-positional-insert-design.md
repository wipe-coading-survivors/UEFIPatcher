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

Живая геометрия (вердикт §7, образ `refs/amibcp/450x — копия.bin`;
уточнение 2026-09-22 по живому экспорту/байт-обходу): бар AMITSE
строится из GOTO-детей корневой формы 10000 (файл `899407D7-…`,
формсет `7B59104A-…` — FORM_SET GUID пакета; item-path
`899407D7-…:0x10:0#10000` валиден как файл-гайд) —
6 шт: 10001 Main, 10002 Advanced, 10008 Chipset (suppress_if TRUE,
TODO:3850), 10009 Security, 10010 Boot, 10012 Save&Exit; «Server Mgmt»
дорисовывается TSE извне (IPMI-приложение). **Их qid = 1..6, а не 0**
— «qid-0 GOTO-дети» из вердикта §7 было неверной читкой. Что остаётся
фактом: REF3 с ненулевым qid (32800) вкладкой не отрисовался —
поэтому наша вставка берёт qid 0 (гипотеза «qid 0 → вкладка»,
проверяется прошивкой). [Снято прошивкой 2026-09-22: qid-0 REF3
тоже не отрисован — см. аддендум «Живой гейт п.3»; новые кандидаты
— EDK2-маркеры vsid/QuestionId 0xFFFF, у нативных AMI-REF3 там
нули.] Пункт TODO «форма 1» неточен: корневая форма
бара — 10000 (живые байты, §7); форма 1 — цель IntelRCSetup-стороны.

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
  хранилища), дублирование легитимно (несколько вкладок в одном
  запросе; с образца «6 qid-0 GOTO» снято уточнением 2026-09-22 —
  барные GOTO формы 10000 несут qid 1..6). Уточнение от 2026-09-21
  (найдено при реализации Task 4):
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
   обязателен (AGENTS.md). **[Провалено 2026-09-22: артефакт
   e22bd0dd… прошит владельцем через TMM, на живой загрузке вкладки
   нет; движок/тулза подтверждают стейтмент в IFR (см. аддендум
   «Живой гейт п.3»).]**
4. Развилки: вкладка не отрисовалась с якорем 10008 → повтор с
   `goto_form_id: 10009` (позиция TSE-вкладки «Server Mgmt» — живой
   эксперимент; обе позиции даёт один и тот же механизм). Qid-0 REF3
   вкладкой не принят → вердикт «топ-уровень бара недостижим через IFR
   на этом TSE», кейс закрывается лучшим из доступных (вложенный пункт
   Advanced уже работает, §7). **[Взято 2026-09-22, с усилением:
   якорь 10009 объединён с мимикрией под нативный AMI-паттерн REF3
   (qid 301, vsid 0, QuestionId 0) — один флеш закрывает все живые
   гипотезы; детали в аддендуме «Живой гейт п.3».]**

### Инструментальная верификация артефакта (2026-09-22, до прошивки)

Артефакт `refs/amibcp/450x-positional-intelrcsetup.bin`, 16 777 216
байт, sha256 `e22bd0ddc2b04dac064a6fd1f7b4b4e84f1d8b3cf6791370a6c8ac91eeeb48c1`;
схема — `refs/amibcp/450x-intelrcsetup-tab.json` (refs/amibcp gitignored,
канонический JSON живёт в тесте `real_amibcp_450x_positional_insert`).

- CLI-путь (`engine` + `uefi-cli`: open write → question add → save):
  принят, qid 0, строки 1113 «Intel RC Setup» / 1114 «Intel RC Setup
  Configuration».
- TUI-путь владельца (`:open … write` → `:hii question add …` →
  `:save`, pty-драйв): сохранённый файл **байт-в-байт идентичен**
  CLI-артефакту (тот же sha256 — пересжатие LZMA детерминировано).
- Re-parse артефакта после полного рестарта движка: форма 10000
  `lossy: cross_formset_ref:1` (сток — 0); строки 1113/1114 резолвятся
  из string-pack ([en-US]); form-level гейтов у 10000 нет (suppress_if
  остался строго вокруг GOTO→10008 внутри формы).
- Независимый обход (парсер): бар = Main → Advanced → **NEW (форма 1,
  qid 0, depth 0)** → Chipset (в suppress, depth 1) → Security → Boot →
  Save&Exit; 311 файлов на месте.
- Попутный факт: стоковые барные GOTO несут qid 1..6 (экспорт +
  байт-обход) — см. уточнение в «Живой геометрии».

### Живой гейт п.3 — отрицательный результат и полевой дифф (2026-09-22)

Артефакт e22bd0dd… прошит владельцем (TMM) и загружен: вкладки
«Intel RC Setup» в баре НЕТ (визуально, живая загрузка). База
артефакта — `refs/amibcp/450x — копия.bin` (sha256 76ec3d3a…);
v-лейндж (v17..v22, параллельный поток §U) в артефакт НЕ входит.

Полевой дифф (байт-обход формы 10000 и родных REF3 формсета,
`tmp`-дамп парсером; сток vs артефакт vs родные):

| поле | барный GOTO (сток, рендерится) | наш REF3 (e22bd0dd, НЕ рендер) | родные REF3 (Main→#1793, Adv→#2/#4/#12, рендерятся) |
|---|---|---|---|
| opcode/len | 0x0F / 15 | 0x0F / 33 | 0x0F / 33 |
| qid | 1..6 | 0 | 7, 8, 0x0e, 0x0f, 0x10 (последовательные) |
| VarStoreId | 0x0000 | **0xFFFF** | **0x0000** |
| offset | 0xFFFF | 0xFFFF | 0xFFFF |
| flags | 0x00 | 0x00 | 0x00 |
| QuestionId@15 | — (нет поля) | **0xFFFF** | **0x0000** |
| FormSetGuid | — (нет поля) | EC87D643-… | EC87D643-… |

Вывод: наш `emit_ref3` писал EDK2-маркеры «invalid» (vsid 0xFFFF,
QuestionId 0xFFFF — паттерн `CIfrRef3`), тогда как AMI-генератор
этого образца везде пишет нули и последовательные ненулевые qid.
Ни одна собственная вставка с этими маркерами на живом TSE не
рендерилась (бар qid 32800 §7, бар qid 0 — этот гейт). Родные REF3
с нулями рендерятся (§7: «Processor Configuration» на живом BIOS).

**v2-эксперимент (один флеш, все живые гипотезы разом):**
`emit_ref3` → нативный паттерн (vsid 0, QuestionId 0), схема
`question_id: 301` (первый свободный qid формсета ≥7: бар 1..6,
вложенные REF 7..240, вопросы до 10093), `insert_before
{goto_form_id: 10009}` (вариант Б — после suppress-блока Chipset).
Если v2 не отрисуется — остаются только длина 15 vs 33 / наличие
GUID → вердикт «топ-уровень бара недостижим через IFR на этом
TSE», кейс закрывается лучшим из доступных.

v2-артефакт собран и верифицирован (2026-09-22, коммиты f931502
docs + 4c5d0e1 fix): `refs/amibcp/450x-positional-intelrcsetup-v2.bin`,
16 777 216 байт, sha256
`cca796fb9780b42d7af43f86d3b82fb4527282fe82a9e989d256523fdf958fca`;
схема — `450x-intelrcsetup-tab.json` (обновлена до v2), канонический
JSON — в тесте `real_amibcp_450x_positional_insert`. Байт-дамп бара:
REF3 @0x06ef len 33 depth 0, qid 301, vsid 0, voff 0xFFFF, flags 0,
FormId 1, QuestionId 0, GUID EC87D643-… — между suppress-блоком
Chipset и GOTO→10009; строки 1113/1114 на месте; рост образа 0 байт.
Отличия от стоковых вкладок остаются только структурные: len 33
(у барных GOTO 15) и наличие FormSetGuid.

### Как «Server Mgmt» попадает в бар (форензика 2026-09-22, write-цензус)

Вопрос владельца: форма 2049 формсета `01239999-FC0E-4B6E-9E79-
D54D5DB6CD20` — как она оказывается в корневом баре? Данные:

- В образе ровно **5 forms-пакетов** (полный write-обход всех PE
  .rsrc): формсеты `7B59104A` (18665 б — корневой бар, файл
  899407D7), `01239999` (4929 б — Server Mgmt, файл 9221315B,
  форма 2049 на месте), `80E1202E` (660 б), `EC87D643` (198785 б —
  IntelRCSetup), `932D37B0` (233 б).
- **Ни один REF (любой длины) во всех 5 пакетах не указывает на
  01239999/2049** — IFR-ребра в бар для Server Mgmt не существует.
- У **всех** пяти формсетов один и тот же единственный class GUID
  `93039971-8545-4B04-B45E-32EB8326040E` (flags 0x01) — класс не
  дискриминатор (IntelRCSetup с тем же классом вкладки не имеет).
- GUID 01239999 встречается в NVRAM-записях (магазин CEF5B9A3 —
  переменные формсета) и в несжатых секциях файла **B1DA0ADF**
  (498820 б и 297056 б; в 297-КБ секции — таблица GUID с шагом
  0x20, содержащая и 7B59104A, и 01239999).

Вывод: вкладку Server Mgmt добавляет **не IFR** — TSE программно
на рантайме (AMI-механизм хука/elink в SetupBrowser: формсет
публикуется драйвером, TSE добавляет root-page по своей логике).
Подтверждает легенду §7 «Server Mgmt дорисовывается извне».
Следствие: если v2 не отрисуется — **план C**: найти в TSE-модуле
(кандидат B1DA0ADF) таблицу/хук root-page и добавить EC87D643.

### Чистота базы v2 (2026-09-22)

`hack/uncore_trilock_patch.py --check`: и «450x — копия.bin», и
v2-артефакт — **NOT PATCHED** (тройка 0xe91d7c/0xe94c6c/0xe96460 =
стоковые 0x02) — базы без v-лейнджа. Дифф v2 ↔ база = ровно 2
региона: `0x8dcb70-0x8e7bd4` (45 157 б) и `0xafb9b0-0xb292a1`
(186 610 б) — два LZMA-пересжатых Setup-потока с вставкой; больше
в образе не изменён ни один байт.

Вердикт: (заполняется после гейта владельца).
