# TODO — отложенные и несрочные правки

> Сюда заносим правки, которые не входят в текущий цикл, но должны быть сделаны:
> известные дефекты, рефакторинг, deprecated-код, пробелы в API.
> Формат: `* [ ] <область> — <что сделать>. Контекст: <почему>.`

## Deprecated: выпиливание DumpTree RPC

`DumpTree` дублирует `ListItems` + клиентское форматирование через
`uefi-common::format`. Legend в stderr через DumpTree не получить; TUI и
Gateway уже мигрируют на `ListItems` (см. spec
`2026-08-09-display-and-search`). Из CLI уже удалён. Дальнейший порядок:

* [ ] **uefi-tui** — убрать `dump_tree` client-вызов и `parse_tree_dump`
  (команда `dump` в TUI должна ходить через `list_items` + рендерить
  дерево локально). Контекст: TUI парсит текстовый дамп строками, теряет
  subtype, читает `subtype=07` как name (см. `crates/uefi-tui/src/commands.rs:191`).
* [ ] **uefi-gateway / WebUI** — убрать `DumpTree`-маршрут и
  клиентский вызов `gateway::client::dump_tree`, переключить WebUI на
  `list_items`. Контекст: `crates/uefi-gateway/src/client.rs:127` и
  `crates/uefi-gateway/src/routes/image.rs`.
* [ ] **uefi-proto + uefi-engine** — после миграции всех клиентов
  удалить `rpc DumpTree` из `engine.proto`, убрать сообщения
  `DumpTreeRequest`/`DumpTreeResponse`, убрать `dump_tree` handler из
  `rpc/server.rs`, убрать stub'ы из 3 mock'ов
  (`uefi-cli`/`uefi-tui`/`uefi-gateway` tests), убрать
  `parser::image::dump_tree` и `DumpFormat` enum.

## Ревизия CLI (2026-08-10)

Прогон по всем командам/подкомандам `uefi-cli`. Срочности нет, по
бóльшей части UX/консистентность. Решение о плане принимается отдельно.

### Семантика команд image

* [ ] **`image find <target>` бесполезна в текущем виде** — handler
  `find_item` (`rpc/server.rs:160`) только валидирует существование
  узла и возвращает echo введённого target как `item_id`. Не возвращает
  данных об узле (path/type/subtype/guid/name/offset/size). Варианты:
  (а) расширить `FindItemResponse` до полного `Item`; (б) выпилить
  `image find` altogether, т.к. `image list --filter` + `image search`
  покрывают discovery.
* [ ] **`image list` конфликтует семантически** — это `ListItems` RPC
  (список FFS-узлов внутри образа), а не список открытых образов.
  Имя сбивает с толку. Варианты: переименовать в `image items` /
  `image tree`, а под `image list` завести список открытых образов.
* [ ] **Нет `ListImages` RPC в proto** — нельзя узнать, какие образы
  открыты на сервере. Без этого `image switch <id>` бесполезен: пользователь
  не знает валидные id, кроме как из вывода прошлых `image open`.
* [ ] **`image switch <image_id>` не валидирует образ на сервере** —
  только перезаписывает локальный state-файл (`commands/image.rs:36`).
  Любая опечатка молча сохраняется и проявится на следующей операции.
  Нужна проверка через будущий `ListImages`/`GetImage`.
* [ ] **Нет `image status`** — пользователь не может узнать активный
  образ без чтения `.uefipatcher`. Аналогично `session status`.

### UX/cli-rendering

* [ ] **Все подкоманды без `about:`** — `image --help`, `edit --help`,
  `setup --help` показывают голые имена без описания. Проставить
  `#[command(about = "...")]` и `#[arg(help = "...")]` везде.
* [ ] **`--format` глобальный — String, не ValueEnum** — валидация
  формата происходит в runtime через `output::parse_format`, лучше
  `#[derive(clap::ValueEnum)]` (см. `SearchModeCli` в main.rs:136).
* [ ] **`--mode` у `image open` — String, не ValueEnum** — то же
  (`read|write` парсится в `commands/image.rs:13`). Тоже для `--mode`
  у `edit insert` (`into|before|after`, `commands/edit.rs:17`).
* [ ] **Семантика `ffs`/`data` в `edit insert`/`edit replace` не очевидна**
  — позиционный arg, на самом деле путь к файлу **на стороне сервера**
  (engine читает файл из своей FS), либо `--from-artifact` для артефакта.
  Переработать в группу: `--file <path>` / `--artifact <id>` (взаимоисключающие).
* [ ] **`session init` имя = `env::var("PWD")`** — неявно. Если PWD не
  задан, имя пустое. Явный `--name <name>` с fallback на PWD был бы понятнее.

### Прочее

* [ ] **`output::print_find` печатает только `item_id`** — используется
  в `image find`, `edit insert`, `edit replace`. После расширения
  `FindItemResponse` (см. выше) — обновить вывод.
* [ ] **`setup list-items` противоречит `image list` по именованию** —
  `list-items` через дефис, `list` без. Привести к единому стилю
  (clap конвертирует `list_items` → `list-items` автоматически, но
  исходник стоит унифицировать).

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

План A (спека `2026-08-10-cli-topology-and-image-storage-design.md`)
мигрирует TUI **механически** — только rename client-методов под новые
RPC, чтобы workspace компилировался. Глубокая работа — отдельным циклом:

* [ ] **Выкинуть `parse_tree_dump`** (`crates/uefi-tui/src/commands.rs:191`)
  — текстовый парсинг flat-списка, теряет subtype, читает `subtype=07` как
  name (см. TODO в файле). Заменить на локальный tree-рендер через
  `uefi-common::format` (`format_tree` + `format_legend`).
* [ ] **TUI: убрать зависимость от DumpTree-формата** — сегодня
  `app.tree` строится из текста; переключить на структурированные
  `Item`-данные из `image_nodes_list`.
* [ ] **Fix существующих TUI-багов** — провести ревизию после миграции.

## Gateway + WebUI rework (после Плана A)

План A мигрирует Gateway **механически** (rename client-методов, старые
маршруты сохраняются, но внутри дёргают новые RPC). WebUI сейчас в
поломанном состоянии — если компилируется, минимально обновить fetch-имена;
если нет — оставить и явным образом зафиксировать. Полный rework —
отдельным циклом:

* [ ] **Gateway: новые маршруты** — `/api/v1/image/:id/dump` → `/nodes`;
  удалить `/dump` и `/dump/ws`; добавить `/api/v1/images` (ImagesList),
  `/api/v1/image/:id/forms`, `/api/v1/image/:id/strings`,
  `/api/v1/image/:id/status`, `DELETE /api/v1/image/:id` (ImageClose).
* [ ] **Gateway: `routes/setup.rs`** — добавить `forms`, `strings` handlers.
* [ ] **Gateway: `routes/ws.rs`** — удалить или переделать (сегодня только
  `dump_ws`).
* [ ] **WebUI: полный fix** — обновить все fetch-вызовы под новые маршруты,
  подключить forms/strings listing, починить существующие баги.
