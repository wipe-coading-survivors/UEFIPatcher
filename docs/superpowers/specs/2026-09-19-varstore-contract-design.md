# Спека: varstore-контракт — дискаверибельность, валидация, экспозиция

Дата: 2026-09-19. Ветка реализации: `varstore-contract` (спека и её
ревью-правки — docs-коммитами в master по паттерну проекта; код — только
в ветке).
Первоисточники: TODO.md поз. 490 (varstore/question id недискаверибельны),
496 (per-item varstore-валидация пакета), 3029 (граница занятости ids 21–30),
1353 (дефолт сокета `/run`), тестовые долги value-op 2673 / 2676 / 2730 / 2681,
ограничение спеки `2026-09-19-hii-form-export-design.md` §3.6 («экспорт
varstores не заполняет»); брейншторм владельца 2026-09-19.

## Контекст и проблема

Varstore-карта в движке **уже есть** — `values::varstore_map` (hii/values.rs:182)
читает декларации IFR_VARSTORE_OP (0x24) и IFR_VARSTORE_EFI_OP (0x26):
id, GUID, size, имя. Но дальше per-question поля `QuestionInfo.varstore`
(proto) она не экспонирована: формсет-уровневой карты у клиентов нет
(`FormSetInfo` — только guid/title/forms, ifr.rs:439). Следствия:

- автор схемы не может выбрать незанятый varstore-id / question-id и берёт
  магические high-id (конвенция 0x7F00/E16, `max(0x7F00, …)+1` в
  form-export-плане) — TODO:490;
- `hii import` не может сделать per-item pre-check «`var_store_id`
  существует в целевом формсете или объявлен в пакете» — TODO:496 прямо
  блокируется TODO:490;
- экспорт форм (спека form-export) не заполняет `varstores` конверта —
  round-trip в чужой формсет может ссылаться на несуществующий стор.

Второй мисбиндинг-канал, не зафиксированный в TODO (найден брейнштормом):
`form_add` emit'ит декларации `varstores`-блока пакета через
`build_varstores` (hii/form_add.rs:190) **без проверки конфликта id** с уже
существующими в целевом формсете — дубль декларации вставляется молча
(round-trip пакета с varstores в тот же формсет даст два VARSTORE с одним
id). Ссылки items на несуществующие id тоже не валидируются (в
`check_question_add` валидация есть — mod.rs:1566–1575 и 1534–1546 — а в
`form_add` её нет).

Хвост того же контракта: занятость varstore ids 21–30 на 450x не проверена
(TODO:3029; метод — check-режим, прошивка не нужна); тестовые долги
value-op вокруг varstore-карты (2673/2676/2730/2681); дефолт сокета движка
`/run/uefipatcher.sock` требует root и рассинхронён с клиентским дефолтом
`~/.local/state/uefipatcher/uefipatcher.sock` (TODO:1353) — из коробки без
`UEFIPATCHER_SOCK` клиент и движок не встречаются.

## Решения владельца (брейншторм 2026-09-19)

- **Подход B — отдельный RPC `HiiListVarstores` + команда `hii varstore
  list`**. Первоначально предложенный вариант A (расширение
  `HiiListFormsResponse`) отклонён после находки: `HiiListFormsRequest` не
  имеет formset-селектора и агрегирует все формсеты образа — карта в
  агрегированном ответе потребовала бы группировки per-formset, а точечный
  RPC возвращает плоскую карту одного формсета, позволяет ленивую загрузку
  в TUI и буквально отвечает исходной боли «выбрать незанятый id».
- **Тестовые долги value-op — в цикл хвостом** (кластер 3: дёшево,
  локально, не разрастается).
- **TUI-панель varstores** (клавиша `V` в Forms View) — новая задача
  владельца, не записанная в TODO.
- **Попутный микро-фикс сокета** (TODO:1353) — в цикл: движок и так
  правим.
- **NVAR-дискриминатор (TODO:2730)** — пиннинг-тест текущего first-match
  поведения + rustdoc-контракт; смену дискриминатора не делаем (на живых
  образах уникален, YAGNI).

## §1 Движок: varstore_map + name-value декларации

`values::varstore_map` расширяется третьим опкодом
`IFR_VARSTORE_NAME_VALUE_OP` (0x25, r-efi): id u16@+2, имя UCS-2z@+4,
минимальная длина записи 6 → `VarStoreMap { id, guid: None, size: 0, name }`.
Семантика `guid: None`/`size: 0`: name-value стор не даёт ни GUID, ни
границ — в offset-валидациях (`check_question_add` declared_size, set_value)
участвовать не может; его роль в карте — только занятость id и имя.
Формат IFR не меняется ни на чтении, ни на записи (цикл не пишет IFR-дублей
новых видов — см. §6 границы).

## §2 Движок: валидации form_add (закрывает TODO:496)

Два новых чека в `form_add` **до первой мутации образа** (и в check-фазе,
если план выделит check-функцию; зеркало существующих проверок
`check_question_add`):

1. **Per-item**: для каждого item любой формы пакета с `var_store_id != 0`
   (REF/Action и прочие storage-less — пропуск): id должен существовать в
   `varstore_map` целевого формсета **или** быть объявлен в
   `varstores`-блоке пакета → иначе
   `InvalidSchema("var store id {id:#x} is not declared (add it to schema varstores)")`
   — текст зеркален mod.rs:1571.
2. **Дубль декларации**: для каждой декларации `varstores`-блока пакета:
   id не должен существовать в `varstore_map` целевого формсета → иначе
   `InvalidSchema("varstore id {id:#x} already exists in the formset")`
   — зеркало mod.rs:1540. Дубли **внутри** пакета отвергаются тем же
   текстом (первый же повтор).

Новые отказы — осознанные: ранее оба случая проходили молча и давали
мисбиндинг хранилища. Дуги E16/E39/E43 varstore-форм не используют
(TODO:3048) — регрессии исторических сценариев нет.

## §3 Proto/RPC: HiiListVarstores

```proto
message HiiListVarstoresRequest { string image_id = 1; string target = 2; }
message HiiListVarstoresResponse { repeated VarStoreInfo varstores = 1; }
```

- `target` — формсет-таргет по существующей грамматике HII-команд
  (`<guid>:<formset_idx>[:<form_id>]`); резолв тем же парсером, что у
  `form add`; form-часть таргета игнорируется. Неизвестный таргет →
  NotFound.
- `VarStoreInfo { id, guid, size, name }` переиспользуется без изменений;
  для name-value деклараций `guid` — пустая строка, `size` = 0.
- `HiiListFormsResponse` не меняется.
- Движковая функция `list_varstores(image, target) -> Vec<VarStoreInfo>`:
  resolve target → forms-package → `varstore_map`. Порядок — порядок
  деклараций в пакете.

## §4 CLI: `hii varstore list`

- text: по строке на стор — `id=0x0002 guid=EC87D643-99DC-4D14-B25D-8AC6D5C7B27A
  size=0x0094 name="Setup"` (name-value: `guid=- size=0x0000`). Легенда — по
  существующему паттерну `hii_legend`/`HiiLegendCmd`
  (uefi-common/format.rs): третий вариант `HiiLegendCmd::VarstoreList`
  (грамматика target + строка `guid '-' = name-value declaration (no
  GUID, no bounds)`), команда в text-формате делает `eprint!` перед
  печатью списка — как `form list`/`question list` (commands/hii.rs:12–18).
- json: массив VarStoreInfo; tsv: заголовок `id\tguid\tsize\tname`
  (легенда не печатается — tsv машинный).
- Команда read-only (образ в любом режиме), completion по target-грамматике
  `hii form add`.

## §5 TUI: панель varstores (Forms View, клавиша `V`)

- `V` в Forms View переключает на панель varstore-карты **формсета под
  курсором** (тот же дискриминатор, что у трёхзонной структуры); повторное
  `V`/ESC — назад к списку форм.
- Данные — ленивый RPC `HiiListVarstores` при первом открытии панели;
  кэш per formset (guid:idx) в состоянии App, инвалидируется вместе с
  остальным HII-состоянием после мутаций.
- Рендер списка — существующий `ui/scroll.rs` (персистентный ListState);
  колонки как в CLI text. Это четвёртая боковая поверхность после
  strings-браузера (`S`) — тот же паттерн входа/выхода.

## §6 Контракт для form-export (реализация — цикл hii-form-export)

Спека form-export ещё не реализована (плана нет) — этот цикл **не трогает**
её клиентский код (walker/конверт/планнер). Здесь готовятся данные и
закрывается движковая половина ограничения её §3.6:

**Уже в этом цикле (движковая защита, работает для любого клиента):**
валидации §2 — при будущей реализации `hii import` мисбиндинг
`var_store_id` и дубль декларации будут отвергнуты движком независимо от
качества клиентских pre-check'ов.

**Данные для цикла hii-form-export (готовы этим циклом):** RPC
`HiiListVarstores` — источник карты целевого формсета для импорт-планнера
и для заполнения конверта на экспорте.

**Контракт, который цикл hii-form-export реализует у себя (здесь только
фиксируем правила):**

- экспорт: в `formset.varstores` конверта кладутся **только referenced**
  (`var_store_id != 0` items формы) декларации VARSTORE/VARSTORE_EFI из
  карты формсета-источника; форма без storage-биндингов → пусто. Name-value
  стор в `VarStoreSchema` не выражается (типы Buffer/Efi): items ссылаются
  на name-value id → без декларации + `meta.lossy`-строка
  `varstore {id:#x} is name-value, not exportable`;
- импорт-планнер (`uefi-common`, рядом с будущим `plan_ref_step`):
  id свободен в целевом формсете (карта из `HiiListVarstores`) → декларация
  остаётся (движок emit'ит); занят и определение идентично (guid+size+name)
  → drop из bare-тела (round-trip в тот же формсет без §2-отказа); занят
  и отличается → fail fast «varstore id {id:#x} already exists with
  different definition»;
- при планировании hii-form-export снять в его спеке §3.6 пометку
  «блокируется TODO:490» (инфраструктура готова) и забрать правила выше.

## §7 Real-image: карта HNX + зонд ids 21–30 (закрывает TODO:3029)

- **HNX99TF** (`#[ignore]`-гейт, образ `refs/fw/HNX99TF_…E5C88C6F.bin`):
  `list_varstores` корневого Setup — ассерты на известные декларации
  (Setup id 2, GUID EC87D643…, size 0x94 — TODO:1507) и полноту карты
  (id уникальны, все три вида опкодов покрыты живыми данными).
- **450x-зонд** (`#[ignore]`, `refs/fw/450x.bin`): для каждого id 21..30 —
  `check_question_add` с пробной схемой numeric `var_store_id=<id>`,
  `var_offset=0xFFFE`, size 1; текст ошибки различает
  «not declared» (id свободен) и «exceeds var store size» (id занят).
  Результат — таблица занятости 21–30 фиксируется в этой спеке (§7.1)
  после первого прогона и в TODO при закрытии. Check-режим не мутирует
  образ — прошивка и железо не нужны.

### §7.1 Таблица занятости (заполняется первым прогоном зонда)

| id | статус | определение |
|----|--------|-------------|
| 21–30 | TBD (зонд) | — |

## §8 Тестовые долги value-op (кластер 3)

- **2673**: параметризованные тесты `hii/values.rs` — NUMERIC width 2/4/8
  (size-flags @+13) и DEFAULT types 2..4; продакшн-код не меняется.
- **2676**: параметризованный набор на 7 веток `ValueOpUnsupported`:
  record-missing-in-store, nameless varstore, var_offset+width>size,
  width 0/>8, CheckBox>1, Numeric вне диапазона, kind Other — синтетика
  против `validate_set_value`/collect.
- **2730**: пиннинг-тест `find_varstore_record` — first-match по (имя +
  data_len) на сторе с двумя одинаковыми записями (флипается первая) +
  rustdoc-контракт на функции: дискриминатор осознанный, смена — отдельной
  правкой при живом дубле у другого вендора.
- **2681**: `is_store_body` ужесточить до leaf-sections (Section с детьми
  → не стор) + синтетический тест с дочерней секцией.

## §9 Попутный микро-фикс: дефолт сокета (закрывает TODO:1353)

- `bin/engine.rs:38`: дефолт `/run/uefipatcher.sock` →
  `uefi_common::state::default_sock()` (XDG-state, уже документирован в
  AGENTS.md и уже является клиентским дефолтом) + `create_dir_all` на
  parent (зеркало существующего `data_dir`).
- `uefi-gateway/src/config.rs:16`: тот же дефолт из `default_sock()`.
- Явный `UEFIPATCHER_SOCK` (вкл. `/run/...`) не затрагивается — rc-файлы
  владельца и SOL-сессии не ломаются. AGENTS.md правок не требует (код
  выравнивается на документ).

## §10 Тестирование и гейты

- **Unit (engine)**: `varstore_map` — 0x25-декларация, truncation-стоп;
  form_add-валидации — not-declared / дубль-в-формсете / дубль-в-пакете /
  ок-existing / ок-package-declared; `list_varstores` — резолв таргета,
  name-value поля. Сокет-дефолт — unit на новую дефолт-функцию, если план
  выделит (иначе проверка компиляцией + gateway-config тест).
- **Unit (uefi-common)**: `hii_legend` VarstoreList-вариант (по образцу
  `hii_legend_questions_variant`, format.rs); сокет-дефолт `default_sock()`
  уже покрыт (state.rs); других новых uefi-common-функций в цикле нет
  (планнер §6 — цикл hii-form-export).
- **Integration (mock-server, CLI и TUI)**: новый RPC в журналах вызовов;
  TUI-юниты панели `V` (переключение, кэш, скролл).
- **Real-image (`#[ignore]`)**: §7 (HNX-карта, 450x-зонд); гейт валидаций
  §2 на живом образе — `form_add` с пакетом (а) item на несуществующий
  `var_store_id`, (б) с дублем декларации → оба InvalidSchema, дифф образа
  пустой (отказ до мутаций), happy-path пакет с varstores → одна
  декларация на id, ре-парс читает их картой.
- **Живой гейт владельца: не требуется** — цикл не меняет прошиваемый
  формат; все изменения — читаемые поверхности, валидации и дефолты
  окружения.

## Скоуп-границы (сознательно вне цикла)

- Заполнение `formset.varstores` конверта и импорт-планнер form-export —
  реализация в цикле hii-form-export по правилам §6 (здесь только данные
  и движковая защита).
- Varstore-ремап id при импорте в чужой формсет (id занят другим
  определением → авто-перенумерация + переписывание `var_store_id` items)
  — пока fail fast (§2 движок / §6 планнер); ремап отдельной правкой по
  живому use case.
- Formset-уровневый экспорт целиком (`FormSetSchema` со всеми формами и
  varstores) — разблокирован этим циклом, но отдельная задача.
- Эмит name-value деклараций (`VarStoreSchema` + IfrBuilder) — вне;
  name-value остаётся только читаемым (карта/занятость).
- Смена NVAR-дискриминатора `find_varstore_record` (2730) — вне (пиннинг).
- Gateway/WebUI-панель varstores — вне (gateway трогаем только
  сокет-дефолтом; REST-обвязка — цикл Gateway + WebUI rework).
- Вопросы qid-дискаверибельности (вторая половина TODO:490) — частично
  закрыто `hii question list`/`busy_qids` form-export-плана; расширение
  FormInfo formset-ordinal'ом — TODO:508, отдельная позиция.

## Гигиена TODO при закрытии цикла

Закрыть: 490, 496 (движковая половина — §2; клиентский pre-check и
заполнение конверта — цикл hii-form-export, в его спеке §3.6 снять
пометку «блокируется TODO:490»), 3029 (таблицей §7.1), 2673, 2676, 2730
(пиннинг; смена дискриминатора — новая позиция при живом дубле), 2681,
1353; конвенцию high-id (0x7F00) в form-export-плане
пометить как «кандидат на пересмотр после `hii varstore list`» (сам
`max(0x7F00, …)+1`-дефолт не меняем — обратная совместимость пакетов).
