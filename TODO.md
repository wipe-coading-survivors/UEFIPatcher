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

## Аппаратная валидация unhide + set-value на Huananzhi X99-TF (2026-09-02..03): ЗАВЕРШЕНА УСПЕХОМ (E12 unlock, E14 значение)

> Итог (E12, 2026-09-03 02:43): конвейер «парсинг → адресная правка →
> liblzma-рекомпрессия → вписывание в слот → прошивка» валидирован на
> живой плате: скрытая страница PCI + Above 4G Decoding раскрыты,
> значения меняются и сохраняются в NVRAM после перезагрузки.
> Полный отчёт с побайтовыми уликами: `docs/reports/2026-09-02-hw-validation-hnx99tf.md`.
> Образы-эксперименты E1–E12 и пробники — `refs/amibcp/` (вне репо).

### Критические находки (источник цикла фиксов)

* [x] **ENGINE: LZMA-поток lzma-rs валит плату (видеописк)** — E1
  (рекомпрессия lzma-rs: props 0x5D, dict 8 МиБ, файл вырос 3x, последующие
  файлы FV2 сдвинуты) → POST идёт, но «нет видеокарты» (звуковой код).
  Контрольные: E6 (чистый сдвиг +0x2C0A8 pad-файлом, контент нетронут) —
  грузится; E3/E7 (liblzma: props 0x00, dict 16 МиБ; E7 = движковая
  раскладка с ростом) — грузятся, видео есть. Вывод: сдвиг/рост/паддинги
  невиновны, виноват класс потока lzma-rs. Фикс: замена `compress_lzma`
  (compress.rs) на параметризуемый кодировщик liblzma-класса — LZMA1
  alone, size в заголовке, свип pb/lc (pb0/lc0 сжимает лучше AMI: файл
  0x1611B против родных 0x16279 и влезает в исходный слот 0x161a1).
  Фикс-цикл: кодировщик liblzma + slot-fit готовы (real-image (c)/(e) зелёные).
  **HW-подтверждено: E8 загрузился (POST/видео ✓) — закрыто.**
* [x] **ENGINE: пустые подписи раскрытой формы — PRC-теория
  ФАЛЬСИФИЦИРОВАНА на E8 (2026-09-03)** — вставка UPG-токенов в
  x-UEFI-AMI ничего не изменила: подписи/значения по-прежнему пустые,
  help рендерится (у help-строк токенов никогда нет). Побайтовый разбор
  (§9 отчёта): строки есть/уникальны/без коллизий, SDP-записи AMITSE
  (FE612B72) полны (prompt id прямо в записи, access@+16=0x09 как у
  рабочих), varstore покрывает, REF идентичен рабочему, таблиц страниц
  нет. Эталон сообщества (UEFI-Editor: access@+16 + failsafe@+52 +
  optimal@+53 + suppress-removal) уже покрыт/совпадает. Остаточные
  гипотезы: форма-специфичный рендер-путь vs уровень вопросов.
  Дискриминатор E9 прошит (2026-09-03): подписи пустые ДАЖЕ в рабочей
  форме 896 (help рендерится, меняется по рядам) — причина следует за
  вопросами, не за формой. Гипотеза уровня вопроса: вендорская аномалия
  (сравнить с «нормальной» блокировкой — E10, Above 4G Decoding).
  UPG-механику из `set_item_visibility` по итогам либо удалить, либо
  оставить опцией.
  Закрыто мини-циклом unlock-op: UPG/PRC-механика удалена полностью
  (не опция) — `c8998cb` (patch-путь) + `f39f6a0` (хвост).
* [x] **hii string list отдаёт только первый string-пакет** — на HNX99TF
  16 строк вместо ~5.7k: walker останавливается на первом непустом пакете
  (известный minors «found-флаг»), а на этом образе в первом же HII-файле
  мелкий пакет. PRC-патчинг и полная карта строк требуют полного обхода
  всех пакетов всех файлов. Фикс-цикл: полный обход готов (real-image (a):
  >5k строк), закрытие после E8-ревью Setup.
  Закрыто циклом 2026-09-02 (коммит `883ca14` — полный обход всех
  пакетов bare+resource); E8-ревью Setup состоялось 2026-09-03 (PRC
  фальсифицирован, пункт выше). Реальный прогон 2026-09-03:
  `real_image_string_list_full_traversal` зелёный.

### Находки разбора E8–E11 (2026-09-03, побайтово)

* **E10/E11 (Above 4G Decoding)**: Setup 899407D7, форма 10029 «PCI
  Subsystem Settings». Два слоя гейта: (1) REF→10029 в форме 10002
  «Advanced» обёрнут `SUPPRESS_IF` со строковым выражением (опкоды 0x45)
  — страница скрыта из меню; (2) каждый вопрос страницы — личный
  `GRAYOUT_IF(qid 0x009A==1)`, 0x009A — невидимый NUMERIC (prompt=0/
  help=0). E10 снял только слой (2) — страница не появилась; **E11**
  снимает оба length-preserving-преобразованиями движка
  (`refs/amibcp/e11-pci-page-above4g.bin`, sha256 92fa4ebb…). Токен
  419='PCIS006' — неймспейс PCIS### тулинга.
* **E9 зависание (вход в форму 896) → SDP-указатели**: в Table-3
  setupdata зашиты АБСОЛЮТНЫЕ смещения IFR-опкодов вопросов (поле
  `d54e0200` q1=0x024ED5 — ровно позиция ONE_OF). Перенос вопросов на
  +0x180 (E9) оставил записи протухшими → браузер виснет на разборе
  страницы. Движковый unsuppress (E7/E8) — length-preserving, ±2
  толерантно (страница 901 открывалась). E9 не прошивать; relocate без
  патча SDP запрещён.
* **E11 виснет на входе в Setup, отменён**: непроверенный паттерн
  «пустой скоуп + stray-опкоды» (UINT64/EQUAL как операторы) роняет
  AMITSE на setup-модуле. Правило усилено: в setup-модуле — только
  флипы данных внутри выражений (E12: 3 байта: `1→2` в
  `UINT64(1)==UINT64(1)` always-true suppress страницы; `1→0xFFFF` в
  `EQ_ID_VAL(0x009A==1)` grayout 4G; `refs/amibcp/e12-above4g-dataflip.bin`,
  sha256 506f068b…). Декод: UINT64=0x45, EQUAL=0x2F (r-efi).
* **ENGINE-задача [валидировано на железе, E12]: вопрос-level unlock** —
  операция «unlock вопроса/страницы» = найти скоуп suppress/grayout
  вокруг вопроса или REF→форму и заменить литерал в выражении на
  недостижимый (`1→2`, `1→0xFFFF`), БЕЗ структурных правок (класс,
  проверенный E12; значения сохраняются в NVRAM). Кандидат в цикл 6;
  UPG-токен-механика set_item_visibility — кандидат на удаление
  (доказано: на рендер не влияет). Декомпозиция — секция
  «Мини-цикл „unlock-op“» ниже.
* **Правила IFR-правок длин**: при изменении длины form-пакета обновлять
  u24-length пакета и u32 total списка; длину PE сохранять (паддинг в
  .rsrc, движковый shrink-паттерн fc0977c) — иначе сдвигается .reloc.

* **x-UEFI-AMI — тулинг, не рантайм**: значения пакета = регистровые имена
  Intel (IIO/PCH/PRC/QPI/MEM/ME0/EVL/OC0/CMRF), id1 = «AmiMappingLanguage»;
  токены есть только у промптов (не help/не опции) видимых вопросов.
  Побочный продукт: 274 PRC-токена продублированы utf16 в данных PE за
  пределами .rsrc (кластер @dec+0x99b87).
* **Раскладка SDP setupdata (UEFI-Editor regex)**: записи с qid@0,
  pageId@+12, access@+16 (байт), help-id@+20, prompt-id@+48;
  failsafe@+52, optimal@+53. Наша прежняя интерпретация «108-байтных
  записей с failsafe@104/optimal@106» — те же поля в hex-чарах.
* **IFR-смещения калиброваны по r-efi** (IfrQuestionHeader):
  prompt@2/help@4/qid@6/varstore_id@8/var_offset@10/flags@12; suppress-if
  формы 901 в ORIG обёрнут СНАРУЖИ формы (открывается за 14 байт до FORM,
  END после END формы). Для движка: проверить, что ifr.rs-валкеры
  используют именно эти смещения (test-покрытие форм-уровневых скоупов).
* **B1DA0ADF = браузер Setup (PE, «Aptio Setup Utility»)**, содержит
  собственный package-лист (en-US 163 + x-UEFI-AMI 1 = «x-AMI», регистрация
  языка). Кандидат для дизасма при пустом E9.

### Мини-цикл «unlock-op» (engine): unlock вопроса/страницы — завершён (2026-09-03)

> **Спека и план готовы** (2026-09-03): `docs/superpowers/specs/2026-09-03-hii-unlock-op-design.md`
> + `docs/superpowers/plans/2026-09-03-hii-unlock-op.md`. Решения брейнсторма:
> u2 — строго E12-классы флипов (TRUE→FALSE отложен); u3 — **полное удаление**
> UPG/PRC (фальсифицирован на E8, не опция); API — два RPC (`HiiGatesList`
> чтение + `HiiUnlock` правка) и четыре CLI-команды (`hii form|question
> gates|unlock`); item_id-грамматика `#<form_id_ifr>[:<qid>]` (qid — dec или
> 0x-hex).
>
> **Цикл завершён** (2026-09-03): u1–u5 закрыты (коммиты в пунктах ниже);
> real-image приёмка — byte-exact против эталона E12, 20/20 `#[ignore]`-
> тестов `real_image.rs` зелёные (включая `real_image_hii_unlock_matches_e12`).
> Отложенные миноры ревью — секция ниже. HW-кандидат E13 не собирался —
> по желанию пользователя, после ревью дифа.

> Основание: E12 — единственный класс правок, валидированный на железе
> для setup-модуля (флипы литералов в гейтящих выражениях, БЕЗ структурных
> изменений; значения сохраняются в NVRAM). Порядок работ — superpowers
> (brainstorm → spec → plan в `docs/superpowers/`), реализация по плану.
> Эталон для гейтов: `refs/amibcp/e12-above4g-dataflip.bin`
> (sha256 506f068b…).

* [x] **u1. Карта гейтинга (только чтение)** — для цели (REF→form_id или
  question qid) находить объемлющие SUPPRESS_IF/GRAYOUT_IF-скоупы и
  декодировать выражения (UINT64=0x45, EQUAL=0x2F, EQ_ID_VAL=`[12 06]`
  qid@+2/val@+4 — r-efi). Двухслойная вендорская схема HNX99TF:
  always-true suppress на REF (`UINT64(1)==UINT64(1)` в Advanced) +
  персональные grayout вопросов (`EQ_ID_VAL(0x009A==1)`, где 0x009A —
  невидимый NUMERIC-мастер-переключатель). Выход — отчёт «что гейтит
  цель и каким выражением», без правок.
  Закрыто: expression-декодер E12-классов (`1bf9d9f`) + grammar-aware
  walker с vendor scope-bit quirk (`258cd57`); scoped-операнды с
  END-терминаторами (AMI quirk, находка приёмки u4) — `0f9d554`.
* [x] **u2. Операция unlock (правка)** — флип литерала на недостижимый:
  `1→2` в always-true suppress (`1==2`=false), `1→0xFFFF` в EQ_ID_VAL
  (недостижимо для 1-байтового значения). Инварианты: длины
  IFR-пакета/списка/PE неизменны, опкоды и смещения нетронуты; отказ с
  диагностикой, если выражение не сводится к распознанному безопасному
  классу. Запрещённое — пустые скоупы/stray-опкоды (E11 — виснет),
  relocate вопросов (E9 — SDP-смещения).
  Закрыто: literal-flip planner с width-guard (`670db68`) + движковая
  операция gates_list/unlock — атомарные E12-флипы (`8453e1a`); RPC
  `HiiGatesList`/`HiiUnlock` (`11a9274`).
* [x] **u3. Ревизия UPG/PRC-механики** — теория PRC-токенов фальсифицирована
  (E8: help рендерится без токенов; x-UEFI-AMI — AMI-тулинг, не рантайм).
  Убрать автоматический PRC-патч из пути unlock — вынести опцией либо
  удалить `plan_prc_entries`/UPG-ветку `set_item_visibility`.
  Закрыто: выбрано полное удаление (не опция) — `c8998cb` (UPG/PRC
  patch-путь) + `f39f6a0` (хвост: unconstructable `SibtBlockUnsupported`).
* [x] **u4. Real-image gate** — `tests/real_image.rs`: unlock страницы PCI
  (form 10029) + «Above 4G Decoding» на HNX99TF; контроль — байт-диф
  декомпрессата против эталона E12 (ровно 3 байта: pkg+0x67A `1→2`,
  pkg+0xDD1..DD2 `1→0xFFFF`), slot-fit (0x63BF ≤ 0x6491, lc2/pb0),
  round-trip.
  Закрыто: `7493900` — диф декомпрессата byte-exact против эталона E12,
  slot-fit, round-trip; 20/20 `#[ignore]`-тестов зелёные. Приёмка вскрыла
  AMI-quirk scoped-операндов — фикс декодера/walker'а `0f9d554`.
* [x] **u5. CLI** — команды вида `hii form unlock` / `hii question unlock`
  (имена и семантика — в спеке); вывод: найденные скоупы и изменённые
  байты/выражения; smoke на живом образе (`hii form list` после unlock —
  цель видна). HW-подтверждение кандидата (E13) — по желанию, после
  ревью дифа.
  Закрыто: `80f37ed` — `hii form gates`/`hii form unlock`/`hii question
  gates`/`hii question unlock` (+ тесты на mock-сервере). E13 не
  собирался — по желанию пользователя.

### Находки ревью мини-цикла unlock-op (2026-09-03)

> Отложенные minors из per-task и финального ревью цикла (ветка
> `fix/cycle6-reimplent`; u1–u5 закрыты выше).

* [ ] **hii/gates: TRUE→FALSE-класс флипов** — флип всегда-истинных
  выражений (напр. `GateExpr::True`-гейт) не реализован: planner
  отказывает как unflippable вместо перепланировки в false. Контекст:
  решение брейнсторма u2 — строго E12-классы (hardware-validated);
  расширять при живом прецеденте.
* [ ] **hii/ifr: quirk 0x8a не перенесён в легаси-walker'ы** — gates-walker
  маскирует vendor scope-bit (`len & 0x7F`, 0x0A→0x8A), легаси-обходчики
  `ifr.rs` — нет: на setup-модуле HNX99TF путь `set_item_visibility`
  (unsuppress) даёт no-op. Контекст: выровнять при следующем касании
  легаси-путей.
* [ ] **FormInfo: поле «gated» для REF-гейтнутых форм** — форма, скрытая
  suppress-ом на REF из родительской формы, не видна в `hii form list`
  как гейтнутая (`visible` покрывает только собственные suppress-скоупы
  формы). Контекст: клиенты TUI/WebUI при подключении; CLI-путь —
  `hii form gates`.
* [ ] **set_item_visibility: error-precedence** — malformed item_id в
  Read-режиме даёт NotFound вместо NotWritable (parse item_id идёт до
  проверки режима в `resolve_writable_path`). Контекст: `hii/mod.rs`;
  проверять режим до парсинга при следующем касании.
* [ ] **hii/gates: decoder допускает ровно один мусорный байт после
  валидного префикса операндов** — exact-consumption check снят ради
  END-quirk (scoped expression operands). Контекст: захватить
  ограничение тестом; ужесточить, если появятся новые quirk'и.
* [x] **hii/gates: EqIdVal re-unlock — user-visible дефект отчётности** —
  повторный unlock уже-0xFFFF гейта планирует no-op флип (from==to==`ff ff`),
  который попадает в applied (CLI печатает «applied … ff ff -> ff ff»), а
  gates_list продолжает отдавать flippable=true; байты не трогаются.
  Исправлено в `1711de9` (guard `value != 0xFFFF` в `plan_flip`, симметрично
  отказу EqConst при `a != b`; спека §4.2 и план Task 3/8 актуализированы в
  `01360e3`; реальный образ подтверждает: после unlock `flippable=false`).
* [ ] **hii/gates: plan_flip/plan_gates/apply_flips без bounds-guard'ов на
  hand-constructed Gate** — безопасность держится на инвариантах
  decode_expr (expr_offset/expr_end из живого обхода). Контекст: ввод
  уже wired, но приходит через guarded `find_gates`; захарденить при
  появлении иных источников Gate.
* [ ] **rpc/server hii_unlock: get_or_load_image-клон используется только
  для touch** — полный clone образа берётся ради `img.session_id`
  (`sm.touch`), мутация идёт через повторный lock `images.get_mut`.
  Контекст: наследованный стиль хендлеров (как `hii_form_add`);
  упростить при следующем касании (доставать session_id без клона).
* [ ] **hii/gates: тест обрезанного пакета ассертит `gates.len() <= 1`** —
  слабый предикат: silent under-walk (0 гейтов) тоже пройдёт. Контекст:
  закрипить точное ожидаемое количество/содержимое.
* [ ] **uefi-cli e2e: TSV-инвокация `hii form gates` ассертит только exit
  success** — содержимое колонок TSV не проверяется. Контекст: добавить
  content-ассерты stdout.
* [ ] **hii: тест `gates_list_bare_channel…` выводит ожидаемый офсет флипа
  из тестируемого `scope_offset`** — точные офсеты закриплены только в
  unit-тестах gates.rs. Контекст: захардкодить ожидание в интеграционном
  тесте.
* [ ] **hii unlock/gates_list: non-HII канал — Ok с пустым списком вместо
  NotASetupItem** — расхождение с таблицей ошибок §5 спеки
  (plan-sanctioned `unwrap_or_default` PE-resource-канала); безвредный
  no-op success. Контекст: задокументировать расхождение либо вернуть
  NotASetupItem.
* [ ] **CLI: несуществующий item_id в gates/unlock отдаёт
  `RPC_NOT_FOUND`** — найдено при ревизии u1–u5 (§13 отчёта): ошибка
  «цель не найдена» маппится в код `RPC_NOT_FOUND`, который читается
  как «метод RPC не поддерживается». Контекст: hii_error_status /
  отдельный код NotFound для HII-таргетов.
* [ ] **engine: дефолт сокета `/run/uefipatcher.sock` недоступен без
  root** — `bin/engine.rs:38` дефолтит на `/run` (требует прав даже на
  bind; без systemd-юнита, создающего сокет/каталог, демон не
  стартует), при этом AGENTS.md документирует дефолт
  `~/.local/state/uefipatcher/uefipatcher.sock`. Найдено при сборке E15
  (§14.1). Контекст: выровнять дефолт на AGENTS.md или
  `$XDG_RUNTIME_DIR/uefipatcher/uefipatcher.sock` с фолбэком.

### Мини-цикл «value-op» (engine): задать значение настройки — завершён (v1+v5, 2026-09-03)

> Ревизия u1–u5 — §13 отчёта: движковый unlock ≡ E12 побайтово на уровне
> декомпрессата, CLI-smoke на живом образе зелёный. Следующая цель —
> патчер задаёт значение, а не только открывает доступ (UI-путь записи
> валидирован ещё на E12: значение, изменённое в Setup, сохранилось в
> NVRAM после перезагрузки).
>
> **Цикл завершён** (2026-09-03): v1+v5 закрыты (коммиты в пунктах ниже);
> v2/v3 отложены (E13: инертны для посева первого бута; остаются
> кандидатами только для Load-Defaults-семантики меню Setup). Real-image
> приёмка — движковый set-value побайтово ≡ аппаратно-валидированному
> эталону E14, 22/22 `#[ignore]`-тестов `real_image.rs` зелёные (включая
> `real_image_hii_set_value_matches_e14`). Отложенные миноры ревью —
> секция ниже.
>
> Ключевой факт образа HNX99TF: переменного стора в прошивке НЕТ — во
> всём 16 МиБ нет сигнатур EVSA/$VSS/FTWS/FTWB; дескриптор: BIOS-регион
> 0x800000..0xFFFFFF, Data/PDATA-региона нет; FV3 0xDA0000..0x1000000
> содержит пустой (весь 0xFF) хвост 0xEA0000..0xFFD000 (~1.4 МиБ) — стор
> создаётся рантаймом при первом буте после прошивки. Следствия:
> (а) «пропатчить стор в образе» невозможно — путь исключён на этом
> образе; (б) каждая полная прошивка = посев дефолтов, значит
> образ-сторонняя запись значения = патч дефолтов — значение
> применяется само, без входа в Setup. Источник посева по итогам E13/E14:
> NVAR-стор «StdDefaults» (v5), а НЕ SDP/IFR-дефолты (v2/v3 — на посев
> не влияют, E13; остаются только для Load-Defaults-семантики меню Setup,
> не проверялось — низкий приоритет).

* [x] **v1. Экспозиция карты «вопрос → значение»** — RPC/CLI `hii
  question info`: varstore (GUID/имя/размер), var_offset, width,
  допустимые значения (OneOf-опции со значениями, Numeric
  min/max/step), текущие дефолты (IFR DEFAULT / DEFAULT-флаг опции).
  (замечание TODO «schema.rs уже читает varstore-карту» не
  соответствовало реальности — schema.rs это serde-модель входной
  схемы formset-add; карта читается новым walker'ом `hii/values.rs`,
  фиксация в спеке §1).
  Закрыто: values-walker карты вопросов/сторов (`09e9c83`, bounds-фикс
  `5437dc7`) + движковый `question_info` (`72c6576`) + RPC
  `HiiQuestionInfo` (`f03cb58`) + CLI `hii question info` (`add5d49`);
  real-image-гейт `bbddb5d`.
* [ ] **v2. Операция set-default (класс E12)** — флип значения в
  IFR DEFAULT-опкоде / перенос DEFAULT-флага на нужную OneOf-опцию:
  только данные, длины form-пакета/PE неизменны; валидация значения по
  v1-карте; отказ на нераспознанные классы (как в unlock).
  Пробник (4G, form-пакет Setup): ONE_OF qid 0x3B @pkg+0xDD3
  (prompt 0x01A3/help 0x01A4, varstore 1, var_offset 0x3A); опции
  Disabled=0 (строка 4, флаги 0x30=DEFAULT|MFG @pkg+0xDE8) и Enabled=1
  (строка 3, флаги 0x00 @pkg+0x0DEF) — IFR DEFAULT-опкода нет, дефолт
  живёт во флагах опций; флип = `0x30→0x00` + `0x00→0x30`.
  Отложено (E13: инертно для посева первого бута — флаги опций/IFR-дефолты
  посевом НЕ читаются; остаётся кандидатом только для Load-Defaults-
  семантики меню Setup).
* [ ] **v3. SDP setupdata: optimal/failsafe** — парсер Table-3 записей
  (qid@0, access@+16, failsafe@+52, optimal@+53) + флип байтов.
  УТОЧНЕНИЕ (пробник E13): setupdata НЕ plain-RAW — $PFS-стор лежит
  в LZMA-сжатой GUIDed-секции файла FE612B72 (HNX99TF: файл@0xA9C708,
  секция@0xA9C720, слот 0xC2AC, dec 0x7719C) — патчить
  decompress→flip→repack→slot-fit (механика движка уже валидирована на
  Setup-модуле). Пробник (4G): запись @dec+0x99EC (дискриминация по
  help@+20=0x01A4 / prompt@+48=0x01A3), access=0x09, failsafe@+52=0x00,
  optimal@+53=0x00 (Disabled); поле @+28 = 0x0DD3 — абсолютное IFR-смещение
  вопроса (третье независимое подтверждение правила E9; у мастера 0x9A
  @+28=0x252F ✓).
  Отложено (E13: инертно для посева первого бута — SDP optimal/failsafe
  посевом НЕ читаются; остаётся кандидатом только для Load-Defaults-
  семантики меню Setup).
* [x] **v4. Real-image gate + E13 (прошит — НЕ сработал)** — кандидат был
  `refs/amibcp/e13-unlock-plus-4g-default.bin` (unlock + IFR-флаги +
  SDP optimal/failsafe). Итог на железе: 4G [Disabled] — посев первого
  бута НЕ читает ни SDP optimal/failsafe, ни IFR-флаги опций. Реальный
  источник — см. v5. Real-image gate (диффы/round-trip/slot-fit обоих
  механизмов) — оставить для value-op.
* [x] **v5. NVAR «StdDefaults» — реальный источник посева дефолтов**
  [**валидировано на железе, E14** — чеклист пройден полностью: 4G
  поднялся [Enabled] сам при первом буте, пережил Save/ребут]:
  NVAR-стор с записями `StdDefaults`/`Setup` (данные «Setup» =
  114-байтовый образ дефолтов varstore 1) лежит ДВАЖДЫ: RAW-файл
  CEF5B9A3@0x800048 (FV0, данные @0x800088) и файл 9221315B@0xa77d28
  (FV2, LZMA-секция @0xa77d40, внутри RAW-секция, данные @dec+0x2C).
  Задачи движку: находить NVAR-сторы дефолтов (обе копии), сопоставлять
  записи с varstore-картой вопросов (v1), операция set-default =
  согласованный флип байта по var_offset в ОБОИХ копиях (FV0 — сырье;
  FV2 — decompress→flip→repack→slot-fit). Кандидат был
  `refs/amibcp/e14-stddefaults-4g.bin` (sha256 e58bc77b…).
  Закрыто: NVAR-reader (`ac1a707`, bounds-фикс `015e610`); движок
  `set_value` — согласованный флип обеих копий, no-op-копии
  пропускаются (`72c6576`, `1200d75`); RPC `HiiSetValue` (`f03cb58`);
  CLI `hii question set-value` (`add5d49`); real-image `bbddb5d` —
  диф ≡ эталону E14 (FV0 байт 0x8000C2, FV2 dec-диф ровно
  `[(0x66,0,1)]`), slot-fit, round-trip; 22/22 ignored-тестов зелёные.
* Вне скоупа: runtime-запись в живой NVRAM (setup_var-класс) — нужен
  UEFI-приложение-агент, не задача флеш-патчера.

### Первичный осмотр ASRock-образов: C275D4I3.20, 226D2IL3.30/.50 (2026-09-03)

> Новый валидационный материал вне Huananzhi: оба — AMI Aptio, 8 МиБ,
> БЕЗ Intel-дескриптора (обрезанные region-образы). Лежат в
> `PLAENING/refs/amibcp/`. Разбор — чат-сессия от 2026-09-03.

* [x] **движок открывает оба образа** (включая 226D2IL3.50, который не
  читает AMIBCP): полные деревья FV/файлов/секций (978/1128 узлов);
  «StdDefaults» CEF5B9A3 — первый файл первого FV в обоих (FV@0x200000
  у C275, FV@0x500000 у 226D2I), контейнер той же nested-NVAR
  архитектуры, что на HNX: C275 — «Setup» 215b + «IntelSetup» 597b +
  «ServerSetup» 445b + «NetConfigData»…; 226D2I — «Setup» 1206b +
  вторая малая «Setup» 195b. Механизм v5 (маппинг по имени varstore)
  переносим. Капсулы нет ни в одном (первые байты 0xFF, заголовка
  капсулы нет).
* [ ] **BLOCKER: Tiano-декомпрессия — заглушка** — 228 Compressed-секций
  (алгоритм 1) не разворачиваются (`decompress_tiano` -> Unsupported,
  `decompress.rs:23`), весь DXE-контент включая Setup/HII непрозрачен
  → `hii form list` пуст, вопросы/гейты/set-value недоступны на
  ASRock. Кандидат в мини-цикл «tiano-op»: порт TianoDecompress (LZ77+
  Huffman; рефы `../refs/UEFITool-ai-fork/common/decompressor.*`,
  edk2 TianoDecompressLib), сжатие для ребилда — опционально/потом
  (HNX-паттерн «декомпресс-ONLY + канонический слот» не переносится —
  тут слоты впритык). После этого проверить: вторая (сжатая) копия
  StdDefaults в образах, живость HII-канала, гейты на ASRock-вопросах.
  *Уточнение (2026-09-03, проверено оракулом):* все 228 секций C275 —
  **EFI-standard 4-bit** вариант (`EfiDecompress`, EFIPBIT=4);
  5-битный Tiano не нужен (0/228). Rust-крейт `efi-compress` 0.2.0
  (BSD-2-Clause-Patent, единственный на тему) на реальной секции валит
  ОБА варианта («bad Huffman table») — порт из C обязателен.
  Эталон собран и проверен: `refs/UEFITool-ai-fork/common/Tiano/
  EfiTianoDecompress.c` (единый декодер, параметр pbit 4/5) → 228/228.
  Фикстуры для TDD: `/tmp/c275_secs/` + оракул. Для записи на ASRock
  ещё нужен компрессор (`EfiTianoCompress.c` там же в refs; риск —
  слот-фит, EDK2-поток может выйти крупнее AMI-шного).
* [ ] **open-вопрос: почему AMIBCP не читает 226D2IL3.50** — формат
  идентичен читаемому C275 (те же заголовки, что и у 226D2IL3.30);
  наш парсер оба переваривает. Не блокирует нас, просто маркер
  «движок читает то, что AMIBCP не может» №2 (после StdDefaults-write).

### Первичный осмотр Lenovo RD450x «450x — копия.bin» (2026-09-03): ПОЛНАЯ поддержка из коробки

> Гиперскейлерский Lenovo RD450x (Purley), 16 МиБ, `PLAENING/refs/amibcp/`.
> Движок открыл, распарсил (1635 узлов, 1 нерелевантный отказ
> декомпрессии) и выполнил set-default БЕЗ единой правки кода.

* [x] **структура = класс HNX один в один**: валидный Intel-дескриптор;
  FV0@0x800000, StdDefaults CEF5B9A3 первым файлом; Setup-модуль —
  тот же GUID 899407D7-99FE-43D8-9A21-79EC328CAC21; varstore «Setup»
  тот же GUID EC87D643 (id 2, size 0x94); вторая копия StdDefaults —
  тот же файл 9221315B-30BB-46B5-813E-1B1BF4712BD3. Вывод: HNX99TF —
  производный Lenovo-референса Purley, «экзотика» HNX оказалась
  мейнстримом AMI.
* [x] **HII-канал живой**: 217 форм с именами; «PCI Subsystem
  Settings» = форма 10056, «Above 4G Decoding» = qid 0x5E (prompt
  str 108), offset 0x4A, опции 0/1.
* [x] **set-value 1 применён и сверен** (образ
  `refs/amibcp/450x-4g-default.bin`, sha256
  `d5cfabff8f6835ce6ec8af63630316c85ca12f37d9d3d26a569312f17b0029c6`):
  обе копии (raw store+0x72 → байт @0x8000D2 0→1; LZMA-секция
  @0xafb538, dec-дифф ровно 1 байт @0x76=0x2C+0x4A); flash-дифф =
  1 байт + слот 0xafb550..0xafb967, больше ничего.
* [ ] **мелочь: string-id коллизии между списками пакетов** — `hii
  string list` агрегирует строки разных package-list'ов, id уникальны
  только внутри списка (в 450x id 3/4 в списке UiApp = «Removable
  Drive»/«Hard Drive», а в опциях Setup-вопроса те же id читаются как
  Disabled/Enabled). Контекст: string list/list-scoping; проявилось
  при поиске «Above 4G» на 450x.

### E16: вставка формы на HNX — кандидат собран, ждёт железо (2026-09-03)

> Первый кандидат класса «вставка» (единственный класс правок без
> аппаратной валидации). Собран ЧИСТО от оригинала (без unlock/
> set-value — изоляция переменной).

* [x] **e16-form-add.bin собран через CLI** (`hii form add --target
  899407D7-…:0x10:0 --file e16-form.json`; схема — форма 10041 «UEFIPatcher
  Test Page» c Text «If you can read this, form insertion works» +
  Numeric в buffer-varstore 0x7F00 «PatcherVar» [никакого NVRAM],
  строки id 749–754). Образ `refs/amibcp/e16-form-add.bin`, sha256
  `d96ebc4cc707615a81c9db1963da7e3cc9f1852eda88c7712068b66ec5876180`.
* [x] **верификация**: 16 МиБ; flash-дифф = ОДИН регион
  0x8D1760..0x8D7BEF (~25 КБ) внутри слота LZMA-секции setup-модуля —
  рекомпрессия выросшего декомпрессата легла в прежний слот, размеры
  файла/секции/FV не сдвинуты; ре-парс: форма 10041 в списке,
  visible=true, заголовок резолвится.
* [ ] **чеклист на железе**: прошить → первый бут → Setup → искать
  «UEFIPatcher Test Page» (корневое меню и все категории; вопрос
  «Patcher Value» — buffer-store, значение не персистит, это норма).
  Видима → класс «вставка» валидирован. НЕ видима → TSE не показывает
  orphan-формы без REF из родителя; следующий шаг — REF-инъекция в
  корневую форму (ItemSchema::Ref уже есть в билдере) или formset_add
  отдельной категорией. Любой исход — данные.

**Результат E16 на железе (2026-09-03, протокол владельца): плата
загрузилась, Setup жив; наш пункт СУЩЕСТВУЕТ, но невидим и находится
между Advanced и Security — «на месте IntelRCSetup»: сам IntelRCSetup
стал недостижим (курсор упирается в «стену» с обеих сторон).**

Диагноз: страница попала в верхнюю цепочку страниц TSE, но без
page-метаданных — AMI TSE строит навигацию по $PFS setupdata
(AMITSESetupData) и AMITSE; формы без записей там неотображаемы и
НЕПРОХОДИМЫ (TSE не умеет их перепрыгнуть — отсюда «стена», накрывшая
и IntelRCSetup за ней). Коллизий form id нет (10041 свободен; на HNX 5
формсетов, IntelRCSetup = 932D37B0-0D4A-11E0-81E0-0800200C9A66).
Вывод: **вставка на уровне HII/прошивки валидирована** (плата
переварила IFR+строки, страница реально в page-tree), класс «вставка
TSE-видимой страницы» требует $PFS+AMITSE-интеграции.

### E17 → баг: patch_ami теряет записи при ребилде (2026-09-03) — ЗАКРЫТ мини-циклом ami-patch-op

* [x] **e17-formset-add.bin собран через `hii formset add`** (схема с
  setupdata_guid=FE612B72 (AMITSESetupData=$PFS) и
  amitse_guid=B1DA0ADF (AMITSE), host=899407D7, формсет 5C61E016
  «UEFIPatcher», форма 31249): строки+пакет добавлены в ресурс,
  образ пересобран, sha256 e1a4df53…cb79a84b. РЕАЛЬНЫЙ дифф: только
  слот setup-модуля 0x8D1760..0x8D7BEF — файлы AMITSESetupData и
  AMITSE байт-в-байт равны оригиналу (@0xa9c708/0xa780a8, размеры
  0xC2DC/0x23890). НЕ ПРОШИВАТЬ — эквивалент E16 без интеграции.
* [x] **BLOCKER-баг: `patch_ami` редактирует body ФАЙЛА, а builder
  пересобирает файл из header+children и выбрасывает правку**
  (`ami_patcher.rs:44-64`: `setupdata.body.extend_from_slice`,
  `amitse.body.splice` + `mark_rebuild_to_root_by_path` → ребилд
  затирает). Real-image тест formset_add проверял рост PE-ресурса, но
  НЕ проверял наличие AMI-записей в СОБРАННОМ образе — тест-пробел.
  Фикс (мини-цикл «ami-patch-op»): записи должны идти в payload-секцию
  — setupdata: RAW/$PFS-секция внутри файла, на HNX обёрнута в
  GUIDed-LZMA → decompress→append records→recompress slot-fit
  (механика = set_value flip + E13-рецепт); AMITSE: правка в PE32-секции
  (children), не в file body. Тест: после build_image записи ищутся в
  собранных байтах.
  Закрыто мини-циклом «ami-patch-op» (2026-09-03): спека
  `2026-09-03-ami-patch-op-design.md` + план + отчёт
  `docs/reports/2026-09-03-ami-patch-op.md`; коммиты `61b9c31..7f3308a`:
  payload-finders + усиленный precheck (`d27d455`), patch_ami →
  $SPF/PE32-секции (`82fe7e6`), LZMA slot-fit гейт (`cd5455c`),
  атомарность formset_add (`2b66f3c`), real-image гейт
  `real_image_hii_formset_add_ami_records_in_built_bytes` — записи в
  собранных байтах, дифф confined к трём Rebuild-файлам, 23/23 ignored
  зелёные (`7f3308a`).

### Мини-цикл «$PFS-формат + TSE-видимость» (кандидат, после ami-patch-op)

> Разведка $SPF-контейнера HNX99TF (побайтово) — в отчёте
> `docs/reports/2026-09-03-ami-patch-op.md` §3: сигнатура/хедер/таблицы,
  живые Table-3 записи = 72 байта (0x48) со стридом, раскладка полей
  (qid@0/page@12/access@16/help@20/ifr-off@28/prompt@48/fs@52/opt@53).

* [ ] **Writer SDP-записей → живой Table-3 формат (72 байта)** — текущий
  `make_ami_record` (108 байт, page@24/access@32/fs@104/opt@106) не
  читается AMITSE; нужны поля help-id/prompt-id/ifr-offset (источник —
  form-пакет/строки добавляемого формсета).
* [ ] **Парсер блочной структуры $SPF + регистрация записей** — аппенд в
  хвост контейнера не регистрирует записи в u32-таблицах (первый блок
  ~0x210); вставка в середину ломает абсолютные смещения (урок E9).
* [ ] **AMITSE form-id списки: семантика splice-позиции** — текущий маркер
  GUID[12..14] даёт ложные вхождения в PE32 (HNX: @pe+0x2561); где AMITSE
  хранит списки form-id формсетов — не разведано.
* [ ] **E18: HW-валидация «TSE-видимой страницы»** — вставка +
  зарегистрированные $SPF-записи (+ при необходимости REF-инъекция, вывод
  E16: без page-метаданных страницы невидимы и непроходимы).

### page-hijack-op: E18 прошит; разведка моста закрыта — таблица строк одна, следующий шаг hijack-v2/E19 (2026-09-04)

Реализована in-place схема захвата страницы (спека
`docs/superpowers/specs/2026-09-04-page-hijack-op-design.md`, план
`docs/superpowers/plans/2026-09-04-page-hijack-op.md`): same-length
правки IFR (title/prompt/help string-id) + fs/opt в 72-байтовых записях
$SPF, атомарный precheck, дифф confined к Setup+$SPF слотам. На образах
класса HNX99TF файл $SPF лежит за GUIDed-LZMA-обёрткой, которую
эвристики автодискавери не видят насквозь — передавать `--setupdata-guid
FE612B72-203C-47B1-8560-A66D946EB371` явно. PE32-resource-канал
(реальные Setup-модули) покрыт синтетическим регрессионным тестом и
real-image гейтом. Открытые вопросы закрываются экспериментом E18
(диагностическая матрица исходов — спека §6): строковый мост записей
(0x1A3 vs 0x6C), источник заголовка страницы, живость fs/opt. После E18:
выбрать страницу-жертву с пользователем, собрать e18-кандидат, прошить,
заполнить матрицу.

E18-кандидат собран 2026-09-04: жертва — форма 10029 «PCI Subsystem
Settings» (только она покрыта real-image гейтом), qid 59 one_of 0/1,
запись rec@0x99E8, сток fs/opt=0/0 → в образе 1/1; title «UEFIPATCHER
E18» (string 749), prompt 750, help 751. Артефакт
`refs/amibcp/e18-page-hijack.bin`; побайтовая верификация артефакта:
дифф 75271 байт строго внутри слотов Setup [0x8D1760,0x8D7BF8) и $SPF
[0xA99508,0xAA89E4), AMITSE и все остальные файлы байт-идентичны,
re-parse видит новый title.

E18 прошит (2026-09-04, отчёт
`docs/reports/2026-09-04-page-hijack-op-e18.md`): страница-жертва
**исчезла** из меню, наши тексты нигде не появились, остальные страницы
целы (вкл. правленная ранее HWPM — строки/хелпы/опции живы). Жёстче
строки 3 матрицы: IFR string-id вне строкового пространства TSE убивает
страницу — заголовки/тексты живут за строковым мостом §2.5 (0x1A3 vs
0x6C) и структурами страниц §2.3. Попутно hardware-подтверждено:
пересборка $SPF-слота движком (lzma-rs, slot-fit) загружается; строковые
аппенды не ломают рендер; AMIBCP-оракул на образы со строковыми
апендами валится («unsupported string blocks» — E16 так же) и для этого
класса неприменим. Мини-цикл закрыт; следующий — разведка строкового
моста (amitseSct / иная база нумерации) и семантики «страница →
контролы» (§2.3, стрид 0x40), затем аддитивная регистрация новой
страницы.

Разведка выполнена 2026-09-04 (отчёт
`docs/reports/2026-09-04-tse-bridge-recon.md`, побайтовый Python-разбор +
кросс-чек живым движком): **строкового моста не существует** — «108» был
ошибкой подсчёта в прошлой разведке; записи $SPF, A-поле страниц и IFR
индексируют один en-US строковый пакет Setup-модуля (419=«Above 4G
Decoding», 420=хелп, 395=«PCI Subsystem Settings»); второй таблицы в образе
нет (скан всех 208 LZMA-секций). Механизм E18: нарушен инвариант
согласованности IFR↔$SPF string-id (E18 поменял id только в IFR, $SPF
оставил старые; appended-id 749–751 сами по себе валидны). Инвариант
проверен массово: 34/34 записей s30/s14 == IFR prompt/help, 23/23 страниц
A-поле == IFR FormTitle, 0 расхождений. Карта $SPF: count страниц
u32@0x60=188, таблица 0x64–0x354 со свободным нулевым слотом @0x354,
пул структур 0x358–0x8288, записи 72B 0x8288–0x5AD20 (488 f8), записи
2-го класса ~124 КБ до 0x73F0C (не декодированы), хвосты до 0x77164;
свободного места внутри нет. Аддитивная вставка = аппенды в конец + слот
таблицы + bump count (без сдвигов). Следующий мини-цикл — «hijack-v2»
(E19): same-length правка ОБЕИХ сторон одними appended-id (IFR
title/prompt/help + $SPF title@+0xE, prompt@+0x30, help@+0x14) — ожидание:
страница выживает, тексты наши; только после него — «новая страница».

E19-кандидат собран питон-скриптом БЕЗ миницикла (2026-09-04,
`refs/amibcp/e19-hijack-v2-py.bin`, sha256
`dedc73232e8a5445a2d67568049dbe0c14f182dbca491a88b11318dbfcc9830e`,
скрипт /tmp/tse-bridge/build_e19.py): база — уже прошитый E18-артефакт
(строки 749/750/751 и IFR-правки уже в нём), добавлены ровно 3
same-length поля в $SPF-контейнер (title-id страницы 395→749, записи
q59: help 0x1A4→751, prompt 0x1A3→750) + пересжатие LZMA-слота
(props/dict оригинала, nice 64, поток 0xBDF0 ≤ слота 0xC2AC, slot-fit).
Проверки: контейнер отличается от оригинала ровно 8 байтами (6 E19 + 2
fs/opt от E18), дифф образа против E18 confined к $SPF-секции, re-extract
видит 749/750/751, UI-секция цела, FFS-attr 0x0 (data-checksum не
требуется). Дифференциальный эксперимент к прошитому E18: если страница
вернётся с текстами «UEFIPATCHER E18» — инвариант согласованности
подтверждён на железе; если исчезнет снова — инвариант шире пары
IFR↔запись (кандидаты: x-AMI пакет, кэш TSE) и нужен миницикл с
углублённой разведкой.

E19 прошит (2026-09-04, позднее): страница-жертва по-прежнему
отсутствует — **инвариант согласованности IFR↔$SPF опровергнут** (E18 с
разными id и E19 с одинаковыми 749/750/751 убивают страницу одинаково,
хотя все три id реально есть в en-US пакете). Дедукция: аппенды строк и
пересборка Setup невиновны (E16-класс); fs/opt — слабый кандидат (это
значения опций one_of, (1,1) живёт на 35 записях видимых страниц);
выжившая гипотеза — TSE резолвит id 749+ не через живой en-US пакет, а
через ограниченный источник; главный кандидат — x-AMI mapping-пакет
(второй STR-пакет списка, ids 1..748, аппендили только в en-US).
Попутно (область MPDT @0xA778A0): @0xa77d40 = сжатый NVAR-стор
«StdDefaults»/«Setup» (живой источник дефолтов E13–E15 найден в
образе); @0xa780c0 = сам AMITSE (299 КБ, копий строк формсетов нет —
amitseSct закрыт окончательно, блоб сохранён для разбора кода TSE).

E21-кандидат собран (`refs/amibcp/e21-xami-append.bin`, sha256
`764a0d7c…09a5c8a`): E19 + аппенд UPCH001/002/003 в x-AMI пакет (ids
749/750/751 в обоих пакетах; длины пакета/листа/ресурса/.rsrc
VirtualSize обновлены; дифф против E19 строго внутри LZMA-секции
Setup). Теперь IFR + $SPF + оба строковых пакета согласованы. Исходы:
страница вернулась с нашими текстами → валидация/рендер через x-AMI,
hijack-v2 = движковая реализация; нет → снимок/лимит TSE, копать код
AMITSE и записи 2-го класса [0x5AD20..0x73F0C]. Доп. копий id
395/0x1A3/0x1A4 в контейнере нет (пул-«хиты» — старшие половины
u32-указателей).

E21 прошит: пункты меню по-прежнему отсутствуют (по сообщению
пользователя — пункты, мн.ч., т.е. не лучше E18/E19, возможно хуже) —
семейство гипотез «резолв через x-AMI/ограниченную таблицу» закрыто:
при согласованных id во ВСЕХ источниках (IFR, $SPF, en-US, x-AMI)
страница мертва. Аппенды в x-AMI больше не трогать (риск расширения
ущерба). Побочный аудит (бесплатный, побайтово): Setup-модуль E18
отличается от оригинала ровно ожидаемым — 6 байт IFR (title blob+0xC98,
prompt/help blob+0xDE9), 3 аппенднутые SCSU-строки, поля длин
PE/ресурса; движка-багов нет, все прежние выводы стоят. Непроверенными
по-одному остались ровно две переменные E18: (1) правка IFR string-id,
(2) fs/opt 0/0→1/1.

По решению пользователя — поэтапный план («сначала вернуть пункт как
раньше, потом вставлять UEFIPATCHER»): (a) контрольный флеш оригинала —
страница должна вернуться (заодно снимает возможные хвосты E21);
(b) **E23 собран** (`refs/amibcp/e23-ifr-only.bin`, sha256
`865f55d6…97c5ef42`): E18 минус fs/opt (контейнер $SPF байт-идентичен
оригиналу, откат 2 байт + пересжатие слота, дифф против E18 confined к
$SPF-секции; Setup-модуль байт-идентичен E18 — IFR 749/750/751 и
строки на месте). Единственная семантическая переменная против
оригинала — правка IFR string-id (аппенды строк невиновны по E16).
Исходы: страница жива с текстами «UEFIPATCHER E18» → убийца fs/opt,
рецепт hijack = IFR+строки, миницикл в движке; страница мертва →
убийца правка IFR-id → разбор кода AMITSE (блоб сохранён), слепые
прошивки прекращаем.

КОНТРОЛЬНЫЙ ФЛЕШ ОРИГИНАЛА (2026-09-04, вечером) — РАЗВОРОТ: страницы
PCI Subsystem Settings в стоке НЕТ. Жертва 10029 **сток-скрыта
вендором** (suppress-if), и «исчезновение страницы» в E18–E23 было
базовым состоянием, а НЕ эффектом наших правок — вывод E18 §4
(«TSE убивает страницу при нерезолве») **отклонён как построенный на
неверном базлайне** (жертва выбиралась по покрытию real-image гейтом,
не из живого меню оригинала). Поправка пользователя: AMIBCP никогда
не правил прошитые образы (только просмотр) — вся E-серия = правки
движка/питона. Unlock видимости = op движка, 3 байта (дифф
e13-engine-unlock против оригинала: $SPF — 0 байт, Setup — 3 байта,
два suppress-if): goto-пункт страницы в форме 10002 «Advanced»
EQ(qid 0x0A45, 1) → EQ(0x0A45, 2) [dec 0x8FB2]; вопрос q59
EQ(qid 0x9A, 1) → EQ(0x9A, 0xFFFF) [dec 0x9709-A]. Hardware-подтверждён
(E13/E15, скрин 2026-09-03 02-43-56).

**E24 собран** (`refs/amibcp/e24-unlock-hijack.bin`, sha256
`26bfd958…561c0e52`): E23 + 3 байта unlock (дифф против E23 confined к
Setup-секции; $SPF байт-идентичен оригиналу; IFR: title 749,
q59 750/751; строки на месте). Семантические дельты против оригинала:
unlock + строковые аппенды + IFR string-ids. Исходы: (a) страница
видна, заголовок/вопрос = «UEFIPATCHER E18»/наши → рендер идёт из
IFR-канала, hijack достигнут, миницикл в движке; (b) страница видна со
СТОК-текстами («PCI Subsystem Settings»/«Above 4G Decoding») → рендер
из $SPF-полей, тогда E25 = E24 + $SPF ids 749/750/751 (unlock уже в);
(c) страницы нет → правка IFR-id реально убивает страницу на корректном
базлайне → разбор кода AMITSE, слепые прошивки прекращаем.

E24 ПРОШИТ — ИСХОД (a), ЗАХВАТ СТРАНИЦЫ ДОСТИГНУТ (2026-09-04,
вечером): страница появилась (unlock работает поверх hijack-модуля),
вопрос q59 рендерит **наш** текст «E18 hijack probe (qid 59)» вместо
«Above 4G Decoding» → канал рендера вопросов = IFR string-id →
аппенднутые id → en-US пакет Setup-модуля, hardware-подтверждён
end-to-end. Загадка E18-серии закрыта окончательно: страницу никто не
«убивал» — она была сток-подавлена (suppress-if) во всех образах
E18–E23; ни правка IFR-id, ни fs/opt, ни $SPF-id, ни x-AMI на
видимость не влияют (в рамках наблюдавшихся состояний). Рецепт hijack:
unlock (3 байта suppress-if) + строковые аппенды + same-length правки
IFR string-id — ровно содержимое E24 ($SPF нетронут). Открытый
вопрос знаний: источник заголовка страницы/пункта меню (goto в форме
10002 несёт prompt-id 395, $SPF A-поле = 395, IFR FORM title = 749) —
уточняется по уже прошитому E24 (какой текст у пункта меню и в шапке
страницы), без новой прошивки. Следующий шаг — миницикл hijack-v2 в
движке: op «unlock + строки + IFR string-id» (+опционально $SPF-id),
спека/план по процессу.

E24, детали от пользователя (2026-09-04): (1) на странице виден только
вопрос q59 Enable/Disable — «раньше было ещё что-то»; (2) хелпы
оригинальные «про 4G». Объяснение: (1) suppress-if `EQ(var 0x9A, 1)`
вычисляется на рантайме по NVRAM-переменной — на E12-эпоху (скрин
02:43:56: вся страница — Latency Timers, Palette Snoop, PERR/SERR,
Above 4G, SR-IOV, BME + 2 сабменю) переменная была ≠1, сейчас =1;
e12/e15/e24 несут одинаковый unlock (только goto + q59), поэтому
остальные скрыты снова. Хелп-канал: IFR help-id=751 не рендерится →
**help вопроса берётся из $SPF-записи (s14@+0x14)**, prompt — из IFR
(асимметрия, hardware-факт). Итоговая карта каналов: видимость ← IFR
suppress-if (рантайм-переменные), prompt ← IFR prompt-id, help ← $SPF
s14, пункт меню ← goto prompt-id, заголовок страницы ← TBD (IFR FORM
title 749 vs $SPF A-поле 395).

**E25 собран** (`refs/amibcp/e25-fullpage-help.bin`, sha256
`6c9c84e5…34d21eaa9`): E24 + перевод ВСЕХ suppress-констант формы 10029
в «никогда» (9 шт. `EQ(0x9A,1)`→FFFF по образцу q59 + 1 шт.
`EQ(0x0A45,0)`→2 по образцу Advanced-goto; q59 и goto уже были) +
$SPF s14 записи q59 → 751 (хелп наш). Проверено: IFR — 8 вопросов
(q54–q61, у q59 750/751, title 749), контейнер отличается от
оригинала ровно 2 байтами (s14). Ожидание: полная страница как на
скрине 02:43:56, у q59 наш prompt И наш хелп; заодно закроется вопрос
источника заголовка (749 «UEFIPATCHER E18» в IFR против сток 395 в
$SPF/goto).

E25 прошит (скрин 18-09-15): страница **полная** (все 8 вопросов +
сабменю — unlock всех точек сработал), наш вопрос выделен. Открытые
детали: (1) у q59 попап = [Disabled]/[Enabled] — «раньше было три
варианта»; (2) хелп-панель показывает сток-текст 420, хотя IFR
help=751 и $SPF s14=751. По (1): IFR-блок опций q59 идентичен во всех
образах (оригинал, e10, e11, e12, E-серия) — ровно 2 опции (string
4=Disabled val 0 default, string 3=Enabled val 1); трёх опций нет нигде
в наших данных (вероятно, память по AMIBCP-виду или другой версии
BIOS); текущее = сток, ничего не сломано. По (2): третьей копии id в
$SPF не найдено (все «хиты» 419/420 при u16/u32-сканах — ложные:
старшие байты счётчиков/указателей; банк 0x4C-записей @0x10448+ и
stride-0x48-массив @0x40F50+ принадлежат другим формам/полям).
Рабочая гипотеза: **AMITSE кэширует $SPF-данные в NVRAM при первом
буте** (та же линия, что «мёртвые» fs/opt и StdDefaults): IFR/строки
рендерятся с флеша (наш prompt ✓), а $SPF-поля (help) — из кэша
оригинала (420). Бесплатная проверка на текущем E25: Load Optimized
Defaults / сброс CMOS → если хелп станет нашим (751) — кэш
подтверждён. Заголовок страницы на скрине не виден однозначно —
уточнить при случае. Мини-цикл hijack-v2: каналы unlock+строки+IFR
закрыты, help-канал — последний открытый вопрос (решается в цикле,
в т.ч. NVRAM-кэш-тестом и движковым диффом).

Кэш-гипотеза СНЯТА пользователем (2026-09-04): хелп сток-4G и после
джампера CMOS, и после Load Optimized Defaults → источник хелпа на
флеше. Найден главный кандидат: в пуле структур $SPF — **пара
рендер-контролов строк с u16 str-id: 419 @0x4FFE и 420 @0x5106**
(каждый с массивом 57 указателей на 0x4C-записи банка @0x40F50+;
соседние поля 1207/1208, 128/129 — последовательные индексы банка).
E25 правил запись s14, но не контрол. Попутные находки: банк 0x4C —
построчные записи (у пары @0x17784/0x177D4 первые поля = 419/420 —
идентификаторы строк, не qid); «запись»-близнец @0xE1A8 (qid 40,
ifr 0xA08, s14=420/s30=419) — вопрос другой формы на тех же строках.

**E26 собран** (`refs/amibcp/e26-help-control.bin`, sha256
`3c61030a…beab948fd`): E25 + 2 байта — u16 хелп-контрола
@0x5106: 420→751. Контейнер против оригинала = ровно 4 байта
(s14 записи + контрол), дифф образа против E25 confined к
$SPF-секции. Ожидание: хелп нашей строки = «UEFIPatcher E18: if you
read this, IFR string rewrite works». Если снова сток — оставшиеся
кандидаты: близнец @0xE1A8 и 0x4C-пара @0x17784/0x177D4; после E26
хелп-канал уводим в миницикл (слепые прошивки прекращаем — основные
каналы доказаны).

**E26 прошит — SUCCESS (2026-09-04)**: хелп-панель нашей строки
показывает наш текст («UEFIPatcher E18: if you read this, IFR string
rewrite works»). Хелп-канал = **$SPF пул-контрол строк u16@0x5106**
(пара контролов @0x4FF8/@0x5104 с u16 str-id @0x4FFE/@0x5106); s14
записи и IFR help-id на хелп НЕ влияют. Карта каналов рендера
финальная (все hardware-доказаны): видимость ← IFR suppress-if
(константы EQ→FFFF/2), prompt ← IFR prompt-id, **help ← $SPF
пул-контрол @0x5106**, опции ← сток IFR one_of (2 шт.), пункт меню ←
goto prompt-id, заголовок страницы ← TBD (единственный открытый
вопрос, не блокирует: уточним по скринам/в цикле). **Рецепт hijack
страницы полный: unlock suppress-констант + строковые аппенды en-US +
same-length IFR string-ids (title/prompt/help) + u16 хелп-контрола в
$SPF-пуле.** Асимметрия prompt(IFR)/help($SPF) — факт архитектуры
AMITSE на этой плате; для движковой op hijack-v2 это значит: правки
IFR недостаточно, нужен ещё точечный патч $SPF-пула (поиск контрола
по str-id). Серия слепых прошивок E19–E26 закрыта; дальше —
мини-цикл hijack-v2 в движке по полному рецепту.

**Заголовок страницы закрыт словами пользователя (без прошива)**:
шапка/пункт = «PCI Subsystem Settings» (сток 395), хелп-строка пункта
в меню = «PCI, PCI-X and PCI Express Settings» (сток goto help-id);
наш hijack виден только в вопросе Above 4G Decoding (prompt + help).
IFR FORM title (749) не отрендерился нигде → источник заголовка —
**$SPF title-id страницы@+0xE** (структура формы 10029 @0x694 в
контейнере, title-id=395; проверено re-extract'ом) и/или goto
prompt-id 395 (неотличимы при сток 395). Для op: если нужно
переименовать страницу/пункт — same-length патчи $SPF title-id@+0xE +
goto prompt/help-ids; IFR FORM title можно не трогать. **Карта
каналов рендера закрыта полностью (6/6).**

**Мини-цикл hijack-v2 запущен (2026-09-04)**: брейнсторм закрыт,
спека `docs/superpowers/specs/2026-09-04-hijack-v2-design.md`
(u1 авто-unlock жертвы в hijack; u2 удалить fs/opt+title; u3 gate +
флеш E27; подход — стадии в `hijack_form` + gates-слой напрямую,
$SPF — сигнатурный скан контролов). Попутная находка при ревью кода:

**E27 прошит (2026-09-05)**: страница открылась, вопрос q59 рендерит наш
prompt («UEFIPatcher E27: engine hijack v2»), но хелп — сток-4G.
Побайтовая статика (python, независимо от движка): образ валиден —
$SPF-контейнер = оригинал + ровно 2 байта (контрол @0x5106: 420→750),
Setup-модуль =unlock+IFR 749/750+строки, title/goto не тронуты. Сводка
по образцам: E25 (s14=751, контрол=420) → сток; E26 (s14=751,
контрол=751) → НАШ; E27 (s14=420, контрол=750) → сток. Вывод: канал
хелпа требует ОБЕ копии id — запись s14 @0x99ec И пул-контрол @0x5106;
карта каналов в спеке hijack-v2 была неполной (s14 ошибочно выброшен
вместе с мёртвыми fs/opt при чистке v2). Дискриминатор **E28 собран**
(`refs/amibcp/e28-help-record-s14.bin`, sha256
`b24953a68a43bec862bd827e1faebd5a88a0b45b31dda487d5636378f9974244`):
E27 + ровно 2 байта (s14: 420→750), контейнер vs оригинал = 4 байта
[0x5106, 0x99ec] (форма E26). Ожидание: хелп наш, всё остальное как
E27 (7 вопросов, сток-шапка). **E28 прошит — SUCCESS (2026-09-05)**: хелп нашей строки наш
(«UEFIPatcher E27: engine-built help channel»), prompt-строка наша,
остальное как E27. Гипотеза подтверждена: хелп-канал = ОБЕ копии id
(запись s14 @0x99ec + пул-контрол @0x5106). Карта каналов 6/6 закрыта.
Движковая правка v2.1: возврат стадии записи s14 (rule-11 docs →
задача → gate → движковый E29).
v2.1: стадия записи s14 возвращается в op (rule-11 docs + задача +
gate), финальный образ собирает движок.

**E29 собран движком (2026-09-05, Task 10 hijack-v2.1)** — первый
полностью движковый артефакт полного аппаратно-валидированного
рецепта (unlock + строки + IFR + пул-контрол + запись s14, всё одной
op `hijack`). Артефакт `refs/amibcp/e29-hijack-v2_1-engine.bin`
(НЕ коммитить, refs/ — данные), sha256
`e1da17c7c98fe939f8f81c809e272b5935fddb8613bb7671c9ec1df39f8bcb0a`.
Сборка чисто engine+CLI (сокет `/tmp/uefipatcher-e29.sock`):
`session init --name e29 --force` → `image open … --mode write` →
`hii form unlock 899407d7…:0x10:0#10029` (1 флип: хаб-REF
`1==1`→`1==2` @pkg+0x8fae) → 7× `hii question unlock …#10029:{54..60}`
(по 1 флипу `01 00 -> ff ff` на EqIdVal(0x9A,1)-grayout'ах, q61 не
трогали — составной гейт) → `hii form hijack --file
/tmp/e29-schema.json --setupdata-guid FE612B72-203C-47B1-8560-A66D946EB371`
→ `image save`. Контракт hijack: **string_ids=2** (prompt=749
«UEFIPatcher E29: engine hijack-v2.1», help=750 «UEFIPatcher E29:
engine-built full help channel»), **unlock_flips=0** (идемпотентность:
всё вскрыто шагами unlock), **help_controls=1** ($SPF str@0x5116:
1A4→2EE) И **help_records=1** (запись s14 rec@0x99E8: 1A4→2EE) —
обе копии хелп-id записаны движком. Верификация (throwaway-скрипт,
не в git): длина 16MiB; декомпрессия $SPF guided-секции (заголовок
@0xa9c720, LZMA_ALONE @+0x18) = 487836 байт, не изменилась;
контейнер vs оригинал = **ровно 4 байта** — [0x5106, 0x99ec], оба
u16 420→750 (= аппенднутый help id 750, форма E26/E28); побайтовый
дифф образа confined в два слота — Setup 899407D7
[0x8D16D0..0x8D7BF1) и SetupData FE612B72 [0xA9C708..0xAA89E4), вне
слотов 0 изменённых байт. Примечание к методу: батч-
`lzma.decompress` движковых потоков требует +1 байт после EOS (то же
на эталонном e27) — корректный декод — стриминговый с
`max_length=487836` (eof=True, хвост секции = 841 байт нулевого
паддинга, поток короче оригинального).

* [x] **флеш-чеклист E29** (ожидание = наблюдаемое состояние E28,
  собрано движком): (1) страница «PCI Subsystem Settings» видна в
  Advanced, заголовок страницы и пункт меню — сток; (2) на странице
  7 из 8 вопросов, q61 скрыт составным гейтом (не вскрывался);
  (3) наш q59 «UEFIPatcher E29: engine hijack-v2.1» с опциями
  [Disabled]/[Enabled]; (4) выделение q59 показывает хелп
  «UEFIPatcher E29: engine-built full help channel»; (5) смена
  значения q59 сохраняется после Save&Exit + reboot (NVRAM).
  **ПРОЙДЕН ПОЛНОСТЬЮ (2026-09-05)**: строки и хелпы наши после
  флеша и после reboot; [Disabled]→[Enabled] сохраняется через
  reboot; Load Optimized возвращает [Enabled]→[Disabled], наши
  строки переживают сброс дефолтов; скрин «Снимок экрана от
  2026-09-05 18-04-32.png». Закрывающий отчёт:
  `docs/reports/2026-09-05-hijack-v2-e29.md`. **Цикл hijack-v2
  (+v2.1) закрыт.** Merge в master — по явному указанию.

* [ ] **`plan_gates` неидемпотентен** — повторный `unlock` на уже
  вскрытых гейтах падает с `GateExpressionUnsupported` (`plan_flip`
  не матчит `EqConst` с `a≠b` / `EqIdVal` с `value==0xFFFF`, а
  `plan_gates` трактует `None` как ошибку). Для hijack-v2 в спеке
  заложен `plan_gates_skip_unlocked`; сам `unlock` op не меняется —
  при необходимости сделать его идемпотентным отдельной правкой.

**E27 собран движком (2026-09-05, Task 8 hijack-v2)**: артефакт
`refs/amibcp/e27-hijack-v2-engine.bin` (НЕ коммитить, refs/ — данные),
sha256 `a91c1a778c9bd79bfb8a2a49b49675d1a6fad8069144b2bbf4fba3382afe200a`.
Собран чисто engine+CLI (сокет `/tmp/uefipatcher-e27.sock`): `session
init --name e27 --force` → `image open … --mode write` → `hii form
unlock 899407d7…:0x10:0#10029` (1 флип: хаб-REF `1==1`→`1==2`
@pkg+0x8fae) → 7× `hii question unlock …#10029:{54,55,56,57,58,59,60}`
(по 1 флипу `01 00 -> ff ff` на EqIdVal(0x9A,1)-grayout'ах; q59
вскрыт ДО hijack — зеркало gate-сценария B; синтаксис CLI —
позиционный ITEM_ID, не `--target`) → `hii form hijack --file
/tmp/e27-schema.json --setupdata-guid FE612B72-203C-47B1-8560-A66D946EB371`
→ `image save`. Контракт hijack: **string_ids=2** (prompt=749
«UEFIPatcher E27: engine hijack-v2», help=750 «UEFIPatcher E27:
engine-built help channel»), **unlock_flips=0** (идемпотентность на
живых данных: всё вскрыто шагами unlock), **help_controls=1** ($SPF
str@0x5116: 1A4→2EE, т.е. хелп-контрол q59 → id 750). Верификация
(throwaway-скрипт, не в git): длина 16MiB сохранена; побайтовый дифф
против оригинала confined в два слота — Setup-файл 899407D7
[0x8D16D0..0x8D7BF1) и SetupData FE612B72 [0xA9C708..0xAA89E4)
(GUID-скан FFS-заголовков, как `find_file_range` в gate-тестах),
вне слотов 0 изменённых байт; re-parse артефакта: форма 10029 «PCI
Subsystem Settings» на месте со сток title, строки 749/750 = наши
аппенды, гейты form+q59 читаются вскрытыми (flip `-`), у q61 grayout
остаётся планируемым + составной suppress без flip.

* [ ] **флеш-чеклист E27** (отклонение от E26: **7 из 8** вопросов,
  q61 скрыт — составной suppress-гейт `EQ(1,0) AND
  EQ_ID_VAL(0x9A,1)` не вскрывается op unlock →
  `GateExpressionUnsupported`, находка Task 7; его unlock НЕ
  выполнялся): (1) страница «PCI Subsystem Settings» видна в Advanced;
  (2) на странице 7 вопросов, включая наш q59 «UEFIPatcher E27:
  engine hijack-v2» с опциями [Disabled]/[Enabled]; (3) выделение q59
  показывает help «UEFIPatcher E27: engine-built help channel»;
  (4) q61 ожидаемо ОТСУТСТВУЕТ (скрыт); (5) заголовок страницы и
  пункта меню — сток; (6) смена значения q59 сохраняется после
  Save&Exit + reboot (NVRAM).

### 450x: PCIe-бифуркация — AMIBCP правит, на плате не применяется (2026-09-03)

> Со слов владельца: bifurcation в AMIBCP меняется и сохраняется, но
> на плате не применяется / применяется «криво и одноразово». Плата
> будет доступна для экспериментов (достанут из коробки).

* [ ] **проверить гипотезу «мёртвых структур» (основная)** — предсказ
  ание нашей модели (§14.2 отчёта HNX): AMIBCP пишет SDP/IFR-дефолты,
  которые на плате инертны; живой источник — StdDefaults-снимок.
  План: движком найти вопросы бифуркации (кандидаты — формы «PCI
  Express Settings» 10057+ / per-slot, вопрос может жить в varstore
  «IntelSetup»/«ServerSetup» — v5-маппинг по имени уже покрывает),
  `question info` покажет varstore/offset/опции; set-default в обе
  копии StdDefaults → прошить → чеклист E14 → lspci/фактические
  линки. Применится и переживёт reseed ⇒ легенда закрыта.
* [ ] **легенда о PE32-гарде Lenovo (проверить/опровергнуть)** — по
  слухам (гугль-AI), в образе зашит PE32-модуль (не DXE), который
  детектит отсутствие «специальных» леновских райзеров в слотах и
  жёстко перезаписывает настроенный режим бифуркации. Если наш
  StdDefaults-флип с валидным значением НЕ применится на живой плате
  — легенда получает очко, искать переопределяющий модуль (кандидаты:
  Setup-поллинг значения в другом varstore, platform-policy DXE,
  PCIe-порт-конфиг PEI). Признак «одноразово и криво» у владельца
  больше похож на SDP-vs-StdDefaults рассинхрон (правится одно,
  сеется другое), чем на «жёсткий гарда».
* [x] **скриншот AMIBCP на C275 (23:02:28): безымянный корень +
  корреляция формсетов со StdDefaults-переменными** — безымянный
  корень = следствие бездескрипторного образа (нет региона/имени
  верхнего уровня), AMIBCP всё равно спускается в FV и находит HII.
  Дерево: Setup(07BD) [Main, H/W Monitor, Advanced(NB/SB Config),
  Security, Boot, Exit], Server Mgmt(09FD), Event Logs(0AD2),
  IntelRCSetup(0104), CSM parameters. Сопоставление с контейнером
  StdDefaults (наш разбор): «Setup» 215b ↔ Setup(07BD);
  «ServerSetup» 445b ↔ Server Mgmt; «IntelSetup» 597b ↔ IntelRCSetup.
  Т.е. на C275 дефолты живут в ТРЁХ varstore-образах — v5-маппинг по
  имени varstore покроет их без изменений после tiano-op. Колонки
  Failsafe/Optimal у формсетов пустые (заполняются на вопросах, ср.
  HNX, где у 4G показывали Disabled).

### Находки ревью мини-цикла value-op (2026-09-03)

> Отложенные minors из per-task и финального ревью цикла (ветка
> `fix/cycle6-reimplent`; v1+v5 закрыты выше).

* [ ] **hii/gates: `question_storage_width` читает qflags@+12 вместо
  numeric-flags@+13** — `hii/values.rs` (Task 1) следует r-efi-раскладке
  (numeric size-flags @+13), gates.rs читает байт вопросных флагов; для
  реального мастера 0x9A непокрыт (width-guard пропускает) — латентно,
  E12-тесты не задеты. Контекст: values.rs корректен; выровнять gates.rs
  при следующем касании.
* [ ] **values: покрытие NUMERIC width 2/4/8 и DEFAULT-типов 2..4** —
  width-вывод тестируется только на SIZE_1/type 1. Контекст: тесты
  `hii/values.rs`.
* [ ] **set_value: 7 веток `ValueOpUnsupported` без тестов** —
  record-missing-in-store, nameless varstore, var_offset+width>size,
  width 0/>8, CheckBox>1, Numeric вне диапазона, kind Other. Контекст:
  `hii/mod.rs` validate_set_value/collect; параметризованный набор
  закрыл бы дёшево.
* [ ] **set_value: `is_store_body` принимает Section с детьми** —
  Section с детьми и телом-стором шорт-кружит обход (не спускается);
  на живых образах сторы — листья. Контекст: `hii/mod.rs`
  collect_std_defaults_hits; ужесточить до leaf-sections при встрече.
* [x] **real_image: `decompressed_diff` обрезает zip'ом до короткого
  потока** — нет `assert_eq!(old.len(), new.len())`; регрессия хвоста
  декомпрессата пройдёт. Плюс нет pre-check `data[0x8000C2]==0`
  (направление 0→1 держится на фикстуре). Контекст: тест set_value
  в `tests/real_image.rs`. Закрыто финальным ревью-циклом (коммит
  `d2ebc3e`: len-ассерт + fixture pre-check).
* [ ] **CLI: defaults-ветка принтера question info не покрыта тестами** —
  `mock_question()` всегда с пустыми defaults. Контекст: `uefi-cli`
  output.rs; одна DefaultEntry в фикстуре.
* [ ] **CLI: e2e tsv-подкейс без content-ассертов** — `--format tsv hii
  question info 0#42:0x1` ассертит только exit success. Контекст:
  e2e.rs.
* [ ] **uefi-cli мок: дублирование ~40-строчного QuestionInfo-литерала**
  между hii_question_info/hii_set_value. Контекст: tests/mock_server.rs;
  кандидат — fn `mock_question()`.
* [ ] **set_value: NotWritable проверяется до парсинга item_id** —
  Read-режим + мусорный item_id → NotWritable вместо NotFound; зеркально
  `resolve_writable_path`. Контекст: `hii/mod.rs`; выровнять при
  следующем касании (как minors unlock-op).
* [ ] **option-тексты в question info** — отдаются string_id без
  резолюции по string-package. Контекст: спека value-op «Отложенное»;
  при подключении TUI/WebUI.
* [ ] **hii unlock/gates: напечатанные смещения флипов нестабильны ±4
  относительно фактических байт** — живой прогон E15: grayout-флип
  напечатан `pkg+0x9705`, байт реально изменён @dec+0x9709;
  suppress-scope напечатан `@pkg+0x8fa0`, реально @dec+0x8FA4, при этом
  suppress-флип `pkg+0x8fae` — точный. Применённые байты верны
  (dec-дифф == E12 побайтово), дефект только в выводе: базы
  scope_offset/flip-строк смешаны (package start vs opcodes start).
  Контекст: `hii/gates.rs` plan_flip/GateInfo, `hii/mod.rs` flip_text;
  найти при следующем касании gates-вывода.
* [ ] **nvar: find_varstore_record матчит первую запись по (имя +
  data_len)** — две записи с одинаковым именем И длиной в одном
  сторе молча флипнут только первую. Контекст: спека §3.1/§7
  (осознанный дискриминатор, на живом образе уникален); отметить
  при работе с прошивками других вендоров.
* [ ] **rpc hii_set_value: flush_image (полная сборка) и на no-op** —
  зеркально hii_unlock (унаследованный шаблон). Контекст: кандидат
  в цикл чистки RPC-хендлеров.

### R2-обновление (риск «новые формы не рендерятся»)

* [x] **R2 смягчён железом** — июль [07-19 11:00]: вставленная извне форма
  появилась в живом Setup (пункты переключались, строки были пустые);
  E7/E8: раскрытая форма работает (виджеты/help). Блокер юзабельности —
  не принципиальный отказ AMITSE, а механизм пустых prompt-подписей
  (PRC исключён на E8; см. пункт выше и §9 отчёта — E9 дискриминирует).

### Мелочи, найденные по ходу сессии

* [x] **ops::rebuild не принимает GuidSection-таргеты** — `target_path`
  (ops.rs:130) возвращает NotFound для `<guid>:0x02:0`; rebuild работает
  только по Path. Временно обходится `node list` → путь.
  Закрыто (цикл 2026-09-02).
* [x] **artifact export пишется в CWD движка** — экспорт из CLI положил
  файлы в корень репо (движок был запущен оттуда). Резолвить путь
  относительно CWD клиента или требовать абсолютный.
  Закрыто (цикл 2026-09-02).
* [x] **Билдер: +2 байта хвоста декомпрессированного потока и +1 байт
  хвоста FFS-файла** (выравнивание последней секции) — на железе
  безвредно (E7 грузится), но засоряет дифф; убрать.
  Закрыто (цикл 2026-09-02).
* [ ] **AMIBCP не отражает IFR suppress-if** — его Show/Access = флаги
  вопросов; сравнение оригинал/патч через AMIBCP бессмысленно для
  visibility-патчей (обе картинки Show=Yes). Метод верификации —
  побайтовый IFR-анализ (пробник `refs/amibcp/probes/suppress_probe.py`).
* [ ] **gateway integration-тесты и sandbox-прокси**: `HTTP_PROXY` окружения
  валит `health`/`create_session_and_list` 503 (reqwest гоняет loopback через
  прокси); харднинг — `reqwest::Client::builder().no_proxy(true)` в
  `crates/uefi-gateway/tests/integration.rs` (найдено в цикле hijack-v2, Task 6).
* [ ] **spf::write_record_defaults без production-вызовов** — после
  hijack v2 (запись fs/opt в $SPF удалена, коммит 03b9b09) функцию
  (`crates/uefi-engine/src/hii/spf.rs:46`) никто в production не зовёт;
  кандидат на удаление в будущей чистке. Пока остаётся pub с unit-тестом.

### Находки финального ревью fix-цикла unhide (2026-09-02)

> Отложенные minors из whole-branch ревью fix-цикла (ветка
> `fix/cycle6-reimplent`). Пункт про SIBT_EXT-блоки с pending-вставками
> закрыт в цикле (коммит `0f6a330`, `HiiError::SibtBlockUnsupported`).

* [ ] **uefi-tui + uefi-gateway/WebUI: absolutизация output_path
  артефакта** — TUI (`:export`) и gateway/WebUI пробрасывают output_path
  `artifact export` verbatim; движок с 2026-09-02 отвергает относительные
  пути (гейт `artifact_export`). Контекст: нужен client-side резолв в
  абсолютный путь по образцу `resolve_output_path` uefi-cli
  (`crates/uefi-cli/src/commands/artifact.rs`).
* [ ] **string_pack: shrink-путь `insert_strings_at_ids_in_resource`** —
  (1) после усадки string-пакета внутри raw-extent .rsrc остаются
  stale-байты старого хвоста (длины авторитетны, безвредно; zeroing хвоста
  дало бы byte-reproducible rebuild); (2) `plan_rsrc_blob_growth`
  возвращает None, если после HII-blob следует любой leaf ресурса —
  отвергает представимые усадки (на HNX99TF не наблюдалось).
* [ ] **string_pack: span-арифметика `insert_strings_at_ids`** —
  `next_id.wrapping_add(count)` может заворачивать u16 на skip-прогонах
  через id 65536, молча роняя блок. Контекст: перевести на
  checked-арифметику с ошибкой (родственный пункт про исчерпание 0xFFFF —
  выше, «PE-resident IFR-патчинг»).
* [ ] **compress: `lzma_props_byte` молча усекает out-of-range lc/lp/pb**
  (`as u8`); `encode_raw_lzma1` без post-loop ассерта
  `total_in() == input.len()`. Контекст: `crates/uefi-engine/src/compress.rs`.

### Находки живого образа hijack-v2 Task 7 (2026-09-05)

> Прогон unlock/hijack на `HNX99TF_200525_original_E5C88C6F.bin`, форма
> 10029 «PCI Subsystem Settings» (Setup 899407D7, PE32-канал .rsrc).

* [ ] **form-level unlock НЕ каскадирует на гейты вопросов** —
  `unlock("…#10029")` применяет ровно **1** флип (хаб-REF EqConst
  `1==1`→`1==2`, pkg+0x67a / PE32+0x8fae), а не 10, как ожидал план по
  образцу E25: `find_gates` с формой-таргетом (`question_id: None`)
  матчит только обёртки FORM-стейтмента и REF'ы на форму. 9× EqIdVal-гейтов
  E25 — это построчные grayout/suppress вопросов q54–q61, движок вскрывает
  их только построчными `unlock("…#10029:{qid}")` (по 1 флипу
  `01 00 -> ff ff` каждый). Полная страница = form-unlock + 7 вопросных
  unlock'ов; «≈10 флипов за один form-unlock» в движке недостижимо.
* [ ] **q61 не вскрывается движком** — у вопроса 61 (form 10029) два гейта:
  EqIdVal(0x9A,1) (flippable) И suppress с составным выражением
  `EQ(u64 1, u64 0) AND DUP EQ_ID_VAL(0x9A,1)` → `plan_gates`/`unlock`
  возвращают `GateExpressionUnsupported` (не hardware-validated класс).
  Итог: полная страница «все 8 вопросов вскрыты» недостижима текущим op
  unlock; q61 остаётся со locked EqIdVal-гейтом (его suppress с EQ(1,0)
  всегда false, т.е. вопрос скрыт независимо).
* [ ] **список qid сценария B скорректирован** — план исключал 59 из
  построчных unlock'ов при ожидании пустых `unlock_flips` у hijack; но
  hijack планирует гейты [form, q59], и без предварительного
  `unlock("…:59")` планировщик выдал бы 1 флип. Сценарий B: qid =
  [54,55,56,57,58,59,60] (по 1 флипу), q61 — ожидаемый Err, hijack → 0
  флипов (идемпотентность на живых данных доказана).

### Находки финального ревью S1 serial-console (2026-09-06)

> Отложенные minors из whole-branch ревью ступени S1 (ветка
> `fix/cycle6-reimplent`, план `docs/superpowers/plans/
> 2026-09-06-serial-s1-edk2-build.md`, отчёт
> `docs/reports/2026-09-06-serial-s1-build.md`). Единственный Important
> ревью (ложный маркер 1 при неудаче ConOut-append) закрыт в цикле
> (`cdfe527`); ниже — parking-lot.

* [ ] **build_serial.sh: пиновать исходник edk2 на bcd1687, а не HEAD**
  — скрипт архивирует `refs/edk2` через `git archive HEAD`; обновление
  чекаута молча сменит выходные артефакты при следующей пересборке.
  Контекст: `docker/edk2/build_serial.sh` (строка `git -C /src/edk2
  archive --format=tar HEAD`); закрепить ревизию `bcd1687` в скрипте
  или ассертовать её перед архивацией.
* [ ] **edk2-builder containerfile: dnf install без пинов** — пакеты
  (gcc/make/nasm и пр.) ставятся из текущего репо fedora:44, образ
  дрейфует со временем; байт-воспроизводимость сборки держится только
  на уже закоммиченных .ffs. Контекст:
  `docker/edk2-builder.containerfile`; пиновать версии пакетов или
  зафиксировать базовый образ digest'ом.
* [ ] **BaseTools: unit-test noise TianoCompress ×2 за сборку** — в
  логе каждой эпизодичной сборки (свежий /work + make BaseTools) по
  две не-фатальных строки провалившихся unit-тестов TianoCompress.
  Контекст: на артефакты не влияет; отфильтровать при следующем касании
  `build_serial.sh`.
* [ ] **hack/edk2_build_check.py: edge-кейсы** — (1) TDS-only
  false-FAIL под FFS_ATTRIB_CHECKSUM: file-checksum байт 0x11 покрывает
  данные, `zero_coff_tds` обнуляет только TDS внутри PE32-секций →
  легитимная «reproducible (COFF TimeDateStamp only)» пара помечается
  NOT reproducible (латентно: наши .ffs не ставят 0x40); (2)
  `walk_sections` не проверяет `off + size <= end` — секция с
  объявленным размером за пределами файла молча обрезается срезом;
  (3) UI-имя извлекается, но не ассертуется (`assert ui == …`);
  (4) словарь `results` в `main()` мёртв (заполняется, не читается);
  (5) FFS2/LARGE_FILE расширенный заголовок не поддержан (size24==len
  падает на >16MB файлах — для S1 недостижимо).
* [ ] **SerialConsoleGlue.inf: declares unused MdeModulePkg.dec** —
  [Packages] содержит MdeModulePkg/MdeModulePkg.dec, но модуль не
  потребляет из него ничего (все GUID/библиотеки — MdePkg). Контекст:
  `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/
  SerialConsoleGlue.inf`; убрать при следующем касании (на артефакты
  не влияет, build-time только).
* [ ] **PE32 дублируется в .ffs по FDF-правилу — root cause неизвестен**
  — правило формы `PE32 PE32 |.efi` кладёт PE-образ каждого модуля
  дважды (SerialDxe 16 388 Б ×2, TerminalDxe 32 772 ×2, glue 12 292 ×2
  — избыточно 61 452 Б), при этом та же форма правила в OvmfPkgX64.fdf
  не удваивает. Контекст: §6 отчёта S1; установить механику GenFds
  (почему материализуется вторая секция) до любых S2-решений про
  «FDF cleanup» (~61,7 КБ экономии) — иначе чистка может сломать
  неочевидную зависимость.

### Находки финального ревью S2 serial-console (2026-09-06)

> Финальное ревью E30-flashpack (отчёт
> `docs/reports/2026-09-06-serial-s2-e30-pack.md`; кандидат v1
> sha256 `09f5e897…` дважды независимо валидирован — по находкам
> правки docs-only, код не трогался). Parking-lot:

* [ ] **`regions` в `real_image_ops_insert_serial_s2` считает diff-байты,
  не регионы** — переменная накапливает счётчик изменившихся байт
  (косметика имени). Контекст: `crates/uefi-engine/tests/real_image.rs`
  (`let mut regions = 0usize;` в цикле по `data.zip(rebuilt)`);
  переименовать (напр. `diff_bytes`) при следующем касании теста.
* [ ] **Юнит-тест «state 0xF8 в polarity-1 томе проходит verbatim» —
  докупить** — ветка `ops::insert` (артефакт уже в polarity-1 форме
  не адаптируется) покрыта только косвенно real-image гейтом на
  живом HNX99TF. Контекст: коммит `e88691a`, зеркало
  `insert_keeps_state_byte_verbatim_in_polarity0_volume`; добавить
  синтетический polarity-1 том c FFS state 0xF8.
* [ ] **Спека §8.3 «маркеры ASCII» — уточнить при следующем
  аддендуме** — внутри glue-модуля строки UTF-16LE (CHAR16),
  ASCII они становятся на проводе только после TerminalDxe.
  Контекст: спека `2026-09-05-serial-console-ladder-design.md` §8.3;
  переформулировать как «ASCII на проводе после TerminalDxe».

### Наблюдения вердикта E30 (2026-09-06, parking)

* [ ] **0xB2-hang: вход в «настройки UEFI» из загрузчика NixOS** вешает
  POST на B2 на E30-образе (штатный ребут и холодный старт — чистые,
  Setup доступен). Наше (третий консольный инстанс в энумерации TSE
  UI-пути) или платформенное — не установлено. A/B-тест на базовом
  образе = цикл прошивки — только по решению владельца.
* [ ] **Маркеры SC-S1 не наблюдаемы на E30** (гипотезы — отчёт E30-pack
  §6 п.1: ClearScreen Setup / stale TextOut в ReadyToBoot).
  Диагностическая ценность исчерпана вердиктом; при будущей пересборке
  glue (S3+) — маркер 2 печатать через gST->ConOut.

### Наблюдения вердикта E31 (2026-09-06, parking)

* [ ] **IfrBuilder: зеркалировать нативные флаги one_of (`0x10/0x10`)**
  — наши вопросы строятся с question-flags `0x00`; все нативные one_of
  Setup-формсета несут `0x10` (RESET_REQUIRED). На железе (E31) коммит
  и сохранение с 0-флагами работают; различие F10-поведения (нативные —
  запись+перезагрузка, наши — тихий выход) зафиксировано как
  **непонятое поведение** (гипотеза флагов владельцем не принята как
  диагноз, 2026-09-06 — «сомнительно», отложено). Зеркалирование
  остаётся дешёвой мерой UX-паритета при любой следующей прошивке с
  новыми вопросами. Контекст:
  `crates/uefi-engine/src/hii/ifr_builder.rs` (`emit_one_of`,
  `push(0)`), отчёт `docs/reports/2026-09-06-serial-s3-e31-verdict.md`
  §3.1.
* [ ] **COM-переезд (2F8/IRQ3) клинит обе консоли на S2-стеке** —
  воспроизводимый факт (симптомы: тормоги серийного UI до сохранения,
  глухая консоль после, локальный TUI виснет, лечение — CMOS-джампер);
  **механизм — непонятое поведение** (гипотеза «фикс 0x3F8 → записи в
  пустоту → ConSplitter-дрен» владельцем не принята, 2026-09-06 —
  «фантастика», отложено). Кандидаты лечения: S4 (resource-aware
  serial / донорский AMI SerialIo) либо guard в glue (не поднимать
  консоль при нестандартных ресурсах UART). Контекст: отчёт
  `docs/reports/2026-09-06-serial-s3-e31-verdict.md` §3.2.
* [ ] **(опционально) Видимая проверка Terminal Type** — значение
  применяется (glue выбирает GUID терминального чайлда по
  `Setup[0x5E]`), но ASCII-вывод между VT-UTF8/VT-100/PC-ANSI не
  различается; видимая разница возможна в рамках TUI — косметика, по
  желанию владельца.

### Наблюдения вердикта E32 (2026-09-06, parking)

> Отчёт `docs/reports/2026-09-06-serial-s4-e32-verdict.md`; S4 закрыта.

* [x] **Дискриминатор A/B: применяется ли `PNP0501_0_NV[1]` физически**
  — закрыт пост-вердиктной проверкой владельца (2026-09-06): после
  Change Settings=2F8 ОС пишет `ttyS1 at I/O 0x2f8 (irq = 3)` —
  **гипотеза A**: платформа переезжает физически, BIOS-консоль следует
  за портом (донорный SerialIo берёт базу из SIO-ресурсов —
  resource-aware доказан позитивно), ОС-вывод ушёл с ttyS0 (статическая
  консоль ОС, не отказ; для вывода ОС нужен `console=ttyS1`).
  Контекст: отчёт E32-вердикта §6.
* [ ] **Терминальный рендер: цвет как на rd450x** — на rd450x (полный
  AMI-стек TermSrc 54891A9E, живой референс владельца) serial-вывод
  цветной; на нашем стеке (TerminalDxe) цвета/различия типов терминала
  не наблюдается. Проверка владельца (2026-09-06): переключение
  UTF-8/ANSI/VT-100+ влияния не оказывает — цветов и ASCII-эскейпов
  нет; **тот же симптом на Orange Pi 5 Plus со стоковым EDK2**
  (наблюдение владельца, возможно совпадение) — намёк на общий
  характер пути TerminalDxe, не на нашу glue-цепочку. Владелец связал
  цвет с TermSrc («TermSrc тот же» — тот же модуль/GUID, билд
  платозависим: rd450x-билд ждёт Setup 0x94 с CR-полями +0x31..+0x3D,
  LIVE — 0x72 с иным лэйаутом; вставка = патч-карта спеки §8.3-а).
  Контр-факт: наш TerminalDxe `TerminalConOutSetAttribute` сам эмитит
  ANSI SGR-цвета (без гейта по типу). Косметика; путь лечения при
  желании — резерв (а) полный AMI TermSrc. **В работе (решение
  владельца 2026-09-06 «идём на полный AMI»)**: пре-чек закрыт
  (аддендум спеки §11) — минимальный вариант БЕЗ патчей бинаря
  (дефолты TermSrc при Setup-несовпадении = порты 0/1 enabled,
  115200 8N1, PcAnsi; NVRAM самоинициализируется; нужен glue v3 —
  search-all GUID); план
  `docs/superpowers/plans/2026-09-06-serial-s4a-termsrc-swap.md`
  исполнен (2026-09-07, «жги»): кандидат **E33** собран (sha256
  `969aa67f…`, TermSrc-свап + glue v3, тройная валидация зелёная —
  отчёт `docs/reports/2026-09-06-serial-s4a-e33-pack.md`), готов к
  прошивке владельцем; приёмка: (a) живая консоль, (b) цвет, (c)
  q512. Контингенси (i)–(iii) — в pack-отчёте §4.
* [ ] **E31-COM-клин не воспроизвёлся на E32** — контраст-факт (на E31
  та же операция клинила обе консоли до CMOS-сброса; на E32 консоль
  жива и следует за портом). С учётом доказанного переезда (закрытый
  дискриминатор выше) глухота E31 после сохранения объяснима
  фиксированной базой SerialDxe на освободившемся 0x3F8 (два Proof-
  факта, без гипотетических звеньев); непонятыми остаются тормоза
  серийного UI ещё ДО сохранения и вис локального TUI (паркинг
  вердикта E31 §3.2).
