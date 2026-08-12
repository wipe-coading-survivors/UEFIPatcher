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
