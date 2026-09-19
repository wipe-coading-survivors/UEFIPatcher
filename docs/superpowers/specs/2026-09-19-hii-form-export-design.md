# Спека: HII form-export (round-trip JSON) + «форма под формой» (конверт с метой)

Дата: 2026-09-19. Ветка: `hii-form-export`.
Первоисточник: TODO.md «uefi-tui: „форма под формой“ — UX ref-шага» (поз. 3358);
брейншторм владельца 2026-09-19 (циклы tui-cmdline-ux / tui-registry-polish /
forms-panel-three-zone закрыты ранее).

## Контекст и проблема

IFR плоский: `hii form add` вставляет форму в конец формсета, позиции «под
формой» в бинарном формате нет. Вложенность в Setup — это ref-вопрос (GOTO) в
**родительской** форме, ведущий на дочернюю. Сегодня вложенность = две ручные
операции: `hii form add <target> --file my.json` + авторинг `np_ref.json` +
`hii question add <target>#<parent> --file np_ref.json` (жалоба владельца
2026-09-12: одно-кнопочного флоу нет; `a` в TUI префиллит только первый шаг,
`commands.rs:1305`).

Обратное направление тоже отсутствует: содержимое живой формы нельзя вытащить
в JSON. Schema-формат для этого уже есть — `schema.rs` (`FormSetSchema` /
`FormSchema` / `ItemSchema`) — тот самый JSON, который ест `form add`
(`parse_schema`, `rpc/server.rs:875`). Экспорт в него даёт round-trip
«экспорт → правка → импорт» без нового формата.

Текущее состояние walker'а:

- summary-проход (`hii/questions.rs:30`): question_id, prompt_sid (+текст
  через `prompt_texts`), kind (OneOf/CheckBox/Numeric/Other), var_store_id,
  var_offset, width — на весь форм;
- детальный per-question (`HiiQuestionInfo`): min/max/step, options с
  текстами, defaults, varstore;
- ref-рёбра — `HiiFormTree` (`FormEdge`).

Не собирается: help_sid, display-флаги (IntDec/UintHex), Text/Action-итемы,
ref-опкоды внутри самой экспортируемой формы, formset-уровневые varstores
(открытая поз. TODO:490). Lossless-экспорт невозможен в принципе: suppress-if /
grayout-if / кросс-формсетные REF3/REF4 в schema не выражаются — фиксируем
границу честно (см. §2 `meta.lossy`).

## Решения владельца (брейншторм 2026-09-19)

- **Fidelity = round-trip** (вариант 2): полный `ItemSchema` с options /
  defaults / help / display / границами. Экспортированный файл — одновременно
  авторский шаблон и машиночитаемый объект для `form add`.
- **Конверт вместо поля** (владелец): никакой `ref_into`-правки schema — файл
  получает обёртку `{"meta": {...}, "formset": {...}}`. Мета — про
  оркестрацию клиента, schema — про IFR-семантику; не пересекаются ни в
  пространстве имён, ни в парсере.
- **Мета — Rust-only**: единственная реализация в `uefi-common`; потребители
  CLI и TUI сейчас, gateway — третий при WebUI-rework. Браузер мету не парсит
  никогда (TS-реализация запрещена — рассинхрон); браузер получает REST-глаголы
  и уже-оркестрированные результаты (паттерн `/api/v1/image/upload`).
- **Движок мету не знает вовсе**: RPC экспорта возвращает голые факты
  (`schema_json` + `formset_guid` + `parent_form_id`), конверт собирает клиент.
- **Автозаполнение `meta.ref_into`** из рёбер `HiiFormTree`: ровно один
  same-formset родитель → его form_id в мету; иначе None.
- **Довески**: клавиша `e` на строке формы → prefill `:hii form export`;
  клавиша `A` (Shift+a) → prefill второго шага `hii question add <t>#<f> `
  (стал осмысленным после cmdline-цикла — completion-меню рендерится).
- **`ref_into` полиморфный** (ревью спеки, владелец): число-«просто вложить»
  или объект с авторскими override'ами (prompt/help/question_id GOTO-строки,
  кейс живого `np_ref.json` с разными prompt/help). Плейсхолдерных id
  ещё-не-существующей формы в файле нет — id приходит только из ответа RPC.

## §1 Конверт: `uefi-common::envelope` (новый модуль)

Формат файла:

```json
{
  "meta": {
    "ref_into": 10001,
    "source": { "formset_guid": "EC87D643-..." },
    "lossy": ["suppress_if:2"]
  },
  "formset": { "...": "валидная FormSetSchema (schema.rs)" }
}
```

- `meta` — типизированная структура (не free-form): `ref_into:
  Option<RefInto>`, `source: Option<SourceMeta { formset_guid: String }>`,
  `lossy: Vec<String>`. Неизвестные ключи меты игнорируются (forward-compat).
- `RefInto` полиморфный (serde untagged): bare-число — id родителя, «просто
  вложить»; либо объект с override'ами авторской GOTO-строки:

```json
"ref_into": {
  "form_id": 10001,
  "prompt": "UEFIPatcher Setup",
  "help": "UEFIPatcher serial console settings",
  "question_id": 528
}
```

  Объектная форма требует `form_id` (serde); `prompt`/`help`/`question_id`
  опциональны — недостающее планировщик достраивает дефолтами (§3).
- **Никаких магических чисел в файле**: id новой формы в конверте не
  встречается ни в каком виде (плейсхолдеры запрещены) — он становится
  известен только из `inserted_form_ids[0]` ответа RPC. Единственное
  «магическое» число системы — базис высоких question-id `0x7F00` (конвенция
  до закрытия TODO:490) — и то не вшито в файл, а вычисляется планировщиком.
- Тело — ключ `formset`, значение — валидная `FormSetSchema`: `hii_form_add`
  парсит `schema_json` через `parse_schema` → `FormSetSchema`
  (`rpc/server.rs:875`), значит re-сериализованное тело клиент шлёт в RPC
  как есть. Коррекция брейнсторма («form»): тело называется и типизируется
  по реальному контракту потребителя.
- `split_envelope(text) -> (Meta, String)`: снифф ключа `meta`; файл без
  `meta` = bare — тело возвращается как есть (re-serialize только при
  наличии конверта), мета пустая. Bare-файлы (нынешние schema, `np_ref.json`,
  тестовые data) работают байт-в-байт как сегодня — нулевая регрессия.
- `wrap_export(bare_json, formset_guid, parent_form_id, lossy) -> String` —
  сборка конверта на экспорте.
- Обоснование конверта против поля-в-схеме (помимо решений владельца):
  парсеры движка асимметричны по строгости — `parse_question_add_schema`
  `deny_unknown_fields` (`schema.rs:264-266`), `FormSetSchema` — нет;
  единое поле «где-то работает, где-то нет». Конверт даёт одну семантику для
  всех путей и исключает коллизию имён при гипотетическом варианте C
  (движковый `ref_into`).

## §2 Экспорт: движок

**Walker-расширения** (`hii/questions.rs` / `values.rs`, один проход
`walk_statements`):

- `help_sid` из IFR-question-header (лежит рядом с prompt, `+4/+5`) + резолв
  текста через существующий `prompt_texts`-канал;
- display-флаги Numeric/OneOf → `DisplayMode`;
- Text/Action-итемы (не-вопросные опкоды) → `ItemSchema::Text`/`Action`;
- ref-опкоды (GOTO) внутри формы → `ItemSchema::Ref`;
- defaults → `DefaultClass` (default_id → optimized/failsafe).

**Новый RPC** (proto, рядом с `HiiFormAdd`, ~`engine.proto:219`):

```proto
rpc HiiFormExport(HiiFormExportRequest) returns (HiiFormExportResponse);
message HiiFormExportRequest  { string image_id = 1; string item_id = 2; }
message HiiFormExportResponse {
  string schema_json = 1;     // голое тело: FormSetSchema-минимум, БЕЗ meta
  string formset_guid = 2;    // происхождение (для meta.source)
  uint32 parent_form_id = 3;  // 0 = нет; ровно один same-formset родитель
  repeated string lossy = 4;  // счётчики непокрытых опкодов ("suppress_if:2")
}
```

- Тело-минимум: `FormSetSchema { formset_guid, title (из walker'а), help: "",
  varstores: [], default_stores: [], forms: [экспортируемая] }` — валиден для
  `parse_schema`; формсет-поля не семантичны для `form add` (дискриминатор
  `#N` в таргете — ординал формсет-пакета, TODO:499; тело с одной формой
  вставляет её одну).
- `parent_form_id`: из рёбер `HiiFormTree` — `Some` только при ровно одном
  same-formset родителе; кросс-формсетные (`target_formset_guid` непуст) и
  многородительские → 0.
- `lossy`-подсчёт: walker считает непокрытые statement-опкоды по видам;
  кросс-формсетный GOTO внутри формы **пропускается** (GOTO не туда в том же
  формсете хуже отсутствия) с записью `cross_formset_ref`.

## §3 Импорт с вложенностью: планировщик + клиенты

Общий слой — `uefi-common::envelope` (чистые функции, без RPC):

- `plan_ref_step(meta, body, form_add_outcome, busy_qids)
  -> Option<RefStepPlan>`: при `meta.ref_into = Some(...)` и непустом
  `inserted_form_ids` строит план — refs-список формата
  `parse_question_add_schema`:
  `{form_id: inserted_form_ids[0], prompt, help, question_id}`. Поля
  достраиваются каскадом: override из объектной `ref_into` → дефолт.
  Дефолты: `prompt` = `help` = title вставляемой формы (envelope-модуль
  достаёт его из тела мягким reach-in `forms[0].title` по тому же
  `serde_json::Value`, что и для re-serialize; жёсткой зависимости
  common→engine-schema нет; title недоступен → строка `Form <id>`);
  `question_id` = `max(0x7F00, max(busy_qids)+1)`. Высокие id — конвенция
  до закрытия TODO:490; `busy_qids` — занятые вопрос-id родителя
  (`hii_list_questions` доступен обоим клиентам: TUI — состояние Forms-view,
  CLI — RPC-обёртка; `check_ref_slots` в движке — вторая линия обороны).
  Пустые `inserted_form_ids` при заданном `ref_into`
  → ошибка «форма не вставлена, ref пропущен».

Порядок исполнения (CLI и TUI, каждый своим RPC-клиентом; gateway позже —
третий исполнитель):

1. **Pre-check до мутации**: `meta.ref_into` сверяется со списком форм
   целевого формсета (`HiiListForms`; TUI — состояние, CLI — `hii form list`).
   Родителя
   нет → fail fast, ноль мутаций.
2. `split_envelope` → bare-тело → `HiiFormAdd` как сегодня.
3. При плане: `HiiQuestionAdd` на родителя с синтезированным refs-списком.
4. Отчёт двухфазный и честный: «форма вставлена (id X); ref построен» либо
   «форма вставлена (id X), ref не построен: <причина>». Автоотката нет
   (движок атомарность двух RPC не умеет — вариант C вне цикла).
5. Предупреждение при импорте в чужой формсет: целевой formset_guid ≠
   `meta.source.formset_guid` → warn (varstore-id могут разойтись, зона
   TODO:490; полный ремап — после его закрытия).

## §4 Клиентские поверхности

- **CLI**: `hii form export <item_id> [--out FILE]` — stdout по умолчанию,
  при `--out` absolutization-конвенция (образец — `resolve_output_path`,
  `uefi-cli/src/commands/artifact.rs`). `hii form add --file x.json` прозрачно
  получает конверт-обработку (§3).
- **TUI**: `:hii form export <item_id> [--out FILE]`; клавиша `e` на строке
  формы → prefill (в духе `add_prefill`); клавиша `A` на строке родительской
  формы → prefill `hii question add <target>#<form> ` (второй шаг руками,
  довесок к конверт-флоу). `:hii form add ... --file envelope.json` —
  оркестрация §3, статус-сообщение двухфазное.
- **Completion**: item_id-кандидаты для `export` — переиспользовать
  существующую грамматику `hii form add`.

## §5 Ошибки и граничные случаи

- Bare-файл → поведение байт-в-байт как сегодня (нулевая регрессия).
- Неизвестные ключи меты → игнор. Объектная `ref_into` без `form_id` →
  invalid meta (serde), файл целиком отвергается до RPC.
- `ref_into` на несуществующего родителя → pre-check §3.1, fail fast.
- Кросс-формсетный GOTO в экспортируемой форме → skip + `meta.lossy`.
- `meta.lossy` непуст → файл честно помечен; round-tripвер знает, что
  suppress-if и пр. не перенеслись.
- Varstore-мисматч при реимпорте в чужой формсет → warn §3.5 (не блок).
- Движок ни при каком пути не читает и не пишет мету: на экспорте отдаёт
  голые факты, на импорте получает bare-тело.

## §6 Тестирование и гейты

- **Движок, синтетика (unit)**: по фикстуре на каждое расширение walker'а
  (help_sid, display, Text/Action, ref-опкоды, defaults-маппинг). Главный
  инвариант — **симметрия с билдером**: `ifr_builder` (schema→IFR) уже есть;
  тест «schema → build → export → семантически равно исходной schema» ловит
  дырки walker'а автоматически.
- **uefi-common (unit)**: `split_envelope` — bare-проход насквозь, разбор
  конверта, unknown-ключи; `plan_ref_step` — план/пустые id/конфликт qid/
  override объектной формы против дефолтов.
- **Клиенты (integration, mock-server)**: журнал вызовов mock'а ассертит
  последовательность `list_forms → form_add → question_add` на конверт-файле;
  паритет CLI/TUI — равенство RPC-журналов на одном файле (сильнее и дешевле
  sha256-сверки). TUI-юниты на префиллы `e`/`A`.
- **Real-image гейт (engine, автоматизирован на HNX99TF)**: экспорт известной
  формы → ассерты на конкретику (число вопросов, тексты опций, ожидаемый
  `meta.lossy`); round-trip: экспорт → `form add` назад → реэкспорт новой
  формы → семантическое равенство (id форм/вопросов различаются).
- **Живой гейт владельца** (вердикт-аддендум дописывается в эту спеку после
  прогона, паттерн tui-forms V1–V3): в TUI на живом образе — `e` на форме →
  файл на диск → правка руки → импорт с `ref_into` → форма видна под
  родителем.

## Скоуп-границы (сознательно вне цикла)

- Formset-уровневый экспорт (`FormSetSchema` целиком со всеми формами и
  varstores) — упирается в TODO:490 (walker не собирает formset-varstores).
- Varstore-ремап id при импорте в чужой формсет.
- `unsuppress` и прочие будущие директивы меты — дом готов (`Meta`
  расширяется), мебель завозим по use case.
- Gateway/WebUI-обвязка (`POST /api/v1/hii/form/add` с multipart и
  конверт-оркестрацией на стороне gateway) — цикл Gateway + WebUI rework.
- Вариант C (атомарный движковый `ref_into`, одна RPC) — отдельная дуга,
  если двухфазность начнёт болеть.
- Миграция `question add` / `formset add` на конверт — по мере надобности;
  в этом цикле конверт потребляет только `form add`.
