# Спека: hii-read-truth — честность читающего пути HII (класс A)

Дата: 2026-09-25. Ветка реализации: `hii-read-truth` (спека и её ревью-правки —
docs-коммитами в master по паттерну проекта; код — только в ветке).

Первоисточники: TODO.md поз. 169 (SIBT_EXT не обрабатываются), 183 (обрезанный
u16-count молча даёт count=0), 3463 (HiiListQuestions режет form_id u32→u16),
1668 (string-id коллизии между списками пакетов), 3475 (bare PE form-пакеты —
вопросы не возвращаются); устаревшие записи 177 (found-флаг) и 202 (остаток
cross-file титулов) — закрыть docs-коммитом. Классификация «класс A: молча
неверные данные читающего пути» — триаж-сессия владельца 2026-09-25 (разбиение
миноров HII-ядра на классы A/B/C/D по радиусу поражения, не по объёму работы).

## Контекст и проблема

Читающий путь HII (list-forms / list-strings / list-questions / question-info /
gates-list / form-export) в четырёх местах врёт молча — пользователь принимает
решения по неверному выводу (худший класс дефектов для инструмента, чьи правки
едут на железо):

1. **SIBT_EXT1/2/4 (0x30–0x32)** — `parse_string_package` (hii/strings.rs)
   трактует EXT-блоки как unknown-opcode → warn + stop: пакет с EXT-блоками
   теряет все последующие строки без видимых признаков (TODO:169).
2. **Усечённый u16-count** STRINGS_*-блоков — `read_u16` fallback
   `(0, body.len())` без warn: блок читается как count=0, строки молча
   пропадают; одиночный хвостовой байт UCS2/SCSU даёт одну фальшивую пустую
   запись (TODO:183).
3. **`r.form_id as u16`** в RPC-хендлере `hii_list_questions`
   (rpc/server.rs:1111): form_id > 0xFFFF усекается без диагностики — запрос
   возвращает пустой список «по форме 1» вместо ошибки (TODO:3463).
4. **String-id коллизии**: `StringInfo {language, string_id, text}` не несёт
   источника; `collect_strings` флаттенит пакеты всех package-list'ов в один
   список — id уникальны только внутри списка. На 450x id 3/4 в списке UiApp =
   «Removable Drive»/«Hard Drive», а те же id в опциях Setup-вопроса =
   Disabled/Enabled (TODO:1668).
5. **Асимметрия bare-канала**: формы bare-конст-массивов EDK2 видны в
   `hii form list` (forms.rs включает `bare_form_packages`), но
   `form_package_ranges` (hii/mod.rs:214) в PE32-ветке покрывает только
   resource-канал → `hii question list/info/gates` по bare-таргетам пусты
   (TODO:3475). Проверено на edk2-rk3588: формы есть, вопросы — NotFound.

Уже закрытое (не входит): per-file scoping титулов форм (forms.rs:36, тест
`collect_forms_titles_are_scoped_to_file`); полный обход всех string-пакетов
(`883ca14`). Остаток cross-file титулов — fallback-эвристика «крупнейший пул
образа» (questions.rs:87–94) — решение владельца: known-behavior (см. §6).

## Решения владельца (брейнсторм 2026-09-25)

- **Состав — полный класс A**: A1–A5 + docs-балансировка TODO (A6).
- **A5 — read/write-сплит селектора рангов**, не расширение везде: bare-канал
  в мутациях — осознанное ограничение (TODO:383, не аппаратно-валидировано).
  Сплит заодно готовит класс B: все мутации берут ранги из одной точки, куда
  Б-цикл добавит явные отказы (сегодня отказ bare — молчаливое «0 gates»).
- **A4 — одно строковое поле `source` = GUID владельца + канал** (не голый
  GUID: коллизия en-US/x-AMI пакетов одного файла остаётся различимой; не
  группировка вывода — ломает TSV-парсинг скриптами).
- **L202-остаток — known-behavior**, признак `title_source` не вводим до
  живого ложного срабатывания.

## §1 A1: SIBT_EXT1/2/4 в parse_string_package

Семантика по edk2 `UefiInternalFormRepresentation.h:365–399`
(`../refs/edk2/MdePkg/Include/Uefi/UefiInternalFormRepresentation.h`):

| Опкод | Layout | Размер блока |
|---|---|---|
| EXT1 0x30 | Header(0x30) + BlockType2(u8, игнор) + Length(u8) | 3 + Length |
| EXT2 0x31 | Header + BlockType2 + Length(u16 LE) | 4 + Length |
| EXT4 0x32 | Header + BlockType2 + Length(u32 LE) | 6 + Length |

EXT-блок не определяет строк и не двигает next_id: ридер пропускает `Length`
байт extended-данных и продолжает walk со следующего блока. Константы — свои
(r-efi SIBT-констант не содержит, как и у существующих SIBT_* в strings.rs).

Границы: `pos + header_size` и `pos + header_size + Length` — через
checked-арифметику; выход за `body.len()` = усечённый EXT → warn
`"truncated SIBT_EXT block; stopping string parse"` + останов walk (без
фальшивых строк, без паники). Writer не трогаем: `SibtBlockUnsupported`
уже честно отказывает; унификация SIBT-кода reader/writer (TODO:173) —
отложенный пункт, не блокер.

## §2 A2: усечённый count и фальшивые пустые записи

`read_u16` → `Option<(u16, usize)>`; `None` = за телом пакета. Единая
семантика для всех потребителей внутри walk (STRINGS_* count, DUPLICATE
ref_id, SKIP2 count): усечённое чтение = warn
`"truncated SIBT block (<opcode>); stopping string parse"` + `break 'outer`
— вместо молчаливых count=0 / дубликата пустой строки / тихого конца.

Фальшивые пустые записи: если для тела строки доступно меньше минимального
завершимого объёма — SCSU: 0 байтов до NUL; UCS2: < 2 байтов до терминатора
00 00 — это усечение (warn + stop), не `push("")`. Валидные пустые строки
(NUL сразу / 00 00 сразу) остаются валидными — поведение не меняется.

## §3 A3: form_id u32→u16 в hii_list_questions

Единственная точка усечения — rpc/server.rs:1111 (остальные RPC парсят
form_id из item_id-строки через `parse_item_id`, где u16 уже enforced).
Замена на `u16::try_from(r.form_id)` с ошибкой invalid_argument и контекстом
`"form_id out of range: {n}"` (маппинг по образцу существующего
`hii_error_status`-семейства). Пустой список «успехом» больше не маскирует
опечатку form_id в вызывающем коде.

## §4 A4: StringInfo.source

Proto (proto3-additive, обратная совместимость):

```proto
message StringInfo {
  string language = 1;
  uint32 string_id = 2;
  string text = 3;
  string source = 4;   // NEW: "<UPPER-FFS-GUID>/res" | "<UPPER-FFS-GUID>/bare" | "res" | "bare"
}
```

- **Engine**: `collect_strings` (hii/strings.rs) больше не теряет владельца —
  source собирается из `StringPackageRef {file_guid, channel}`
  (`collect_string_packages` уже несёт оба). Формат: GUID верхним регистром
  (`guid_to_upper_string`, конвенция как formset_guid) + `/res`|`/bare`
  (Resource|Bare); файл-владелец неизвестен — голый канал.
- **CLI** (`print_strings`): колонка `source` в text и tsv (TSV-заголовок
  становится `language\tstring_id\tsource\ttext` — обновить e2e-ассерты в
  той же задаче; предупредить владельца: скрипты, парсящие TSV, увидят новую
  колонку), поле в json.
- **TUI** (strings-браузер `S`): source выбранной строки в деталях/статусе
  (минимум — где уже рендерится выбранная строка; без новой колонки списка).
- Моки/фикстуры: prost struct-literals требуют все поля — обновить литералы
  StringInfo в тестах uefi-tui (app.rs, ui/forms.rs) и uefi-cli
  (mock_server.rs) пустым `source` или осмысленным.

Коллизия 450x (id 3/4 UiApp vs Setup) становится видимой: одинаковые
string_id с разными source — различимы в выводе.

## §5 A5: read/write-сплит form_package_ranges

`form_package_ranges(node)` сохраняет текущую семантику и получает
rustdoc-контракт: «ранги для мутаций и их пре-чеков; bare-канал не входит —
мутации bare не аппаратно-валидированы (TODO:383); чтение —
`form_package_ranges_read`».

Новый `form_package_ranges_read(node) -> Vec<(usize, usize)>` — суперсет:
все ветви как у мутационного, PE32-ветка = resource-ранги
(`pe_resource_form_packages`) + `bare_form_package_ranges(pe, exclude)`
(новая ranges-вариация `bare_form_packages`: та же валидация кандидата
u24-len + FORMS + FORM_SET_OP + `parse_form_package`, возврат `(off, len)`
вместо среза; exclude = resource-ранги, чтобы не задваивать пакеты,
достигнутые обоими каналами).

Переключение потребителей:

| Потребитель | Тип | Ранги |
|---|---|---|
| `list_questions` (mod.rs:750) | read | **read** (новое) |
| `find_question_map` (mod.rs:659) | read | **read** (новое) — питает `question_info` и валидацию `set_value` |
| `gates_list`, own-секция (mod.rs:338) | read | **read** (новое) |
| `form_export` (form_export.rs:91) | read | **read** (новое) |
| `unlock` (mod.rs:388) | write | mutation (как сейчас) |
| `set_item_visibility` | write | mutation (как сейчас) |
| `form_add` (mod.rs:1676/1779/1791/1808) | write+пре-чеки | mutation (как сейчас) |
| `own_formset_guid` (mod.rs:449) | read, но общий с unlock-фазой | mutation (как сейчас — см. ниже) |
| `cross_formset` (cross_formset.rs:83) | общий gates_list+unlock | mutation (как сейчас) |

Обоснования границ:

- **set_value не задет**: его мутация — согласованный флип байтов NVAR-сторов
  (FV0 raw + FV2 recompress), не IFR; read-карта вопросов только даёт ему
  varstore/offset/width. На rk3588 value-op по bare-вопросам начинает
  работать (раньше — NotFound на find_question_map).
- **own_formset_guid и cross_formset остаются на mutation-рангах**: их
  переключение открыло бы cross-формсетную фазу unlock для bare-таргетов —
  новую write-поверхность через чёрный ход. Явная ошибка unlock на bare —
  территория Б-цикла (точка уже одна: мутационный селектор).
- Осознанная асимметрия gates_list: own-гейты bare-пакетов видны, донорские
  REF-гейты на bare-цель — нет (cross-фаза не стартует без formset_guid).
  Фиксируется как known-behavior до Б-цикла.

## §6 A6: docs-балансировка TODO

- **TODO:177 (found-флаг)** — закрыть как устаревшее: полный обход всех
  пакетов уже реализован (`883ca14`), walk не останавливается на первом
  непустом.
- **TODO:202 (cross-file титулы)** — закрыть: per-file scoping реализовано
  (forms.rs:36 + тесты); остаток — fallback-эвристика крупнейшего пула,
  known-behavior: AMI централизует setup-строки в одном пакете, живых ложных
  титулов не зафиксировано. При первом живом ложном срабатывании — отдельный
  пункт на признак `title_source` в FormInfo.
- TODO:169/183/3463/1668/3475 — закрываются по факту своих задач с
  коммит-ссылками (по месту, как принято).
- Класс-метки (класс: silent-data / write-path / cosmetic) для остальных
  пунктов TODO — вне скоупа, уйдут в цикл sqlite-трекера (TODO:4266).

## §7 Тестирование

Юнит (красный тест до фикса, по пункту):

- strings.rs: EXT1 между строками (строки до/после видны, next_id не
  двигался), EXT2/EXT4, усечённый EXT (warn-семантика: строки до блока
  возвращены, после — нет, пустых нет); усечённый count STRINGS_SCSU/UCS2
  (частичные строки до усечения, без count=0-молчания); хвостовой байт
  UCS2/SCSU — нет фальшивой пустой записи; валидная пустая строка — есть.
- mod.rs: `form_package_ranges_read` включает bare (фикстура rk3588
  bare-form), не дублирует resource-пакеты (exclude), RAW/0x18-ветви
  неизменны; `form_package_ranges` (mutation) bare не включает — пиннинг.
- rpc: form_id 0x10000 → invalid_argument (не пустой список).
- cli: print_strings text/tsv/json с source; e2e — заголовок TSV, json-поле,
  form_id 0x10000 → ненулевой exit с контекстом.
- tui: strings-детали с source (по механизму из плана).

Real-image `#[ignore]`-гейты:

1. **HNX99TF**: `real_image_hii_forms_and_strings` остаётся зелёным; счётчики
   строк/форм не изменились; source непустой у строк файловых владельцев
   (Setup/Platform); round-trip цел.
2. **rk3588** (`refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img`,
   `UEFIPATCHER_TEST_FW`): новый гейт — bare-таргет отдаёт вопросы > 0
   (`list_questions`), `question_info` резолвится (раньше NotFound).
3. **450x sanity** (опционально, образ в refs/amibcp): у string_id 3/4
   записи с разными source различимы (иллюстрация закрытия TODO:1668).

Регресс-обоснование: EXT-блоки на живых образах корпуса не встречались
(TODO:171 «в реальном firmware редкость») — счётчики HNX99TF обязаны не
сдвинуться; сдвиг = детектор ложного срабатывания новых веток.

## §8 Границы (non-goals)

- Мутации bare-канала (unlock/unsuppress/hijack/form_add по bare) —
  write-path, класс B/D, отдельное решение с аппаратной валидацией.
- `title_source` в FormInfo — до живого прецедента (§6).
- Унификация SIBT-кода reader/writer (TODO:173), writer-side EXT.
- Класс B целиком: wrapping-арифметика id/span, bounds-guard'ы Gate, откат
  question_add, `.rsrc`-неоднозначность, атомарность кросс-фазы.
- Класс-метки TODO / sqlite-трекер.

## §9 Риски

- **TSV-заголовок string list меняется** — единственное ломающее изменение
  вывода; все внутренние e2e-ассерты обновляются в задаче A4, владельцу
  объявлено.
- **prost-литералы** StringInfo в тестах трёх крейтов перестают компилироваться
  (additive поле) — механическое обновление в той же задаче.
- **bare-скан в read-селекторе** — O(n) по телу PE; `collect_forms` уже несёт
  тот же скан (замер 546 мс на полный образ), list_questions работает по
  одной секции — деградации нет.
- **Новые ветки walk (EXT) на враждебных данных** — все границы через
  checked-арифметику, усечение = warn+stop, паник нет; гейты HNX99TF
  фиксируют отсутствие изменений на образах без EXT.
