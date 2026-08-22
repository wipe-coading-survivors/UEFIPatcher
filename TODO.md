# TODO — отложенные и несрочные правки

> Сюда заносим правки, которые не входят в текущий цикл, но должны быть сделаны:
> известные дефекты, рефакторинг, deprecated-код, пробелы в API.
> Формат: `* [ ] <область> — <что сделать>. Контекст: <почему>.`

> **Plan A complete** (commit `a02654a`, цикл 2026-08-10):
> CLI topology (5 top-level), proto noun-first rename, image persistence
> with write-through, lazy re-load. Разделы ниже актуализированы — что
> закрыто Plan A помечено `[x]`, что осталось — `[ ]`. Deep work (TUI
> bugfix, Gateway+WebUI rework, Plan B) тречится отдельно.

## Deprecated: выпиливание DumpTree RPC

> Plan A complete (commit `a02654a`). Весь раздел закрыт — DumpTree RPC,
> `DumpFormat`, `dump_tree` handler, `parser::image::dump_tree`, все
> клиентские вызовы и mock-stub'ы удалены. WebUI `/dump` маршрут пока
> оставлен (внутри вызывает `image_nodes_list`, возвращает `{nodes}`) —
> глубокий rework в разделе "Gateway + WebUI rework".

* [x] **uefi-tui** — `dump_tree` client-вызов и `parse_tree_dump` удалены
  (Plan A Task 6); заменён на `image_nodes_list` + `parse_nodes_flat`
  (плоский список — известный display-bug, отдельный TODO ниже).
* [x] **uefi-gateway / WebUI** — `DumpTree` клиентский вызов убран,
  `/dump/ws` удалён (Plan A Task 7); WebUI `findItem()` убран (Task 8).
  `/dump` маршрут сохранён, внутри `image_nodes_list`.
* [x] **uefi-proto + uefi-engine** — `rpc DumpTree`, `DumpTreeRequest`/
  `DumpTreeResponse`, `dump_tree` handler, mock-stub'ы, `parser::image::
  dump_tree` и `DumpFormat` enum удалены (Plan A Task 3 + Task 4g).

## Ревизия CLI (2026-08-10)

> Plan A complete (commit `a02654a`) для большинства пунктов. Оставшиеся
> (image switch валидация, `about:` на подкомандах) — ниже.

### Семантика команд image

* [x] **`image find <target>` бесполезна** — закрыто: `FindItem` RPC и
  `image find` удалены (Plan A Task 3/5); discovery через `node search`.
* [x] **`image list` конфликтует семантически** — закрыто: `image list`
  теперь `ImagesList` (список открытых образов); FFS-узлы → `node list`.
* [x] **Нет `ListImages` RPC в proto** — закрыто: добавлен `ImagesList` +
  `ImageInfo` (Plan A Task 3) + CLI `image list` (Task 5).
* [ ] **`image switch <image_id>` не валидирует образ на сервере** —
  только перезаписывает локальный state-файл. Опечатка молча сохраняется.
  Контекст: добавить проверку через `ImageStatus`/`ImagesList` перед
  записью state.
* [x] **Нет `image status`** — закрыто: `ImageStatus` RPC + CLI
  `image status` (Plan A Task 3/4c/5).

### UX/cli-rendering

* [ ] **Все подкоманды без `about:`** — `image --help`, `node --help`,
  `setup --help` показывают голые имена без описания. Проставить
  `#[command(about = "...")]` и `#[arg(help = "...")]` везде. Plan A
  топологию поменял, но `about:` не добавил.
* [x] **`--format` глобальный — String, не ValueEnum** — закрыто:
  `OutputFormat` теперь `clap::ValueEnum` (Plan A Task 5a).
* [x] **`--mode` у `image open` — String, не ValueEnum** — закрыто:
  `ImageModeCli`/`InsertModeCli`/`SearchModeCli` ValueEnum (Plan A Task 5e).
* [x] **Семантика `ffs`/`data` в insert/replace** — закрыто: ArgGroup
  `--file <path>` / `--artifact <id>` (взаимоисключающие) для `node
  insert`/`node replace` (Plan A Task 5e).
* [x] **`session init` имя = `env::var("PWD")`** — закрыто: явный
  `--name <name>` (Plan A Task 5d).

### Прочее

* [x] **`output::print_find` печатает только `item_id`** — закрыто:
  переименовано в `print_node_id` (Plan A Task 5a); для `node insert`/
  `replace` возврат echo target как `item_id` сохранён намеренно
  (target — это и есть идентификатор узла в текущей модели).
* [x] **`setup list-items` противоречит `image list`** — закрыто: setup
  реструктурирован в `setup form {list,set-visibility}` + `setup string
  {list}` (Plan A Task 5d).

### Новые находки Plan A (code review)

* [ ] **`client.rs image_status` делает `.info.unwrap()`** (Task 5b,
  verbatim из brief) — паника, если сервер вернёт `None` (race с close).
  Контекст: `crates/uefi-cli/src/client.rs` image_status. Заменить на
  `ok_or_else(|| AppError::new(ErrKind::NotFound, ...))`. Mock'и
  компенсируют возвратом `Some`, но production-сервер может race'нуть.
* [ ] **`write_through_persists_mutation_to_disk` тест тафтологичен** —
  `before==fixture_volume()` делает assertion `after != before || after
  == fixture_volume()` всегда истинным. Тест не ловит регрессию удаления
  `flush_image`. Контекст: `crates/uefi-engine/src/rpc/server.rs` tests.
  Усилить: mtime-check или реально меняющая байты мутация (`node remove`).
* [ ] **Интеграционные тесты CLI проверяют только exit-code, не stdout**
  — `cli_integration.rs`/`e2e.rs` (Task 5f.0). Регрессия в print-fn
  пройдёт незамеченной. Добавить content-assertions.
* [ ] **Нет теста на ArgGroup exclusivity** — `--file X --artifact Y`
  (clap ловит) и ни `--file`, ни `--artifact` (runtime ловит) не покрыты.


## План B: IFR forms/strings extraction + слияние с setup_advanced

Спека `2026-08-10-cli-topology-and-image-storage-design.md` (План A)
заводит stub'ы RPC `SetupListForms` / `SetupListStrings`, возвращающие
`UNIMPLEMENTED`. Полная реализация — План B: парсинг IFR-форм (FFS-секций
с IFR-байткодом) и string-package'ей, слияние модуля `setup_advanced/` с
`setup/` в одну согласованную иерархию.

> **Plan B core complete** (актуализация 2026-08-22): реализовано спекой
> `2026-08-14-hii-forms-strings-extraction-design.md` (фазы 1–6, ветка
> `fix/cycle6-reimplent`): `setup/`+`setup_advanced/` слиты в единый `hii/`
> с полным ребрендингом `Setup*`→`Hii*` (модуль, proto, CLI-noun), ридеры
> форм/строк работают на живом HNX99TF через PE-resource + bare каналы
> (фаза 6). От исходного scope остались: client-wiring Gateway/WebUI
> (отложен решением #2 спеки — см. «Gateway + WebUI rework») и TUI HII-view
> (пункт ниже). Развитие дальше — планы `2026-08-14-lzma-recompression.md`
> и `2026-08-14-hii-form-addition.md` (фазы A–C завершены).

**Baseline-коммит для onboarding** (где найти актуальный на момент
написания код `setup_advanced/` и `setup/`):
`0470553bcc579af0eb72075533bc2c73f77d543f` —
`crates/uefi-engine/src/setup_advanced/{mod,ffs_assembler,ifr_builder,ami_patcher,schema,string_pack}.rs`
и `crates/uefi-engine/src/setup/{mod,ifr}.rs`. Если код переехал/удалён —
искать через `git log --all -- crates/uefi-engine/src/setup_advanced/`.

* [x] **Реализовать `SetupListForms`** — закрыто фазами 4+6 под именем
  `HiiListForms` (ребрендинг фазы 2): IFR-walker `hii/ifr.rs`
  (`parse_form_package`, `ff1a5ab`), `collect_forms` + резолюция титулов
  по string-package в `hii/forms.rs` (`5967e9b`), RPC-handler (`844e77c`);
  real-image канал — PE-resource extraction фазы 6 (`e74cb32`, `a35c085`).
  `FormInfo`: form_id (GUID-target), formset_guid, form_id_ifr, title,
  visible (suppress-скоупы: `find_suppress_if_scopes` + per-form
  `find_form_suppress_scope`, `8160c5e`).
* [x] **Реализовать `SetupListStrings`** — закрыто фазой 3 под именем
  `HiiListStrings`: SIBT-reader `hii/strings.rs` (`parse_string_package`,
  `cfe64a3`) + `collect_strings` и RPC-handler (`dee2bcb`); edk2-раскладка
  языка @+46 (`f5198f9`). `StringInfo`: language / string_id / text.
* [x] **Слить `setup_advanced/` с `setup/`** — закрыто фазой 1 (`b4242b1`,
  `458beac`), с отличием от исходного замысла: итоговый модуль — `hii/`, не
  `setup/` (решения #5/#6 спеки: полный ребрендинг `setup`→`hii`). Текущий
  состав: `hii/{mod,strings,ifr,forms,string_pack,schema,ami_patcher,
  ffs_assembler,ifr_builder,package_list,pe_resource,formset_add,
  form_add}.rs`.
* [x] **Подключить CLI** (исходный пункт — CLI/TUI/Gateway/WebUI) — CLI на
  реальных RPC: `hii form list`, `hii form set-visibility`, `hii string
  list` (фаза 2 `afb5156`; stdout-ассерты фазы 5 `ad70b2c`); позже расширено
  `hii formset add` (`08ff7b0`) и `hii form add` (`ce2e57d`). TUI/Gateway/
  WebUI отложены решением #2 спеки на отдельные циклы: gateway имеет только
  `/set-visibility` на реальном RPC (`d4423fc`), forms/strings-листинги не
  подключены — см. «Gateway + WebUI rework»; TUI — пункт ниже.
* [ ] **TUI: отдельный HII-view для показа/редактирования форм и строк** — не
  встраивать HII-иерархию (formset'ы/формы/строки/языки) в дерево
  BIOS-регионов: дерево — физическая модель хранения (FV/файлы/секции), HII —
  логический слой поверх пакетов. Контекст: решение фазы PE-resource
  extraction (подход 1 — ленивое извлечение, без материализации в дерево);
  отдельный view становится потребителем `collect_forms`/`collect_strings`.
  Каноничные инструменты (AMIBCP и т.п.) показывают формы отдельным
  экраном; в дереве BIOS-регионов HII всё равно не виден (пакеты внутри
  PE-ресурсов сжатых секций).

### Находки ревизии фазы 3 string-reader (2026-08-14)

> Фаза 3 (`hii/strings.rs`, reader) завершена; ниже — отложенные minors
> из per-task и финального ревью (branch `fix/cycle6-reimplent`).

* [ ] **hii/strings: SIBT_EXT1/2/4 (0x30–0x32) не обрабатываются** —
  трактуются как unknown-opcode, walk останавливается с warn; пакет с EXT-
  блоками молча теряет все последующие строки. Контекст: writer
  (`string_pack.rs`) ведёт себя так же; в реальном firmware редкость.
* [ ] **hii/strings + string_pack: унифицировать SIBT-код** — константы
  опкодов и u16-хелперы дублируются reader'ом и writer'ом и уже дрейфуют
  (clamp `<=` vs `<` в info_off). Контекст: вынести в общий `sibt`
  submodule при следующем касании.
* [ ] **hii/strings: walk-сигнал «found» = `!out.is_empty()`** — пустой,
  но валидный первый string-package не останавливает обход (может
  вернуться пакет позже по дереву); guard `if let Some(pkg)` вокруг
  `parse_string_package` в walk — мёртвый (None недостижим после
  `is_string_package`). Контекст: дегенеративный случай, verbatim из
  плана фазы 3; поправить found-флагом при следующем касании файла.
* [ ] **hii/strings: обрезанный u16-count STRINGS_\* блока молча даёт
  count=0** — `read_u16` fallback `(0, body.len())` без warn; одиночный
  хвостовой байт UCS2-блока даёт одну пустую запись. Контекст:
  ограничено одной записью, массовая фабрикация пустых строк исправлена
  в фазе 3 (commit `f5198f9`).

### Находки ревизии фазы 4 IFR-reader/forms (2026-08-14)

> Фаза 4 (`hii/ifr.rs` walker + `hii/forms.rs` driver + `HiiListForms`
> handler) завершена; ниже — отложенные minors из per-task и финального
> ревью (branch `fix/cycle6-reimplent`, commits `ff1a5ab..844e77c`).

* [ ] **hii/forms: round-trip тест останавливается на `parse_target`** —
  live-резолюция `find_item` по form_id не покрыта тестом (контракт
  зеркальности счётчика держится только на параллельном чтении кода;
  `find_by_guid` умеет матчить guided-секции через parsing_data — этот
  путь тоже не зафиксирован). Контекст: добавить в фазу 5 вместе с
  `find_item_mut` GuidSection-arms: `find_item(&root, &t)` → найденный
  node re-парсится в тот же `FormSetInfo`.
* [ ] **hii/forms: глобальная карта титулов из первого string-package по
  всему образу** — StringId уникальны per package-list; в реальном образе
  с несколькими HII-файлами формы чужих файлов могут получить неверные
  титулы (cross-file id collision), не только пустые. Контекст: главный
  fidelity-риск фазы; real-image `#[ignore]` тесты фазы 5 — tripwire;
  если проявится — scoping карты per-file.
* [ ] **hii/ifr: walker игнорирует заявленную длину пакета в header
  bytes 0..2** — идёт до `body.len()`; при теле с хвостовыми данными
  за пределами одного пакета возможен over-walk. Контекст: контракт
  «одно тело пакета на секцию» (как в фазе 3); worst case — spurious
  `None` → секция пропускается, не паника; усилить при находке на
  real-image тестах.
* [ ] **hii/ifr: недостающие edge-тесты** — stray END на пустом стеке,
  `length < 2`, короткий IfrFormSet (`length` 2..23), nested-suppress
  порядок. Контекст: код-пути проверены ревью чтением (`Vec::pop` no-op,
  `length < 23 → None`), добавить тестами при касании.
* [ ] **план фазы 4: внутренняя несогласованность** — текст плана
  требует «reuse `guid_from_bytes`», а mandated-код использует
  `Guid::from_bytes` (реализация следовала коду). Контекст: семантически
  идентичны (`parser/file.rs:11` — это `Guid::from_bytes` с проверкой
  длины); заметка, чтобы текст плана не вводил в заблуждение.
* [ ] **тест-фикстуры `opcode`/`form_set`/`package` дублируются** между
  test-модулями `ifr.rs` и `forms.rs` (~70 строк). Контекст: план
  объявил дублирование намеренным (развязка prod-модулей); при росте —
  вынести в общий `#[cfg(test)]` fixture-модуль.

### Находки фазы 5 real-image validation (2026-08-14) — HII в PE-ресурсах

> План-дефект, обнаружен живым запуском тестов Task 3 (branch
> `fix/cycle6-reimplent`, commit `9413d5a`). Ридеры фаз 3–4 валидировались
> на синтетических фикстурах (bare HII-пакет = тело секции 0x19); в
> реальном образце HNX99TF (AMI Aptio V) HII-списки лежат иначе.

* [x] **Реальный образец: HII package lists внутри PE32 `.rsrc`-ресурсов
  (тип `'H'`, EDK2 HiiAddPackages-механизм), а не bare-пакетами в телах
  секций.** Предикаты `is_form_package`/`is_string_package` дают 0 hits
  по всему дереву (включая распакованные LZMA-детей); `collect_forms`/
  `collect_strings` на реальном образе возвращают пусто. Данные
  подтверждаются: FFS Setup `899407D7-99FE-43D8-9A21-79EC328CAC21` →
  PE32 → resource 'H' (55 266 B): FORM len=10 082 (formset
  `7B59104A-C00D-4158-87FF-F04D6396A915`) + 2 STRING; FFS Platform
  `ABBCE13D-E25A-4D9F-A1F9-2F7710786892` → PE32 → resource 'H'
  (545 682 B): FORM len=207 249 (formset
  `EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9`) + 2 STRING. Секции обоих
  файлов: `[DEPEX(0x13), GUIDed-LZMA(0x02)]`, внутри LZMA —
  `[PE32(0x10), UI(0x15), VERSION(0x14)]`, RAW-секции в образе — ACPI.
  **Нужен engine-этап:** PE32 `.rsrc`-обход → 'H'-ресурс → package-list
  header (16-byte GUID + u32 length + конкатенация пакетов) → диспетч
  в существующие `parse_form_package`/`parse_string_package`. Ресурс-
  каталог можно брать через крейт `object` (уже в workspace). Acceptance-
  тесты — два красных `#[ignore]`-теста в `real_image.rs`
  (`real_image_hii_forms_and_strings`,
  `real_image_hii_form_visibility_round_trip`); ожидаемые formset-GUID'ы
  см. выше.
  *Уточнения probe-разведки 2026-08-14 (брейнсторм фазы 6, скрипт
  `hack/hii_probe.py` на HNX99TF + EDK2-образах OVMF/edk2-rk3588):*
  - Имя типа ресурса — **`L"HII"`** (у AMI и EDK2 одинаково); «'H'» в
    отчёте фазы 5 — неполное чтение имени.
  - На HNX99TF всего **6 package lists / 4 формсета**: помимо Setup и
    Platform ещё `70E1A818` (FORM 660, formset `80E1202E`) и
    `CDC1C80D` (FORM 233, formset `932D37B0`) + 2 строковых списка.
    `list_guid` списка = GUID FFS-файла-владельца.
  - **EDK2-образы хранят HII двойственно**: STRING/IMAGES-пакеты — в
    'HII'-ресурсах ( найдены списки только с type 0x04/0x06), а
    FORM-пакеты — **bare-конст-массивами в теле PE** (VFR→массив,
    HiiAddPackages в рантайме). Ресурс-канал один покроет AMI, но не
    формы EDK2 — нужен второй канал: bare-скан внутри PE32-тел с
    глубокой валидацией `parse_form_package`.
  - EDK2-образы (OVMF 4MB ×2, edk2-rk3588 1.9MB + капсула 6.9MB) —
    лицензионно чистый источник фикстур для тестов фазы 6.
  *Закрыто фазой 6* (спека `2026-08-14-hii-pe-resource-extraction-design.md`,
  план `2026-08-14-hii-phase6-pe-resource-extraction.md`): package-list
  сплиттер `hii/package_list.rs` (`e74cb32`), PE `.rsrc` `L"HII"`-extraction
  через `object` (`a35c085`), bare-скан PE32-тел для EDK2 FORM-массивов.
  Оба acceptance-теста зелёные на HNX99TF (перепроверено 2026-08-22 — 4/4
  HII-тестов, включая formset/form add); `real_image_hii_forms_and_strings`
  зелёный и на edk2-rk3588 (см. заметку в разделе фазы 6 ниже).
* [x] **`parse_string_package`: язык по реальному
  `EFI_HII_STRING_PACKAGE_HDR`** — закрыто probe-разведкой 2026-08-14
  (байты HNX99TF и edk2-rk3588 идентичны): `LanguageWindow CHAR16[16]`
  @+12 (обычно нули), `LanguageName u16` @+44 (string-id отображаемого
  имени), `Language CHAR8` **@+46** переменной длины до NUL (ограничено
  `HdrSize`); «en-US» → HdrSize=52, длиннее → 57. `strings.rs`
  (`LANGUAGE_OFFSET=46`, граница `info_off`) уже корректен для реальных
  данных. Осталось: синтетические фикстуры фаз 3–4 пишут язык @+12 —
  выровнять под реальный layout в фазе 6. Выполнено: тест-фикстура
  `make_pkg` (`hii/strings.rs` tests) пишет язык по реальному offset 46.
* [x] **`real_image.rs`: ignore-строки разъехались** — 10 старых тестов
  используют `"... (gitignored)"`, 4 новых (full-flash + HII) — без
  суффикса. Мелочь; унифицировать при следующем касании файла. Закрыто
  фазой 6 (commit `d781aea`, Task 8 — все 14 ignore-строки приведены к
  длинной форме `"(gitignored)"`).
* [ ] **language-assert в `real_image_hii_forms_and_strings`** — предикат
  `all(...)`, а сообщение об ошибке семплирует только `first()`; при
  падении укажет не тот пакет. Verbatim из плана фазы 5; поправить при
  следующем касании.
* [x] **`set_item_visibility` не проверяет `ImageMode::Write`** перед
  мутацией (наследуется от Path-only версии, фаза 5 семантику сохраняла).
  Контекст: пункт финального ревью фазы 5; добавить гейт при следующем
  касании `hii/mod.rs`. Закрыто фазой 6 (commit `5a1d543`, Task 7 —
  NotWritable-гейт).
* [x] **Формат таргета `guid:subtype:idx` не композируется по вложенным
  секциям** — `collect_forms` протаскивает file_guid внутрь guided/
  compressed-секций, а GuidSection-arm `find_item`/`find_item_path`
  сканирует только прямых детей файла (зеркально by design). Контекст:
  при фазе PE-resource extraction пересмотреть формат/обход, чтобы
  вложенные 0x19-секции разрешались; туда же — зелёный тест на
  `section_index: Some(n>0)` через `find_item_path`. Закрыто фазой 6
  (commit `7e654b0`, Task 6 — DFS-зеркало `find_item`/`find_item_path`
  через обёртки 0x01/0x02; требуемый кейс `section_index: Some(n>0)`
  покрыт тестами `find_item_path`).
* [x] **RPC: HiiError всплывает как `Status::internal` (blanket `map_err`)** — закрыто
  error-mapping pass'ом (коммит `22fe28b`): `hii_error_status` (NotWritable/
  MutationBehindCompression → `failed_precondition`, NotASetupItem →
  `invalid_argument`, NotFound/StringPackageNotFound → `not_found`, прочие —
  `internal`) в `hii_set_form_visibility`; `image_save` маппит через
  `builder_error_status`; `Compression(EmptyInput)` → `failed_precondition`
  (удаление последнего ребёнка LZMA-обёртки — предусловие, не internal).
  `hii_form_set_add` сохраняет собственный богатый маппинг (InvalidSchema →
  invalid_argument и т.п.) — сознательно не тронут.

### Фаза 6 PE-resource extraction: nested-FV HII отложен (2026-08-14)

> Находка финальной верификации Task 9 фазы 6 (branch
> `fix/cycle6-reimplent`). Спека `2026-08-14-hii-pe-resource-extraction-design.md`
> §8 ожидала, что rk3588-образ пройдёт `real_image_hii_forms_and_strings`
> через bare/resource-каналы — фактически HII этого образа недостижим на
> уровне дерева: контр-зеркальная дисциплина §4.6 (не спускаться в 0x17)
> побеждает. Основной acceptance HNX99TF не затронут (real-image тесты
> зелёные, включая оба HII).

* [x] **ENGINE: nested-FV HII extraction (спуск в 0x17 FV-image секции)**
  — закрыто на уровне парсера (коммит `721f045`): `parse_section` для
  0x17 с валидным FVH в теле материализует Volume-узел (переиспользован
  `parse_firmware_volume` = parse_volume + parse_volume_files) при строгом
  условии `vol_size == body.len()` (FV с хвостовым слаком не спускается —
  гарантирует byte-фиделity rebuild). Walkers (`walk_files`/
  `walk_for_string_package`) и `find_by_guid`/`find_item*` рекурсивны по
  дереву — HII внутри nested-FV достижим без правок hii-модуля; нумерация
  §4.6 не меняется (счётчики per-file, 0x17-дети — Volume-тип).
  Мутабельность FV-nested таргетов: 0x17 не барьер (гейт-2 не срабатывает),
  LZMA-обёртка выше — recompress (фаза 7); remove/rebuild покрыты тестом
  `nested_fv_round_trips_and_removal_preserves_length`. Регрессия на живом
  HNX99TF: 15/15 `#[ignore]`-тестов зелёные (full-flash round-trip
  байт-идентичен). Ограничение: живая валидация на edk2-rk3588 не
  проводилась (образ недоступен в refs/fw после фазы 6) — механизм покрыт
  синтетикой; прогнать `real_image_hii_forms_and_strings` c
  `UEFIPATCHER_TEST_FW=<rk3588.bin>` при появлении образа. Прогнано
  2026-08-22 (образ вернулся в refs/fw как
  `orange-pi-5-plus-uefi-edk2-rk3588.img`): тест зелёный. Специфичные
  для HNX99TF HII-мутационные тесты на rk3588 не применимы — падают на
  собственных предпосылках (нет Setup-formset / suppressed-форм), не на
  движке.
* [x] **clippy: `cargo clippy -p uefi-engine --all-targets -- -D warnings`
  падает (pre-existing)** — закрыто (коммит `2796c8a`: `manual_contains` в
  тесте `bare_scan_drops_candidates_covered_by_resource_ranges`).

### PE-resident IFR-патчинг: сделано 2026-08-14, отложенные миноры

> Закрыто коммитом `8160c5e`: третий гейт `set_item_visibility` сужен —
> PE32-таргет с FORMS-пакетами в 'HII'-ресурсах мутабелен; добавлен
> форм-дискриминатор `item_id` (`<target>#<form_id_ifr>`, ищет скоуп,
> обёртывающий конкретную форму, через `ifr::find_form_suppress_scope`);
> `unsuppress` переписан на `&mut [u8]` (in-place, длина неизменна —
> обязательное условие PE-resource патча; guard-false случай теперь no-op,
> а не безусловная вставка FALSE). Acceptance: real-image
> `real_image_hii_form_visibility_round_trip` на HNX99TF — полный цикл
> (форма 901 Platform-formset → PE-патч → LZMA recompress → re-parse →
> visible), 15/15 ignore-тестов зелёные.

* [ ] **FormInfo не отдаёт готовый item_id с дискриминатором** — для
  `setup form set-visibility` на конкретную форму пользователь должен сам
  конкатенировать `#<form_id_ifr>`; поле в FormInfo (или конкатенация в
  CLI/TUI) при подключении клиентов. Контекст: engine API поддерживает
  оба синтаксиса, но в выводе списков дискриминатора нет.
* [ ] **bare-канал FORM-пакетов (EDK2 конст-массивы в теле PE) не
  мутабелен** — unsuppress работает только по resource-каналу ('HII');
  для rk3588-подобных образов с suppressed-формами в bare-массивах нужен
  offset-вариант `bare_form_packages`. Контекст: на живых данных таких
  кейсов не встречено; расширение — точечное.
* [ ] **PE checksum не пересчитывается после resource-патча** — как и
  UEFITool; в firmware-PE поле обычно 0. Пересчёт при появлении живого
  прецедента отказа. (Дополнено 2026-08-21, решение B: с reloc-aware
  ростом актуальность выросла — меняются SizeOfImage/RVA/
  PointerToRawData; любой инструмент, валидирующий checksum, отклонит
  патченный модуль.)
* [x] **formset_add: вставка нового FFS в `Target::Path(vec![0])` теряет
  файл на full-flash** — исправлено в фазе A плана
  `2026-08-14-hii-form-addition.md` (коммит `73c54b5`): insert-цель —
  volume, в котором найден string-пакет. Там же закрыты: string-поиск
  только bare-каналом (замена на walker — `f806d5a`, спуск через
  обёртки; PE-resource-канал честно отказывает `StringPackageNotFound`
  до фазы B) и отсутствие CLI (`hii formset add` — `08ff7b0`).
  Осталось (фаза C плана): форма в существующий формсет.
  Resource-канал formset_add закрыт в фазе B (коммиты `5a7c4fd..f22f8f2`,
  reloc-aware рост .rsrc; позитивный acceptance на HNX99TF).
* [x] **add_setup_formset: частичная мутация при ошибке patch_ami** —
  закрыто в фазе B (коммит `c316da9`): pre-check `precheck_ami_modules`
  (обе находки `find_ami_module`) выполняется до любой мутации в обеих
  ветках (bare/resource); после pre-check у `patch_ami` не остаётся
  Err-путей (подтверждено ревью фазы B).
* [ ] **Ревью фазы A, minors** — (1) коллизия имён
  `walk_for_string_package` в `hii/strings.rs` (чтение) и
  `hii/string_pack.rs` (путь для мутации) — переименовать одну (напр.
  `locate_string_package_section`); (2) mock-сервер не эхоит поля
  запроса — CLI e2e `formset_add_flow` не проверяет доставку
  `schema_json`/`target_ffs_guid`, `--ffs` не покрыт e2e; (3) тест
  gap-aware может дополнительно проверять рост string-пакета и
  видимость формсета в `collect_forms` после re-parse.
* [x] **add_strings_to_resource: дискриминатор отказа роста** — закрыто
  в Task 9 фазы C плана `2026-08-14-hii-form-addition.md` (коммит
  `feat(uefi-engine,uefi-cli): form add RPC + subcommand`):
  `add_strings_to_resource` возвращает
  `Result<_, AddStringsToResourceError>` (NotFound |
  GrowthUnsupported), formset_add-вызов маппит отказ роста в
  `PeGrowthUnsupported` (не `StringPackageNotFound`); предикат
  `pe_resource_has_string_package` единый pub(crate) в string_pack.rs;
  walker formset_add пропускает PE32-кандидатов за non-recompressable
  обёрткой и продолжает поиск (нет годных, есть заблокированные →
  `MutationBehindCompression`).
* [ ] **string_pack: исчерпание string-id 0xFFFF** — `wrapping_add` в
  `scan_sibt`/`add_strings_to_body` заворачивает `next_id` в 0
  (невалидный HII string id); возвращать ошибку при исчерпании.
* [ ] **pe_resource: неоднозначный выбор .rsrc-секции на мусорных
  таблицах** — собственный предикат span=max(vsize,raw) first-match
  (`rsrc_raw_end`/`rsrc_virt_end`/`rsrc_grow_plan`) отличается от
  `object` min(vsize,raw) в `pe_file_range_at`; при пересекающихся
  диапазонах возможен выбор разных секций — отказывать при
  неоднозначности.

### Последствия reloc-aware роста .rsrc (решение B фазы B, 2026-08-21)

> Фиксация анализа последствий решения расширить `try_grow_rsrc_tail`
> до сдвига хвостовых секций (коммиты `7f56bb2..f22f8f2`, спека §4.B1
> поправка №2). До отдельного брейнсторма; повестка — последним пунктом.

* [ ] **Аппаратная валидация reloc-aware роста** — acceptance фазы B
  доказан на уровне build → re-parse → `collect_forms`, НЕ на железе:
  EDK2-лоадер применяет релокации через reloc-dir (обновляется —
  стандартный путь корректен), но вендорские DXE-ядра могут содержать
  допущения о геометрии модулей, недоступные парсеру. Митигации:
  глубокая UEFITool-проверка перед прошивкой (обязательна, §7.1 спеки),
  жертвенная плата или эмулятор (OVMF) для boot-уровня. Геометрия
  «.reloc после .rsrc» подтверждена только на HNX99TF (6/6 HII-модулей);
  до обследования других вендоров решение B не считать переносимым.
  Отказ при неверном сдвиге = модуль не грузится на DXE-фазе; риск
  локализован одним целевым модулем (вне-FV1 байты идентичны — ассерт
  теста).
* [ ] **Подписанные HII-модули — вне поддержки навсегда** — cert-table
  (security directory) привязан к raw-смещениям хвоста, сдвиг его ломает
  → честный отказ `PeGrowthUnsupported`. Без переподписи (отдельный
  дизайн) formset add на подписанных модулях невозможен. Оценить
  встречаемость на живых образах.
* [ ] **Big-diff патченного модуля** — после reloc-сдвига diff против
  оригинала «большой» (сдвинут весь .reloc), хотя осмысленные изменения
  — только HII-пакеты; осложняет ручную UEFITool-проверку. Кандидат:
  «семантический diff» (отчёт только по HII-пакетам/строкам/формам) в
  CLI/WebUI.
* [ ] **Компактфикация STRING-пакетов (удаление help-строк)** —
  воркэраунд пользователя до проекта: удаление help-строк ради
  освобождения места (на нём же столкнулся с тем же классом последствий
  — модуль грузится по заголовкам, сдвиг хвоста меняет diff/загрузку).
  Как фича: уменьшение STRING-пакета перед ростом (переиспользует
  writer) снижает потребность в росте PE и суммарный риск. В брейнсторм.
* [ ] **Брейнсторм: последствия решения B** — повестка: (1) план
  аппаратной валидации; (2) политика PE-checksum; (3) обследование
  геометрии HII-модулей других вендоров/образов; (4) дешёвый
  DXE-эмулятор как acceptance-стадия; (5) компактфикация STRING-пакетов.

### Фаза C form-add: находки acceptance (2026-08-20)

> Task 10 плана `2026-08-14-hii-form-addition.md` (live-тест
> `real_image_hii_form_add_into_setup_formset`, HNX99TF, 17/17 ignore-тестов
> зелёные). Мелкие API-наблюдения, surfaced живым тестом.

* [ ] **FormInfo: несогласованный регистр GUID между полями** — `formset_guid`
  приходит upper (`guid_to_upper_string`), а GUID-префикс `form_id` — lower
  (`Guid` Display в `format!("{}:{:#04x}:{}", fg, ...)` формы walk_sections).
  Клиент, сопоставляющий `form_id` с каноническим GUID-таргетом, обязан
  сравнивать case-insensitively (тест споткнулся об это на первом запуске).
  Контекст: `hii/forms.rs` `walk_sections`; унифицировать на upper при
  следующем касании (миграция клиентов: CLI/TUI уже выводят как есть).
* [ ] **add_form: существующие varstore/question id недискаверибельны** —
  `FormSetInfo` (ifr.rs) отдаёт только guid/title/forms; автор схемы не может
  динамически выбрать незанятый var-store id / question id и вынужден брать
  высокие «магические» значения (тест: 0x7F00/0x7F01). Контекст: расширить
  IFR-walker сбором varstore-id (и опционально question-id) при подключении
  TUI/WebUI редакторов; до тех пор документировать конвенцию high-id.
* [ ] **TUI/WebUI обёртки над `HiiFormAdd`/`HiiFormsetAdd` RPC** — CLI-обёртки
  есть (`hii form add`, `hii formset add`), интерактивных/WebUI-путей нет;
  отложено планом фазы C (§«Отложенное»).
* [ ] **FormInfo: formset-порядковый номер не виден в `hii form list`** —
  дискриминатор `#<n>` таргета `hii form add` не дискаверибелен: FormInfo не
  несёт formset ordinal, на multi-formset-пакетах пользователь не может
  узнать `n` (промах `#1` на single-formset-пакете — лёгкий first-attempt
  typo). Кандидат — расширить FormInfo (engine+proto+CLI) formset-порядковым
  номером или селектором по anchor-форме. Дополняет pre-check I1: неверный
  ordinal теперь отбивается NotFound до мутации строк.
* [ ] **add_form: `default_stores` игнорируются by design (решение R2)** — v1
  сознательно не обрабатывает `default_stores` (и прочие formset-level поля
  схемы); автор схемы не получает фидбека, что поле отброшено. Вернуться к
  вопросу при появлении живого use case.

## ImageUpload RPC (docker-развертывание)

`ImageOpen` читает файл с **серверной FS** по пути (`OpenImageRequest.path`).
В docker-развертывании (где client FS != server FS) это ломается: CLI/TUI
передают путь из своей FS, которого нет на сервере. WebUI/Gateway решают
это через `/api/v1/image/upload` (multipart → `/tmp/<uuid>.bin` →
`image open /tmp/...`), но это двухшаговый hack.

* [ ] **Добавить `ImageUpload` RPC** — принимает байты напрямую
  (`bytes: bytes`, `session_id`, `name`, `mode`), persist'ит в data_dir,
  парсит, возвращает `image_id`. Аналог `ImageOpen`, но без round-trip
  через серверную FS.
* [ ] **CLI: `image upload <local-path> [--name <n>] [--mode ...]`** —
  читает файл локально, шлёт байты через `ImageUpload`. Для docker-use-case.
* [ ] **TUI: `:upload PATH`** — аналог для интерактивного режима.
* [ ] **Gateway: переиспользовать multipart-эндпоинт `/api/v1/image/upload`**
  и дёргать новый `ImageUpload` RPC вместо текущего tmp-file workflow.
* [ ] **Server: переиспользовать `flush_image`/storage logic** — вынести
  общую часть `ImageOpen`/`ImageUpload` в helper.

## TUI: migration + bugfix (после Плана A)

> Plan A mechanical migration complete (commit `a02654a`, Task 6): все
> client-вызовы переименованы, `dump_tree` заменён на `image_nodes_list`
> + `parse_nodes_flat` (плоский список). Глубокая работа — ниже.

* [ ] **Выкинуть `parse_nodes_flat`** (`crates/uefi-tui/src/commands.rs`)
  — плоский список (depth=0 для всех), замена старому `parse_tree_dump`.
  Перевести на локальный tree-рендер через `uefi-common::format`
  (`format_tree` + `format_legend`), восстановив иерархию по `path`.
  Контекст: Plan A Task 6 сознанно сохранил display-bug (плоский список).
* [ ] **Fix существующих TUI-багов** — провести ревизию после миграции.

## Gateway + WebUI rework (после Плана A)

> Plan A mechanical migration complete (commit `a02654a`, Tasks 7+8):
> gateway client переименован, `/find` и `/dump/ws` маршруты удалены,
> `/dump` возвращает `{nodes}`, WebUI `findItem()` убран (был green до и
> после). Глубокий rework — ниже.

* [ ] **Gateway: новые маршруты** — `/api/v1/image/:id/dump` → `/nodes`;
  удалить `/dump`; добавить `/api/v1/images` (ImagesList),
  `/api/v1/image/:id/forms`, `/api/v1/image/:id/strings`,
  `/api/v1/image/:id/status`, `DELETE /api/v1/image/:id` (ImageClose).
* [ ] **Gateway: `routes/hii.rs`** (ex-`setup.rs`, переименован фазой 2
  Плана B, `d4423fc`) — добавить `forms`, `strings` handlers.
* [ ] **WebUI: полный fix** — обновить все fetch-вызовы под новые маршруты,
  подключить forms/strings listing, починить существующие баги (dead
  `dumpTree` import, unused `openImage` name field, a11y warnings).
  Дополнено актуализацией 2026-08-22: `addFormSet()` в `api.ts` зовёт
  `/api/v1/image/:id/add-formset` — маршрут в gateway никогда не
  существовал (кнопка Add FormSet на setup-странице даёт 404); gateway
  `/set-visibility` уже на реальном RPC, `/setup-items` (фильтр
  `image_nodes_list` по type 67) WebUI не использует.

## TUI: deferred enhancements (after cycle 3 rework)

> Из ревизии спеки `2026-08-12-tui-migration-bugfix-design.md`. Не вошло в цикл 3
> по YAGNI; записано для будущих циклов.

* [ ] **FIND/GO — jump to section** — интерактивным клиентам не нужен full-search
  как в CLI (`node search`), но нужен прямой переход по target/адресу (`:goto 0/3/1`
  или `/`-промпт), чтобы быстро прыгнуть на нужную секцию (например, на DXE/PEI при
  раскрытом первым ME). Curse перемещается + auto-expand родителей по пути.
* [ ] **Persistent search-results panel** — если когда-либо добавим поиск по
  содержимому/имени секции (не прямой goto), результаты должны отображаться
  **постоянно** (не исчезать после первого выбора) с навигацией по ним. Вариант:
  4-я панель со скроллингом (как tree/registry), либо таб в правой колонке, либо
  модалка. **Пустую заглушку сейчас не ставим** — layout/focus-ring из 4 элементов
  лучше проектировать вместе с самим search-ом. Решение оставить layout B (details +
  registry в правой колонке) принято в спеке 2026-08-12.
* [ ] **Unified path addressing** — переход с `""`-корня на унифицированную
  `/`-адресацию: `/` = корень-образ, `/0` `/1` = первые дети (FV), `/0/0` `/0/1` =
  внуки. Сейчас корень имеет path `""` (engine `parser/image.rs:107,115,127-133`),
  что ломает наивное `depth = path.matches('/').count()` и `parent_path + "/"`
  префиксы — приходится isolировать спец-случай в `segments()` (TUI `tree.rs`).
  Cross-cutting рефакторинг: engine parser, CLI `parser/target.rs`, proto-семантика,
  gateway, webui. **TUI переделывать не придётся** — `segments()` уже хэнделит оба
  формата идентично (`segments("") == segments("/") == []`).
* [ ] **Tab-completion + shared flag-parsing в `uefi-common`** — вынести парсинг
  флагов ex-команд (`--file`/`--artifact-id`/`--mode`/`--body-only`/TARGET-default)
  из `uefi-tui/src/commands.rs` и `uefi-cli` в общий модуль `uefi-common::cli` (или
  подобный), чтобы CLI/TUI не дублировали. Заодно — tab-completion для TUI-cmdline
  (имена команд, флаги, `--artifact-id` из текущего registry, target-ы из дерева).

## Ревизия TUI cycle 3 (2026-08-13)

> Находки пользовательского ревью после цикла `2026-08-12-tui-migration-bugfix`.
> Дефекты парсера/engine (не TUI-специфичные) — ниже.

* [ ] **Имя файла не поднимается из UI-секции, обёрнутой в GUIDED/LZMA** —
  `node_name` (`crates/uefi-engine/src/parser/image.rs:225-232`) для `FfsType::File`
  инспектирует **только прямых детей** на `EFI_SECTION_UI`/`EFI_SECTION_VERSION`.
  На реальных образах UI/Setup-секция DXE-файлов часто завёрнута в GUIDED (LZMA)
  compression-секцию → имя не поднимается на уровень файла (виден только путь,
  например `1/28`, а Setup-имя живёт в `1/28/1/1`). В PEI UI-секция — прямой ребёнок
  FFS-файла, поэтому там lift работает (напр. `2/2/2 WtdPei` поднимается на `2/2`).
  Контекст: подтверждено коммитом `fbd593eb` (display-and-search plan Task 12 fix).
  Влияет одинаково на CLI и TUI (оба берут имя из `node_name`). Фикс: рекурсивный
  спуск через GUIDED/compression-обёртки до UI/Version-секции (как делает UEFITool).
* [ ] **Том ME показывает только одну секцию** — первый ребёнок образа (path `"0"`,
  ME-регион) парсится в одну секцию, хотя по памяти их там больше. Регион ME
  использует нестандартный формат (FTPR), а `parse_image`
  (`crates/uefi-engine/src/parser/image.rs:20-51`) сканирует только `EFI_FVH_SIGNATURE`.
  Подозрение: либо совпадение сигнатуры `_FVH` внутри ME даёт один «объём», после
  которого FFS-парсинг обрывается (`parse_volume_files` делает `break` на
  erase-all-байтах `image.rs:82-83` или на ошибке `image.rs:97-99`), либо FFS-файлы
  ME не выравниваются на FFS_ALIGN=8. Контекст: проверить парсер на реальном образе
  `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, сравнить кол-во узлов ME с UEFITool.
  Возможно потребуется отдельный распознаватель ME/FTPR-регионов (как flash-descriptor).

### TreeView: пустые элементы (issue I, ревизия 2026-08-13)

> Рендерер `ui/tree.rs:30-37` собирает строку
> `{indent}{expand} {icon} {name} {guid} {marker}`.
> Если `node.name` пустое и `guid` равен `None`, строка визуально пустая
> (только иконка). `node_name` (`parser/image.rs:218-235`) возвращает "" для
> всего, кроме UI/Version-секций и файлов с прямым UI-ребёнком.

* [ ] **TUI: метка для узла Image** — корневой узел (type 62, path `""`)
  показывается пустым. Выводить имя образа из Registry (найти `ImageInfo.name`
  по `active_image_id`) или `image_id`. Контекст: `ui/tree.rs:35` рендерит
  `node.name`, для Image оно всегда пустое.
* [ ] **TUI: метка для узла Volume** — тома (type 65) показываются пустыми.
  Минимум: вывести строку `"Volume"`. Идеал: классификация региона
  `Volume (ME/DXE/PEI)`. Контекст: регион кодируется не в GUID тома (у FV нет
  GUID-региона; `VolumeParsingData` содержит только `extended_header_guid` /
  `ffs_version`), а в Intel Flash Descriptor (первые 0x1000 байт full-flash
  образа). Требует парсера IFD — отдельная задача engine (связана с пунктом
  про ME-регион выше).
* [ ] **TUI/engine: fallback-метка по subtype для безымянных узлов** — секции
  без UI-имени (DXE dependency, RAW, PE32…) и файлы без UI-ребёнка показываются
  пустыми. Выводить имя subtype через `section_type_name_or_raw` /
  `file_type_name_or_raw` (уже используется в `details_text`, `app.rs:245-254`).
  Контекст: `node_name` (`parser/image.rs:218`) возвращает "" — рендерер
  `ui/tree.rs` должен fallback'ить на subtype-name, когда `name` пуст.

### Remove: root cause бага + stale-tree (issue II, ревизия 2026-08-13)

> Баг в **engine**, не в TUI. `build_section`
> (`crates/uefi-engine/src/builder/mod.rs:103-126`) **не проверяет
> `Action::Remove`** — в отличие от `build_volume` (mod.rs:40-42) и
> `build_file` (mod.rs:73-75), которые обе проверяют Remove и пропускают узел.
> `build_section` проверяет только `NoAction` и `is_compressed_or_guided`, затем
> проваливается в rebuild-путь. Поэтому удалённая секция молча пересобирается в
> output — файл на диске не меняется по содержимому. Баг воспроизводится и в CLI,
> и в TUI. Дополнительно: TUI не обновляет дерево после мутаций, а `Node` proto
> не содержит `action` — клиенты не видят pending-операций.

* [x] **ENGINE (баг): `build_section` игнорирует `Action::Remove`** — root cause
  бага remove. Добавить `if node.action == Action::Remove { return Ok(()); }` в
  начале `build_section` (`builder/mod.rs:103`), аналогично `build_volume:40` и
  `build_file:73`. Покрыть тестом: remove секции → `build_image` → секции нет в
  выводе. Примечание: `build_file` (mod.rs:88-91) не скипает Remove-детей в
  цикле (в отличие от `build_volume:56-58`) — после фикса `build_section` это
  безопасно (секция даст 0 байт), но стоит проверить выравнивание/padding.
  Дополнительно: `remove` не создаёт артефакт (только `extract` создаёт) —
  ожидание пользователя не совпадает; задокументировать или добавить флаг
  auto-extract.
* [x] **TUI: дерево не обновляется после мутаций** — после
  remove/insert/replace/rebuild дерево stale (узел остаётся на экране).
  Контекст: `commands.rs:319-341` (remove) и др. ставят только `status_msg`,
  не re-fetch'ат `image_nodes_list`. Все ex-команды мутаций должны rebuild'ить
  дерево (как `open` в `commands.rs:133-146`).
* [x] **Proto: добавить `action` в `Node`** — клиенты (TUI/CLI/WebUI) не видят
  pending-операций (+/-/*/~). Контекст: `engine.proto:76` `message Node` —
  добавить `uint32 action = 8;`; `list_recursive` (`parser/image.rs:111`) уже
  имеет доступ к `node.action`.

### Stale-tree: in-memory дерево не синхронизируется с байтами после flush (issue II-bis, ревизия 2026-08-13)

> Findings расследования `remove 3/13` на реальном образе. Предыдущие пункты
> issue II закрыли байт-уровень (`build_*` скиплет Remove) и добавили `action`
> в proto — но узел **остаётся в выдаче `image_nodes_list`** (с маркером `-`)
> даже после `remove → build_image → flush_image → image_save`. Root cause:
> движок хранит **два представления**, которые расходятся и не
> синхронизируются:
>
> 1. **In-memory дерево** `Image.root` (кэш `rpc/server.rs:24` `images:
>    HashMap`). `ops::remove` (`ops.rs:66` `node.action = Action::Remove;`)
>    только **переключает флаг** — узел из дерева не удаляется.
> 2. **Байты на диске** — `flush_image` (`server.rs:31-66`) строит их через
>    `build_image` (скипает Remove, `builder/mod.rs:58,75,106`) и пишет на диск.
>    Дерево при этом **не трогает** — ни prune, ни re-parse.
>
> `image_nodes_list` (`server.rs:295`) читает представление №1 через
> `list_recursive` (`parser/image.rs:155-180`), которое выдаёт все узлы
> включая Remove-маркированные. `get_or_load_image` (`server.rs:68-97`) на
> кэш-хите отдаёт ту же закэшированную копию → маркер висит бесконечно.
> Рестарт TUI не помогает: кэш в **процессе движка**, не в TUI. Объясняет
> жалобу «удалил/пересобрал/сохранил — а секция на месте»: байты корректны
> (216→215 файлов, GUID удалён — проверено save+reopen), но `node list`
> показывает устаревшее дерево.
>
> Дизайн задумывался как «pending-операции видны» (см. TODO:281), но
> write-through на каждой мутации (commit `aa6ed56`) эту семантику нарушает:
> изменение уже на диске, но показывается как «запланированное».

* [ ] **ENGINE: prune Remove-узлов из дерева после успешного `build_image`**
  (вариант 2 из трёх рассмотренных). После того как `flush_image`
  (`server.rs:31-66`) успешно собрал и записал байты, удалить из
  `Image.root` все узлы с `action == Action::Remove` (рекурсивный splice
  children) и сбросить `Action::Rebuild`/`Replace` обратно в `NoAction` у
  задействованных предков — чтобы дерево отражало **применённое** состояние, а
  не pending. Контекст: почему не варианты 1/3 — №1 (re-parse после flush)
  дорог (~700мс на 16MB по `engine-debug-2.log:19`) и теряет pending-вид;
  №3 (фильтр в `list_recursive` + флаг в реквесте) перекладывает логику на
  клиентов и не чинит расход дерева↔байты. Вариант 2 дёшев (один проход по
  дереву), сохраняет дерево как source-of-truth и убирает ложный pending.
  Контрвариант для отдельных узлов внутри LZMA/GUID_DEFINED (issue IV) — prune
  делать **после** проверки, что билд реально их применил (иначе при барьере
  узел пропадёт из дерева, оставшись в байтах); до фикса issue IV prune
  ограничить узлами, чей вывод действительно попал в `build_image` (т.е. не
  внутри compression-барьера).
  Покрыть тестом: `remove` top-level файла/секции → `flush_image` →
  `image_nodes_list` не содержит узел (без reopen); проверить что повторный
  `build_image` на том же дереве идемпотентен (байты совпадают). Регрессия на
  `real_image_ops_remove_last_file` (должен остаться зелёным).

### rebuild/replace молча отменяют Remove (issue II-ter, ревизия 2026-08-13)

> Findings `engine-debug-3.log`. Последовательность `remove 3/13` (21:03:17,
> строка 15) → `rebuild 3/13` (21:03:39, строка 25) → `image_save` (строка 43)
> → `image_open HN-1.bin` (строка 49, files=**276** = как оригинал, без секции
> было бы 275). Секция осталась в сохранённых байтах не из-за stale-tree, а
> потому что `rebuild` **перезаписал** `Action::Remove` → `build_file`/`build_section`
> пошли по rebuild-ветке и пере-сериализовали узел (включили его в вывод) вместо
> скипа. Root cause: `ops::rebuild` (`ops.rs:105`) и `ops::replace`
> (`ops.rs:95`) ставят action **безусловно**, без проверки, был ли узел уже
> помечен Remove. Эмпирически подтверждено: `remove 3/13` → action=54; `rebuild
> 3/13` → action=**55**; save+reopen → files в FV[3]=216 (не 215), GUID
> 40BEAB40 present. Затронут также `insert` (Replace/Insert после Remove на том
> же пути). Отдельный баг от issue II/II-bis — там байт-уровень и stale-tree,
> тут — cancellation pending-операций.

* [x] **ENGINE: гейт `Action::Remove` в `ops::rebuild`** — закрыто для
  rebuild (no-op guard, фаза рекомпрессии): `remove` → `rebuild` того же
  пути → `action` остаётся `Remove` (не `Rebuild`) → `build_image` узел
  отсутствует. Для `replace` семантика «воскрешает узел» задокументирована
  в спеке рекомпрессии §6 (`2026-08-14-lzma-recompression-design.md`,
  матрица мутаций end-state) — опциональный явный отказ остаётся отдельным
  пунктом ниже.
* [ ] **ENGINE: явный отказ `ops::replace`/`insert` на Remove-узле** —
  опционально: отказ с ошибкой `OpsError`, если требуется явное
  подтверждение (по умолчанию replace «воскрешает» узел новым содержимым —
  спека рекомпрессии §6). Аналогично `insert` на тот же путь после Remove.
  Контекст: `Action` enum в `types.rs:23-29` (`NoAction=50, Replace=53,
  Remove=54, Rebuild=55`).

### Replace: конфликт режимов (issue III, ревизия 2026-08-13)

> Нажатие `r` вводит `Mode::Insert` с prefill `replace {path} --file `
> (`main.rs:82-87`). В Insert-режиме **все** клавиши идут в cmdline
> (`handle_command`, `main.rs:129-150`) → навигация по панелям отключена, выбрать
> артефакт нельзя. Prefill хардкодит `--file`, даже когда нужен `--artifact-id`.
> Hotkeys `i/r/d` работают только при `focus == Tree` (`main.rs:73-74`).

* [ ] **TUI: конфликт режимов при replace** — выбрать источник замены
  (артефакт) не выходя из Insert-режима невозможно. Варианты решения:

  | Вариант | Описание | Плюс | Минус |
  |---------|----------|------|-------|
  | **A. Smart prefill** | При `r`/`i`: если Registry-курсор на артефакте → prefill `--artifact-id <id>`; иначе `--file ` | Минимум переключений; использует уже выбранное | Требует проверки типа registry-строки (Image vs Artifact) |
  | **B. Auto-resolve на Enter** | На Enter: если cmdline имеет пустой `--file` и Registry курсор на артефакте — подставить `--artifact-id` | Не ломает существующий flow | Неинтуитивно (магия); промах если Registry на image |
  | **C. Раздельные hotkeys** | `r` = replace-from-file, `R` (Shift-R) = replace-from-registry (берёт активный артефакт без cmdline) | Явный, быстрый | Две кнопки запоминать |
  | **D. Guided replace** | `r` помечает target → модальный промпт «source: [f]ile / [a]rtifact» → подсвечивает соответствующую панель | Самый интуитивный | Сложнее реализовать (модал/новый режим) |

  **Рекомендация: A + D.** Вариант A решает 90% кейса минимальным кодом (в
  `handle_normal` перед `enter_insert_mode` проверить `current_registry_row()` —
  если `RegistryRow::Artifact(i)` и артефакт существует, prefill
  `--artifact-id {id}`). Вариант D — целевой UX-апгрейд на следующий цикл
  (guided-промпт после выбора target). B и C — альтернативы, если A+D неприемлемы.
  Дополнительно к любому варианту: в Insert-режиме разрешить `Tab`/`Ctrl-L` для
  смены фокуса (выбор артефакта не выходя из режима).

### Registry: TUI обрезает UUID образов/артефактов (issue VI, ревизия 2026-08-14)

> `short()` (`crates/uefi-tui/src/ui/registry.rs:80-82`) урезает `image_id` и
> `artifact_id` до 8 символов (`id.chars().take(8).collect()`). Применяется в
> рендере Images (`registry.rs:34`) и Artifacts (`registry.rs:44`). Полный
> UUID (36 символов) нигде не показывается → пользователь видит только
> префикс: `956ad394  NH-6.bin  16.0 MB` (образ), `eab517d5  whole  8.8 KB  3/13`
> (артефакт).
>
> **Impact:** ломаются пути через **ручной ввод** ID. `:image switch <id>`
> (`commands.rs:373`) и `:insert/replace --artifact-id <id>`
> (`commands.rs:69`, `parse_node_cmd_args`) отправляют ID в движок as-is;
> движок ищет по полному UUID → введённый 8-символьный префикс не найдёт.
> Путь через **Enter на строке Registry** (`main.rs:157,169`) берёт полный ID
> из модели данных (не из рендер-строки) → работает; баг затрагивает только
> display + ручной ввод ID в cmdline.

* [ ] **TUI: показывать полный UUID в Registry** — убрать `short()` для ID
  (или truncation с раскрытием полного значения при выборе строки — в details/
  status). Контекст: `crates/uefi-tui/src/ui/registry.rs:34,44,80-82`. Узкая
  панель может не вместить 36 символов — рассмотреть двухстрочный рендер для
  выбранной строки или вынос полного UUID в status-bar при `focus == Registry`.
* [ ] **TUI: copy-to-clipboard ID из Registry** — даже с полным отображением
  набирать 36 символов вручную в `:image switch`/`--artifact-id` неудобно.
  Добавить `y`/Enter-вариант для копирования `image_id`/`artifact_id` в
  буфер (и/или вставку в cmdline). Снимает зависимость от ручного ввода.

### Compression barrier: мутации внутри LZMA/GUID_DEFINED (issue IV, ревизия 2026-08-13)

> Symptoms (репрод на `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`):
> `:remove 1/13/2/1` в TUI → маркер красный (-), каскад `~` на предках.
> `:rebuild` → маркер стал жёлтым (~). Удалённая секция осталась на месте.
> После рестарта TUI дерево без изменений (секция вернулась). Вопрос: «у нас же
> write-through на мутациях секций?» — да, но он ineffective для этого кейса.

#### Root cause

Таргет `1/13/2/1` — UI-секция **внутри GUID_DEFINED (LZMA)-обёртки**. Цепочка
предков (по реальному образу):

```
[1] Volume (216 files)
[2] File   subtype=0x07, 3 секции
[3] Section subtype=GUID_DEFINED  ← compression barrier
[4] Section subtype=UI            ← target 1/13/2/1
```

`build_section` (`crates/uefi-engine/src/builder/mod.rs:103-109`) для
COMPRESSED/GUID_DEFINED-секций emits оригинальный `body` **дословно** и **не**
ре-сериализует распакованные children:

```rust
fn build_section(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.action == Action::Remove { return Ok(()); }   // фикс issue II
    if node.action == Action::NoAction || is_compressed_or_guided(node) {
        out.extend_from_slice(&node.header);   // сжатый блоб — как есть
        out.extend_from_slice(&node.body);     // children полностью игнорируются
        out.extend_from_slice(&node.tail);
        return Ok(());
    }
    ...  // rebuild-путь сюда для сжатых не доходит
}
```

Парсер для отображения секцию **распаковывает** и строит children
(`parser/section.rs:34-50` — `decompress_guided` / `decompress::decompress`),
но билдер обратно **не упаковывает**. Поэтому `Action::Remove` (и `Replace`/
`Insert`) на дочернем узле внутри сжатия не влияет на выходные байты. Барьер
глушит и `NoAction` (verbatim), и `Rebuild` (та же ветка verbatim — т.к.
`is_compressed_or_guided` стоит в условии раньше проверки action).

#### Evidence (интерактивное исследование на реальном образе)

remove(`1/13/2/1`) → `build_image` → re-parse:
- `before_build.len() == after_build.len()` (delta=0);
- `before_build != after_build` (байт-разница только от ре-компутации
  checksum/size родительского File — cosmetics);
- повторный парсинг rebuilt-образа: **target still present = true** (UI-секция
  физически осталась в сжатом блобе).

Write-through **срабатывает** на каждой мутации (`flush_image` в
`image_node_remove`, `rpc/server.rs:352` пишет байты на диск). Но записаны
**неизменные** байты сжатого блоба → удаление невидимо на диске → после
рестарта парсинг восстанавливает секцию из исходных байт. Для **несжатых**
секций фикс issue II работает корректно (см. тест `real_image_ops_remove_last_file`
— удаление файла верхнего уровня проходит и персистится).

#### Что можно и нельзя мутировать (reference-матрица)

> Эмпирически проверено на `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`
> (target `1/13` = FFS-файл; `1/13/2` = GUID_DEFINED/LZMA; `1/13/2/1` = UI
> внутри LZMA). Сигнал — `watch_present` после `remove/replace → build_image →
> re-parse` (длина/`bytes_changed` нерелевантны из-за issue V ниже).

| Мутация            | Вне сжатия            | Сама GUID_DEFINED/COMPRESSED (`1/13/2`) | Внутри блоба (`1/13/2/1`)      |
|--------------------|-----------------------|-----------------------------------------|--------------------------------|
| **Remove**         | ✅ работает           | ✅ работает (весь блоб удаляется)        | ❌ барьер (секция остаётся)     |
| **Replace body-only** | ✅ работает        | ⚠️ **корпортит** (stale header-size → re-parse падает, узел пропадает как битый) | ❌ барьер |
| **Replace whole**  | ⚠️ `parse_file`-семантика (заточено под FFS-файл, не секцию) | ⚠️ то же | ❌ барьер |
| **Insert**         | ✅ работает           | ❌ барьер (новый child выбрасывается)    | —                              |
| **Rebuild**        | cosmetic (recompute)  | ❌ no-op (verbatim)                      | ❌ no-op + перетирает Remove    |

**Вывод (ответ на пользовательский кейс):** удалить UI-секцию можно **только**
если она не внутри сжатия (на DXE-модулях реальных BIOS UI почти всегда внутри
LZMA → как правило нельзя). Удалить можно: весь FFS-файл (`1/13`), **или**
цельную несжатую секцию, **или** цельную сжатую секцию (`1/13/2` — дропает всё
содержимое блоба). Точечная мутация одной секции **внутри** блоба невозможна
без рекомпрессии. Replace body-only на самой сжатой секции — **небезопасен**
(не пересчитывает size в header).

#### Связанные баги (найдены при разборе)

* [x] **`ops::rebuild` перетирает `Action::Remove`** (`crates/uefi-engine/src/
  ops.rs:95-101`) — `node.action = Action::Rebuild` безусловно. Поэтому
  `:rebuild` по Remove-узлу отменяет удаление (маркер красный → жёлтый).
  Фикс: не даунгрейдить Remove (и иные pending-операции) до Rebuild — rebuild
  должен быть no-op для уже-Remove-узла (или возвращать ошибку «узел помечен на
  удаление»). Закрыто вместе с issue II-ter (фаза рекомпрессии).
* [ ] **`build_file`/`build_volume` впустую ре-сериализуют детей сжатой секции**
  — при Rebuild-каскаде build_file пересобирает body из children, но сжатый
  child всё равно выбросит verbatim-body; ре-компутация size/checksum на
  родителе делает байты `!=` (cosmetic), скрывая факт «ничего не изменилось».
  Это мешает диагностике (см. тафтологичный тест `write_through_persists...`
  из code-review находок).

#### Рекомендации по фиксу (выбрать объём)

| Вариант | Описание | Плюс | Минус |
|---------|----------|------|-------|
| **Минимум (честные ошибки)** | Билдер возвращает явную `BuilderError` при Remove/Replace/Insert внутрь COMPRESSED/GUID_DEFINED (детект: dirty-узел внутри `is_compressed_or_guided` предка). + починить `ops::rebuild` (не перетирать Remove). + документировать в user-facing help | Честность вместо silent no-op; мало кода; покрывается unit-тестами | Не решает задачу — редактировать сжатые секции всё ещё нельзя |
| **Полный (рекомпрессия)** | Билдер: при dirty-сжатой секции — собрать decompressed payload из children → compress (LZMA/Tiano через `decompress`-инфра) → пересобрать compression/guided header (размеры, dict-size, attributes) | Реально решает юзер-кейс | Большой объём, отдельный цикл, нужны фикстуры и тесты на реальном образе, риск рассинхрона с парсером |

**Рекомендация:** сначала **Минимум** (отдельный коммит/цикл) — перестать
молча глотать мутации внутри сжатия и починить `:rebuild`. Рекомпрессию
планировать отдельной спекой (baseline-коммит текущего `builder/mod.rs`), т.к.
это функциональный апгрейд, а не багфикс. Для `Replace body-only` на самой
сжатой секции (не на дочернем узле) барьер можно обойти уже сейчас —
зафиксировать как разрешённый кейс.

* [x] **Минимум: BuilderError на мутации внутри compression barrier** —
  покрыть тестом: `remove` секции `1/13/2/1`-вида → `build_image` возвращает
  ошибку (не silent-verbatim). Клиенту (CLI/TUI) показать человекочитаемое
  сообщение «cannot mutate inside compressed section without recompression».
  Закрыто фазой рекомпрессии: `BuilderError::RecompressionUnsupported`
  (Tiano/F86/COMPRESSION/unknown), LZMA — recompress.
* [x] **Минимум: `ops::rebuild` не перетирает `Action::Remove`** — unit-тест
  `rebuild_after_remove_keeps_remove`. Закрыто фазой рекомпрессии (no-op
  guard).
* [x] **Спека: рекомпрессия LZMA/GUID_DEFINED в билдере** — декомпозиция,
  референсы (`refs/UEFITool-ai-fork/common/ffsbuilder.cpp` — recompress path),
  тест-фикстуры на `HNX99TF_200525_original_E5C88C6F.bin`. Закрыто: спека
  `2026-08-14-lzma-recompression-design.md` + план
  `2026-08-14-lzma-recompression.md`.

### CRITICAL: Builder не round-tripит полный flash-образ (issue V, ревизия 2026-08-13)

> Severity: **high** — потенциальная порча данных / кирпич при прошивке.
> Влияет на **все** мутации на полном образе, независимо от issue IV.
>
> **Fixed** (вариант a — gap-aware). Дизайн `435e77a`, план `f924367`,
> реализация `918b6da` (parse_image captures non-FV bytes as Padding),
> регрессионные тесты `b3ca457`, safety-guard `flush_image` `95791b9` +
> `89cc606` (empty-output edge). Ревизия: `973209a` (dynamic Volume index
> in target test). Оставшиеся ограничения — в разделе «Known limitations:
> gap-aware round-trip» ниже.

#### Symptom

`build_image(parse_image(<полный 16MB образ>))` → **7.8MB** (а не 16MB). Замерено
на `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`: `build_image` даёт `7798784`
байта во **всех** кейсах (даже без мутаций). `bytes_changed=true` в матрице issue
IV — артефакт этой потери, не мутации; relевантный сигнал там только `watch_present`.

#### Root cause

`parse_image` (`crates/uefi-engine/src/parser/image.rs:12-72`) сканирует буфер по
`EFI_FVH_SIGNATURE` и кладёт в `root.children` **только** найденные FV (на
реальном образе — 3 тома: 0x800000, 0x890000, 0xda0000). Всё остальное — Intel
Flash Descriptor (первые 0x1000), ME-регион, межтомные gap'ы, padding в конце —
**не моделируется** как узел. `build_image` (`builder/mod.rs:18-22`) эммитит
только `root.children` (FV) → ~половина образа теряется.

Существующий тест `real_image_builder_round_trip` проверяет round-trip только
**отдельных FV-слайсов** (`parse_image(slice)`, где slice = один FV) — не полного
образа. Поэтому регрессия не ловилась.

#### Impact

`flush_image` (`rpc/server.rs:31-55`) на **любой** мутации (remove/insert/
replace/rebuild/form-visibility) вызывает `build_image` и пишет результат на диск
atomic_write. После первой мутации хранимый файл становится **обрезанным**
(7.8MB вместо 16MB). На рестарте `get_or_load_image` читает уже 7.8MB → дерево
меньше оригинала (нет ME/descriptor). Прошивка такого образа = потеря ME/IFD =
**кирпич**. Write-through «работает» механически, но персистит обрезанные байты.

#### Рекомендации по фиксу

* [x] **Спека: full-flash round-trip в билдере** — закрыто вариантом (a)
  gap-aware: `parse_image` создаёт `Padding`-узлы для non-FV диапазонов
  (IFD, ME, межтомные gap'и, trailing), билдер эммитит их дословно.
  Дизайн `435e77a`, план `f924367`, реализация `918b6da`. Исходные
  варианты (a)/(b) и референс `ffsbuilder.cpp` — в спеке
  `docs/superpowers/specs/2026-08-13-full-flash-round-trip-design.md`.
* [x] **Регрессионный тест:** закрыто — `real_image_full_flash_round_trip`
  (parse→build→len==orig + byte==orig) и `real_image_full_flash_repatch_stability`
  (parse→build→parse→build идемпотентен) в `crates/uefi-engine/tests/real_image.rs`
  с `#[ignore]`. Коммит `b3ca457`.
* [x] **Bugfix `flush_image`:** закрыт safety-guard'ом — `flush_image`
  сравнивает размер `build_image`-вывода с существующим файлом и при
  усечении (или 0 байт) возвращает `FailedPrecondition` вместо тихой
  порчи. Коммиты `95791b9` (size guard) + `89cc606` (drop
  `!bytes.is_empty()` bypass, test
  `flush_image_rejects_zero_byte_build_output`).

### Known limitations: gap-aware round-trip (после фикса issue V)

> Результаты ревизии gap-aware фикса. Фикс (issue V) устраняет усечку
> полного образа, но следующие ограничения остаются:

* [ ] **Volume-level Remove на full-flash** — `build_volume`
  (`builder/mod.rs:40-42`) эммитит 0 байт для `Action::Remove`. На полном
  flash-образе это сдвигает все последующие регионы → ломает IFD layout
  (absolute region boundaries). Future fix: при Remove Volume → emit
  erase-byte Padding того же размера (preserve total flash size). Пока:
  Volume-level Remove на полном flash-образе **опасен**, не использовать
  без ручной проверки output-байтов.
* [ ] **Padding node `Action::Remove` игнорируется** — `build_node`
  (`builder/mod.rs:34`) всегда эммитит `body` для Padding, не проверяя
  action. Safe для round-trip (нет data loss), но пользовательский intent
  (удалить padding) молча игнорируется. Low priority — Padding-манипуляция
  нестандартна.
* [ ] **ME/IFD регионы opaque** — captured как raw Padding bytes, не
  structured. Нет ME version display, нет IFD region labeling. Future:
  IFD parser upgrade (Padding → Region nodes with subtype), отдельный цикл.
  Референс: `refs/UEFITool-ai-fork/common/descriptor.cpp`,
  `refs/UEFITool-ai-fork/common/meparser.cpp`.
* [ ] **Positional `Target::Path` нестабилен на full-flash** — gap-capture
  сдвигает индексы Volume'ов (первый Volume теперь не `root.children[0]`,
  а после Padding-узлов IFD/ME). GUID-based targeting (`<guid>:<type>`,
  `<guid>/<index>`) остаётся стабильным и предпочтительным. Future fix:
  динамический поиск Volume по offset/GUID в path-resolver. Референс:
  `crates/uefi-engine/src/parser/target.rs`.
