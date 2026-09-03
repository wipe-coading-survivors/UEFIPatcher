# Спека: мини-цикл «value-op» — карта «вопрос → значение» + set-value через NVAR «StdDefaults»

> Дизайн-документ, 2026-09-03. Статус: scope согласован с пользователем —
> **v1 (экспозиция карты вопросов) + v5 (StdDefaults-флип)**, оба механизма
> подтверждены на железе HNX99TF питоновыми скриптами параллельной сессии
> (v5 — эксперимент E14, чеклист пройден полностью; v1-факты выведены
> пробниками из того же образа). Источник: секция TODO
> «Мини-цикл „value-op“ (engine)» и отчёт
> `docs/reports/2026-09-02-hw-validation-hnx99tf.md` §13–14 (E13/E14).
> Эталон: `refs/amibcp/e14-stddefaults-4g.bin` (sha256 `e58bc77b…`, вне
> репо) — побайтовые координаты флипа в §3.

## 1. Контекст и проблема

Мини-цикл unlock-op закрыл «раскрыть доступ» (E12-класс флипов гейтящих
выражений, движок побайтово ≡ E12). Следующая цель — патчер **задаёт
значение** настройки, а не только открывает доступ. UI-путь записи
валидирован ещё на E12 (значение, изменённое в Setup, живёт в NVRAM);
образ-сторонняя запись = патч источника посева дефолтов.

E13 (2026-09-03) **фальсифицировал** два кандидата источника посева:
флипы IFR DEFAULT-флагов опций и SDP optimal/failsafe были в прошивке —
плата подняла 4G [Disabled]. E14 нашёл и **валидировал на железе**
настоящий источник: NVAR-стор «StdDefaults» (PEI-переменный драйвер сеет
свежий стор из этих записей задолго до HII/AMITSE). Полный продуктовый
путь «раскрыть скрытую настройку И задать ей значение без единого
действия в Setup» доказан end-to-end (§14 отчёта).

Движок сегодня: (а) не отдаёт карту «вопрос → (varstore, var_offset,
width, допустимые значения, текущий дефолт)» — `hii/forms.rs` собирает
только формы, varstore-карта не читается вовсе (замечание TODO v1
«schema.rs уже читает varstore-карту» — не соответствует реальности:
`schema.rs` — это serde-модель ВХОДНОЙ схемы formset-add, не ридер
прошивки; правка предпосылки зафиксирована здесь); (б) не умеет ни
находить NVAR-сторы, ни флипать в них байты.

## 2. Цели и не-цели

**Цели (v1 + v5 из TODO, в scope пользователя):**

1. **v1 — экспозиция карты «вопрос → значение»** (только чтение):
   RPC `HiiQuestionInfo` + CLI `hii question info` для item_id
   `<target>#<form_id>:<qid>` — varstore (id/GUID/имя/размер),
   var_offset, width, допустимые значения (OneOf-опции: string_id,
   value, флаги; Numeric: min/max/step), текущие дефолты (DEFAULT-флаги
   опций 0x10/0x20, IFR DEFAULT-опкоды скоупа вопроса). Работает на
   Read-образе, оба канала (bare + PE-resource).
2. **v5 — операция set-value (класс E14)**: согласованный флип байтов
   значения (width × var_offset) в ОБОИХ экземплярах NVAR-стора
   StdDefaults (FV0 — сырье; FV2 — decompress→flip→repack→slot-fit);
   валидация значения по v1-карте; атомарность (plan→apply, все копии
   или ни одна); Write-гейт и compression-гейт как у unlock.

**Не-цели:**

- **v2 (IFR set-default) и v3 (SDP optimal/failsafe)** — отложены
  сознительно: E13 доказал их инертность для посева первого бута;
  остаются кандидатами только для семантики «Load Optimized/Fail-Safe
  Defaults» меню Setup (не проверялось, низкий приоритет). TODO-пункты
  остаются открытыми.
- Runtime-запись в живой NVRAM (setup_var-класс) — нужен UEFI-агент,
  не задача флеш-патчера.
- TUI/Gateway/WebUI-обёртки — как у unlock-op, отдельные циклы.
- Разрешение option-текстов по string-package (в v1 отдаём string_id;
  тексты — minors при подключении клиентов).
- Парсинг NVAR-сторов как узлов дерева (моделирование записей) —
  value-op читает/пишет сторы как байтовые тела, без расширения
  parser-модели.

## 3. Верификационные данные

Все координаты сверены пробниками этой сессии по байтам живого образа
`refs/fw/HNX99TF_200525_original_E5C88C6F.bin` (+ r-efi 7.0 как
авторитет смещений).

### 3.1 Формат NVAR-стора (UEFITool `common/nvram.h` + `ksy/ami_nvar.ksy`)

- Файлы-носители (UEFITool GUID-конвенция): `CEF5B9A3-476D-497F-9FDC-
  E98143E0422C` — «NVAR store» (FV0@0x800048, filetype RAW, тело =
  стор БЕЗ секционной обёртки); `9221315B-30BB-46B5-813E-1B1BF4712BD3`
  — «NVAR external defaults» (FV2@0xa77d28, файл = одна GUIDed-LZMA
  секция @0xa77d40, слот 0x366, dec 0x1A03, внутри RAW-секция 0x1A03,
  стор = dec+4). Существуют ещё 77D3DC50 (PEI defaults) и AF516361
  (BB defaults) — на HNX99TF не встречены; поиск движком —
  **контентный**, не по GUID (см. §4.3).
- Запись NVAR: `'NVAR'` (u32 LE) + `size` u16 (вся запись с заголовком)
  + `next` u24 (0xFFFFFF — последняя) + `attributes` u8 (бит 0x80
  valid, 0x08 data-only, 0x04 local-guid, 0x02 ascii-name, 0x01
  runtime) = 10 байт заголовок. Тело при `valid && !data_only`:
  `guid_index` u1 (если !local_guid) либо GUID 16 байт (local_guid);
  имя (ascii strz при 0x02, иначе UCS2); данные — остаток записи.
- **Вложенность StdDefaults**: первая запись стора — `StdDefaults`
  (attr 0x82, guid_index 0, size 0x19DF); её **данные** — снова
  последовательность NVAR-записей (внутренний обход, обе копии стора
  побайтово идентичны): `Setup` (guid_idx 0, data_len **114** =
  varstore 1) @store+0x17, `PlatformLang`, `Timeout`, `AMITSESetup`
  (81 байт), `PNP0501_0_NV`, `UsbSupport`, **вторая `Setup`**
  (guid_idx 4, data_len 6 — доказательство: имя одно, дискриминатор —
  имя + длина), `IntelSetup` (6091 байт), хвост 0xFF; в конце стора —
  GUID-область (UEFITool: `guid_index` с конца, по 16 байт).
- **Флип E14**: байт 4G (var_offset 0x3A) в данных записи `Setup`
  (114-байтовый образ дефолтов varstore 1) — FV0: `0x8000C2: 00→01`
  (= store@0x800060 + 0x28(data) + 0x3A); FV2: `dec+0x66` (= store@dec+4
  + 0x28 + 0x3A), поток 0x327 ≤ слот 0x366. Итог на железе: 4G
  поднялся [Enabled] сам при первом буте, пережил Save/ребут.
- FFS-заголовок FV0-файла: attrs=0x00 (бит CHECKSUM 0x40 не выставлен),
  header-checksum корректен (0x34 == пересчёту) — rebuild не даст
  лишних байтов диффа.

### 3.2 IFR-карта вопросов (form-пакет Setup-модуля 899407D7, formset 7B59104A)

- **Varstore-объявления — два вида** (r-efi 7.0 нумерация опкодов:
  `IFR_VARSTORE_OP=0x24`, `IFR_VARSTORE_EFI_OP=0x26`; walker читает
  оба):
  1. `IFR_VARSTORE_OP` (buffer): guid@+2, id@+18, size@+20, имя
     **ASCII** strz@+22 (CHAR8 по edk2, подтверждено байтами);
  2. `IFR_VARSTORE_EFI_OP`: **id@+2**, guid@+4, attributes u32@+20,
     size@+24, имя UCS2@+26 (порядок полей r-efi отличается от
     buffer-варианта!).
  На HNX99TF varstore 1 «Setup» size 0x72 GUID `EC87D643-EBA4-4BB5-
  A1E5-3F3E36B20DA9` — стандартная buffer-запись @pkg+0x61
  (имя ASCII `5365 7475 7000`), EFI-записей в пакете нет.
- **Вопрос 4G** (form 10029, qid 0x3B) @pkg+0xDD3: prompt 0x01A3 /
  help 0x01A4 / varstore 1 / var_offset 0x3A / qflags 0x10; опции
  @pkg+0xDE4 `Disabled=0` (str 4, flags 0x30 = DEFAULT|MFG) и
  @pkg+0xDEB `Enabled=1` (str 3, flags 0x00); IFR DEFAULT-опкода в
  скоупе вопроса НЕТ. ONE_OF_OPTION: str@+2, flags@+4, type@+5,
  value@+6 (LE, ширина по типу; AMI пишет type=0 — ширина выводится
  из значений). IFR_DEFAULT: default_id@+2, type@+4, value@+5.
- **Question-header** (r-efi `IfrQuestionHeader`): prompt@+2, help@+4,
  qid@+6, varstore_id@+8, var_offset@+10, qflags@+12; собственный
  flags NUMERIC — @+13 (`1 << (flags & IFR_NUMERIC_SIZE)`, минимум/
  максимум/шаг — width-байтные LE с +14). CHECKBOX width = 1.
- Мастер-гейта 0x9A в Setup-пакете НЕТ (объявлен в другом модуле) —
  латентная неточность `gates.rs::question_storage_width` (читает
  qflags@+12 вместо numeric-flags@+13) не влияет на E12-тесты;
  фиксируется minors-пунктом TODO, в этом цикле gates.rs не трогаем.
- NVAR-запись `Setup` не несёт GUID varstore (guid_idx → 4599D26F-
  1A11-49B8-B91F-858745CFF824, глобальный gEfiSetup-подобный) —
  соответствие записи varstore'у вопроса устанавливаем по
  **имени + длине данных == размеру varstore**, не по GUID.

## 4. Дизайн

### 4.1 Новый модуль `hii/values.rs` — карта «вопрос → значение»

```rust
pub struct VarStoreMap { pub id: u16, pub guid: Option<Guid>,
                          pub size: u16, pub name: String }
pub enum QuestionKind { OneOf, CheckBox, Numeric, Other }
pub struct OptionEntry { pub string_id: u16, pub flags: u8,
                          pub value: u64 }
pub struct DefaultEntry { pub default_id: u16, pub type_: u8,
                          pub value: u64 }
pub struct QuestionMap {
    pub question_id: u16, pub kind: QuestionKind,
    pub var_store_id: u16, pub var_offset: u16, pub width: u8,
    pub varstore: Option<VarStoreMap>,
    pub options: Vec<OptionEntry>,          // OneOf
    pub defaults: Vec<DefaultEntry>,        // DEFAULT-опкоды скоупа
    pub min: u64, pub max: u64, pub step: u64,  // Numeric
}
pub fn varstore_map(pkg: &[u8]) -> Vec<VarStoreMap>;      // все 3 вида
pub fn find_question(pkg: &[u8], form_id: u16, qid: u16)
    -> Option<QuestionMap>;
```

Walker — грамматик-осознающий, как `gates.rs::find_gates` (стек фреймов,
scope-bit, statement-множество, linear-by-length обход выражений);
varstore-объявления (0x24/0x26) читаются при обходе как statement'ы;
вопрос — по матчу `qid@+6` внутри формы `form_id`; опции и
DEFAULT-опкоды — линейно до END скоупа вопроса. Width: CHECKBOX=1;
NUMERIC=`1<<(flags@+13 & 3)`; ONE_OF=байтовая ширина max значения
опций (min 1; DEFAULT-тип скоупа уточняет при наличии). Значение опции
читается width-байтами LE c +6.

### 4.2 Новый модуль `hii/nvar.rs` — формат NVAR

```rust
pub struct NvarEntry<'a> { pub offset: usize, pub size: usize,
    pub attributes: u8, pub guid_index: Option<u8>,
    pub name: Option<&'a str>, pub data: &'a [u8] }
pub const ATTR_VALID: u8 = 0x80; pub const ATTR_DATA_ONLY: u8 = 0x08;
pub const ATTR_LOCAL_GUID: u8 = 0x04; pub const ATTR_ASCII_NAME: u8 = 0x02;

pub fn is_std_defaults_store(body: &[u8]) -> bool;  // первая запись 'StdDefaults' с валидной геометрией
pub fn std_defaults_data(body: &[u8]) -> Option<&[u8]>;   // данные первой записи
pub fn find_record<'a>(inner: &'a [u8], name: &str, data_len: usize)
    -> Option<(usize /*record off*/, &'a [u8] /*data*/)>;
pub fn walk(body: &[u8]) -> impl Iterator<Item = NvarEntry>;  // для тестов/диагностики
```

Разбор записи: сигнатура/размер/атрибуты по §3.1; выход за границы или
size < минимума — остановка обхода (без паники); `data_only`/invalid —
имени нет, данные до конца записи. `is_std_defaults_store` требует:
первая запись валидна, ascii-имя `StdDefaults`, размер согласован с
телом.

### 4.3 Интеграция `hii/mod.rs`

- `question_info(&Image, item_id) -> Result<uefi_proto::QuestionInfo, HiiError>`
  — только чтение, грамматика item_id как у gates (qid обязателен,
  иначе NotFound); каналы `form_package_ranges`; по всем FORM-пакетам
  секции — `values::find_question(pkg, form_id, qid)`; не найден —
  NotFound. Отображение в proto — §4.4. Отсутствие varstore-объявления
  — не ошибка: поля varstore пустые, `set_value` на таком вопросе
  откажет с диагностикой.
- `set_value(&mut Image, item_id, value: u64) -> Result<ValueOutcome, HiiError>`
  — Write-гейт; сбор карты вопроса (`find_question`); валидация:
  вопрос найден, storage-backed (`var_store_id != 0`, varstore
  известен, есть имя), width ≤ 8, value помещается в width байт;
  OneOf — value ∈ значения опций; Numeric — min ≤ value ≤ max;
  CheckBox — value ∈ {0,1}. Нарушения — новая ошибка
  `ValueOpUnsupported(String)` (failed_precondition).
- Поиск NVAR-сторов: обход дерева (рекурсивно, path-aware): (а) File с
  `children.is_empty()` и `nvar::is_std_defaults_store(body)` — FV0-вид;
  (б) любая Section(RAW/прочая) с телом-стором — FV2-вид (RAW-секция
  внутри GUIDed-LZMA уже распакована парсером в children). Найденные
  локации: `{path, data_node_kind: FileBody|SectionBody}`. Compression-
  гейт: предок-Section(COMPRESSION/GUID_DEFINED) нерекомпрессируемая →
  `MutationBehindCompression` (проверка ДО мутаций, по всем копиям).
- План: для каждой копии `nvar::find_record(std_defaults_data(node.body),
  varstore.name, varstore.size)` → offset данных записи; flip =
  `[data_off + var_offset, width)` ← value LE; from = текущие байты.
  Ноль копий с записью → `ValueOpUnsupported` («StdDefaults-сторы не
  найдены»). Apply — атомарно во все копии (FV0 — правка `node.body`
  файла; FV2 — правка тела RAW-секции), затем
  `ops::mark_rebuild_to_root_by_path` на каждую копию. Длины всех тел
  инвариантны; ассерт как у unlock.
- `ValueOutcome { question: QuestionInfo, applied: Vec<String>,
  stores: Vec<String> }` — applied: `"fv0 CEF5B9A3… @store+0x62: 00 -> 01"`
  стиль flip-строк unlock; stores — описание найденных копий.

Новая ошибка:

```rust
#[error("value operation not supported: {0}")]
ValueOpUnsupported(String),
```

### 4.4 Proto + RPC

```proto
rpc HiiQuestionInfo(HiiQuestionInfoRequest) returns (HiiQuestionInfoResponse);
rpc HiiSetValue(HiiSetValueRequest)         returns (HiiSetValueResponse);

message VarStoreInfo { uint32 id = 1; string guid = 2; uint32 size = 3; string name = 4; }
message OptionEntry  { uint32 string_id = 1; uint64 value = 2; uint32 flags = 3; }
message DefaultEntry { uint32 default_id = 1; uint32 type = 2; uint64 value = 3; }
message QuestionInfo {
  uint32 form_id = 1; uint32 question_id = 2; string kind = 3;
  uint32 var_store_id = 4; VarStoreInfo varstore = 5;
  uint32 var_offset = 6; uint32 width = 7;
  uint64 min = 8; uint64 max = 9; uint64 step = 10;
  repeated OptionEntry options = 11; repeated DefaultEntry defaults = 12;
}
message HiiQuestionInfoRequest  { string image_id = 1; string item_id = 2; }
message HiiQuestionInfoResponse { QuestionInfo question = 1; }
message HiiSetValueRequest      { string image_id = 1; string item_id = 2; uint64 value = 3; }
message HiiSetValueResponse     { QuestionInfo question = 1;
                                  repeated string applied_flips = 2;
                                  repeated string stores = 3; }
```

`build.rs`: serde-derive для QuestionInfo/VarStoreInfo/OptionEntry/
DefaultEntry. Handlers зеркалят `hii_gates_list` (чтение) и
`hii_unlock` (мутация + `flush_image`). `ValueOpUnsupported` →
failed_precondition (arm в `hii_error_status`). Mock-серверы uefi-cli /
uefi-tui / uefi-gateway получают заглушки в той же задаче, что и proto
(полный server-trait).

### 4.5 CLI

```
uefi-cli hii question info      <item_id>          # чтение, Read-образ ок
uefi-cli hii question set-value <item_id> <value>  # мутация; value: dec | 0x-hex (u64)
```

`client.rs`: `hii_question_info(image_id, item_id) -> QuestionInfo`,
`hii_set_value(image_id, item_id, value) -> (QuestionInfo, Vec<String>,
Vec<String>)`. `output.rs`: `print_question_info` (JSON — serde;
TSV — скалярные поля + опции отдельными строками; text — блочное
резюме с опциями «value = …», дефолтами и varstore) и
`print_set_value` (по образцу print_unlock). `main.rs`:
`HiiQuestionCmd::Info`, `HiiQuestionCmd::SetValue`.

## 5. Обработка ошибок

| Ситуация | Ошибка | RPC-статус |
|---|---|---|
| item_id без qid / не парсится / цель не найдена | `HiiError::NotFound` | not_found (существующий) |
| цель не секция / не HII-канал | `HiiError::NotASetupItem` | invalid_argument (существующий) |
| вопрос не найден в пакетах секции | `HiiError::NotFound` | not_found |
| set-value в Read-режиме | `HiiError::NotWritable` | failed_precondition (существующий) |
| копия стора за нерекомпрессируемой обёрткой | `HiiError::MutationBehindCompression` | failed_precondition (существующий) |
| вопрос не storage-backed / varstore неизвестен/без имени / width > 8 / value не в опциях/диапазоне / сторы или запись не найдены | `HiiError::ValueOpUnsupported(String)` | failed_precondition |
| varstore без объявления в карте | `question_info` — Ok с пустым varstore; `set_value` — `ValueOpUnsupported` | — / failed_precondition |

## 6. Тестирование и приёмка

**Синтетика (TDD, по задачам):** values.rs — оба вида varstore-объявлений
(buffer-ASCII 0x24 как на живом образе + EFI-UCS2 0x26), вопрос 4G-фикстуры (varstore 1 /
var_offset 0x3A / опции 0x30+0x00 / width 1), Numeric min/max/step,
DEFAULT-опкоды, отсутствие объявления, чужая форма/вопрос; nvar.rs —
разбор записи StdDefaults/Setup по живой геометрии (фикстура из §3.1
синтетически), data_only/invalid записи, обрезанный стор, вторая
'Setup' с другой длиной не матчится, is_std_defaults_store на мусоре;
mod.rs — question_info bare/resource, set_valuehappy-path (обе копии,
атомарность, длина инвариантна, каскад Rebuild), отказы
(ValueOpUnsupported на все ветки §5, NotWritable,
MutationBehindCompression), from-байты в отчёте; CLI — parse,
mock-e2e с content-ассертами stdout.

**Real-image `#[ignore]`** (`tests/real_image.rs`):

- `real_image_hii_question_info_4g` — info для
  `899407D7…:0x10:0#10029:0x3B`: varstore name "Setup", size 0x72,
  var_offset 0x3A, width 1, kind one_of, опции
  `[(str 4, 0, flags 0x30), (str 3, 1, flags 0)]`, defaults пусто.
- `real_image_hii_set_value_matches_e14` — set_value(1) → build:
  длина образа сохранена; `built[0x8000C2] == 1`; декомпрессат
  LZMA-секции 9221315B: дифф против оригинала ровно `[(0x66, 0→1)]`
  и побайтово равен декомпрессату эталона E14; флеш-дифф против
  оригинала только в [0x8000C2] ∪ экстенте секции 9221315B; re-parse:
  question_info видит дефолт байта 1 (повторный set_value(1) —
  no-op-план: applied пуст); round-trip (build→parse→build
  идемпотентен). Регрессии вокруг (20/20 существующих) — зелёные.

**Smoke (живой образ, вручную):** `hii question info …#10029:0x3B` →
карта; `hii question set-value …#10029:0x3B 1` → applied обеих копий;
`image save`. HW-кандидат E15 — по желанию пользователя, после ревью
дифа (диф побайтово наследует аппаратно-проверенный E14).

## 7. Риски и открытые вопросы

- **Основа pkg/store-координат**: офсеты §3 выведены пробниками по
  живым байтам; если real-image приёмка покажет расхождение — план-
  дефект (AGENTS.md п.11): docs-коммит с уточнением, затем правка.
- **Один стор в образе**: флипаем все найденные копии; ровно одна —
  законный случай (FV0-only прошивки). Ноль — честный отказ.
- **Свип LZMA-энкодера** может дать поток ≠ python-lzma — ассерт
  побайтового равенства **декомпрессатов** (не потоков) и слот-fit.
- **Width OneOf из значений опций** может занизить ширину против
  фактической (значения 0/1 в 2-байтовом поле) — guard: запись данных
  не выходит за пределы записи NVAR; фактический префикс данных
  сравнивается в from. При живом прецеденте — уточнение по
  DEFAULT-типу.
- **Тонкие сторы**:guid_index-область в хвосте не парсится (записи
  ищутся по имени+длине) — намеренно, YAGNI.

## 8. Влияние на компоненты

- **uefi-engine**: новые `hii/values.rs`, `hii/nvar.rs`; `hii/mod.rs`
  (question_info/set_value, ValueOpUnsupported); `rpc/server.rs`
  (2 handler'а, arm маппинга); `tests/real_image.rs` (2 теста).
- **uefi-proto**: `engine.proto` (2 RPC + 7 message), `build.rs` (serde).
- **uefi-cli**: `client.rs`, `commands/hii.rs`, `main.rs`, `output.rs`,
  `tests/{mock_server,cli_integration,e2e}.rs`.
- **uefi-tui / uefi-gateway**: только mock-заглушки server-trait
  (тесты).
- **TODO.md**: v1/v5 закрыты; v2/v3 помечены отложенными (инертность
  E13); новые minors (gates width-байт, option-тексты).

## Отложенное (не в этом цикле)

- v2 IFR set-default / v3 SDP — см. §2 не-цели.
- Option-тексты в question info (резолюция по string-package).
- `gates.rs::question_storage_width` читает qflags@+12 вместо
  numeric-flags@+13 — миноры TODO.
- Формализация «gated»/карты вопросов в FormInfo для TUI/WebUI.
