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
ref-опкоды внутри самой экспортируемой формы. Formset-уровневые varstores,
напротив, уже покрыты картой цикла varstore-contract (`varstore_map` + RPC
`HiiListVarstores`). Lossless-экспорт невозможен в принципе: suppress-if /
grayout-if / кросс-формсетные REF3/REF4 в schema не выражаются — фиксируем
границу честно (см. §2 `meta.lossy`).

## Решения владельца (брейншторм 2026-09-19)

- **Fidelity = round-trip** (вариант 2): полный `ItemSchema` с options /
  defaults / help / display / границами. Экспортированный файл — одновременно
  авторский шаблон и машиночитаемый объект для `form add`.
- **Конверт вместо поля** (владелец): никакой вложенной правки schema — файл
  получает обёртку `{"meta", "formset", "refs"}`. Мета — про происхождение,
  schema — про IFR-семантику, refs — про вложение; не пересекаются ни в
  пространстве имён, ни в парсере.
- **Мета — Rust-only**: единственная реализация в `uefi-common`; потребители
  CLI и TUI сейчас, gateway — третий при WebUI-rework. Браузер мету не парсит
  никогда (TS-реализация запрещена — рассинхрон); браузер получает REST-глаголы
  и уже-оркестрированные результаты (паттерн `/api/v1/image/upload`).
- **Движок мету не знает вовсе**: RPC экспорта возвращает голые факты
  (`schema_json` + `formset_guid` + `parent_form_id`), конверт собирает клиент.
- **Автозаполнение refs-секции** из рёбер `HiiFormTree`: ровно один
  same-formset родитель → `refs.parent_form_id` (+ prompt/help его GOTO в
  `entries`); иначе секция не пишется.
- **Довески**: клавиша `e` на строке формы → prefill `:hii form export`;
  клавиша `A` (Shift+a) → prefill второго шага `hii question add <t>#<f> `
  (стал осмысленным после cmdline-цикла — completion-меню рендерится).
- **Refs — отдельная секция конверта** (ревью спеки, два круга: полиморфный
  `ref_into` отклонён владельцем как замаскированное магическое число):
  форма (с вопросами) и np_ref-сущность перпендикулярны — не смешиваются;
  мета — чистая provenance-мета (source/lossy). Плейсхолдерных id
  ещё-не-существующей формы в файле нет — id приходит только из ответа RPC.
- **`hii import` — макро над низким уровнем** (ревью-3, владелец):
  низкоуровневые `hii form add` (`'a'`) и `hii question add` (`'A'`) не
  ломаются; конверт-файлы (с ключом `meta`) маршрутизируются только в новый
  вход `hii import` (TUI — `'I'`), агрегирующий оба действия. Движок видит
  те же два низкоуровневых RPC.
- **Refs-only пакет = реф, не «move»** (ревью-3, владелец): тело `formset`
  опционально; пакет из одних refs вставляет только ссылку на существующую
  форму (возможно кросс-формсетную — REF3, движок умеет `emit_ref3`,
  `hii/mod.rs:1740`), кейс IntelRCSetup/rd450x. Клавиша `'R'`. Термин
  «move» в TODO — историческая неточность, правится при закрытии цикла.
- **Varstore-рулинг** (ревью-3 владелец; ревью-4 — контракт
  varstore-contract §6): `'R'`-путь varstore-free по построению; движковые
  валидации per-item id + дубли деклараций до мутаций уже в мастере
  (varstore-contract §2); экспорт заполняет `formset.varstores`
  (referenced-only, name-value → `meta.lossy`); импорт-планнер — drop
  идентичных деклараций / fail fast отличающихся. Блок-пометка «TODO:490»
  снята (ревью-4).
- **Дефолт сокета движка** — вне цикла и исчерпан: закрыт соседним циклом
  varstore-contract (`aa3fe19`, XDG-state `default_sock`).

## §1 Конверт: `uefi-common::envelope` (новый модуль)

Формат файла:

```json
{
  "meta": { "source": { "formset_guid": "EC87D643-..." }, "lossy": ["suppress_if:2"] },
  "formset": { "...": "валидная FormSetSchema (schema.rs)" },
  "refs": {
    "parent_form_id": 10001,
    "entries": [
      { "prompt": "UEFIPatcher Setup", "help": "UEFIPatcher serial console settings" }
    ]
  }
}
```

- `meta` — типизированная структура (не free-form), чистая
  provenance-мета экспорта: `source: Option<SourceMeta { formset_guid:
  String }>`, `lossy: Vec<String>`. На импорте читается только для
  предупреждения о чужом формсете (§3.5). Неизвестные ключи меты
  игнорируются (forward-compat).
- `refs` — опциональная секция-директива импорта: ссылки (GOTO) в родителя.
  `{ parent_form_id: u16 (обязателен при наличии секции),
  entries: Vec<RefEntry> (опционален, может быть пуст) }`;
  `RefEntry { form_id: Option<u16>, formset_guid: Option<String>,
  prompt: Option<String>, help: Option<String>,
  question_id: Option<u16> }`. Явный `form_id` (+ опциональный
  `formset_guid` → кросс-формсетный REF3) — ссылка на **существующую**
  форму; отсутствие `form_id` — цель «форма, вставляемая этим пакетом»
  (требует `formset`-тела; это семантика отсутствия поля, не плейсхолдер).
  Авторские prompt/help — по образцу живого `np_ref.json` (разные
  prompt/help); недостающее планировщик достраивает дефолтами (§3).
  `np_ref.json` — wire-форма резолвнутых записей: после подстановки
  `form_id` планировщиком авторские entries ей тождественны. Форма (с
  вопросами) и refs — перпендикулярные сущности и живут отдельными
  секциями: форма вставляется в конец формсета, ref — в родителя;
  соединяются только планом исполнения.
- Тело `formset` опционально: **refs-only пакет** (`formset` отсутствует)
  вставляет только ссылки на существующие формы — все entries обязаны иметь
  явный `form_id`, иначе invalid package (отвергается до RPC).
- `split_envelope(text) -> Envelope { meta, refs, body }`: снифф ключа
  `meta`; файл без `meta` = bare — тело возвращается как есть
  (re-serialize только при наличии конверта), мета/refs пустые. Bare-файлы
  (нынешние schema, `np_ref.json`, тестовые data) работают байт-в-байт как
  сегодня — нулевая регрессия.
- `wrap_export(body, source, refs, lossy) -> String` — сборка конверта на
  экспорте (чем заполняется `refs` — §2).
- **Никаких магических чисел в файле**: id новой формы не встречается в
  конверте ни в каком виде — ни плейсхолдером, ни «полиморфной» формой
  (вариант отклонён на ревью) — он становится известен только из
  `inserted_form_ids[0]` ответа RPC и вшивается планировщиком. Единственное
  «магическое» число системы — базис высоких question-id `0x7F00` (конвенция:
  TODO:490 закрыт циклом varstore-contract, кандидат на пересмотр после
  `hii varstore list`, сам дефолт не меняется — обратная совместимость
  пакетов) — и то не вшито в файл, а вычисляется планировщиком.
- Тело — ключ `formset`, значение — валидная `FormSetSchema`: `hii_form_add`
  парсит `schema_json` через `parse_schema` → `FormSetSchema`
  (`rpc/server.rs:875`), значит re-сериализованное тело клиент шлёт в RPC
  как есть. Коррекция брейнсторма («form»): тело называется и типизируется
  по реальному контракту потребителя.
- Обоснование конверта против поля-в-схеме (помимо решений владельца):
  парсеры движка асимметричны по строгости — `parse_question_add_schema`
  `deny_unknown_fields` (`schema.rs:264-266`), `FormSetSchema` — нет;
  единое поле «где-то работает, где-то нет». Конверт даёт одну семантику для
  всех путей и исключает коллизию имён при гипотетическом варианте C
  (движковая директива вложенности).

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

- Тело: `FormSetSchema { formset_guid, title (из walker'а), help: "",
  varstores: <referenced-декларации>, default_stores: [], forms:
  [экспортируемая] }` — валиден для `parse_schema`. Из формсет-полей
  семантичны для `form add` только `varstores` (emit + валидации
  varstore-contract §2); `help`/`default_stores` не семантичны.
  Дискриминатор `#N` в таргете — ординал формсет-пакета (открытая поз. про
  formset-ordinal); тело с одной формой вставляет её одну.
- **Varstore-заполнение экспорта** (контракт varstore-contract §6): в
  `formset.varstores` кладутся **только referenced** (`var_store_id != 0`
  items формы) декларации VARSTORE/VARSTORE_EFI из карты
  формсета-источника (`varstore_map`); форма без storage-биндингов →
  пусто. Name-value стор в `VarStoreSchema` не выражается (типы Buffer/Efi)
  → items, ссылающиеся на name-value id, идут без декларации +
  `meta.lossy`-строка `varstore {id:#x} is name-value, not exportable`.
- `parent_form_id`: из рёбер `HiiFormTree` — `Some` только при ровно одном
  same-formset родителе; кросс-формсетные (`target_formset_guid` непуст) и
  многородительские → 0. На сборке конверта ложится в `refs.parent_form_id`;
  `refs.entries` экспорт заполняет из существующего GOTO родителя
  (prompt/help резолвятся walker'ом; `question_id` сознательно не пишется —
  коллизия при реимпорте в тот же образ, выбор оставлен планировщику).
- `lossy`-подсчёт: walker считает непокрытые statement-опкоды по видам;
  кросс-формсетный GOTO внутри формы **пропускается** (GOTO не туда в том же
  формсете хуже отсутствия) с записью `cross_formset_ref`.

## §3 Импорт (`hii import`): планировщик + клиенты

Общий слой — `uefi-common::envelope` (чистые функции, без RPC):

- `plan_ref_step(refs, body, form_add_outcome, busy_qids)
  -> Option<RefStepPlan>`: при наличии `refs`-секции строит план —
  refs-список формата `parse_question_add_schema`, по одной записи на
  entry. Entry с явным `form_id` переносится как есть (+ `formset_guid`
  → REF3); entry без `form_id` требует непустого `inserted_form_ids`
  (пустой → ошибка «форма не вставлена, ref пропущен») и берёт
  `form_id = inserted_form_ids[0]`. Пустой `entries` → одна
  синтезированная запись на вставляемую форму. Поля достраиваются
  каскадом: авторское значение из entry → дефолт. Дефолты: `prompt` =
  `help` = title формы-цели (для вставляемой — envelope-модуль достаёт
  его из тела мягким reach-in `forms[0].title` по тому же
  `serde_json::Value`, что и для re-serialize; для явного таргета —
  заголовок целевой формы из состояния клиента, если известен; title
  недоступен → строка `Form <id>`); `question_id` =
  `max(0x7F00, max(busy_qids)+1)`. Высокие id — конвенция (TODO:490 закрыт
  циклом varstore-contract): кандидат на пересмотр после `hii varstore
  list`, сам `max(0x7F00, …)+1`-дефолт не меняется — обратная совместимость
  пакетов; `busy_qids` — занятые вопрос-id родителя
  (`hii_list_questions` доступен обоим клиентам: TUI — состояние Forms-view,
  CLI — RPC-обёртка; `check_ref_slots` в движке — вторая линия обороны).

Порядок исполнения (CLI и TUI, каждый своим RPC-клиентом; gateway позже —
третий исполнитель):

1. **Pre-check до мутации**: `refs.parent_form_id` сверяется со списком форм
   целевого формсета (`HiiListForms`; TUI — состояние, CLI — `hii form
   list`); авторские `question_id` из `entries` — со свободными id родителя
   (`busy_qids`); явные таргеты entries (включая кросс-формсетные) — со
   списком форм образа (анти-dangling: движок существование цели REF не
   валидирует). Любой промах → fail fast, ноль мутаций.
2. `split_envelope` → bare-тело (если есть) → `HiiFormAdd` как сегодня.
   Refs-only пакет шаг пропускает.
3. При плане: `HiiQuestionAdd` на родителя с резолвнутым refs-списком.
4. Отчёт двухфазный и честный: «форма вставлена (id X); ref построен» либо
   «форма вставлена (id X), ref не построен: <причина>». Автоотката нет
   (движок атомарность двух RPC не умеет — вариант C вне цикла).
5. Предупреждение при импорте в чужой формсет: целевой formset_guid ≠
   `meta.source.formset_guid` → warn (информативно; содержательные правила —
   §3.6).
6. **Varstore-контракт** (контракт varstore-contract §6, ревью-4): refs-only
   путь varstore-free по построению (REF-вопрос не имеет storage-биндинга —
   в `QuestionAddRefSchema` нет varstore-полей). Движковая гарантия уже в
   мастере: `add_form` валидирует per-item `var_store_id` против карты
   формсета и деклараций пакета + дубли деклараций **до мутаций**
   (varstore-contract §2) — работает для любого клиента. Импорт-планнер
   (`uefi-common`, рядом с `plan_ref_step`) по карте `HiiListVarstores`:
   id свободен в целевом формсете → декларация остаётся (движок emit'ит,
   `form_add.rs:104`, `build_varstores`); занят и определение идентично
   (guid+size+name) → drop из bare-тела (round-trip в тот же формсет без
   движкового отказа); занят и отличается → fail fast «varstore id {id:#x}
   already exists with different definition». Экспорт заполняет
   `formset.varstores` (§2).

## §4 Клиентские поверхности

- **CLI**: `hii form export <item_id> [--out FILE]` — stdout по умолчанию,
  при `--out` absolutization-конвенция (образец — `resolve_output_path`,
  `uefi-cli/src/commands/artifact.rs`). Новый вход `hii import <target>
  --file pkg.json` — макро §3 (конверт-агрегатор). `hii form add` /
  `hii question add` не меняются, но конверт-файл в `hii form add`
  отвергается ошибкой «package file: use hii import» — маршрутизация,
  низкий уровень остаётся чистым.
- **TUI**: `:hii form export <item_id> [--out FILE]`; `:hii import <target>
  --file pkg.json` + клавиша `I` → prefill; `e` на строке формы → prefill
  экспорта; `A` → prefill второго шага `hii question add <t>#<f> `;
  `R` на строке формы — вставка ссылки на форму под курсором в выбираемого
  родителя (цель известна из состояния: `form_id` + `formset_guid`; точная
  механика prefill vs стартовый refs-пакет — решается планом). Статус
  импорта двухфазный.
- **Completion**: item_id-кандидаты для `export` — переиспользовать
  существующую грамматику `hii form add`; для `import` — target-грамматика
  + путь к файлу.

## §5 Ошибки и граничные случаи

- Bare-файл → поведение байт-в-байт как сегодня (нулевая регрессия).
- Неизвестные ключи меты → игнор. Секция `refs` без `parent_form_id` →
  invalid envelope (serde), файл целиком отвергается до RPC.
- `refs.parent_form_id` на несуществующего родителя или авторский
  `question_id` из `entries` на занятый id → pre-check §3.1, fail fast.
- Refs-only пакет с entry без явного `form_id` → invalid package до RPC.
- Явный таргет entry (в т.ч. кросс-формсетный) не найден в образе →
  pre-check §3.1, fail fast (анти-dangling).
- Конверт-файл в `hii form add` → ошибка-маршрутизатор «use hii import».
- Кросс-формсетный GOTO в экспортируемой форме → skip + `meta.lossy`.
- `meta.lossy` непуст → файл честно помечен; round-tripвер знает, что
  suppress-if и пр. не перенеслись.
- Varstore: занятый id с идентичным определением → планнер drop'ает
  декларацию (§3.6); с отличающимся → fail fast; per-item ссылки — движковая
  валидация до мутаций (varstore-contract §2, вторая линия для любого
  клиента).
- Движок ни при каком пути не читает и не пишет мету: на экспорте отдаёт
  голые факты, на импорте получает bare-тело.

## §6 Тестирование и гейты

- **Движок, синтетика (unit)**: по фикстуре на каждое расширение walker'а
  (help_sid, display, Text/Action, ref-опкоды, defaults-маппинг). Главный
  инвариант — **симметрия с билдером**: `ifr_builder` (schema→IFR) уже есть;
  тест «schema → build → export → семантически равно исходной schema» ловит
  дырки walker'а автоматически. Varstore-заполнение: referenced-only набор +
  name-value → lossy-строка.
- **uefi-common (unit)**: `split_envelope` — bare-проход насквозь, разбор
  конверта, unknown-ключи; `plan_ref_step` — план/пустые id/дефолты против
  авторских entries/явный таргет/refs-only/конфликт qid; varstore-правила
  планнера — свободен/оставить, идентичен/drop, отличается/fail fast.
- **Клиенты (integration, mock-server)**: журнал вызовов mock'а ассертит
  последовательность `list_forms → form_add → question_add` на полном
  конверт-пакете; refs-only пакет — `list_forms → question_add` (без
  form_add); паритет CLI/TUI — равенство RPC-журналов на одном файле
  (сильнее и дешевле sha256-сверки). TUI-юниты на префиллы `e`/`A`/`I`/`R`.
  Refs-only с кросс-формсетной entry — mock'ается REF3-путь (emit_ref3).
- **Real-image гейт (engine, автоматизирован на HNX99TF)**: экспорт известной
  формы → ассерты на конкретику (число вопросов, тексты опций, ожидаемый
  `meta.lossy`); round-trip: экспорт → `form add` назад → реэкспорт новой
  формы → семантическое равенство (id форм/вопросов различаются).
- **Живой гейт владельца** (вердикт-аддендум дописывается в эту спеку после
  прогона, паттерн tui-forms V1–V3): в TUI на живом образе — `e` на форме →
  файл на диск → правка руки → `hii import` с refs-секцией → форма видна
  под родителем; плюс refs-only сценарий (`'R'`: ссылка на форму другого
  формсета — на HNX99TF IntelRCSetup открыт, кросс-формсетный REF3
  проверяем на живом).

### Вердикт живого гейта (2026-09-20, владелец) — ПРОЙДЕН

- `e` — экспорт формы серийного порта из патченого E30 (хуанан): выгрузка
  прошла, файл на диске.
- `I` — импорт экспортированного пакета в оригинальный хуанан-BIOS:
  префилл + `<Tab>` (таргет текущей формы) + `--<Tab>` (`--file`) + выбор
  файла; форма появилась в дереве ПОД формой, на которой стоял курсор
  (refs.parent_form_id приехал из конверта экспорта; id совпал — образы
  одной семьи).
- `R` — refs-only пакет, кросс-формсетный REF3 (IntelRCSetup rd450x →
  корневой Setup): `import: forms (none) · strings (none) · refs built
  under 10000: 0x7f00` — виден qid-базис планировщика. Entry требует
  ПОЛНЫЙ formset_guid: обрезанный («EC87D643-EBA4» из панели деталей)
  pre-check отвергает «refs target form N not found in formset …».
- `V` на вопросах (панель деталей) — карта варстора живая
  (id/GUID/size/имя).
- Паперкаты гейта: (1) обрезанный GUID в копируемых местах — закрыт в
  цикле (cf9c56c: дерево и панель деталей показывают полный GUID,
  `short_guid` удалён); (2) PgUp/PgDn в Forms View не работают на
  List-фокусе (руки PageUp/PageDown в `handle_normal_forms` есть только
  под Details, `forms_page_*` не существует вовсе; strings/varstores —
  тот же класс) — TODO, Image View работает (`cursor_page_*`).

## Скоуп-границы (сознательно вне цикла)

- Formset-уровневый экспорт (`FormSetSchema` целиком со всеми формами
  формсета) — отдельная тема (varstore-карта доступна, масштаб — все формы).
- Авто-подбор свободного varstore-id при конфликте определений (планнер
  fail fast'ит; ремап — будущее).
- `unsuppress` и прочие будущие директивы меты — дом готов (`Meta`
  расширяется), мебель завозим по use case.
- Gateway/WebUI-обвязка (`POST /api/v1/hii/form/add` с multipart и
  конверт-оркестрацией на стороне gateway) — цикл Gateway + WebUI rework.
- Вариант C (атомарная движковая директива вложенности, одна RPC) — отдельная дуга,
  если двухфазность начнёт болеть.
- Миграция `question add` / `formset add` на конверт — по мере надобности;
  в этом цикле конверт потребляет только `hii import` (внутри — те же
  низкоуровневые RPC).
