# TUI registry & flow polish — Design

**Date:** 2026-09-18
**Scope:** `uefi-tui` (client-side only; proto/engine без изменений)
**Related:** TODO-разделы: Registry UUID (`TODO.md:826-851`), конфликт режимов
replace (`TODO.md:808-824`), write-UX (`TODO.md:3379-3387`), absolutизация
output_path (`TODO.md:2760-2765`, TUI-часть), help-скролл (`TODO.md:3369-3378`),
V3-мелочи остаток (`TODO.md:3325-3340`). Цикл cmdline UX
(`2026-09-18-tui-cmdline-ux-design.md`, PR #16) — надстраиваемся над его
completion-меню и hint-баром. Второй цикл из декомпозиции брейнсторма
2026-09-18 (первый — cmdline UX).

## Motivation

Registry-панель режет `image_id`/`artifact_id` до 8 символов (`short()`,
`ui/registry.rs:80`, применение `:34`/`:44`) — полный UUID нигде не виден.
Режим открытия образа (read/write) не показывается вовсе; чтобы сделать
HII-мутации, приходится заново набирать `:open <путь> --mode write`
(TODO:3379). Prefill `i`/`r` хардкодит `--file ` даже когда Registry-курсор
стоит на артефакте (TODO:808, вариант A). `:export` шлёт output_path verbatim
(движок с 2026-09-02 отвергает относительные), а без PATH дефолтит в голый
`cwd` — каталог вместо файла (латентный баг; CLI дефолтит в `cwd/<id>`).
HELP — 70 строк одним Paragraph без скролла: на низких терминалах секция
EX-COMMANDS невидима (TODO:3369). Скролла нет и у details-панелей: «Form» в
Forms View (`ui/forms.rs:89`) обрезает длинные формы — маркер `>`
вопроса-курсора и блок question_info (живёт в конце текста) уходят за кадр;
Details основного вида (`ui/details.rs:22`) — то же, а j/k в его фокусе
вообще мертвы. Списки Forms View (формы, strings) пересоздают
`ListState::default()` каждый кадр (`ui/forms.rs:69,109`): офсет не
переживает рендер, выделение прилипает к нижнему краю — ход вверх после
прокрутки скроллит страницу вместо прогулки курсора; margin-поведения дерева
(курсор останавливается за 3 строки до края) нет. Остаток V3-мелочей:
`fmt_u32_ids` и ассерт
пустого forms-списка (TODO:3325). Наконец, грамматика образовых глаголов
разорвана: `open/save/upload` — top-level, `switch/close` — под `image`, а
completion после `image ` не предлагает субкоманд вовсе (мёртвый конец
`:im<Tab>`). Фичесет согласован с владельцем в брейнсторме 2026-09-18
(см. «Решения владельца»).

## Goals

| # | Задача | Суть |
|---|--------|------|
| R0 | Грамматика: образовые глаголы наверх | `switch`/`close` top-level; `:image` остаётся только view-переключателем (паритет `:forms`); чистый разрыв, без алиасов |
| R1 | Registry: режим + полный UUID | Бейдж `R`/`W` у образов; полный UUID выбранной строки в hint-баре при `focus == Registry` |
| R2 | Completion: image-ID слоты | `switch/close <TAB>` (слот-1) → кандидаты-образы; `--mode` cmd-зависимый |
| R3 | Write-UX: `w` + `:reopen` | `:reopen [--mode write|read]` (без `--image-id`) с гвардом READ→WRITE; клавиша `w` — тот же флоу |
| R4 | Smart prefill `i`/`r` | Registry-курсор на артефакте → prefill `--artifact-id <id>`, иначе `--file ` |
| R5 | `:export` absolutизация | PATH через существующий `absolutize_path`; дефолт без PATH — `cwd/<artifact_id>` |
| R6 | Help-скролл | Модальный скролл j/k/↑/↓/PgUp/PgDn с clamp по высоте |
| R7 | V3-мелочи остаток | Хелпер `fmt_u32_ids`; ассерт-тест «пустой forms → add_prefill None» |
| R8 | Скролл details-панелей | «Form» в Forms View — авто-follow маркера вопроса; Details основного вида — ручной скролл (оживляем мёртвый фокус) |
| R9 | Скролл списков Forms View | Персистентный офсет + `SCROLL_PAD`-margin — паритет с деревом; лечение прилипания выделения к нижнему краю |

## Non-goals

- Clipboard/`y` (OSC52/arboard) — выкинут решением владельца: подстановка из
  registry через completion покрывает ввод ID, волатильность системного буфера
  не нужна.
- «Форма под формой» (TODO:3341) — варианты B/C трогают движок; отдельная тема.
- Gateway/WebUI-часть absolutization (TODO:2760 остаётся открытой для gateway).
- Varstore/question-id дискавери и обёртки HiiFormAdd (TODO:490, TODO:496) —
  engine-walker, отдельный цикл.
- Ctrl+Home/End/Delete затенение в `map_key`, `App::new` fs I/O, `Buffer::get`
  deprecation — наследие cmdline-цикла/ratatui-upgrade, не registry-домен.
- Prefix-match по ID на стороне движка (ввод полного UUID больше не нужен —
  меню/prefill закрывают).
- CLI-грамматика не меняется: noun-first (`image|node|artifact|hii`)
  осознанно сохраняется — CLI скриптуется, при последующем чтении даёт
  однозначное понимание (решение владельца 2026-09-18). TUI↔CLI-расхождение —
  политика (TUI verb-first интерактивен, CLI noun-first скриптуем), не дефект
  (ср. TODO:3318).

## Решения владельца (брейнсторм 2026-09-18)

1. Полный UUID — вариант (b): short-ID в списке остаются, полный UUID выбранной
   строки живёт в hint-баре (не двухстрочный рендер, не полный UUID в списке —
   деградация узких терминалов).
2. Вместо copy-paste — подстановка: image-ID слоты получают completion-кандидаты
   (как артефакты); вопрос «OSC52 vs arboard» снят, зависимостей не добавляем.
3. `:reopen` — только READ→WRITE: гвардом закрываем класс data-loss (reopen
   перечитывает образ с диска — повторный open WRITE-образа молча выбросил бы
   несохранённые мутации). Старый образ закрываем после switch.
4. Help — скролл (j/k + PgUp/PgDn), без реорганизации текста.
5. Грамматика TUI — вариант (A) «вынос до конца»: `switch`/`close`/`reopen`
   top-level, субкоманды `image` умирают; bare `:image` остаётся
   view-переключателем (паритет `:forms`, обнаружено при рекогне плана).
   `--image-id` у reopen выкинут (таргетинг: registry-строка или активный
   образ; неактивный — сначала `:switch`). Чистый разрыв, без алиасов.
6. CLI остаётся noun-first (`image|node|artifact|hii`) — скриптуемость и
   однозначность при чтении скриптов.
7. R8 (скролл details-панелей) добавлен по ревью спеки: Forms View —
   авто-follow маркера вопроса и question_info; основной вид — ручной скролл
   с оживлением мёртвого фокуса Details.
8. R9 (скролл списков Forms View) добавлен по ревью спеки: списки форм и
   strings пересоздают ListState каждый кадр — офсет теряется, выделение
   прилипает к нижней кромке, вверх скроллит страницу вместо курсора.
   Поведение — как в дереве: персистентный офсет + `SCROLL_PAD = 3`
   (курсор останавливается за 3 строки до края, затем скролл).

## Design

### R0: Вынос образовых глаголов наверх

Диспетч `execute_command` (`commands.rs`): тела `"switch"`/`"close"`
выносятся из вложенных веток в top-level (слот-1 = ID). Ветка `"image"`
сохраняет ТОЛЬКО bare-поведение — переключение вида обратно в Image-view
(паритет `:forms`, `commands.rs:494`); `:image <что-либо ещё>` — ошибка с
подсказкой «image subcommands moved: :switch/:close». Сопроводительные
правки:

- `const COMMANDS` (`commands.rs:863`) — `"image"` остаётся, добавляются
  `"switch"`, `"close"` (позже — `"reopen"`).
- Внутренний вызов `image switch {id}` в `handle_registry_enter`
  (`main.rs:166`) → `switch {id}`.
- HELP-текст (`ui/help.rs`) — строка `:image switch ID | :image close [ID]`
  → `:switch ID | :close [ID]`; bare-строка `:image` (возврат в Image-view)
  остаётся.

Итоговая грамматика: образы — `open save upload switch close reopen
snapshot(s) restore`; узлы — `extract insert replace remove rebuild goto`;
артефакты — `artifacts export import`; HII — `hii <существительное> …`
(существительное остаётся: formset/form/question/page — их много).

### R1: Registry — бейдж режима и полный UUID в hint-баре

Рендер строк образов (`ui/registry.rs:34`) получает суффикс-бейдж из
`ImageInfo.mode`:

```
► 956ad394  NH-6.bin  16.0 MB  R
```

Артефакты (`:44`) без бейджа — не образы. `short()` остаётся.

Hint-бар (механизм C6 cmdline-цикла, фокус-зависимая строка `ui/mod.rs:68`):
при `focus == Registry` и существующей выбранной строке префиксом добавляется
полный ID — `956ad394-…-полный  j/k select · Enter pick · w write-mode · …`.
UUID идёт первым сегментом, чтобы клипование на узких терминалах резало
хвост-подсказки, а не ID. Источник — модель данных (`app.registry`), не
рендер-строка. Текст подсказки Registry дополняется клавишей `w write-mode`.

### R2: Completion — image-ID слоты

`context_candidates` (`commands.rs`):

- `cmd == "switch" | "close"`, `head.len() == 1` (позиционный слот-1 после
  R0-выноса) → кандидаты `image_id` всех образов registry с префикс-фильтром.
- `cmd == "reopen"`: флаг `--mode`.
- Существующее правило `head.last() == "--mode"` даёт `into/before/after`
  (грамматика `insert`) — становится cmd-зависимым: `insert` → позиционные
  режимы, `reopen` → `read|write`. Бранч по `cmd` до общего правила.

Механика приёма — существующее меню (popup над cmdline, live-фильтр,
Tab/Enter/→), без изменений. Тесты — зеркало артефактных
(`complete_artifact_id_offers_registry…`).

### R3: Write-UX — `:reopen` и клавиша `w`

Грамматика: `:reopen [--mode write|read]` — без `--image-id` (решение
владельца: таргетинг из UI-контекста, не из флага). Дефолты: `--mode write`,
образ = (focus==Registry и строка-образ) > активный. Разрешение образа — из
`app.registry.images` (там `path` и `mode`).

Матрица гварда (до любых RPC):

| Текущий режим | `--mode write` (дефолт) | `--mode read` |
|---------------|-------------------------|---------------|
| READ          | флоу выполняется        | статус «already read-only», no-op |
| WRITE         | статус «already in write mode», no-op | статус «refusing: unsaved write-mode image; :save first» |

Флоу (READ→WRITE): `ImageOpen(path, WRITE)` → новый `image_id` →
`app.active_image_id`/`client.state.active_image_id` = новый →
`ImageClose(old_id)` → `refresh_tree` + `refresh_registry` → статус
`reopened <name> in write mode`. Дерево и курсор сбрасываются (образ формально
новый) — неотъемлемо, компенсируется статус-сообщением. Повторный open того же
пути валиден: `image_open` каждый раз читает файл с диска
(`rpc/server.rs:298`), старый read-only образ потерь не несёт — закрываем.

Клавиша `w` (Normal, Tree/Registry focus; в Forms-view не добавляем):
разрешает цель тем же порядком и зовёт тот же хелпер `commands::reopen(...)`.
Клавиша свободна (Ctrl+W — kill-word только в Command-режиме, конфликта нет).
Hint Registry (R1) упоминает `w write-mode`.

### R4: Smart prefill `i`/`r` (вариант A, TODO:808)

`handle_normal` (`main.rs:81-99`): при `i`/`r` проверяем
`app.current_registry_row()` — `RegistryRow::Artifact(i)` с существующим
артефактом → prefill `insert|replace <path> --artifact-id <id> `, иначе
текущий `--file `. Логика — pure-хелпер `mutation_prefill(kind, path, row,
artifacts) -> String` в `commands.rs` (юнит-тесты без client/async).
Enter-на-артефакте (`main.rs:173`) остаётся как есть.

### R5: `:export` absolutизация

Ветка `"export"` (`commands.rs:322`): `parts.get(2)` → `absolutize_path(p)`
(хелпер уже в `commands.rs:143`, `:save` использует); дефолт без PATH —
`current_dir().join(artifact_id)` вместо голого `cwd` (паритет с
`resolve_output_path` CLI, `uefi-cli/commands/artifact.rs:45`; чинит
отправку каталога). Дефолт-вычисление — pure-хелпер
`export_output_path(arg, artifact_id) -> String` + тест. Перенос в
`uefi-common` не нужен: CLI-вариант остаётся у CLI, gateway — отдельный пункт.

### R6: Help-скролл

`App.help_scroll: u16` (сброс при открытии). При `show_help` — первой
проверкой в `handle_normal` (до Tab/view-диспетча, help полностью модальный):
`?` — toggle, `q` — quit,
`j`/`↓` +1, `k`/`↑` −1, `PgDn` +10, `PgUp` −10, прочее — поглощается
(модальный help). Clamp — в рендере: `offset.min(len − visible)` через
`Paragraph::scroll((offset, 0))` (`ui/help.rs`). Текст HELP не меняется.

### R7: V3-мелочи остаток

- `fmt_u32_ids(ids: &[u32]) -> String` — join через `,`; заменить
  идентичные блоки в `formset_add_status`/`form_add_status` (`commands.rs`).
- Ассерт-тест: пустой `forms`-список → `add_prefill` возвращает `None`.

### R8: Скролл details-панелей

Тот же механизм, что R6 (offset + clamp в рендере по высоте области минус
рамка), для двух оставшихся `Paragraph` без скролла:

- **Forms View, панель «Form»** (`ui/forms.rs:89`): `details_scroll` в
  состоянии forms (сброс при смене выбранной формы и перезагрузке вопросов).
  Авто-follow: движение `question_cursor` (j/k в Details focus) докручивает
  скролл, чтобы строка с маркером `>` оставалась в кадре; появление
  `question_info` (блок в конце текста) докручивает к его началу. Позиции
  строк — при построении текста: `form_details_text` возвращает не только
  String, но и индекс строки маркера / начало info-блока (контракт уточнит
  план; `.wrap`-переносы в этих строках редки — текстовых строк достаточно).
  Ручной скролл вдобавок: PgUp/PgDn ±10.
- **Основной вид, панель Details** (`ui/details.rs:22`): `details_scroll` в
  App (сброс при смене выбранного узла); ручной скролл j/k/↑/↓ ±1, PgUp/PgDn
  ±10 — фокус Details существует, но клавиши сейчас мертвы
  (`handle_normal`: `Focus::Details => {}`), оживляем их под скролл.

### R9: Скролл списков Forms View — паритет с деревом

Причина бага: `ListState::default()` создаётся на каждом рендере
(`ui/forms.rs:69` — список форм, `:109` — strings), офсет обнуляется, и
ratatui минимально сдвигает его до «выделение на нижней кромке» — отсюда
построчный скролл страницы вместо прогулки курсора, в обе стороны.

Фикс — паттерн дерева/registry (`ui/tree.rs:57-61`, `ui/registry.rs:59-67`):
персистентный `ListState` в состоянии forms (отдельно для списка форм и
strings) + перед рендером
`compute_scrolled_offset(cursor, prev_off, inner_h, total, SCROLL_PAD)`
(`tree.rs:53`). `SCROLL_PAD = 3` — паритет с деревом (`ui/tree.rs:11`).
Для strings курсор и total — в координатах видимого (отфильтрованного)
подсписка, как сейчас считает `render_strings`.

## Тестирование

- Юнит: `mutation_prefill` (артефакт/образ/нет строки × insert/replace),
  `export_output_path` (явный/дефолт), `fmt_u32_ids`, completion-ветки
  (`switch/close` слот-1, `reopen --mode`, cmd-зависимый `--mode`;
  `COMMANDS` содержит `switch/close/reopen` и не содержит `image`),
  диспетч после R0 (`switch`/`close` top-level работают, `image …` —
  unknown command), гвард-матрица reopen (pure-часть).
- TestBackend-рендер: бейдж R/W в строках образов, полный UUID выбранной
  строки в hint-баре при `focus == Registry`, clamp help-скролла; R8 —
  длинная форма: маркер `>` в кадре при прогулке question_cursor за низ,
  появление question_info докручивает скролл, смена формы сбрасывает;
  основной вид — j/k в Details focus меняет скролл; R9 — после прокрутки
  вниз ход вверх двигает курсор (офсет на месте), курсор останавливается за
  `SCROLL_PAD` строк до края, затем скроллит; strings-фильтр — офсет в
  координатах видимого подсписка.
- Integration (mock-сервер, паттерн цикла 3): `:reopen` READ→WRITE —
  последовательность open(WRITE)+close(old)+switch; no-op ветки не шлют RPC.

## Живой гейт (HNX99TF, владелец)

Чек-лист: бейдж режима у образов; `w` на read-only образе → write, мутации
проходят; `:reopen` на уже-write → статус-нооп; `i`/`r` при курсоре на
артефакте → prefill `--artifact-id`; `switch <Tab>` → меню образов;
`:image` (bare) по-прежнему возвращает в Image-view; `:image switch` →
подсказка о переносе; `:export ID` без PATH →
файл в cwd; help на низком терминале — докручивается до EX-COMMANDS;
длинная форма (IntelRCSetup) — маркер вопроса в кадре при прогулке j/k,
question_info появляется в кадре; Details основного вида скроллится;
длинный формсет — после прокрутки вверх двигается курсор, не страница,
курсор держится за 3 строки до края.

### Вердикт (владелец, 2026-09-18, живой гейт)

**«ДА ПРОСТО ОХРЕНЕТЬ!!!!»** — цикл принят. Проверено и работает:
completion по путям и ID (`op<TAB> ~/<TAB>` — живой поиск; `:swi<TAB><TAB>`
меню образов; `:ext<TAB>`/`:exp<TAB><TAB>` артефакты), `w` на read-only →
write (статус «reopened … in write mode»), extract → артефакт в реестре,
export из реестра, скролл Forms View (вверх двигает курсор, не страницу),
скролл help.

**Найден дефект дизайна R8 (единственный):** панель «Form» — один скроллируемый
Paragraph на всё (шапка формы + список вопросов с маркером + question_info).
На первом вопросе details ниже сгиба; при скролле строки details накладываются
на вопросы (визуальные артефакты); на последнем вопросе details появляются,
шапка панели уезжает. Предложение владельца — колоть панель на три зоны:

- а) шапка ~7 строк, фиксированная: детали формы + пустая строка + шапка
  вопросов (промпт, qid, …);
- б) скроллируемые вопросы — паттерн Image/Forms View (гистерезис,
  курсор за 3 строки до края) — «ОК»;
- в) низ ~5 строк, фиксированный: детали выбранного вопроса.

Вынесено в TODO («Форма-панель: трёхзонный layout») — следующий цикл.

## Процесс

Ветка `tui-registry-polish`, PR в master по образцу cmdline-цикла (PR #16):
спека → план (`docs/superpowers/plans/2026-09-18-tui-registry-polish.md`) →
реализация по задачам с TDD → живой гейт → закрытие TODO-пунктов
(826-851, 808-824, 2760 TUI-часть, 3369, 3379-3387, 3325-остаток) закрывающим
docs-коммитом.
