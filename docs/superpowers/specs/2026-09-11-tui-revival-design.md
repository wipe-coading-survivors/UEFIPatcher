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
| A6 | Регионы flash-дескриптора | engine | Полная таблица FLREG (BIOS/ME/GbE/PDR/…), ME с FTPR-парсером, read-only-индикация |
| A7 | `ImageUpload` RPC + `:upload` | proto+engine+TUI | Байты образа через RPC вместо server-side пути |
| A8 | Снапшоты образа | proto+engine+TUI | Именованные точки отката байтов (аддендум; журнал отменён — write-through) |

## Non-goals

- Forms View и весь HII-интерактив — дуга цикла B.
- Search-results panel (постоянная панель результатов поиска) — по-прежнему отложено; FIND/GO
  закрывает насущную потребность «прыгнуть на секцию».
- Визарды/multi-field промпты — отложено (решение цикла 2026-08-12).
- Мутации не-BIOS-регионов (A6): билдер сериализует их as-is; правка FTPR/GbE-содержимого —
  вне scope.
- Внутренний парсинг GbE/PDR-структур (`gbe.h`-класс UEFITool): регионы кроме BIOS/ME — raw-узлы
  без детей.
- Gateway на `ImageUpload` (переезд gateway с tmp-file hack на новый RPC) — отдельный TODO-пункт,
  остаётся в «Gateway + WebUI rework».

## Задачи и архитектура

### A1 — Метки узлов (TUI)

`ui/tree.rs` собирает строку из `node.name`; для пустого `name` — fallback-цепочка
(`App::node_label(&TreeNode) -> String`):

- type 62 (Image): `ImageInfo.name` по `active_image_id` из `app.registry`, fallback `image_id`,
  финальный fallback `"Image"`.
- type 65 (Volume): `"Volume"` (после A6 уточняется до `"Volume (ME)"`/`"Volume (BIOS)"`/… по
  полю `region` из A6; не-BIOS-регионы дополнительно рендерятся dim-стилем `immutable_style`
  из `theme.rs` — см. A6 «Индикация немутируемости»).
- прочие: `file_type_name_or_raw`/`section_type_name_or_raw` по subtype (механика уже в
  `details_text`, `app.rs:245-254`) + GUID-префикс как сегодня для файлов.

Details-панель не меняется (она уже показывает имена). Гейт: на реальном образе в дереве нет
визуально пустых строк (кроме genuinely-безымянных RAW с subtype-меткой).

### A2 — Name-lift через обёртки (engine)

`node_name` (`parser/image.rs:225-232`) инспектирует только прямых детей файла. Фикс: рекурсивный
спуск через детей-секций с subtype `EFI_SECTION_COMPRESSION` / `EFI_SECTION_GUID_DEFINED`
(распарсенные дети уже в дереве) до первой `EFI_SECTION_UI` / `EFI_SECTION_VERSION`. Глубина
спуска ограничена 8 (защита от дегенеративных вложений), циклы невозможны (дерево). PEI-кейс
(прямой UI-ребёнок) не ломается — спуск добавляется, прямой поиск остаётся первым шагом.

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

### A6 — Регионы flash-дескриптора: полная таблица FLREG, ME с FTPR (engine)

Сегодня первый ребёнок образа (ME, path `"0"`) парсится одной «секцией» — ложное `_FVH`-совпадение
или обрыв FFS-скана (TODO «Том ME показывает только одну секцию»). Замена на честную модель:

1. **Intel Flash Descriptor**: сигнатура FLVALSIG `0x0FF0A55A` @0x0 → FLMAP0 → region table
   (FLREG: base/limit × 0x1000). Распознаём **все регионы таблицы** (BIOS, ME, GbE, PDR,
   DevExpansion, SecondaryBIOS, uCodePatch, EC, Ptt — имена по референсу
   `../refs/UEFITool-ai-fork/common/ffsparser.cpp:405` `GbeRegion..PttRegion`): таблица уже
   прочитана, каждый регион — ещё одна FLREG-запись; отсекать произвольно только BIOS+ME нет
   смысла (решение владельца 2026-09-11 при ревью спеки).
2. **Содержимое:** ME-регион — FTPR-парсер (partition header → module entries: ASCII-имя, type,
   attrs, offset/size; референс — тот же ffsparser, ME-часть). BIOS — существующий FV-парсинг
   как сегодня. **Все прочие регионы — raw-узлы без детей** (GbE/PDR/… не парсим внутри;
   полный GbE-парсер `gbe.h`-класса — за scope).
3. `Node` получает опциональное поле `region` (string, "" = не опознан) — A1 использует для
   `"Volume (ME)"` и dim-стиля. **Билдер не меняется**: регион-узлы сериализуются как raw slice
   по region-base/limit (не-BIOS-регионы не пересобираются никогда — мутации через ops на них
   запрещены, ошибка как на компрессионном барьере).
4. Дескриптор не найден (не full-flash образ) → поведение как сегодня, ничего не ломаем.

**Индикация немутируемости** (решение владельца 2026-09-11): не-BIOS-регионы рендерятся dim-стилем
(`theme.rs`: `immutable_style` = DarkGray на имя+маркер, опционально глиф 🔒), details-панель
показывает `Region: ME (read-only)`. Engine-отказ при попытке мутации — вторая линия защиты, UX
должен говорить об этом до попытки.

Scope-ограничение: HNX99TF-класс ME (поколение проверяем на живом образе); незнакомые FTPR-версии —
graceful degrade (регион-узел без детей, warn в лог), не краш.

Гейт (real-image): ME-регион показывает >1 узла с именами FTPR-модулей; GbE/PDR-регионы видны как
raw-узлы с dim-стилем; сверка дампа с UEFITool вручную; BIOS-регионы/DXE/PEI-дерево байт-в-байт
не меняется (гейт `image_nodes_list` на FV-часть + round-trip save без диффа).

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

### A8 — Мутационный журнал (engine) — ЗАМЕНЁН, см. «Аддендум»

> Раздел заменён на снапшоты образа аддендумом от 2026-09-11 (ревью планирования): журнал
> исходной редакции оказался мёртвым кодом — write-through уже персистит каждую правку
> немедленно. Актуальная редакция — «A8' — Снапшоты образа» в аддендуме ниже.

## Порядок и зависимости

A2 → A1 (метки получают имена из lift'а), A3 (независима), A4, A5, A6 → A1-уточнение (region-метка
и dim-стиль), A7, A8 (после A3 — разделяет семантику flush/prune/очистки журнала).
Рекомендуемый порядок задач в плане: A2, A1, A3, A4, A5, A6, A7, A8 — engine-фиксы раньше,
TUI-фичи после; A8 последняя (стоится на A3-контракте очистки).

## Тесты

- Каждая задача — TDD (AGENTS.md порядок: тесты → падение → реализация → зелёный → commit).
- Real-image гейты (`refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, `#[ignore]`-класс): A2
  (Setup-имя DXE), A3 (remove→save→list чист), A6 (ME-узлы + GbE/PDR raw-узлы + FV-регрессия +
  round-trip), A8 (restart-цикл: remove→рестарт→маркер; flush→рестарт→чисто).
- TUI-задачи (A1/A4/A5) — юнит на чистых функциях + mock_server integration
  (`tests/mock_server.rs` расширяется по мере надобности).
- A7 — integration на unix-socket (мини-сервер в тесте, upload 16MB фикстуры) + size-limit тест.

## Decisions log

- **D1:** Максимальный состав цикла — решение владельца 2026-09-11 (ME + upload включены, а не
  отложены); при ревью спеки расширено: полная таблица FLREG вместо BIOS+ME (п.1 ревью) и
  мутационный журнал A8 (п.2 ревью).
- **D2:** A3 — вариант 2 ревизии issue II-bis (prune после успешного flush), не re-parse (~700мс) и
  не фильтр в list_recursive (перекладывает логику на клиентов).
- **D3:** A6 — отображение без мутаций: не-BIOS-регионы сериализуются raw, ops на них запрещены.
  Распознаём **всю** таблицу FLREG (GbE/PDR включены — цена нулевая после чтения таблицы,
  решение владельца при ревью); внутри парсим только ME/FTPR. Регион-классификация Volume —
  через новое поле `Node.region`, не через GUID-угадывание. Немутируемость видна до попытки:
  dim-стиль + "(read-only)" в details; engine-отказ — вторая линия.
- **D4:** A7 — bytes-in-one-message (не чанки): unix-socket, 16MB, лимит поднимается настройкой;
  чанкинг добавим только если появятся remote-gateway ограничения.
- **D5:** Tab-completion без popup-списка (варианты в status_msg) — минимальный UI, popup можно
  добавить позже не ломая контракт функции.
- **D6:** A8 — журнал аддитивной таблицей SQLite, контракты RPC не трогаем; `file_hash`-гвард
  против подмены файла (байты — source of truth, журнал — только консистентное дополнение);
  очистка журнала в той же точке, что A3-prune (успешный flush).
- **D7:** Мотивационная рамка гейтов A3/A8: A3 закрывает «экран врёт после save», A8 закрывает
  «pending теряется на рестарте» — это разные дефекты одной подсистемы и обе части семантики
  «pending-операции видны» (см. TODO:701), восстановленной целиком.
  > D7 и D6 переведены аддендумом от 2026-09-11: write-through закрывает «pending теряется на
  > рестарте» сам по себе; A8' (снапшоты) закрывает потребность «промежуточные результаты = точки
  > отката».

## Аддендум (ревью планирования, 2026-09-11)

Открыто при написании плана реализации (дух rule-11: фиксация до кода).

**Факт (write-through):** все мутационные RPC-хендлеры — `image_node_insert/remove/replace/
rebuild`, все hii-опы, `image_save` — уже вызывают `flush_image` немедленно после операции
(коммит `aa6ed56`; на это же жаловался TODO issue II-bis). «Pending-окна» не существует: каждая
правка уже в байтах на диске до возврата RPC; рестарт движка ничего не теряет
(`get_or_load_image` → re-parse с диска).

Следствия:
- **A3 (prune после flush) — в силе**, и работает не только после `:save`, а после **каждой**
  мутации: экран перестаёт показывать применённое как «запланированное» сразу.
- TUI-маркеры действий (`+`/`-`/`~`/`*`) после A3 недостижимы (prune происходит внутри
  mutation-RPC до его возврата). Рендер-код маркеров остаётся как no-op при NoAction; чистка —
  отдельная косметика, не этого цикла.
- **A8 (журнал pending-операций) мёртв** при write-through → заменён снапшотами (решение
  владельца 2026-09-11: «переводим на снапшоты, аддендумом»).

### A8' — Снапшоты образа (proto+engine+TUI)

Именованные точки отката байтов образа — «промежуточные результаты» в смысле воркфлоу дуг E*
(«откат — E32»): перед рискованной правкой — снапшот, после неудачи — restore.

- **Proto:**
  `rpc ImageSnapshotCreate(ImageSnapshotCreateRequest) returns (ImageSnapshotCreateResponse);`
  `rpc ImageSnapshotsList(ImageSnapshotsListRequest) returns (ImageSnapshotsListResponse);`
  `rpc ImageSnapshotRestore(ImageSnapshotRestoreRequest) returns (Empty);`
  `ImageSnapshotCreateRequest { string image_id = 1; string name = 2; }`,
  `ImageSnapshotCreateResponse { string snapshot_id = 1; int64 created_at = 2; }`,
  `ImageSnapshotInfo { string snapshot_id = 1; string name = 2; int64 created_at = 3; uint64 size = 4; }`,
  `ImageSnapshotsListRequest { string image_id = 1; }`,
  `ImageSnapshotsListResponse { repeated ImageSnapshotInfo snapshots = 1; }`,
  `ImageSnapshotRestoreRequest { string image_id = 1; string snapshot_id = 2; }`.
- **Схема** (аддитивно):
  `CREATE TABLE IF NOT EXISTS image_snapshots (id TEXT PRIMARY KEY, image_id TEXT NOT NULL
  REFERENCES images(id) ON DELETE CASCADE, name TEXT NOT NULL, size INTEGER NOT NULL,
  created_at INTEGER NOT NULL);` + индекс по image_id. Файл:
  `data_dir/sessions/<sid>/images/<image_id>.snapshots/<snapshot_id>.bin` — копия текущих
  байтов образа (write-through уже записал их — отдельный flush не нужен). Каскад сессии/GC —
  как у артефактов (`ON DELETE CASCADE` + purge-флаг при файловой чистке).
- **Create:** строка образа существует → читаем stored `.bin` → копия в файл снапшота → insert
  row. Пустое имя → `snap-<unix_secs>`.
- **Restore:** snapshot row существует → чтение файла снапшота → `atomic_write` поверх stored
  image `.bin` → инвалидация in-memory кэша (`images.remove(image_id)` — следующий доступ
  re-parse'ит) → `touch_image`.
- **TUI:** `:snapshot [NAME]` — создать; `:snapshots` — список в status_msg;
  `:restore <snapshot_id>` — restore + refresh tree/registry.
- **Гейт** (integration-стенд, реальный EngineServer на unix-сокете): open(write) → remove →
  `snapshot "before"` → ещё правка → `restore` → `image_nodes_list` равен дереву на момент
  снапшота; «рестарт» движка (новый EngineServer на том же data_dir) → `snapshots_list` жив;
  restore несуществующего id → not_found.

### Уточнение A5 (открыто при планировании)

CLI-сторона парсинга флагов остаётся на clap derive (эксклюзивность `--file`/`--artifact-id` —
ArgGroup, `main.rs:145-180`); ручного «двойника» парсера там нет. Общий модуль `uefi-common::cli`
получает парсер TUI (`parse_node_flags`/`NodeCmdArgs` + `source()`-валидация ровно-один-из);
CLI на общий модуль НЕ мигрирует.

### Уточнение A1 (модель регионов)

Регион — это `FfsType::Region` (63, слот уже есть в enum), а НЕ Volume: Region-узлы несут свои
метки через `RegionParsingData` («ME region», «GbE region», имена $FPT-партиций), все Volume —
BIOS-тома с меткой `"Volume"`. Dim-стиль `immutable_style` вешается по `node_type == Region`
(63) — включая детей (партиции), автоматически. Поле `Node.region` в proto несёт метку региона
для клиентов.
