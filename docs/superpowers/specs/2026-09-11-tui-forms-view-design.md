# TUI Forms View — umbrella-спека дуги V1–V3

**Date:** 2026-09-11
**Scope:** `uefi-tui`, `uefi-engine`, `uefi-proto`
**Related:** цикл A `2026-09-11-tui-revival-design.md` (строится поверх); TODO-пункт «TUI: отдельный
HII-view для показа/редактирования форм и строк»; HII-RPC-поверхность — фазы
`2026-08-14-hii-*-design.md` (ридеры/билдеры) и дуги hijack/setup-new-page.

Это **живой документ** (паттерн serial-ladder): ступени V1–V3 закрываются вердиктами, решения и
уроки фиксируются аддендумами сюда; план — отдельный документ на каждую ступень.

## 1. Цель дуги

Дать TUI полноэкранный Forms View для работы с HII-формами: просмотр (формсет/форма/вопрос/строки)
→ правки на месте (visibility, unlock, set-value) → операции добавления (schema-файлы). Решение
владельца (2026-09-11): конечная цель — полный редактор, но исполняем дугой малых ступеней
«просмотр → правки → добавление».

Дизайн-принцип (TODO:151, решение фазы PE-resource): HII-иерархия **не встраивается** в дерево
BIOS-регионов — дерево это физическая модель хранения (FV/файлы/секции), HII — логический слой
поверх пакетов. Каноничные инструменты (AMIBCP) показывают формы отдельным экраном.

## 2. Отправная точка (что есть на 2026-09-11)

### 2.1 RPC-поверхность HII (engine.proto, готово)

| RPC | Что даёт | Ступень |
|---|---|---|
| `HiiListForms` | `FormInfo{form_id(target), formset_guid, form_id_ifr, title, visible}` | V1 |
| `HiiListStrings` | `StringInfo{language, string_id, text}` — плоский список | V1 |
| `HiiSetFormVisibility` | видимость формы | V2 |
| `HiiGatesList` / `HiiUnlock` | гейты (suppress/grayout/ref) по item_id; авто-флипы | V2 |
| `HiiQuestionInfo` | `QuestionInfo` (kind, varstore, min/max/step, options, defaults) | V2 |
| `HiiSetValue` | значение вопроса + applied_flips + stores | V2 |
| `HiiFormSetAdd`/`HiiFormAdd`/`HiiQuestionAdd`/`HiiPageAdd`/`HiiFormHijack` | schema-JSON операции | V3 |

item_id-синтаксис: `<target>#<form_id>[:<question_id>]`, target — file-section-адрес вида
`GUID:0xNN:<idx>` (`hii/mod.rs:164` `parse_item_id`); `FormInfo.form_id` уже содержит target-часть.

### 2.2 Чего не хватает

- **Перечисления вопросов формы.** `parse_form_package` собирает только формсеты/формы (вопросы —
  «unknown ifr opcode skipped»); `HiiQuestionInfo` требует заранее знать question_id. Механика
  обхода вопросов есть в `values.rs` (`walk_statements`, `find_question`, `varstore_map`) — наружу
  не выставлена. → Единственная engine-добавка дуги, ступень V1.
- Самого View: в TUI нет ни вкладок, ни HII-команд (нулевое покрытие).

### 2.3 TUI-архитектура (после цикла A)

Mode {Normal, Command, Insert} × Focus {Tree, Details, Registry}; cmdline общий; mock-server в
`tests/mock_server.rs` (трейт `EngineService` — прототип RPC-поверхности, уже догнан до
`HiiPageAdd`).

## 3. Архитектура

### 3.1 View-вкладки

```rust
pub enum View { Image, Forms }
```

- `App.view: View`; `Tab`/`Shift-Tab` — переключение; `:forms`/`:image` — командой; hint
  контекстный (внизу показывает активную вкладку: `[Tab] image …`).
- Рендер и key-маршрутизация ветвятся по `app.view`: Image-view = текущий layout B (tree +
  details/registry, focus-ring из 3 панелей); Forms-view — собственный layout (ниже). Mode-машина
  (Normal/Command/Insert) и cmdline — общие: ex-команды доступны из обеих вкладок.
- Switch на Forms при отсутствии активного образа — status_msg «no active image» (списки HII
  привязаны к образу).

### 3.2 Forms-view layout

```
┌─ Forms ────────────────────┬─ Form: <title> ────────────┐
│ ▸ FormSet (Setup) 8AC6…    │ Form 10019 · visible       │
│   10001 Main          [V]  │ target GUID:0x02:0         │
│ ▸ 10002 Advanced           │ ── Questions ──            │
│   10019 Serial Port   [H]  │  q0x210  Serial Port  OneOf│
│ ▸ FormSet (Platform) 901…  │  q0x211  Baud Rate    OneOf│
│ …                          │ ── Gates ── (V2)           │
├────────────────────────────┴────────────────────────────┤
│ cmdline                                                   │
└─ [Tab] image · j/k · l expand · S strings · ? help ─────┘
```

- **Левая панель (list):** FormSet-узлы (развёрнуты по умолчанию) → формы; `[V]`/`[H]`-маркер
  видимости, form_id_ifr и title. Данные — один `HiiListForms` на вход/refresh, группировка по
  `formset_guid` клиент-side.
- **Правая панель (details):** выбранная форма — title/id/target/visible; список вопросов
  (V1 — summary-строки, V2 — детали по `HiiQuestionInfo`); гейты (V2).
- **Strings-браузер:** `S` внутри Forms-view заменяет правую панель (или обе — full-width) на
  список строк с `/`-фильтром по подстроке; `S`/`Esc` — назад. Скроллинг stateful, как tree.
- Focus-ring Forms-view: `[FormsList, FormDetails]` (+ strings-режим: `StringsList`), те же
  `Ctrl-h/j/k/l` по кольцу.

### 3.3 Данные и state

```rust
pub struct FormsData {
    pub forms: Vec<FormInfo>,        // HiiListForms
    pub expanded: HashSet<String>,   // formset_guid развёрнутость
    pub cursor: usize,               // по видимым строкам list-панели
    pub selected: Option<FormKey>,   // formset_guid + form_id_ifr + target
    pub questions: Vec<QuestionSummary>, // лениво: HiiListQuestions по selected
    pub show_strings: bool,
    pub strings: Vec<StringInfo>,
    pub strings_filter: String,
    pub strings_cursor: usize,
}
```

Refresh-точки: вход во view, `:image switch`, мутации V2/V3 (visibility/unlock/set-value/add).
Вопросы — ленивый fetch при смене `selected` (кэш на одну форму; переключение — re-fetch, без
инвалидации всего списка).

### 3.4 Engine-добавка: `HiiListQuestions`

```proto
rpc HiiListQuestions(HiiListQuestionsRequest) returns (HiiListQuestionsResponse);
message HiiListQuestionsRequest { string image_id = 1; string target = 2; uint32 form_id = 3; }
message QuestionSummary {
  uint32 question_id = 1;
  string kind = 2;        // one_of | check_box | numeric
  string prompt = 3;      // резолв StringId через string-пакеты (как титулы форм)
  uint32 var_store_id = 4;
  uint32 var_offset = 5;
  uint32 width = 6;
}
message HiiListQuestionsResponse { repeated QuestionSummary questions = 1; }
```

Walker — новый `hii/questions.rs::list_questions(pkg, form_id)`: обход в стиле `walk_statements`
(opcode-aligned, `len & 0x7F`, bounds по `package_bounds`); трекинг текущей формы (IFR_FORM_OP
push form_id / IFR_END_OP pop); внутри целевой формы — сбор IFR_ONE_OF_OP/IFR_CHECKBOX_OP/
IFR_NUMERIC_OP с question-header полей (question_id, prompt StringId, var-store id/offset/width —
раскладка по `r_efi::hii`-структурам, точные офсеты — в плане по значениям из `find_question`/
`values.rs`). Prompt-резолв — та же схема, что `forms.rs` использует для титулов (string-пакеты
обоих каналов: PE-resource + bare). Handler — по образцу `HiiQuestionInfo` (резолв target →
package → walker).

Почему отдельный RPC, а не расширение `FormInfo`: списки вопросов длинные (десятки на форму),
ленивость обязательна; консьюмеры `HiiListForms` (CLI TSV-вывод) не меняются; item_id-паттерн
уже per-action.

### 3.5 Engine-добавка V2: `HiiFormTree` (REF-дерево форм)

IFR хранит формы плоско — иерархии «форма в форме» нет. Вложенность реального Setup-меню
(Main → Advanced → Serial Port 1 Configuration) строится браузером из **REF-вопросов**: в
родительской форме стоит `IFR_REF_OP`, чей payload содержит `FormId` целевой формы. Все
REF-варианты (REF..REF5) — один опкод `0x0F` (в r-efi отдельных констант REF2..REF5 нет),
различаются длиной; `FormId u16 @ +13` при `length >= 15` — паттерн чтения уже реализован в
движке (`gates.rs`, ветка `Wraps::Ref`).

```proto
rpc HiiFormTree(HiiFormTreeRequest) returns (HiiFormTreeResponse);
message FormEdge {
  string formset_guid = 1;
  uint32 parent_form_id = 2;
  uint32 form_id = 3;      // цель REF-вопроса
}
message HiiFormTreeRequest  { string image_id = 1; }
message HiiFormTreeResponse { repeated FormEdge edges = 1; }
```

Драйвер — walker поверх того же `walk_statements` (quirk-маски/bounds — общий фундамент,
риск §5): в форме X каждый REF-опкод даёт ребро `X → FormId@+13`; рёбра дедуплицируются;
висячие цели (формы нет в `HiiListForms`) отдаются как есть — не молча отбрасываются.
Отдельный RPC (не поле в `FormInfo`) по той же причине, что и 3.4: TSV-вывод
`uefi-cli hii form list` не меняется.

TUI строит дерево клиент-side: корни — формы без входящих рёбер (первая форма формсета,
затем «сироты» в порядке появления); дети группируются по родителю; форма с несколькими
родителями показывается у каждого. Рендер — вложенные отступы, `h`/`l` сворачивают и формы
(не только формсеты); защита от циклов — visited при обходе. Details-панель показывает
полный путь (`Main → Advanced → Serial Port 1 Configuration`) — это дизамбигуирует
одинаковые титулы (две «Boot» в одном формсете, гейт-находка V1). Плоский режим (текущий)
остаётся переключателем — дерево не должно прятать «сирот».

## 4. Лестница V1–V3

### V1 — Просмотр (forms + questions + strings, read-only)

Состав: View-вкладки (3.1), Forms-view layout/list/details (3.2–3.3), `HiiListQuestions` +
walker (3.4), strings-браузер, `:forms`/`:image`.

**Гейт V1** (реальный образ HNX99TF):
- список формсетов/форм в TUI == `uefi-cli hii form list` (кол-во, титулы, visible-маркеры);
- вопросы известной формы (например, serial-целевой формы дуги E30+: q0x210/q0x211-класс) видны с
  kind/prompt, соответствуют `hii question info` по тем же id;
- strings-браузер показывает тот же набор, что `hii string list`, фильтр работает;
- переключение вкладок не ломает Image-view (регрессия текущих mock-тестов зелёная).

### V2 — Правки на месте

Состав:
- REF-дерево форм (3.5): список строится по рёбрам `HiiFormTree` — вложенные отступы, путь в
  details; плоский режим — переключателем;
- форма: `v` — toggle visibility (`HiiSetFormVisibility`), `u` — unlock (`HiiUnlock`); результат —
  applied_flips в status_msg + re-fetch форм (visible-маркер меняется);
- детали: блок гейтов из `HiiGatesList` (gate_kind/expression/flippable);
- вопрос: `Enter` — Insert-промпт `hii set-value <target>#<form>:<qid> ` (`HiiSetValue`); перед
  вводом details-панель показывает диапазон/options из `HiiQuestionInfo` (подсказка при вводе);
  applied_flips/stores — в status_msg. Здесь в TUI появляется существительное `:hii` (команда
  `:hii set-value <item_id> <value>`); V3 достраивает остальные глаголы семейства.

**Гейт V2:** unlock/set-value из TUI дают байт-в-байт тот же образ, что те же операции из CLI
(sha256 сравнение сохранённых образов); visible-маркер в TUI сходится с `hii form list` после
правки. REF-дерево: у «Serial Port 1 Configuration» виден родитель «Advanced» (путь в details);
две «Boot» различимы путём; сумма узлов дерева == `hii form list` по формсету (с учётом кратных
родителей); висячие REF-цели помечены, циклы не зацикливают рендер.

### V3 — Операции добавления (schema-файлы)

Состав — ex-команды с путём к JSON-схеме (паритет с CLI, без визардов — решение цикла
2026-08-12 остаётся в силе):

| Команда | RPC |
|---|---|
| `:hii formset add <file>` | `HiiFormSetAdd` |
| `:hii form add <target> <file>` | `HiiFormAdd` |
| `:hii question add <target>#<form> <file>` | `HiiQuestionAdd` |
| `:hii page add <target> <file>` | `HiiPageAdd` |
| `:hii hijack <target> <file> [setupdata-guid]` | `HiiFormHijack` |

Prefill: на форме клавиша `a` → Insert mode с `hii form add <target> `; на формсете — `hii formset
add `. Вывод (inserted ids, string_ids) — в status_msg + refresh. Табы A5 дополняют пути/таргеты.

**Гейт V3:** добавление вопроса/формы из TUI даёт образ, идентичный той же операции из CLI
(sha256); спека-схемы из fixtures дуги setup-new-page переиспользуются как тест-данные.

## 5. Риски

- **Walker на AMI-образах:** IFR-специфика уже выверена в `values.rs`/`gates.rs` (quirk-маски,
  bounds) — walker V1 обязан строиться на том же фундаменте, не новом парсере. Фикстурная политика
  (решение владельца 2026-09-11): не переживать и делать нужные фикстуры из живых образов —
  полные образы лежат локально в `refs/fw/` (не в git, real-image гейты `#[ignore]` по наличию);
  источник пополнения коллекции — https://github.com/Koshak1013/HuananzhiX99_BIOS_mods/
  (README-заметка про источник — опционально); юнит-фикстуры walker'а — извлечённые из живого
  образа малые IFR/string-блобы, коммитятся в тесты как данные.
- **Объём `HiiListForms` на живом образе** (сотни форм) — группировка/скроллинг заложены; если
  тормозит — фильтр формы `/` (заложена контекстная семантика: `/` = фильтр в Forms-view).
- **MutationBehindCompression** на setup-модулях: V2/V3-операции могут отказывать «за барьером» —
  это честный engine-контракт; TUI показывает ошибку как есть (ошибка ≠ баг TUI).
- **Proto-добавка** (`HiiListQuestions`) тянет regenerate + догон mock-серверов (прецедент
  `f970bf6`/`9933f59`: per-crate прогоны не компилируют чужие тесты — ловится только
  `cargo test --all`); план обязан включать full-workspace прогон.

## 6. Конвенции дуги

- Ступень закрывается планом → исполнением → вердиктом по гейту; вердикт и уроки — аддендум сюда
  (§7+, по одному на ступень), как в serial-ladder.
- Правило AGENTS.md №11 действует: расхождения плана с реальностью — отдельный docs-коммит до
  реализации.
- Каждая ступень заканчивается `cargo test --all` + `cargo clippy --all -- -D warnings` (урок
  `9933f59`).

## 7. Гейт V1 — сценарий для владельца (аддендум, 2026-09-11)

Engine-часть гейта автоматизирована (`real_image.rs::
hii_list_questions_real_image_consistent_with_question_info`, #[ignore]).
Ручная TUI-часть (запускается владельцем на живом движке и HNX99TF):

1. `cargo run -p uefi-engine` (сокет по умолчанию), в другом терминале
   `cargo run -p uefi-tui`;
2. `:open <путь к HNX99TF-образу> --mode write`;
3. `Tab` → Forms-view: формсеты развёрнуты, титулы форм видны (сверка с
   `uefi-cli hii form list` — количество/названия/visible-маркеры);
4. `j/k` до формы с вопросами → правая панель: вопросы с kind и prompt;
   сверка выборочного вопроса с `uefi-cli hii question info <item_id>`;
5. `S` → strings-браузер, `/` → фильтр по подстроке — счётчик строк
   сходится с `uefi-cli hii string list | grep -ci <подстрока>`;
6. `Tab` → возврат в Image-view без потери состояния дерева.

Вердикт (все пункты да/нет + скриншоты) — аддендумом сюда; при негативе —
бисект по задаче цикла. Успех = переход к плану V2.

### Вердикт владельца (2026-09-11)

Гейт пройден на живом образе HNX99TF (вкладка Forms, вопросы, strings,
фильтр). Общая оценка: «результат просто потрясающий». Курсорная подсветка
списков Forms/Strings добавлена по находке гейта (`bac5598`).

Замечания (не блокируют, внесены в backlog TODO.md):
1. Формы плоские: у подчинённых форм не видно родителя — «Serial Port 1
   Configuration» показывает только формсет, родитель «Advanced» не виден.
   IFR хранит формы плоско; вложенность реального меню строится из
   REF-вопросов (`IFR_REF_OP` 0x0F, FormId u16 @ +13; паттерн чтения —
   `gates.rs:201`). Решение — REF-дерево форм в V2 (TODO).
2. Одинаковые титулы в одном формсете (две «Boot» в 7B59104A-C00D) не
   дизамбигуированы. Частично закрыто строкой «Form ID» в деталях
   (`4320e54`); полное решение — путь по REF-дереву (V2).

V1 засчитана; переход к плану V2 (правки: visibility/unlock/set-value; REF-дерево форм
включено в V2-состав — дизайн §3.5, решение владельца 2026-09-11).

## 8. Гейт V2 — сценарий для владельца (аддендум, 2026-09-11)

Engine-часть автоматизирована (`real_image.rs::form_tree_real_image_edges_consistent`,
#[ignore]). Ручная TUI-часть (на живом движке и HNX99TF; байт-в-байт сравнения —
sha256sum):

1. `cargo run -p uefi-engine`, в другом терминале `cargo run -p uefi-tui`;
   `:open <HNX99TF> --mode write`; `Tab` → Forms-view.
2. REF-дерево: «Serial Port 1 Configuration» вложена в «Advanced» (отступ), в
   details — путь «… → Advanced → Serial Port 1 Configuration»; сверка parent
   с `uefi-cli hii form list`.
3. Две «Boot» в формсете 7B59104A-C00D различимы путём в details.
4. `T` — плоский режим (V1-вид) и обратно; сумма Form-строк дерева ≥
   `hii form list` по формсету (кратные родители учитываются дважды).
5. Висячие REF-цели помечены «! … (dangling REF target)»; навигация не
   зацикливается на циклических рёбрах.
6. `v` на видимой форме → маркер `[H]`, статус «visibility …: off»; сверка с
   `uefi-cli hii form list` после правки.
7. Unlock-паритет: `:snapshot`, `u` на форме, `:save /tmp/tui-unlock.bin`;
   `:restore`, из CLI `uefi-cli hii form unlock <item>` + сохранение образа;
   `sha256sum` обоих файлов совпадает.
8. Set-value-паритет: `:snapshot`, Details-focus → `j` до вопроса → `Enter`
   (prefill `hii set-value …`), ввести значение, `:save /tmp/tui-setval.bin`;
   `:restore`, CLI `uefi-cli hii question set-value <item> <value>` +
   сохранение; sha256 совпадает. Перед вводом details показывает диапазон/
   options (подсказка).

Вердикт (все пункты да/нет + скриншоты) — аддендумом сюда; при негативе —
бисект по задаче плана. Успех = переход к плану V3 (операции добавления).

## Решения (decisions log)

- **D1:** Полноэкранные вкладки `Tab`/`Shift-Tab` (Image ↔ Forms), не панель и не оверлей —
  решение владельца 2026-09-11 (соответствует «отдельному экрану» AMIBCP-канона из TODO:151).
- **D2:** Дуга V1→V2→V3 с гейтами на живом образе — решение владельца 2026-09-11 («полный
  редактор, но начать с малого»).
- **D3:** Единственная engine-добавка дуги — `HiiListQuestions` (ленивый, per-form); `FormInfo` не
  расширяется.
- **D4:** Strings-браузер — режим внутри Forms-view (`S`), не третья вкладка. Многоязычность НЕ
  закладываем (решение владельца 2026-09-11: локализованные BIOS — «ужас, ничего нельзя найти»,
  поддерживаем только Eng): строки показываются как отдаёт движок (language-колонка в строке),
  никакого language-switcher'а и per-language кэша; многоязычный образ = просто длиннее список,
  `/`-фильтр достаточно. Если когда-нибудь понадобится — вынесем вкладкой.
- **D5:** V3 — ex-команды со schema-файлами, без визардов (YAGNI; прецедент — non-goal цикла
  2026-08-12). Визарды — отдельное решение будущего цикла, если ревью V3 покажет боль.
- **D6:** item_id для TUI-операций конструируется из `FormInfo` (target) + `form_id_ifr` +
  question_id — без ручного ввода target'ов (для этого и нужен браузинг).
