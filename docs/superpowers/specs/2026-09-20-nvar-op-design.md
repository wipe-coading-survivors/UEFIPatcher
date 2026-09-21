# Спека: nvar-op — NVAR-сторы: парсинг, правка переменных, экспозиция

Дата: 2026-09-20. Ветка реализации: `nvar-op` (спека и её ревью-правки —
docs-коммитами в master по паттерну проекта; код — только в ветке).
Первоисточники: TODO.md:1560–1564 (**Цикл реализации: NVAR-правки в
UEFIPatcher**), TODO.md:3247–3254 (**Метка «NVRAM store»** — закрывается
попутно), отчёт разведки
`docs/reports/2026-09-20-asr1-226d2il-setup-recon.md` (офсеты, вердикт
пути записи), брейншторм владельца 2026-09-20 (подход 1, трёхсекционная
панель, глиф-флоппи, seed-значения вопросов включены).

## Контекст и проблема

Владельческая цель: испечь дефолты SOL/Above-4G на ASRock E3C226D2I
(образы 226D2IL3.30/.50). Разведка 2026-09-20 закрыла семантику: HII на
226D2IL пуст, конфигурация живёт в AMI NVAR-сторе — FV@0x500000 → FFS
Raw `CEF5B9A3`@0x500048 → стор@0x500060 → запись `StdDefaults` →
вложенный NVAR-стор (14 записей) → `Setup` (GUID EC87D643…, 1217 байт
данных @0x500088). Офсеты: SOL = Setup[1] → образ 0x500089; 4G =
Setup[1141] → 0x5004FD; значения 00/01. Вердикт записи: ин-плейс правка
байт в raw-теле FFS + пересборка FFS-чексумм; размер не меняется,
компрессор не нужен (копия одна, raw).

Что уже есть:
- NVAR-ридер в зачатке — `hii/nvar.rs` (parse_entry/walk, поиск записи
  по имя+длина), заточен под механизм v5;
- `set_value` (`hii/mod.rs:834`) — полная механика правки: обход дерева
  (`collect_std_defaults_hits`, `hii/mod.rs:753`), согласованный флип
  всех копий, барьер компрессии, мутация in-place +
  `ops::mark_rebuild_to_root_by_path` (`ops.rs:137`). Но адресация —
  только через HII-вопрос (форма, qid → карта варстора), а на 226D2IL
  HII пуст → непригоден;
- GUID-резолва записей нет, а на 226D2IL во вложенном сторе **две**
  записи `Setup` (1217b EC87D643… и 204b 01239999…), различимые только
  GUID'ом;
- в дереве стор выглядит загадочным «Raw CEF5B9A3-…» (TODO:3247).

Геометрия формата (сверена с живым дампом 226D2IL, формула
`nvramparser.cpp:209`): GUID-стор растёт с конца блоба стора,
`guid_index k → байты [len−16·(k+1) .. len−16·k]`; gi=0 главной `Setup`
→ EC87D643 — совпадает с живой efivar. Референс формата:
`refs/UEFITool-ai-fork/common/ksy/ami_nvar.ksy`.

## Решения владельца (брейншторм 2026-09-20)

- **Подход 1**: обобщить ядро NVAR + операции `nvar list/set`; без
  синтетических узлов-записей в дереве (ломают инварианты путей и
  билдера); без минимального bake-only (не переиспользуется).
- **Экспозиция**: TUI — трёхсекционная панель деталей в Image View
  (1: мета-инфа как сейчас; 2: прокрутка переменных; 3: hex-значение
  выбранной переменной); CLI — `nvar list` / `nvar set`. Мета-инфу
  сохраняем.
- **Глиф стора**: флоппи `\u{F0C7}` (Nerd Font, как остальные иконки
  `theme.rs:23`), сигнал движка — флажок `is_nvar` в `Node`.
- **Seed-значения вопросов — включены** (read-only join HII↔NVAR):
  показываем, что посеет первый бут/Load Defaults; это НЕ живой рантайм
  и НЕ IFR-дефолт (E13/E14: посев читает StdDefaults, а не IFR-флаги) —
  не смешиваем, подписываем.
- **Forms View**: цветные глифы вопросов по типу (сейчас монохром) —
  цвет сам отделяет вопросы от заголовка/gates, строка-разделитель не
  нужна; значения тоже раскрашиваем.
- `set_value` не меняем; его 22/22 real-image гейта остаются зелёными.

## §1 Ядро NVAR: `crates/uefi-engine/src/nvar.rs`

Модуль выносится из `hii/` на верхний уровень (формат AMI NVRAM ортогонален
HII); `hii/mod.rs` переходит на `crate::nvar`. Обобщение существующего:

- `NvarRecord` += `next: Option<u32>` (байты 6..9 заголовка; 0xFFFFFF =
  нет ссылки), `guid: Option<Guid>` (резолв через GUID-стор), остальные
  поля как сейчас (`offset/size/attributes/guid_index/name/data_offset/
  data_len`).
- `resolve_guids(buf)` — резолв guid_index по формуле UEFITool с
  bounds-check: индекс за пределами хвоста → `guid = None` (не ошибка —
  показ без GUID, правка по имени остаётся рабочей).
- Рекурсия: запись, чьи данные начинаются с валидного NVAR-заголовка и
  прогуливаются walk'ом ≥1 записи → вложенный стор; глубина ≤4 (на
  корпусе ровно один уровень: `StdDefaults` → стор).
- `find_var(name, guid)` — обобщённый поиск записи; неоднозначность
  имени без GUID → `Err` с перечнем кандидатов (имя, GUID, размер).
- Экстендед-хедер записи (attr-бит 0x10, auth-data по ksy
  `nvar_extended_attributes`) — парсим геометрию (хвост записи), правку
  таких записей **отказываем**; в дефолт-сторах корпуса их нет.
- `find_varstore_record` (имя+длина, контракт varstore-contract §8)
  сохраняется как есть для `set_value` — обёртка поверх ядра, семантика
  не меняется.

## §2 Метка и глиф в дереве

- `node_name` (`parser/image.rs:336`): File/Section-лист, чьё тело
  начинается с валидной NVAR-записи (проб первого заголовка — дёшево) →
  `«NVRAM store»`. Ловит оба контейнера: raw-файл `CEF5B9A3` и
  RAW-секцию внутри LZMA (HNX FV2). GUID-эвристику не используем —
  body-проб надёжнее. Закрывает TODO:3247.
- `Node` (proto) += `bool is_nvar` — ставится тем же пробом при
  построении Node в list/search; TUI `TreeNode` зеркалит.
- `theme.rs::type_icon` — при `is_nvar` → флоппи `\u{F0C7}` (цвет
  наследует тип узла: File/Section), тест на различимость с другими
  иконками.

## §3 Операции движка: `nvar_list` / `nvar_set`

Логика в `nvar.rs`, RPC-обвязка в `rpc/` по образцу hii.

**`nvar_list(path?)`** — без таргета все NVAR-сторы образа (File/Section-лист
с NVAR-телом; спуск через развёрнутое содержимое, LZMA-копии включаются;
сторы за non-recompressable секциями пропускаются — запечённые слепки,
см. барьер в `nvar_set`); с таргетом — один стор. Выдача на стор: путь,
описание (file/section), записи → {имя, GUID, **абсолютный офсет данных
в образе** (=node.offset+header+body_off+data_offset), data_len,
атрибуты}; записи без имени (data-only) — строка `(data-only)` без GUID;
сводка: N записей, свободный хвост, размер GUID-стора.

**`nvar_set(name, guid?, offset, value, width)`** — правка данных
переменной:
- `width` 1|2|4|8 (default 1; набор ширин IFR, как у `set_value`),
  little-endian; проверка `offset+width ≤ data_len`, value влезает в
  width;
- адресация: имя; GUID опционален — **обязателен при неоднозначности**
  имени в сторе (две `Setup`) — иначе отказ с перечнем кандидатов;
- применяется **во все копии** во всех сторах образа, где найдена
  переменная (философия согласованного флипа v5); no-op копии
  пропускаются с отчётом; ни одной находки → отказ;
- барьер компрессии — семантика v5: запечённые копии за
  non-recompressable секциями (226D2IL/C275: StdDefaults за Tiano —
  слепок дефолтов, живая копия правится) пропускаются и листингом, и
  правкой; отказ `MutationBehindCompression` — только если переменная
  найдена лишь за барьером (живой копии нет); recompressable-LZMA
  (HNX FV2) — живая копия, flip всех;
- мутация: те же in-place байты + `mark_rebuild_to_root_by_path`;
  LZMA-копии — decompress→flip→repack→slot-fit тем же механизмом, что у
  `set_value`;
- результат: per-store applied-строки `… store+0xNN: aa -> bb` (формат
  как у ValueOutcome).

## §4 Join: seed-значения вопросов (read-only)

`question_info` / `list_questions` обогащаются полями
`seed_value: Option<u64>` + `seed_option: Option<String>`:
- источник — байты той же записи, которую адресует `set_value`
  (`find_varstore_record` по имя+размер варстора HII-карты): чтение
  `[var_offset .. +width]` LE → u64;
- `seed_option` — метка OneOf-опции с этим значением (numeric — число,
  checkbox — Enabled/Disabled); 
- записи нет (стора в образе нет) → поля отсутствуют, UI показывает «—»;
  это не ошибка;
- IFR-дефолт (v1-карта) показываем рядом отдельной строкой, не
  смешивая: `Value (NVAR seed): 1 (Enabled)` vs `IFR default: …`.
- расхождение seed≠IFR-дефолт — маркер-акцент (см. §7), движок отдаёт
  оба значения, сравнение на клиенте.

## §5 RPC (engine.proto)

- `NvarList(NvarListRequest{image_id, path?, include_data})` →
  `NvarListResponse{stores: [NvarStoreInfo{path, desc, vars:
  [NvarVarInfo{name, guid, offset, size, attributes, data?}]}]}`;
  `include_data` тянет байты данных (TUI), CLI-листинг без данных;
- `NvarSet(NvarSetRequest{image_id, name, guid?, offset, value, width})`
  → `NvarSetResponse{applied: [string]}`;
- `QuestionInfo` += `seed_value?`, `seed_option?` (аналогично в элементах
  `HiiListQuestions`).

## §6 CLI

По образцу hii-команд (сессия/активный образ, форматы text/tsv/json):
- `nvar list [--path P] [--var NAME]` — таблица переменных; text-режим
  группирует по сторам с метой; легенда — в stderr через общее
  семейство `uefi_common::format` (по образцу `format_legend` /
  `hii_legend`: свой вариант `NvarLegendCmd::VarList`, stdout остаётся
  чистым для пайпов);
- `nvar set <name> --guid <G> --offset <N> --value <V> [--width W]` —
  вывод applied-строк как у `hii question set-value`;
- `hii question info` / список вопросов — строка/колонка seed-значения.

## §7 TUI

**Image View, панель Details** (`ui/details.rs`, layout `ui/mod.rs:35`):
выбран узел с `is_nvar` → вертикальный сплит на три секции: (1) мета —
`details_text` как сейчас, кап по высоте; (2) список переменных (flex) —
курсор j/k при `Focus::Details`, строки
`имя  GUID  +абс_офсет  размер  атрибуты`; (3) hex значения выбранной
перемной (16 байт/строку с офсетами), свой скролл PgUp/PgDn. Данные —
`NvarList(path, include_data=true)` при попадании курсора на стор, кэш по
path, состояние «loading…» (паттерн Forms). Узел не-стор → рендер в
точности как сейчас. Глиф узла — флоппи (§2).

**Forms View** (`forms.rs:289` form_panel, `ui/forms.rs:127`
render_middle): строки вопросов `Vec<String>` → стилизованные `Line`:
- цветной глиф по типу вопроса (аналог `type_color`: one_of — Yellow,
  checkbox — Green, numeric — Cyan, fallback — Gray как сейчас); цвет
  сам отделяет вопросы от заголовка/gates — строка-разделитель не нужна;
- в конце строки `= <seed>`; раскраска: ненулевое/Enabled — зелёный,
  нулевое/Disabled — серый, seed≠IFR-дефолт — жёлтый маркер `≠`;
- `question_bottom` (bottom-зона): + `Value (NVAR seed): …` и
  `IFR default: …` отдельными строками.

## §8 Тесты и живые гейты

Unit (фикстуры в стиле nvar.rs-тестов):
- resolve_guids: gi=0 → последние 16 байт блоба; индекс за хвостом →
  None; фикстура с двумя Setup → разные GUID;
- рекурсия вложенных сторов, depth-cap;
- find_var: имя+GUID уникально; дубль имени без GUID → Err с
  кандидатами; ext-header запись → отказ правки;
- проб метки: NVAR-тело → «NVAR store», не-NVAR → прежнее имя;
- join: фикстура HII-карты + стор → seed_value/seed_option; стора нет →
  None;
- RPC/CLI output smoke (text/tsv/json), TUI TestBackend: три зоны
  панели, флоппи-глиф, цветные глифы вопросов, value-строки,
  loading-состояние.

Real-image `#[ignore]` (образы: 226D2IL3.30/.50, C275D4I3.20, HNX99TF):
- 226D2IL3.30/.50: `nvar list` — 14 переменных; `Setup`/EC87D643 →
  абс-офсет данных **0x500088**, размер 1217; `Setup`/01239999 → 204;
  метка/`is_nvar` на узле стора;
- **bake-гейт**: `nvar set Setup --guid EC87D643… --offset 1 --value 1`
  + `--offset 1141 --value 1` → дифф образа ровно
  `0x500089: 00→01`, `0x5004FD: 00→01`; round-trip байт-точен; FFS
  чексуммы валидны (image save → повторное открытие);
- C275: листинг стора (`Setup` 215b, `IntelSetup` 597b,
  `ServerSetup` 445b — TODO:1499);
- HNX: `nvar set` правит **обе** копии — FV0 raw и FV2
  LZMA(repack+slot-fit); при E14-эквивалентных входах (varstore
  «Setup», var_offset 0x3A, value 1, width 1) дифф побайтово ≡ эталону
  E14 (`refs/amibcp/e14-stddefaults-4g.bin`); `set_value` 22/22
  остаётся зелёным (регресс);
- join-гейт HNX: seed 4G-вопроса == эталону E14.

## §9 Критерии готовности

- все гейты §8 зелёные; `cargo test --all`, `cargo clippy --all --
  -D warnings`, `cargo fmt --all -- --check` зелёные; без unwrap/expect
  вне `#[cfg(test)]`;
- испечённые артефакты `refs/amibcp/asr1-{330,350}-sol4g-on.bin`
  (SOL=1, 4G=1) + sha256 — запись в отчёт цикла;
- TODO:1560 (цикл NVAR-правок) закрыт; TODO:3247 (метка NVRAM store)
  закрыт.

## §10 Границы

- Прошивка asr1 — отдельно, по согласованию (host policy).
- Runtime-запись живого NVRAM (setup_var-класс) — вне скоупа.
- Именованные пресеты настроек («SOL», «4G») — не делаем, офсеты из
  отчёта разведки.
- Синтетические узлы-записи в дереве — отвергнуто (брейншторм).
- Auth/ext-header записи — только геометрия и отказ в правке.
- SDP/IFR-дефолты не трогаем; компрессор Tiano (этап 2 tiano-op) этому
  циклу не нужен.

## Риски

- False-positive body-проба метки (случайное NVAR-подобное тело) —
  маловероятен и информационно безвреден; гейты на трёх живых образах.
- GUID-резолв на сторах с битым хвостом — bounds-check → None, деградация
  только косметическая.
- Join показывает seed, а не живые значения — осознанный контракт
  (подписан в UI), владелец подтверждён.
- Объём TUI-части (панель + формы-цвета) — отдельные задачи плана,
  движимые TestBackend-тестами.

## Аддендум (2026-09-21): семантика барьера §3 распространяется на `set_value`

`collect_std_defaults_hits` (hii/mod.rs, путь `hii set_value`) отказывает
`MutationBehindCompression` на ЛЮБОЙ StdDefaults-копии за барьером — даже
когда доступна живая raw-копия. На 226D2IL это убивает всю операцию:
запечённый слепок за Tiano (путь 6/3/0/0/0/11/0/0) найдётся обходом и
прервёт её, хотя raw-стор FFS CEF5B9A3 @FV0x500000 записываем. Расхождение
с §3 (nvar_set: skip non-recompressable, refuse sole-behind-barrier).

Решение: `collect_std_defaults_hits` переходит на v5 — за-барьерные копии
пропускаются, `MutationBehindCompression` только если после обхода
доступных копий ноль (sole-behind-barrier). Юнит-тест
`set_value_refuses_store_behind_non_recompressable` уточняется: refuse
остаётся для sole-behind-barrier; новый кейс mixed (raw + за-Tiano →
пишется raw).

Мотивация: `hii set-value` на 226D2IL (см. аддендум hii-walker 2026-09-21):
Above 4G = CheckBox form 0x0486 qid 0x010D, varstore `Setup` off=1141;
SOL = OneOf off=1 (формы читаются из 0x18-канала). Реал-гейт: дифф set_value
совпадает с nvar-путём — ровно байты 0x500089/0x5004FD. Граница §10
«SDP/IFR-дефолты не трогаем» подтверждена probe: у вопросов SOL/4G
IFR-дефолты пусты (`defaults=[]`), реальные дефолты живут только в
StdDefaults.

## Аддендум (2026-09-21): TUI ex-команды `:nvar list/set`

§7 дал NVAR-панель Details, но ex-команды не завёл — NVRAM-правки
остались CLI-only (§6). Живая сессия 2026-09-21 это подсветила: Timeout
правили из шелла через `uefi-cli nvar set`, хотя это HII-вопрос
(1031:0x4) и печётся из Forms View; сам путь «узнать офсет → set» в TUI
отсутствовал (TODO:1612, мини-цикл).

Решения:
- `:nvar list [PATH] [--var NAME]` — два режима. С PATH — целевой:
  `goto_path` (с разворотом предков — запечённая копия за барьером
  6/3/0/0/0/11/0/0 достигается одной командой, без ручного разворота
  8 уровней, TODO:3332) + загрузка панель по `NvarList(path,
  include_data=true)`, статус `nvar <path>: N vars · records R · free
  F`. Без PATH — сводка в статус-строку: `nvar: S stores · V vars
  (paths)`; пустой ответ подсказывает behind-barrier-копию и PATH.
  `--var NAME` — строки `имя офсет размер стор` (офсет — как в CLI,
  {:#010x}); `--var` валиден только без PATH: панель всегда показывает
  все переменные стора, половинчатый фильтр отвергнут (usage-отказ).
- `:nvar set NAME --offset OFF --value VAL [--guid GUID] [--width
  1|2|4|8]` — паритет с §6: offset — смещение ВНУТРИ данных переменной,
  width дефолт 1, hex/dec через parse_u64_loose; статус повторяет
  applied-строки движка (`<desc> store+<off>: from -> to`). После
  успешного set кэш панель (`nvar.key`) инвалидируется; курсор на сторе
  — немедленная перезагрузка (hex показывает новые байты).
- Комплишен: подкоманды list/set; PATH — по видимому дереву; имена и
  guid — из загруженной панель (все сторы, дедуп); width — 1/2/4/8.
- Enter в NVAR-панель (Focus::Details на узле стора) — prefill
  `:nvar set <name> [--guid G] --offset 0 --value ` (паттерн Forms View
  Enter → set-value; guid подставляется, если запись его несёт).
- Хелп-оверлей: обе команды в EX-COMMANDS (с семантикой offset и
  behind-barrier) + новая секция NVRAM STORE для клавиш панели —
  до этого панель в хелпе не была документирована вовсе.

Тесты: 12 новых. Unit: parse_nvar_set (дефолты/отказы/unknown-флаг),
nvar_set_prefill (guid/без guid/пустая панель), complete_nvar_* (команды/
флаги/значения), nvar_status_builders. Integration (мок): list сводка+
фильтр+usage-отказы, list целевой (панель+goto+несуществующий путь),
set (RPC-поля, инвалидация кэша, width/usage). Мок-сервер: NvarList
отдаёт фикстуру с эхом path, NvarSet пишется в schema_calls
(target=name, schema_json=offset, extra=`guid|value|width`).

Статус: TODO:1612 закрыт; коммит 9e88d26 (master → PR), клозед-цикл
без правок движка/RPC — только TUI-клиент.
