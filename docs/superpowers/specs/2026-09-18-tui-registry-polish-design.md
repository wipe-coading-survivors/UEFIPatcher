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
EX-COMMANDS невидима (TODO:3369). Остаток V3-мелочей: `fmt_u32_ids` и ассерт
пустого forms-списка (TODO:3325). Фичесет согласован с владельцем в
брейнсторме 2026-09-18 (см. «Решения владельца»).

## Goals

| # | Задача | Суть |
|---|--------|------|
| R1 | Registry: режим + полный UUID | Бейдж `R`/`W` у образов; полный UUID выбранной строки в hint-баре при `focus == Registry` |
| R2 | Completion: image-ID слоты | `image switch/close <TAB>`, `reopen --image-id <TAB>` → кандидаты-образы; `--mode` становится cmd-зависимым |
| R3 | Write-UX: `w` + `:reopen` | `:reopen [--mode write|read] [--image-id <id>]` с гвардом READ→WRITE; клавиша `w` — тот же флоу |
| R4 | Smart prefill `i`/`r` | Registry-курсор на артефакте → prefill `--artifact-id <id>`, иначе `--file ` |
| R5 | `:export` absolutизация | PATH через существующий `absolutize_path`; дефолт без PATH — `cwd/<artifact_id>` |
| R6 | Help-скролл | Модальный скролл j/k/↑/↓/PgUp/PgDn с clamp по высоте |
| R7 | V3-мелочи остаток | Хелпер `fmt_u32_ids`; ассерт-тест «пустой forms → add_prefill None» |

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

## Design

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

- `cmd == "image"`, `head.len() == 2` (позиционный слот после `switch`/`close`)
  → кандидаты `image_id` всех образов registry с префикс-фильтром.
- `cmd == "reopen"`: флаги `["--mode", "--image-id"]`; значение `--image-id`
  → те же кандидаты.
- Существующее правило `head.last() == "--mode"` даёт `into/before/after`
  (грамматика `insert`) — становится cmd-зависимым: `insert` → позиционные
  режимы, `reopen` → `read|write`. Бранч по `cmd` до общего правила.

Механика приёма — существующее меню (popup над cmdline, live-фильтр,
Tab/Enter/→), без изменений. Тесты — зеркало артефактных
(`complete_artifact_id_offers_registry…`).

### R3: Write-UX — `:reopen` и клавиша `w`

Грамматика: `:reopen [--mode write|read] [--image-id <id>]`. Дефолты:
`--mode write`, образ = `--image-id` > (focus==Registry и строка-образ) >
активный. Разрешение образа — из `app.registry.images` (там `path` и `mode`).

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

## Тестирование

- Юнит: `mutation_prefill` (артефакт/образ/нет строки × insert/replace),
  `export_output_path` (явный/дефолт), `fmt_u32_ids`, completion-ветки
  (`image switch/close`, `reopen --mode/--image-id`, cmd-зависимый `--mode`),
  гвард-матрица reopen (pure-часть).
- TestBackend-рендер: бейдж R/W в строках образов, полный UUID выбранной
  строки в hint-баре при `focus == Registry`, clamp help-скролла.
- Integration (mock-сервер, паттерн цикла 3): `:reopen` READ→WRITE —
  последовательность open(WRITE)+close(old)+switch; no-op ветки не шлют RPC.

## Живой гейт (HNX99TF, владелец)

Чек-лист: бейдж режима у образов; `w` на read-only образе → write, мутации
проходят; `:reopen` на уже-write → статус-нооп; `i`/`r` при курсоре на
артефакте → prefill `--artifact-id`; `image switch <Tab>` → меню образов;
`:export ID` без PATH → файл в cwd; help на низком терминале — докручивается
до EX-COMMANDS. §Вердикт дописывается после гейта.

## Процесс

Ветка `tui-registry-polish`, PR в master по образцу cmdline-цикла (PR #16):
спека → план (`docs/superpowers/plans/2026-09-18-tui-registry-polish.md`) →
реализация по задачам с TDD → живой гейт → закрытие TODO-пунктов
(826-851, 808-824, 2760 TUI-часть, 3369, 3379-3387, 3325-остаток) закрывающим
docs-коммитом.
