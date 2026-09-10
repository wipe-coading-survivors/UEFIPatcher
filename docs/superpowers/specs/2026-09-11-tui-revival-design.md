# TUI Revival — Design (цикл A)

**Date:** 2026-09-11
**Scope:** `uefi-tui`, `uefi-engine`, `uefi-common`, `uefi-proto`
**Related:** дуга «TUI Forms View» (`2026-09-11-tui-forms-view-design.md` — цикл B, строится поверх
результатов этого цикла); `2026-08-12-tui-migration-bugfix-design.md` (прошлый TUI-цикл);
TODO-разделы «TUI: deferred enhancements», «Ревизия TUI cycle 3 (2026-08-13)», «ImageUpload RPC».

## Motivation

TUI технически жив (тесты/клippy зелёные, `2026-08-12`-цикл закрыл 8 целей), но по ревизии
2026-08-13 и deferred-списку копятся дефекты отображения и доверия, а навигация по 16MB-образу
(сотни узлов) без goto/completion мучительна. Владелец выбрал максимальный состав revival-цикла:
полировка + навигация + engine-доверие + ME-регион + upload. Это база, на которой дуга Forms View
(цикл B) будет полезна.

Решение владельца (2026-09-11): revival = «максимум всего», одной спекой/планом; Forms View —
отдельная дуга.

## Goals

| # | Задача | Сторона | Суть |
|---|---|---|---|
| A1 | Метки узлов | TUI | Безымянные узлы получают читаемые метки (см. ниже) |
| A2 | Name-lift через обёртки | engine | Имя файла поднимается через GUIDED/compression-секции |
| A3 | Prune Remove-узлов | engine | Дерево отражает применённое состояние после save |
| A4 | FIND/GO | TUI | `:goto <path>` + `/`-промпт, auto-expand родителей |
| A5 | Tab-completion + общий парсинг флагов | common+TUI | `uefi-common::cli`; Tab-дополнение в cmdline |
| A6 | ME/FTPR-регион | engine | Честный распознаватель ME-области с именами модулей |
| A7 | `ImageUpload` RPC + `:upload` | proto+engine+TUI | Байты образа через RPC вместо server-side пути |

## Non-goals

- Forms View и весь HII-интерактив — дуга цикла B.
- Search-results panel (постоянная панель результатов поиска) — по-прежнему отложено; FIND/GO
  закрывает насущную потребность «прыгнуть на секцию».
- Визарды/multi-field промпты — отложено (решение цикла 2026-08-12).
- Мутации ME-региона: A6 даёт отображение, билдер сериализует регион as-is; правка FTPR-модулей —
  вне scope.
- Gateway на `ImageUpload` (переезд gateway с tmp-file hack на новый RPC) — отдельный TODO-пункт,
  остаётся в «Gateway + WebUI rework».
- GBE/PDR-регионы дескриптора — только BIOS и ME.

## Задачи и архитектура

### A1 — Метки узлов (TUI)

`ui/tree.rs` собирает строку из `node.name`; для пустого `name` — fallback-цепочка
(`App::node_label(&TreeNode) -> String`):

- type 62 (Image): `ImageInfo.name` по `active_image_id` из `app.registry`, fallback `image_id`,
  финальный fallback `"Image"`.
- type 65 (Volume): `"Volume"` (после A6 уточняется до `"Volume (ME)"`/`"Volume (BIOS)"`, если
  регион опознан — поле в `Node` из A6).
- прочие: `file_type_name_or_raw`/`section_type_name_or_raw` по subtype (механика уже в
  `details_text`, `app.rs:245-254`) + GUID-префикс как сегодня для файлов.

Details-панель не меняется (она уже показывает имена). Гейт: на реальном образе в дереве нет
визуально пустых строк (кроме genuinely-безымянных RAW с subtype-меткой).

### A2 — Name-lift через обёртки (engine)

`node_name` (`parser/image.rs:225-232`) инспектирует только прямых детей файла. Фикс: рекурсивный
спуск через детей-секции с subtype `EFI_SECTION_COMPRESSION` / `EFI_SECTION_GUID_DEFINED`
распарсенные дети уже в дереве) до первой `EFI_SECTION_UI` / `EFI_SECTION_VERSION`. Глубина
спуска ограничена 8 (защита от дегенеративных вложений), циклы невозможны (дерево). PEI-кейс (прямой UI-ребёнок) не ломается —
спуск добавляется, прямой поиск остаётся первым шагом.

Гейт (real-image, HNX99TF): DXE Setup-файл (`1/28`, имя сегодня пустое — UI-секция живёт в
`1/28/1/1` за GUIDED/LZMA) получает имя; TUI/CLI `node list` показывают его. Регресс-гейт: PEI
имена (`2/2 WtdPei`) на месте.

### A3 — Prune Remove-узлов после flush (engine)

Контекст — TODO «Stale-tree: in-memory дерево не синхронизируется» (issue II-bis, вариант 2).
После успешного `build_image` + записи в `flush_image` (`rpc/server.rs:31-66`):

1. Рекурсивно удалить из `Image.root` узлы с `action == Action::Remove`.
2. Сбросить `Action::Rebuild`/`Action::Replace` → `NoAction` (изменение уже в байтах; pending
   отображение после write-through ложно).

Контракт с issue IV (узлы за не-рекомпрессируемыми барьерами): prune выполняется только для
поддеревьев, которые билдер реально пересобрал. `ops::remove` сегодня отказывает на
`MutationBehindCompression`-путях (см. `resolve_writable_path`, `hii/mod.rs:181`) — тот же guard
должен стоять в `ops::remove`; план проверяет и при необходимости добавляет. Prune в ветке успеха
flush: если build упал — дерево не трогаем (pending остаётся видимым, это честно).

Гейт (real-image): `remove` → `save` → `image_nodes_list`: узла нет, маркеров `-`/`~` нет; повторный
`open` того же образа — то же дерево.

### A4 — FIND/GO (TUI)

- `:goto <path>` — нормализация через `tree::segments` (уже хэнделит `""` и `/`), поиск узла с
  точным `path`; expand всех предков по префиксам сегментов; cursor на позицию узла в
  `visible_rows()`. Не найдено — status_msg с ошибкой.
- `/` в Normal mode — `enter_insert_mode("goto", "goto ")`: тот же путь через промпт.
- `Tab`-переключение View (цикл B) не конфликтует: `/` остаётся за goto в Image-view (в Forms-view
  `/` станет фильтром — контракт цикла B).

### A5 — Tab-completion + общий парсинг флагов (common+TUI)

- Новый модуль `uefi-common::cli`: `parse_node_flags` (перенос `parse_node_cmd_args` из
  `uefi-tui/src/commands.rs:54` и его двойника из CLI `node.rs`), контракт ровно-один-из
  `--file`/`--artifact-id` сохраняется. CLI и TUI переходят на общий парсер (поведение
  идентично, тесты переезжают/дублируются на общий модуль).
- Tab-completion в cmdline (`commands.rs::complete(app, &cmdline) -> Option<String>`):
  - первая лексема → имя команды/алиас;
  - лексема начинается с `--` → флаг текущей команды;
  - позиция значения `--artifact-id` → id из `app.registry.artifacts`;
  - первая non-flag позиция мутаций → path из `app.tree` (видимые узлы).
- Один Tab — дополнение до общего префикса/единственного варианта; повторный — ничего (список
  вариантов в status_msg, popup не делаем).

### A6 — ME/FTPR-регион (engine)

Сегодня первый ребёнок образа (ME, path `"0"`) парсится одной «секцией» — ложное `_FVH`-совпадение
или обрыв FFS-скана (TODO «Том ME показывает только одну секцию»). Замена на честную модель:

1. **Intel Flash Descriptor**: сигнатура FLVALSIG `0x0FF0A55A` @0x0 → FLMAP0 → region table
   (FLREG: base/limit × 0x1000 для BIOS/ME). Референс — `../refs/UEFITool-ai-fork/common/ffsparser.cpp`
   (parseFlashDescriptor / region-логика).
2. **ME-регион**: synthetic-узел Region (аналог Volume) с детьми из FTPR-парсера: partition header
   → module entries (ASCII-имя, type, attrs, offset/size). Отображение как у UEFITool («FTPR» +
   модули); референс — тот же ffsparser (ME/FTPR-часть).
3. `Node` получает опциональное поле `region` (string, "" = не опознан) — A1 использует для
   `"Volume (ME)"`. **Билдер не меняется**: регион-узлы сериализуются как raw slice по
   region-base/limit (ME не пересобирается никогда — мутации через ops на регионе запрещены,
   ошибка как на компрессионном барьере).
4. Дескриптор не найден (не full-flash образ) → поведение как сегодня, ничего не ломаем.

Scope-ограничение: HNX99TF-класс ME (поколение проверяем на живом образе); незнакомые FTPR-версии —
graceful degrade (регион-узел без детей, warn в лог), не краш.

Гейт (real-image): ME-регион показывает >1 узла с именами FTPR-модулей; сверка дампа с UEFITool
вручную; BIOS-регионы/DXE/PEI-дерево байт-в-байт не меняется (гейт `image_nodes_list` на FV-часть +
round-trip save без диффа).

### A7 — ImageUpload RPC + `:upload` (proto+engine+TUI)

- Proto: `rpc ImageUpload(ImageUploadRequest) returns (ImageOpenResponse)`;
  `ImageUploadRequest { string session_id = 1; bytes data = 2; ImageMode mode = 3; string name = 4; }`
  (симметрия с `ImageOpenRequest`; mode — read/write как у open). Ответ переиспользует
  `ImageOpenResponse` — это и есть open из байтов.
- Сервер: вынести общую storage-логику записи (сегодня в `flush_image`/`image open`-пути), записать
  blob → та же схема хранения/сессии, что у open-by-path. **gRPC message size:** образ 16MB >
  дефолтного лимита tonic (4MB) — поднять `max_decoding_message_size` (например до 64MB) на
  сервере и `max_encoding_message_size` на клиенте; unix-socket, локально дёшево.
- TUI: `:upload PATH` — читает файл из FS клиента, шлёт RPC. Использование — docker/remote-кейсы
  (client FS != server FS); локально `:open` остаётся путём сервера.

## Порядок и зависимости

A2 → A1 (метки получают имена из lift'а), A3 (независима), A4, A5, A6 → A1-уточнение (region-метка), A7.
Рекомендуемый порядок задач в плане: A2, A1, A3, A4, A5, A6, A7 — engine-фиксы раньше, TUI-фичи
после; A7 последняя (требует proto-смены + size-настройки, наименьший риск для остального).

## Тесты

- Каждая задача — TDD (AGENTS.md порядок: тесты → падение → реализация → зелёный → commit).
- Real-image гейты (`refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, `#[ignore]`-класс): A2
  (Setup-имя DXE), A3 (remove→save→list чист), A6 (ME-узлы + FV-регрессия + round-trip).
- TUI-задачи (A1/A4/A5) — юнит на чистых функциях + mock_server integration
  (`tests/mock_server.rs` расширяется по мере надобности).
- A7 — integration на unix-socket (мини-сервер в тесте, upload 16MB фикстуры) + size-limit тест.

## Decisions log

- **D1:** Максимальный состав цикла — решение владельца 2026-09-11 (ME + upload включены, а не
  отложены).
- **D2:** A3 — вариант 2 ревизии issue II-bis (prune после успешного flush), не re-parse (~700мс) и
  не фильтр в list_recursive (перекладывает логику на клиентов).
- **D3:** A6 — отображение без мутаций: ME-регион сериализуется raw, ops на нём запрещены.
  Регион-классификация Volume — через новое поле `Node.region`, не через GUID-угадывание.
- **D4:** A7 — bytes-in-one-message (не чанки): unix-socket, 16MB, лимит поднимается настройкой;
  чанкинг добавим только если появятся remote-gateway ограничения.
- **D5:** Tab-completion без popup-списка (варианты в status_msg) — минимальный UI, popup можно
  добавить позже не ломая контракт функции.
