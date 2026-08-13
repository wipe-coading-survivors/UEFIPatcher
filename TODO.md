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

**Baseline-коммит для onboarding** (где найти актуальный на момент
написания код `setup_advanced/` и `setup/`):
`0470553bcc579af0eb72075533bc2c73f77d543f` —
`crates/uefi-engine/src/setup_advanced/{mod,ffs_assembler,ifr_builder,ami_patcher,schema,string_pack}.rs`
и `crates/uefi-engine/src/setup/{mod,ifr}.rs`. Если код переехал/удалён —
искать через `git log --all -- crates/uefi-engine/src/setup_advanced/`.

* [ ] **Реализовать `SetupListForms`** — обход FFS-секций с IFR-байткодом,
  извлечение FormSet GUID, FormId, title-string-id, состояния visibility
  (через `find_suppress_if_scopes`). Возвращает `Vec<FormInfo>`.
* [ ] **Реализовать `SetupListStrings`** — чтение string-package'ей через
  `setup_advanced/string_pack.rs`, возврат `Vec<StringInfo>` с language /
  string_id / text.
* [ ] **Слить `setup_advanced/` с `setup/`** — общая иерархия
  `crates/uefi-engine/src/setup/{mod,ifr,forms,strings,schema,
  ami_patcher,ffs_assembler,ifr_builder,string_pack}.rs`. Решить структуру
  импортов и更新ть `lib.rs`.
* [ ] **Подключить CLI/TUI/Gateway/WebUI** на реальные данные вместо stub'ов.

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
* [ ] **Gateway: `routes/setup.rs`** — добавить `forms`, `strings` handlers.
* [ ] **WebUI: полный fix** — обновить все fetch-вызовы под новые маршруты,
  подключить forms/strings listing, починить существующие баги (dead
  `dumpTree` import, unused `openImage` name field, a11y warnings).

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

* [ ] **`ops::rebuild` перетирает `Action::Remove`** (`crates/uefi-engine/src/
  ops.rs:95-101`) — `node.action = Action::Rebuild` безусловно. Поэтому
  `:rebuild` по Remove-узлу отменяет удаление (маркер красный → жёлтый).
  Фикс: не даунгрейдить Remove (и иные pending-операции) до Rebuild — rebuild
  должен быть no-op для уже-Remove-узла (или возвращать ошибку «узел помечен на
  удаление»).
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

* [ ] **Минимум: BuilderError на мутации внутри compression barrier** —
  покрыть тестом: `remove` секции `1/13/2/1`-вида → `build_image` возвращает
  ошибку (не silent-verbatim). Клиенту (CLI/TUI) показать человекочитаемое
  сообщение «cannot mutate inside compressed section without recompression».
* [ ] **Минимум: `ops::rebuild` не перетирает `Action::Remove`** — unit-тест
  `rebuild_after_remove_keeps_remove`.
* [ ] **Спека: рекомпрессия LZMA/GUID_DEFINED в билдере** — декомпозиция,
  референсы (`refs/UEFITool-ai-fork/common/ffsbuilder.cpp` — recompress path),
  тест-фикстуры на `HNX99TF_200525_original_E5C88C6F.bin`.

### CRITICAL: Builder не round-tripит полный flash-образ (issue V, ревизия 2026-08-13)

> Severity: **high** — потенциальная порча данных / кирпич при прошивке.
> Влияет на **все** мутации на полном образе, независимо от issue IV.

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

* [ ] **Спека: full-flash round-trip в билдере** — билдер должен воспроизводить
  полный буфер образа, сохраняя non-FV регионы. Варианты:
  - **(a) Gap-aware parser+builder:** `parse_image` создаёт `Region`/`Padding`
    узлы для байтовых диапазонов **между** и **вне** FV (raw bytes в `body`);
    `build_node` для Image эммитит children в порядке offset'ов → полный буфер.
    Требует хранения raw-gaps в дереве и сортировки по offset.
  - **(b) Patch-overlay:** билдер хранит оригинальные байты образа и накладывает
    только изменённые FV (по offset) → меньше риска, но другая модель (overlay,
    а не полное перестроение).
  - Референс: `refs/UEFITool-ai-fork/common/ffsbuilder.cpp` — как UEFITool
    собирает полный образ (memcpy немодифицированных регионов + rebuild FV).
* [ ] **Регрессионный тест:** `parse_image(full_16MB) → build_image → len == orig`
  + `== orig` по байтам (на немутрированном образе). Добавить в
  `crates/uefi-engine/tests/real_image.rs` с `#[ignore]`.
* [ ] **Bugfix `flush_image`:** пока round-trip не реализован, `flush_image` на
  полном образе молча портит файл. Минимум — детект `root.node_type == Image &&
  sum(children sizes) < orig size` → отказывать в write-through с понятной
  ошибкой вместо тихой порчи.
