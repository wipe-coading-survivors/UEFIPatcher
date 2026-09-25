# Спека: hii-write-guard — гарды пишущего пути HII (класс B)

Дата: 2026-09-25. Ветка реализации: `hii-write-guard` (спека и её ревью-правки —
docs-коммитами в master по паттерну проекта; код — только в ветке).

Первоисточники: TODO.md поз. 428 (исчерпание string-id 0xFFFF — wrapping_add
в scan_sibt/add_strings_to_body), 1319 (plan_flip/plan_gates без bounds-guard'ов
на hand-constructed Gate), 3240 (rpc hii_question_add — нет отката при сбое
середины цикла), 431 (неоднозначный выбор .rsrc-секции), 3744 (кросс-фаза
unlock между донорами не атомарна); устаревшие записи 2946/2952 (span-арифметика
insert_strings_at_ids* — функции удалены из кода) — закрыть docs-коммитом.
Классификация «класс B: молча порча/полусостояние пишущего пути» —
триаж-сессия владельца 2026-09-25 (продолжение разбиения миноров HII-ядра на
классы A/B/C/D; класс A закрыт циклом hii-read-truth, PR #26).

## Контекст и проблема

Пишущий путь HII в пяти местах молча портит данные или оставляет полусостояние.
В отличие от класса A (врущий вывод), здесь неверные байты едут в артефакт,
который прошивают на железо:

1. **String-id exhaustion (TODO:428)** — `add_strings_to_body`
   (hii/string_pack.rs:46-58) и `scan_sibt` (string_pack.rs:179-251) считают
   next_id через `wrapping_add` (~15 мест). Пакет, заполненный до конца id-
   пространства, молча заворачивает next_id в 0 — невалидный HII string-id
   (0 = NULL-строка) запекается в пакет: добавленные вопросы получают
   prompt/help, которых не существует. Валидные назначаемые id: `1..=0xFFFE`
   (0 — NULL, 0xFFFF — EFI_STRING_ID_INVALID, UEFI spec §HII).
2. **Gate bounds (TODO:1319)** — `plan_flip` (hii/gates.rs:308-332) индексирует
   `body[gate.expr_offset + 1]` (строка 311) и `body[expr_offset + first_len +
   2]` (312-313) без проверок границ; срез `mod.rs:317`
   `&pkg[gate.expr_offset..gate.expr_end.min(pkg.len())]` паникует при
   expr_offset > len. Сегодня безопасность держится на инварианте «Gate
   приходит только из guarded find_gates» — при появлении иного источника
   (сериализация, клиентский ввод) это паника процесса-движка. `apply_flips`
   (gates.rs:393-404) границы уже проверяет — план-фаза не симметрична.
3. **question_add полусостояние (TODO:3240)** — RPC-хендлер
   `hii_question_add` (rpc/server.rs:1146-1209): check-фаза
   (`check_question_add` + `check_ref_add`, 1161-1170) проходит списком, но
   apply-фаза (1171-1200: `add_varstores` → N× `add_question` → N× `add_ref`)
   — по одному без отката. Сбой в середине оставляет частичную мутацию в
   in-memory дереве; она персистится любым последующим save/flush другого
   вызова (write-through). check-фаза не может предвидеть всё: например,
   string-id для 2-го вопроса выделяется в apply и может столкнуться с
   выделением 1-го (после B1 — явный Err-путь).
4. **Кросс-фаза unlock не атомарна (TODO:3744)** —
   `apply_cross_formset_gates` (hii/mod.rs:491+) план/apply чередуются по
   донорскому сайту: ранний донор флипнут и помечен rebuild
   (`mark_rebuild_to_root_by_path`), поздний проваливает планирование
   (`GateExpressionUnsupported`) → unlock вернёт Err, но частичная кросс-мутация
   уже в дереве и сохранится при следующем save. Окно: только мульти-донорские
   образы (у 450x донор один).
5. **Неоднозначный .rsrc (TODO:431)** — собственный предикат секции-носителя
   ресурса `span = max(vsize, raw)` first-match (pe_resource.rs:290-300 в
   rsrc_grow_plan; 637-648 rsrc_raw_end; 661-672 rsrc_virt_end) отличается от
   `object`-предиката min(vsize,raw) в `pe_file_range_at` (используется
   push_leaf:117 для leaf-резолюции). На мусорных/пересекающихся таблицах
   секций каналы чтения и мутации могут выбрать разные секции — рост/патч
   посчитается для одной, лист-данные читаются из другой.

Уже закрытое/не входит: refusal-семантика `plan_rsrc_blob_growth` при leaf за
HII-blob (остаток TODO:2946(2) — переформулировать, refusal = безопасно);
унификация SIBT-кода reader/writer (TODO:173); compress-хардинг lzma_props_byte
(TODO:2957); no-op flush hii_set_value (TODO:2892) — вне скоупа по решению
владельца.

## Решения владельца (брейнсторм 2026-09-25)

- **Состав**: 5 пунктов класса B + docs-актуализация TODO. Compress/no-op-flush
  (L2957/L2892) — в следующий цикл.
- **Атомарность — snapshot-rollback для обоих** (L3240 и L3744), не
  plan-all-then-apply: add_question/add_ref глубоко совмещают планирование и
  мутацию, рефакторинг живых легаси-путей — отдельная дуга. Цена deepcopy
  дерева (~MB-диапазон) приемлема: flush уже делает re-parse ~700мс на 16MB.
- **Гранулярность снапшота — root целиком**: varstores/questions/refs/$SPF
  трогают разные секции дерева; частичный клон хрупче и не даёт гарантии.

## §1 B1: string-id exhaustion — checked-арифметика + явный отказ

Контракт: writer никогда не назначает id вне `1..=0xFFFE` и не читает
next_id, завёрнутый через 0, из существующего пакета.

- `scan_sibt(body, start) -> Option<(u16, usize)>`: все инкременты/скачки
  (SIBT_STRING_*, SIBT_STRINGS_*, SIBT_SKIP1/2, SIBT_DUPLICATE) — через
  `checked_add`; переполнение u16 на обходе существующего пакета → `None`
  (данные пакета не согласованы с id-пространством — писать в него нельзя).
- `add_strings_to_body -> Result<HashMap<String, u16>, StringIdExhausted>`,
  где `struct StringIdExhausted { next: u16, requested: usize }`:
  `next_id.checked_add(1)`; назначение запрещено, когда next_id или
  next_id + strings.len() - 1 выходит за 0xFFFE.
- `add_strings` → новый вариант `HiiError::StringIdExhausted { next: u16 }`
  (рядом с существующим `IdOccupied(u16)`); `add_strings_to_resource` →
  новый `AddStringsToResourceError::IdExhausted`; formset_add/form_add-пути,
  потребляющие resource-канал, маппят его в `HiiError::StringIdExhausted`
  (не в StringPackageNotFound, куда сегодня проваливаются отказы роста).
- RPC: новая рука в `hii_error_status` (rpc/server.rs:54-71) →
  `Status::resource_exhausted("string id space exhausted at {next}")`.
- Читательский близнец: strings.rs:186-191 (`parse_string_package` walk тоже
  wrapping) — при переполнении warn + останов walk (стиль A2 hii-read-truth),
  не Err: листинг не покажет невалидные id, read-путь остаётся толерантным.
- Drive-by (TODO:4382, тот же walk): убрать мёртвый let-else в STRINGS_SCSU/
  SCSU_FONT arm'ах — guard `p >= body.len()` покрывает то же условие.

Тесты: (1) пакет, чей walk доходит до next_id=0xFFFE → `add_strings` → Err
StringIdExhausted, байты пакета не тронуты; (2) scan по пакету со SKIP2-прыжком
через переполнение → None; (3) reader: синтетический пакет с переполнением →
warn + стоп, ни одной записи с id 0; (4) happy-path регрессия: существующие
тесты add_strings_* остаются зелёными (id-выдача не изменилась).

## §2 B2: bounds-guard'ы план-фазы гейтов

Контракт: plan-функции гейтов не паникуют на любом Gate (в т.ч.
сконструированном вручную) и различают «вне границ пакета» и «не E12-класс».

- `plan_flip(body, gate) -> Result<Option<PlannedFlip>, String>`. Pre-check до
  любого индексирования: `gate.expr_offset + 2 <= gate.expr_end.min(body.len())`.
  Для EqConst: чтение first_len и `expr_offset + first_len + 2` — с проверкой
  `< body.len()`; для EqIdVal: `expr_offset + 6 <= body.len()` (литерал
  value@+4..+6). Выход за границы → `Err("gate bounds out of package at
  pkg+{expr_offset:#x}")` — не None (None остаётся = «не hardware-validated
  класс», диагностика не смешивается).
- `plan_gates` / `plan_gates_skip_unlocked` (gates.rs:334, 366) — пробрасывают
  Err; срез региона в их Err-сообщениях и в mod.rs:317 — через
  `body.get(range)` с guard.
- `apply_flips` уже guarded — не трогаем.

Тесты: (1) Gate с expr_offset за телом → Err (не паника); (2) Gate c
first_len, уводящим offset литерала за тело → Err; (3) EqIdVal с
expr_offset+6 > len → Err; (4) валидные E12-гейты — регрессия: flip-офсеты не
изменились (существующие тести + pinned офсеты pkg+0x21/0x2f/0x52/0x58).

## §3 B3: атомарность apply-фаз (snapshot-rollback)

Контракт (rustdoc на обеих точках входа): «apply-фаза атомарна: при Err
дерево байт-идентично состоянию до вызова».

- Хелпер в hii/mod.rs (или переиспользуемый замыканием): до первой мутации
  `let snapshot = image.root.clone()` (FfsNode — Clone, types.rs:148); при Err
  в apply-фазе — `image.root = snapshot` и propagate исходной ошибки.
- **hii_question_add** (rpc/server.rs:1171-1200): снапшот после check-фазы,
  до `add_varstores`; откат в месте любого `?` apply-цикла (varstores /
  questions / refs).
- **apply_cross_formset_gates** (hii/mod.rs:491+): снапшот на входе, откат
  при Err любого донорского сайта — закрывает TODO:3744 без перестройки
  в два прохода.
- Семантика для пользователя: Err = «ничего не применено», повторный вызов с
  исправленной схемой начинается с чистого дерева. Частичных outcomes в
  ответе при Err не бывает (ответ — Err целиком, как сегодня).

Тесты: (1) пакет вопросов, где apply 2-го падает (string-id exhaustion из B1 —
батч, заполняющий хвост id-пространства) → Err; после Err `build_image`-байты
и `list_recursive`-дерево идентичны состоянию до вызова; (2) кросс-фаза:
image с двумя донорскими сайтами, первый flippable, второй
GateExpressionUnsupported → Err; во всём дереве нет флипов и rebuild-маркеров
от этой операции; (3) happy-path: успешный question_add и unlock — поведение
и байты не изменились (существующие тесты).

## §4 B4: единый резолвер .rsrc-секции с отказом при неоднозначности

Контракт: секция-носитель ресурса выбирается одной функцией; при любой
неоднозначности (несколько кандидатов или расхождение с min-семантикой
object) — отказ (None + warn), не «первый попавшийся».

- `resolve_rsrc_section(pe) -> Option<RsrcSpan>` (`RsrcSpan { header_off,
  va, vsize, raw_size, raw_ptr }`) поверх собственной парс-раскладки секций:
  - max-множество = секции, где `rsrc_rva - va < max(vsize, raw)`
    (сегодняшний предикат, покрывает vsize>raw slack — сузивать нельзя);
  - требование: max-множество строго одноэлементно, иначе
    `tracing::warn!("ambiguous .rsrc section mapping")` + None;
  - min-множество = секции, где `rsrc_rva - va < min(vsize, raw)`
    (арифметическое зеркало `object::pe_file_range_at`, без зависимости от
    крейта — таблица та же); если min-множество непусто и выбирает другую
    секцию, чем max — расхождение каналов чтения/мутации → None + warn;
    пустое min-множество (rva в slack-хвосте за raw) — легальный случай,
    max-выбор остаётся.
- Потребители переводятся на резолвер: `rsrc_grow_plan` (pe_resource.rs:
  289-300), `rsrc_raw_end` (629-651), `rsrc_virt_end` (653-674). push_leaf
  (117) остаётся на object — каналы теперь либо согласованы, либо операция
  отказана.
- Refusal — не регресс: неоднозначные таблицы сегодня дают неподдающийся
  диагностике выбор first-match; все реальные образы (HNX/450x/asrock)
  однозначны — существующие real-image гейты остаются зелёными.

Тесты: (1) синтетический PE: две секции с пересекающимися диапазонами,
покрывающими rsrc_rva → resolve/grow/end = None; (2) синтетический PE:
max- и min-множества расходятся (vsize-перекрытие соседа) → None; (3) slack-
case (vsize > raw, rva в slack-хвосте, min-множество пусто) → резолвится,
как сегодня; (4) real-image регрессия: HNX99TF Setup-модуль — все
существующие resource-тесты зелёные (однозначный случай ничего не заметил).

## §5 Порядок работ и зависимости

B1 → B3 (Err-путь B1 — триггер теста атомарности B3). B2 и B4 независимы.
Каждый пункт — TDD (красный тест → реализация → зелёный), коммит на задачу,
после каждой — `cargo test -p uefi-engine` +
`cargo clippy -p uefi-engine -- -D warnings` (+ `--all-targets` на финальном
гейте цикла, по уроку TODO:3443).

Docs-коммит №0 (до кода, rule-11): актуализация TODO — 2946/2952 устарели
(insert_strings_at_ids* удалены; остаток 2946(2) переформулирован в refusal-
семантику plan_rsrc_blob_growth), 428/1319/3240/431/3744 помечены «в цикле
hii-write-guard». Закрытие строк — в финальном docs-коммите цикла с номерами
коммитов по конвенции.

## §6 Не входит (осознанные границы)

- Plan-all-then-apply рефакторинг add_question/add_ref (снимок закрывает
  гарантию; перестройка — при живой потребности, например undo-стек).
- Refusal-семантика plan_rsrc_blob_growth при leaf за blob (безопасна,
  задокументирована).
- Compress-хардинг (lzma_props_byte, total_in-ассерт) и no-op flush
  hii_set_value — следующий цикл (класс B-хвост).
- Проверка PE-checksum после патча (TODO:388) — отдельное решение (ждёт
  живого прецедента отказа).
