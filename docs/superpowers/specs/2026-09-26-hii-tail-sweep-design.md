# Спека: hii-tail-sweep — хвосты трёх HII-циклов (fix-волна)

Дата: 2026-09-26. Ветка реализации: `fix/hii-tail-sweep` (спека/план и их
ревью-правки — docs-коммитами в master по паттерну проекта; код — только
в ветке).

Первоисточники: TODO.md поз. 1416 (add_varstores: mode-чек до
parse_item_id — находка финального ревью hii-errors-cleanup), 1422
(холодный кэш hii_unlock + set_value-клон ради session_id — там же),
87 (тафтологичный тест `write_through_persists_mutation_to_disk` —
code review Plan A, 2026-08-10), 4506 (тихий u32→u16 cast в rk3588-гейте),
4509 (strings-браузер: титул при пустом `:filter`), 4512 (ignore-подсказка
rk3588 с относительным путём) — последние три из ревью hii-read-truth.
Прецедент формы — мини-цикл `cli-polish` (2026-09-10): пачка разнородных
миноров одной веткой, без аддендумов к чужим спекам.

## Контекст и проблема

Циклы hii-read-truth (PR #26), hii-write-guard (PR #27), hii-errors-cleanup
(PR #28) закрыли классы A/B/C, но их финальные ревью оставили хвосты, плюс
один старый должник Plan A. Всё это «формально миноры», но двух классов:

- `errors` — путающие ошибки/асимметрия соседних RPC-хендлеров (1416, 1422);
- `write-path`-гигиена — тест, охраняющий write-through, не ловит
  регрессию (87);
- косметика тестов/TUI (4506, 4509, 4512).

Пачка сознательно НЕ включает: lost-update окно flush_image (дизайн-задача
per-image сериализации, отдельная позиция TODO), клоны у hijack/form_add/
question_add (не названы TODO-позицией, тот же паттерн — при следующей
чистке RPC-хендлеров), generic opcode-walker.

## Решения

### 1. add_varstores: контракт порядка ошибок (TODO:1416, класс errors)

Четвёртый HII-мутатор сохранил старый порядок: `hii::add_varstores`
(hii/mod.rs:1872) проверяет `ImageMode::Write` **до** `parse_item_id`
(1875). В Read-режиме опечатка в target невидима за NotWritable;
проявляется через `hii_form_hijack`/`hii_question_add` (rpc/server.rs).

Контракт — единый с циклом hii-errors-cleanup §1 (InvalidItemId →
NotFound → NotWritable):

1. malformed item_id → `HiiError::InvalidItemId`;
2. цель не резолвится → `NotFound`;
3. `ImageMode != Write` → `NotWritable`.

Реализация: удалить ранний mode-гейт из `add_varstores` — порядок
обеспечивает `parse_item_id` (InvalidItemId) + `resolve_writable_path`
(NotFound → NotWritable → барьер), которые уже вызываются ниже. Ранний
возврат `varstores.is_empty() → Ok(vec![])` не меняется. Покрывается
контракт-тестом по образцу `set_item_visibility_error_order_contract`.

### 2. Холодный кэш hii_unlock + set_value без клона (TODO:1422, класс errors)

После де-клона (errors-cleanup Task, `f498d41`) `hii_unlock`
(rpc/server.rs:1070) идёт сразу в `images.get_mut` — на холодном кэше
(рестарт движка с живой сессией) отдаёт `not_found`, тогда как
`hii_set_value`/`hii_form_hijack` через `get_or_load_image` образ с диска
загружают. Обратная сторона: `hii_set_value` (1127) ради одного
`session_id` берёт полный клон образа через `get_or_load_image` (211) —
тот же паттерн, что снят с unlock.

Решение — хелпер вместо клона:

- из `get_or_load_image` выделяется `ensure_image_loaded(&self, image_id)
  -> Result<(), Status>`: при отсутствии в кэше читает строку БД + байты +
  parse + insert (без clone); `get_or_load_image` переиспользует его и
  возвращает клон из кэша (поведение читателей не меняется);
- `hii_unlock`: `ensure_image_loaded` до блока мутации; `session_id` —
  из img_slot (как сегодня);
- `hii_set_value`: `ensure_image_loaded` вместо `get_or_load_image`,
  `session_id` — из img_slot внутри блока мутации (зеркало unlock);
  полный клон образа исчезает.

Санкционированная спекой errors-cleanup §Риски семантика (fail-loud, без
порчи) сохраняется: no-op не флашит. Хендлеры hijack/form_add/question_add
НЕ трогаются (Non-goal).

Гейты: синтетический тест — рестарт движка → `hii_unlock` с мусорным
item_id → `invalid_argument` (InvalidItemId), а не `not_found` (доказывает
холодную загрузку); ignore-гейт на 450x — рестарт → unlock формы #1
(0 гейтов) → Ok, артефакт байт-идентичен.

### 3. write-through тест: реальная мутация вместо тафтологии (TODO:87, класс write-path)

`write_through_persists_mutation_to_disk` (rpc/server.rs:3447) строит
assert `after != before || after_mtime != before_mtime` — при
byte-стабильном rebuild (volume без файлов) всегда истинно за счёт mtime;
«байты мутации доезжают до диска» не проверяется вовсе. Регрессию
«мутация есть, персиста нет» тест не ловит.

Решение: реально меняющая байты мутация + холодный ре-открытие:

1. открыть fixture_volume в write;
2. `image_node_insert` Into "0" 32-байтового FFS-файла (рецепт
   two_fv_image_with_markers: GUID + type 0x01 + size24 + marker);
3. `assert_ne!(after, before)` — байты изменились; `after.len() >
   before.len()`;
4. рестарт движка (`spawn_engine_on`) → `image_nodes_list` — вставленный
   файл виден из дисковой копии: write-through доказан end-to-end, не
   только фактом перезаписи.

mtime-ассерты убираются как более слабые. Красной фазы нет (тест-гигиена):
гreen на текущем коде обязателен, усиление — в самих ассертах.

### 4. rk3588-гейт: try-конверсия + абсолютная подсказка (TODO:4506, 4512, косметика)

- `real_image_rk3588_bare_questions_resolve` (tests/real_image.rs:1114):
  `f.form_id_ifr as u16` → `u16::try_from(...)` со skip (let-else
  continue) — анти-паттерн, закрытый A3 в handler'е, не должен жить в
  тестах;
- ignore-подсказка того же теста (1105): относительный путь
  `UEFIPATCHER_TEST_FW=refs/fw/...` не резолвится из CWD теста — заменить
  на `$PWD/refs/fw/...` (запуск из корня репо).

Верификация: образ rk3588 присутствует в refs/fw — ignore-тест
прогоняется явно.

### 5. TUI strings-браузер: титул скрытой строки (TODO:4509, косметика)

`render_strings` (uefi-tui/src/ui/forms.rs:176-183) добавляет source в
титул по `strings[strings_cursor]` без учёта фильтра: при `:filter` с
пустым результатом курсор указывает вне visible — титул показывает source
скрытой строки. Guard: source добавляется только если
`visible.contains(&strings_cursor)`.

## Non-goals

- lost-update окно flush_image (3 lock'а) — дизайн per-image сериализации;
- клоны `get_or_load_image` у hijack/form_add/question_add (не TODO-позиция);
- generic opcode-walker (ждёт роста walker'ов);
- класс D (TRUE→FALSE, bare-мутабельность, PE-checksum) — отдельные циклы
  по живому прецеденту.

## Приёмка

- `cargo test --all` зелёный; `cargo clippy --all --all-targets -- -D
  warnings` зелёный; `cargo fmt --all -- --check` зелёный;
- ignore-гейты пачки прогнаны явно (450x-копия, rk3588 — образы в refs);
- TODO-позиции 87, 1416, 1422, 4506, 4509, 4512 закрыты записями
  «Закрыто: цикл hii-tail-sweep» (+коммиты).
