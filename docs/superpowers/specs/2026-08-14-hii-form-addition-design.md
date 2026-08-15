# Спека: добавление форм/формсетов в HII (фазы A–C)

> Дизайн-документ, 2026-08-14. Статус: черновик для утверждения
> пользователем (brainstorm-инвентаризация по коду, без реализации).
> Ветка: `fix/cycle6-reimplent` (после коммитов `2796c8a..d6afac6`).

## 1. Контекст: что уже есть и чего не хватает

Конвейер «новый формсет из JSON-схемы» реализован в цикле 6 и живёт в
`hii/{schema,ifr_builder,ffs_assembler,string_pack,ami_patcher,formset_add}.rs`
+ RPC `HiiFormSetAdd` (`rpc/server.rs`). Однако на живых образах
современного лейаута (HNX99TF: HII в PE-ресурсах за LZMA) он
неработоспособен, а связки с CLI нет. Ограничения по коду:

1. **Строки — только bare-канал**: `string_pack::add_strings` ищет
   string-пакет сканом прямых детей файлов (`find_string_package`,
   `string_pack.rs:49-68`) — без спуска в LZMA-обёртки и без
   PE-resource-канала. На HNX99TF → `StringPackageNotFound`.
2. **Вставка нового FFS хардкодит цель**: `ops::insert(&mut image.root,
   &Target::Path(vec![0]), …)` (`formset_add.rs:67-69`). На full-flash
   (gap-aware, issue V) `root.children[0]` — Padding (IFD/ME), а не
   Volume: `build_node(Padding)` эммитит только `body`, дети
   игнорируются — вставленный файл молча теряется при сборке.
3. **Нет CLI-команды**: RPC есть, в `uefi-cli` вызова нет (только
   `setup form {list,set-visibility}`, `setup string {list}`).
4. **Нет PE-resource writer**: чтение/локализация есть (фаза 6 + коммит
   `8160c5e`), но рост resource-блоба (добавление пакета/строк) не
   поддерживается нигде.
5. **Нет «формы в существующий формсет»**: единственная in-place
   мутация IFR — length-preserving unsuppress (`8160c5e`). Вставка
   формы (рост в середине пакета) не реализована.
6. **Формсет-копия строк в новом FFS**: `assemble_ffs` кладёт снапшот
   string-пакета в новый файл — на bare-канале дубликат живёт своей
   жизнью (изменения оригинала не видны); на resource-канале новый файл
   с bare-пакетами HII-драйвером не подхватывается как package list
   владельца (HiiAddPackages вызывается драйвером на его ресурсе).

## 2. Цели и не-цели

**Цели:**

1. `setup formset add` работает end-to-end на синтетике (bare-канал) и
   на живом HNX99TF (PE-resource-канал): новая форма появляется в
   `collect_forms` после build+re-parse.
2. Строки новой формы добавляются в string-пакет того же package list,
   что и целевой формсет (без дублирующего снапшота в новом FFS, где
   канал это позволяет).
3. `setup form add` — вставка формы (и вопросов) в существующий формсет
   целевого файла.
4. Честные ошибки на неподдерживаемых лейаутах (PE-рост невозможен,
   .rsrc не последняя секция, EDK2 bare-конст-массивы и т.п.).

**Не-цели:**

- EDK2 bare-конст-массивы в теле PE (канал форм rk3588): вставка в
  середину PE-тела требует переписывания layout PE — вне рамок; читаем
  (фаза 6), не пишем.
- AMI setupdata/amitse-патчинг расширяем только по мере надобности
  (существующий `ami_patcher` сохраняется как есть).
- TUI/WebUI-интеграция (CLI-first; UI-слои добавляют обёртки позже).
- Копирование string-пакета в новый FFS на resource-канале (пункт 6
  §1 закрывается отказом от FFS-пути для resource-канала, см. §4.B2).

## 3. Каналы хранения и матрица поддержки

| Канал | Чтение | Мутация длины (рост) | План |
|---|---|---|---|
| bare 0x19-секция (тело = пакет) | фаза 3–4 | есть (FV-рост поглощается, фаза 5–7) | фаза A |
| PE32 'HII' resource (AMI/HNX99TF) | фаза 6 + `8160c5e` (in-place) | **нет** → фаза B | фазы B |
| EDK2 bare-конст-массив в PE-теле | фаза 6 | нет и не планируется | — |

## 4. Дизайн

### 4.A Фаза A — bare-канал в порядок + CLI

1. **Фикс insert-цели**: вместо `Target::Path(vec![0])` — первый
   `FfsType::Volume`-узел образа (или volume целевого FFS, если задан
   `target_ffs_guid`). Тест на gap-aware-образе (Padding первым):
   добавление → build → новый файл в выводе.
2. **String-поиск через фазу-6 walker**: `find_string_package`
   заменяется на поиск с использованием семантики
   `collect_strings`/`walk_sections` (спуск через обёртки; bare-секции
   всехsubtype с валидным пакетом). PE-resource-канал на фазе A —
   честный `StringPackageNotFound` (расширение в фазе B).
3. **CLI**: `setup formset add --file <schema.json> [--image <id>]
   [--ffs <guid>]` → `HiiFormSetAdd`; вывод `new_ffs_id` +
   `inserted_form_ids`. Mock e2e + интеграционный тест.

### 4.B Фаза B — PE-resource writer (рост)

1. **Примитив роста PE** (`hii/pe_resource.rs`):
   `try_grow_rsrc_tail(pe: &mut Vec<u8>, delta: usize) -> bool` —
   добавить `delta` байт к хвосту .rsrc. Гварды: .rsrc — последняя
   raw-секция в файле (иначе `false`: сдвиг последующих секций меняет
   RVAs каталога ресурсов); обновления: section header
   `SizeOfRawData`/`VirtualSize` (выравнивание до file-align),
   `SizeOfImage` (если .rsrc последняя виртуальная), resource data
   entry `Size`, каталог не трогаем (RVA/смещения прежние). Тесты на
   `synth_hii_pe` (хвост растёт, старые смещения валидны).
2. **Append-пакета в список ресурса**: `append_package_to_resource(
   pe_body, form_pkg_bytes) -> bool` — найти 'HII'-список (первый),
   вставить пакет перед конечным `PACKAGE_END`, пересчитать u32 total
   списка, при необходимости вырастить PE (`try_grow_rsrc_tail`) и
   обновить data entry size. Идемпотентность/валидация —
   `parse_package_list` на новом блобе.
3. **String-append в ресурсный пакет**: `add_strings` обобщается до
   работы на под-блобе STRING-пакета внутри ресурса
   (`add_strings_to_body` уже принимает `&mut Vec<u8>` — переиспользуем
   на срезе с пересчётом длин пакета/списка/ресурса по цепочке
   смещений из §4.B2).
4. **Подключение к `add_setup_formset`**: если целевой string-пакет
   найден в resource-канале — строки append'ятся в него, формсет
   добавляется append-пакетом в тот же список владельца (новый FFS не
   собирается — пункт 6 §1); сборка IFR — существующий `IfrBuilder`.
   LZMA-обёртка над PE32 — recompress фазы 7 (уже работает).
5. **Acceptance**: real HNX99TF — добавить формсет в список ресурса
   Platform-файла → build → re-parse: новый формсет в `collect_forms`,
   длина образа сохранена, байты вне FV1 идентичны; ручная проверка
   образа в UEFITool (risk-митигация §7).

### 4.C Фаза C — форма в существующий формсет

1. **IFR-вставка**: `insert_form_into_package(pkg: &mut Vec<u8>,
   form_ifr: &[u8], varstores: &[u8]) -> bool` — найти закрывающий END
   целевого формсета (walker как в `find_form_suppress_scope`), splice
   формы перед ним; varstore-опкоды — сразу после формсет-заголовка.
   Выбор формсета — по `form_id` (target секции) + порядковый номер
   формсета в пакете (дискриминатор `#<n>`, симметрично `8160c5e`).
2. **Строки** — та же схема §4.B3 (новые id, map для сборки IFR).
3. **RPC/CLI**: `HiiFormAdd` (`image_id`, `target`, `schema_json`) →
   `setup form add --target <form_id> --file <schema.json>`.
4. **Acceptance**: синтетика — форма в формсет ресурса → build →
   re-parse: форма с новыми question-строками; real HNX99TF (Setup).

## 5. Ошибки

- `HiiError::StringPackageNotFound` — целевой string-пакет не найден
  (фаза A: PE-resource не поддержан; фаза B: нет 'HII'-ресурса).
- Новый `HiiError::PeGrowthUnsupported` — .rsrc не последняя секция /
  PE-геометрия не позволяет рост (Display: "cannot grow PE resource
  section: .rsrc is not the last section").
- `InvalidSchema` — как сейчас. RPC-маппинг: PeGrowthUnsupported →
  `failed_precondition` (по образцу `hii_error_status`, коммит
  `22fe28b`).

## 6. Тестирование

- Unit: рост PE (гварды/заголовки), append-пакет (пересчёт total),
  string-append в срезе, IFR-вставка (баланс END, смещения).
- Интеграционные: синтетика bare + `synth_hii_pe`-лейаут; CLI e2e
  (mock).
- Real-image `#[ignore]` (HNX99TF): §4.B5, §4.C4.
- Регрессия: `real_image_full_flash_round_trip`,
  `real_image_hii_form_visibility_round_trip` — зелёные.

## 7. Риски

1. **PE-рост ломает загрузку драйвера** (главный): firmware-PE грузится
   DXE-лоадером по заголовкам; рост хвоста .rsrc с корректными
   SizeOfRawData/SizeOfImage механически валиден, но финальное
   доказательство — железо/UEFITool. Митигации: guard «последняя
   секция», выравнивания, ручная UEFITool-проверка перед прошивкой
   (симметрично риску §9.1 спеки рекомпрессии).
2. **HII-драйвер не видит изменения**: package list кэшируется только
   при HiiAddPackages в рантайме — драйвер читает ресурс при старте,
   пересчёт total обязателен (покрыт тестом §6).
3. **FV-переполнение** — честный `SizeMismatch` билдера (фаза 7, §9.2).
4. **AMI-специфика** (setupdata-патчи) — только для образов, где эти
   файлы есть; не блокирует resource-канал.

## 8. Скелет задач (детализируется в плане)

| Фаза | # | Задача |
|---|---|------|
| A | 1 | Фикс insert-цели (Volume-узел) + тест gap-aware |
| A | 2 | String-поиск через walker + тесты (вкл. честный отказ resource) |
| A | 3 | CLI `setup formset add` + e2e |
| B | 4 | `try_grow_rsrc_tail` (TDD) |
| B | 5 | `append_package_to_resource` (TDD) |
| B | 6 | string-append в resource-канале (TDD) |
| B | 7 | wiring `add_setup_formset` (resource-ветка) + real-image acceptance |
| C | 8 | `insert_form_into_package` (TDD) |
| C | 9 | RPC `HiiFormAdd` + CLI `setup form add` |
| C | 10 | real-image acceptance + финальные гейты |
