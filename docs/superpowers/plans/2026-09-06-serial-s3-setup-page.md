# Serial Console S3 — Setup-настройки (add-question op + config-aware glue) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ступень S3 лестницы Serial Console: serial-настройки (baud, terminal type) видимы и изменяемы в штатном Setup, драйверная тройка читает их из NVRAM (гейт E31: пункты видимы, смена baud меняет скорость на COM).

**Architecture:** Страница «Serial Port 1 Configuration» (форма 10019) уже существует в LIVE и достижима — новая страница не нужна. Новая движковая op `hii question add` втавляет два OneOf-вопроса в форму 10019 (IFR-сплайс + аппенд строк + аддитивные $SPF-записи/контролы с клоном страницы и переприцевкой слота). Glue пересобирается на чтение переменных `Setup` (baud/terminal на свободных офсетах 0x5D/0x5E) и `PNP0501_0_NV` (enable, байт 0 — существующий вопрос q34); SerialDxe/TerminalDxe остаются байт-идентичными S1-артефактам.

**Tech Stack:** Rust (uefi-engine/uefi-cli, binrw/r-efi — как в цикле 6), edk2 (UefiPatcherSerialPkg, стенд S1), python (hack/fv_audit.py).

**Spec:** `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` §3-S3 + аддендумы §7 (решения S0/S2/E30) и §9 (мини-pre-check S3 — вход этого плана). Прецеденты механики: `docs/reports/2026-09-05-hijack-v2-e29.md` (карта каналов 6/6), `docs/reports/2026-09-04-tse-bridge-recon.md` §2–4 (контейнер $SPF, E9-правило аппендов).

## Мини-pre-check S3 (вход плана; детально — аддендум спеки §9)

Скрипты `/tmp/serial-s3-precheck/` (вне git, метод восстановим из §9), сырьё read-only:
`scan_ifr.py` (IFR формсета 899407D7) + инлайн-проходы по записям $SPF.

1. **Форма 10019 «Serial Port 1 Configuration» существует и видима** (form list:
   10018 Super IO Configuration, 10019 Serial Port 1 Configuration; гейтов на
   вопросах нет). Страница $SPF: слот 8 @контейнер+0x56C, title-id 237, cnt=6,
   плато-поля B@+0x12=0x07, u18@+0x18=0x3A. → регистрация новой страницы
   (zero-slot + bump) НЕ нужна; новый пункт меню НЕ нужен.
2. **Существующие вопросы q34/q35 привязаны к PNP0501_0_NV** (varstore id 0x15,
   GUID 560BF58A, 3 Б): q34 CheckBox «Serial Port» (prompt 245/help 246) off 0;
   q35 OneOf «Change Settings» (prompt 291/help 290, 6 опций @pkg 0x965–0x988)
   off 1. Фабричный NVRAM-дефолт `PNP0501_0_NV = 01 00 00` (S0 §5.5) —
   семантика «COMA Enabled», enable-вопрос уже работает штатно — его НЕ трогаем.
3. **Варстор-карта LIVE (узлы IFR_VARSTORE @PE+0x8BD5, stride 0x23):**
   0x12 PNP0510_0_VV(9), 0x13 PNP0510_0_NV(3), 0x14 PNP0501_0_VV(9),
   **0x15 PNP0501_0_NV(3)**, 0x16 PNP0501_1_VV(9), 0x17 PNP0501_1_NV(3) —
   семья 560BF58A. NV-буфер COM1 исчерпан (off 0/1 заняты, свободен 1 байт) →
   новые вопросы вешаем на основной варстор **`Setup` (id 1, GUID
   EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9, 114 Б)**.
4. **Свободные офсеты Setup-буфера** (точные ширины numeric/checkbox, one_of=1):
   окна 0x13–0x23 (17 Б), 0x5C–0x64 (9 Б). Выбор: **baud @0x5D, terminal @0x5E**
   (середина хвостового окна, отступ от границ). Фабричный дефолт Setup в этих
   байтах = 0 → кодируем опции так, что 0 = 115200 / VT-UTF8 (совпадает с
   поведением E30 без изменений).
5. **Записи $SPF (72Б)**: скан движковой подписи (F8×8 @+36, tail 0100 0100
   @+44) находит 389 записей; q35 @spf+0x8EA8: help@+0x14=290,
   counter=0x0001_003C, **ifr@+0x1C=0x954 == позиция опкода q35 в forms-пакете**
   (инвариант tse-bridge §2), prompt@+0x30=291, fs/opt=0/0. q34 — checkbox с
   нулевой сигнатурой (движковый сканер слеп — S0 §3.4), нам не нужен: новые
   вопросы OneOf → клонируем запись q35 (F8-форма). **Правило counter@+0x18 =
   (класс 0x0001 << 16) | (1 + max(counter_low))** — правка по LIVE-факту
   Task 4: max_low = 0x10C8 (не ~148, как предполагалось); формула брифа
   реализована движком и применима к реальным значениям.
6. **E9-безопасность**: $SPF-контейнер (в теле секции SetupData 3/208/0/0 от
   сигнатуры `$SPF`, count=188 @+0x60, таблица @+0x64, пул @+0x358, записи
   0x8288–0x5AD20, хвосты до 0x77164, свободного места внутри нет) — только
   аппенды в конец + переприцевка указателей; рост слота LZMA платой
   подтверждён (E18/E29). Страницу растить нельзя (инлайн-список контролов) →
   **клон страницы cnt 6→8 аппендом + table[8] = офсет клона** (старая страница
   становится сиротой, count@0x60 не меняется).
7. **IFR-сдвиг**: вставка опкодов после q35 (конец вопросов формы 10019,
   pkg+0x993) сдвигает опкоды последующих форм на delta → правим in-place
   ifr-поля **только Setup-формсет-записей**. Правка по LIVE-факту Task 4
   (измерение fix-раунда 2 уточняет числа): из 389 F8-записей 32
   резолвятся (qid, ifr) в forms-пакете Setup — только их и сдвигаем;
   остальные 357 (2-й класс, прочие формсеты) ВИДИМЫ движковому сканеру
   (предположение пре-check «2-й класс без подписи» — неверно) и
   резолвятся в ДРУГИХ форм-пакетах образа (доминирующе abbce13d,
   IntelRC-класс, 0x32991 Б) — их ifr-координаты чужие, сплайс Setup-пакета
   их не сдвигает → fixup избирательный: до IFR-мутации снять карту
   Setup-резолвящихся записей (предикат `spf_record_resolves`), после
   сплайса сдвинуть только их (ifr += delta при ifr >= точки вставки);
   foreign-записи не изменяются вовсе. Формсеты других файлов — не тронуты.
8. **CLI-реалии** (как в S2): цель Setup = `3/28/1/0` (PE32-ресурс, ресурсный
   канал add_strings_to_resource), SetupData = `3/208`; item_id формсета
   `<target>#<form>[:<qid>]`; после S2-вставки тройки (3/216–3/218) индексы
   3/28 и 3/208 не сдвигаются; `--mode write`, сокет задавать явно,
   `session init --force`, движок перед стартом — `rm -f "$UEFIPATCHER_SOCK"`,
   глушить только `pkill -x engine`.

## Global Constraints

- BIOS-образы не коммитятся; `refs/` read-only; маленькие .ffs тест-данные —
  можно (прецедент `crates/uefi-engine/tests/data/serial/`).
- TDD-порядок шагов; module-first rule; комментариев в коде нет (кроме `file:line`).
- После каждой задачи: `env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u all_proxy cargo test -p uefi-engine` (или соответствующий крейт) и `cargo clippy -p <crate> -- -D warnings`; `cargo fmt --all -- --check` в конце каждой задачи с правками кода.
- **E9-правило**: никаких сдвигов внутри $SPF-контейнера — только аппенды в
  конец и переприцевка абсолютных офсетов; внутри IFR-пакета сдвиг допустим
  только точкой вставки вопросов с компенсирующей правкой ifr-полей записей.
- **E26/E28-правило**: хелп-канал вопроса = $SPF-запись s14 **и** строковый
  пул-контроль — обе копии с рождения для новых вопросов.
- **SerialDxe/TerminalDxe не пересобираются** — байт-идентичность артефактам S1
  (d16b78ee…, 64003242…) проверяется гейт-тестом; меняется только glue.
- Кандидат E31 собирается из базы E5C88C6F (sha256 7e9d2544…) полным пайплайном
  (тройка + два add-question) одним транскриптом; прошивка — только владелец.
- Дефекты плана — отдельный `docs:`-коммит ДО реализации (правило-11 AGENTS.md).
- Рабочая ветка `serial-console`; push на dsevosty после каждого коммита.

## Схема данных (контракт между задачами)

Новые вопросы формы 10019 (обa OneOf, varstore id 1):

| вопрос | qid | prompt-текст | var_offset | опции (value → text) | default |
|---|---|---|---|---|---|
| Baud Rate | 0x200 (512) | `Baud Rate` | 0x5D | 0→`115200`, 1→`57600`, 2→`38400`, 3→`19200`, 4→`9600` | 0 (optimized) |
| Terminal Type | 0x201 (513) | `Terminal Type` | 0x5E | 0→`VT-UTF8`, 1→`VT-100+`, 2→`VT-100`, 3→`ANSI` | 0 (optimized) |

help-тексты: `Serial console port speed (applies on next boot)`,
`Serial console terminal type (applies on next boot)`. Op отвергает qid,
уже существующий в формсете, и var_offset, пересекающийся с любым вопросом
того же варстора (валидация по values.rs-механизму).

Контракт glue (Task 5): `gRT->GetVariable`:
- `L"Setup"`, GUID EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9: байт [0x5D]=baud idx,
  [0x5E]=term idx; NOT_FOUND или выход за диапазон → 0/0.
- `L"PNP0501_0_NV"`, GUID 560BF58A-1E0D-4D7E-953F-2980A261E031: байт [0]=enable;
  NOT_FOUND → 1 (attach).
Отображения: baud 0→115200, 1→57600, 2→38400, 3→19200, иначе→9600;
term 0→gEfiVTUTF8Guid, 1→gEfiVT100PlusGuid, 2→gEfiVT100Guid, иначе→gEfiPcAnsiGuid.
enable=0 → glue не подключает консоль (никаких Con*-append/ConnectController);
baud применяется через `SerialIo->SetAttributes` после подключения; term —
выбор vendor-узла device path терминала (существующее место конструирования
пути в glue, R-S1.2). Маркеры SC-S1 и DEPEX не меняются.

---

### Task 1: ifr.rs — вставка вопросов в существующую форму

**Files:**
- Modify: `crates/uefi-engine/src/hii/ifr.rs`
- Test: `crates/uefi-engine/src/hii/ifr.rs` (модульные, в конце файла)

**Interfaces:**
- Consumes: `IfrBuilder` (существующий), константы r-efi `IFR_*`.
- Produces:
  `pub fn splice_question_ops(package: &mut Vec<u8>, formset_idx: usize, form_id: u16, ops: &[u8]) -> Result<(usize, usize), HiiError>` —
  вставляет `ops` перед END_FORM формы `form_id` (после последнего вопроса
  формы); возвращает `(insert_at, delta)`; `insert_at` — позиция первого
  вставленного байта; ошибка `HiiError::NotFound`, если форма не найдена;
  `HiiError::InvalidSchema`, если `ops` пуст. Побочно НЕ правит $SPF (это Task 2)
  и НЕ собирает op-байты (собирает Task 3 через IfrBuilder).

- [ ] **Step 1: failing-тесты** — 4 теста: (1) вставка перед END_FORM указанной
  формы (синт-пакет: formset с формами 100, 200; вставка в 100 → опкоды между
  последним вопросом формы 100 и её END); (2) выбор formset_idx при двух
  формсетах в пакете; (3) NotFound на несуществующую форму; (4) InvalidSchema
  на пустые ops. Тестовый хелпер — синт-пакет из существующих тестов
  `insert_form_into_package` (ifr.rs:591+), переиспользовать `opcode()`.
  Правка по факту (dfbddc1): имя `two_form_package` уже занято в тестах
  (формы 901/902) — старый хелпер переименован в `two_form_raw_package`,
  новое имя отдано синт-пакету 100/200 из теста выше.

```rust
#[test]
fn splice_question_ops_inserts_before_end_form() {
    let mut pkg = two_form_package(); // формы 100 и 200, в 100 — один one_of
    let before_len = pkg.len();
    let ops = opcode(0x05, true, &[0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10]);
    let (at, delta) = splice_question_ops(&mut pkg, 0, 100, &ops).unwrap();
    assert_eq!(delta, ops.len());
    assert_eq!(pkg.len(), before_len + delta);
    // ops лежат после существующего вопроса формы 100 и перед её END (0x29)
    assert_eq!(&pkg[at..at + ops.len()], &ops[..]);
    let window = &pkg[at + ops.len()..at + ops.len() + 2];
    assert_eq!(window, &[0x29, 0x02]); // END_FORM сразу после вставки
}
```

- [ ] **Step 2:** `cargo test -p uefi-engine ifr::` — падают (функции нет).
- [ ] **Step 3:** Реализация `splice_question_ops`: повторить обход
  `formset_at`/`insert_form_into_package` (ifr.rs:216) для нахождения границ
  формы (позиция END_FORM = конец scope-цепочки формы `form_id` внутри
  formset_idx); `Vec::splice(insert_at..insert_at, ops.iter().copied())`;
  обновить 3-байтный заголовок длины форм-пакета по образцу
  `insert_form_into_package` (без него выросший пакет не парсится).
- [ ] **Step 4:** `cargo test -p uefi-engine ifr::` — зелёные; `cargo clippy -p uefi-engine -- -D warnings`; `cargo fmt --all -- --check`.
- [ ] **Step 5:** Commit: `feat(uefi-engine): ifr splice_question_ops — insert question opcodes before target form END`.

### Task 2: spf.rs — аддитивный слой $SPF (записи, строковые контроли, клон страницы)

**Files:**
- Modify: `crates/uefi-engine/src/hii/spf.rs`
- Test: `crates/uefi-engine/src/hii/spf.rs` (модульные)

**Interfaces:**
- Produces (все — чистые функции над `&mut Vec<u8>` контейнера $SPF,
  офсеты контейнерные, от сигнатуры `$SPF`):
  - `pub const SPF_CONTAINER_BASE: usize` — офсет контейнера в теле секции
    (0x10 для LIVE; ищется по сигнатуре `$SPF`, не хардкодится: helper
    `pub fn container_start(body: &[u8]) -> Option<usize>`).
  - `pub fn append_question_record(body: &mut Vec<u8>, template: usize, qid: u16, help_id: u16, prompt_id: u16, ifr_offset: u32, counter: u32, failsafe: u8, optimal: u8) -> usize` —
    клонирует 72Б записи по образцу `template` (запись q35), патчит
    qid/help/prompt/ifr/counter/fs/opt; аппенд в конец контейнера; возвращает
    офсет новой записи. Контейнер растёт `body.extend`.
  - `pub fn append_string_control(body: &mut Vec<u8>, template: usize, string_id: u16) -> usize` —
    клонирует 0x0C-байтный строковый контроль `{05, str_id, 0, a, b, c}`
    по образцу, патчит str_id; аппенд; возвращает офсет (самего блока str_id —
    как в `scan_string_controls`, офсет поля).
  - `pub fn clone_page_with_controls(body: &mut Vec<u8>, page_offset: usize, extra_ctrls: &[u32]) -> usize` —
    читает страницу (cnt@+0x1C, список u32@+0x20, формула размера
    `0x20 + 4*cnt (+4 хвостовой слот)`. Хвостовой слот — терминальный:
    новые указатели вставляются ПЕРЕД хвостовым полем, список cnt+N
    непрерывен от +0x20, хвост остаётся в конце клона; у страниц без
    хвоста новые указатели просто аппендятся после списка),
    строит клон с cnt+N и списком = старые + `extra_ctrls`, аппендит в конец;
    возвращает офсет клона. Плато-поля (marker, form_id, title, seq, B@+0x12,
    u18@+0x18) копируются байт-в-байт.
  - Конвенция офсетов (правка по ревью Task 2): API-параметры
    (`template`, `page_offset`) и возвраты append/clone — контейнерные
    (от сигнатуры `$SPF`); сканеры `scan_question_records`/
    `scan_string_controls` возвращают абсолютные body-офсеты — потребитель
    переводит `abs - container_start`. Patcher-функции
    (repoint/bump/fixup) требуют валидный контейнер — вместо молчаливого
    fallback на неподтверждённой сигнатуре паникуют.
  - `pub fn repoint_page_slot(body: &mut [u8], slot: usize, new_offset: u32)` —
    пишет u32 @ `container + 0x64 + 4*slot`.
  - `pub fn bump_container_length(body: &mut [u8], new_len: usize)` —
    правит поле длины контейнера в заголовке: по карте tse-bridge §3
    последний header-регион-офсет == длина контейнера == максимум u32 в
    зоне офсетов [0x30..0x60). Правка по ревью Task 2: поиск «по значению
    == текущая длина» неосуществим после аппендов (поле хранит старую
    длину, тело уже выросло) — гарантированный no-op; поле ищется как
    максимум зоны и патчится на `new_len` (идемпотентно).
  - `pub fn fixup_record_ifr_offsets(body: &mut [u8], threshold: u32, delta: u32) -> usize` —
    всем 72Б-записям с подписью F8 и `ifr@+0x1C >= threshold` прибавляет
    delta; возвращает число правок (для инварианта 389-x).

- [ ] **Step 1: failing-тесты.** Синт-контейнер-фабрика по карте tse-bridge
  §3 + пре-check §9 (заголовок до 0x64 с count, таблица слотов, страница с
  cnt=2 и списком контролов, 2 записи с F8-подписью, строковый контроль,
  хвост с полем длины). Тесты: (1) append_question_record — клон растит
  контейнер, патчит поля, старые байты не тронуты (сравнить префикс); (2)
  append_string_control аналогично; (3) clone_page_with_controls — cnt 2→4,
  старые указатели на месте + новые в конце списка, плато сохранено; (4)
  repoint_page_slot; (5) fixup_record_ifr_offsets — правит только ifr >=
  threshold, возвращает счёт; (6) bump_container_length. Код теста — по
  образцу существующих `question_record()` в spf.rs:92.

```rust
#[test]
fn append_question_record_clones_template_and_patches_fields() {
    let mut body = synth_container();
    let base_len = body.len();
    let off = append_question_record(&mut body, TEMPLATE_Q35, 0x200, 900, 901, 0x993, 0x1_0095, 0, 0);
    assert_eq!(body.len(), base_len + SPF_RECORD_SIZE);
    assert_eq!(&body[..base_len], &synth_container()[..]); // E9: старое не тронуто
    assert_eq!(u32::from_le_bytes(body[off..off + 4].try_into().unwrap()), 0x200);
    assert_eq!(u16::from_le_bytes(body[off + SPF_RECORD_HELP_ID..off + SPF_RECORD_HELP_ID + 2].try_into().unwrap()), 900);
    assert_eq!(u32::from_le_bytes(body[off + SPF_RECORD_IFR_OFFSET..off + SPF_RECORD_IFR_OFFSET + 4].try_into().unwrap()), 0x993);
}
```

- [ ] **Step 2:** `cargo test -p uefi-engine spf::` — падают.
- [ ] **Step 3:** Реализация по интерфейсам выше (клоны — копирование среза с
  патчем полей; никаких сдвигов середины).
- [ ] **Step 4:** `cargo test -p uefi-engine spf::` — зелёные; clippy; fmt.
- [ ] **Step 5:** Commit: `feat(uefi-engine): spf additive layer — record/string-control append, page clone, slot repoint, ifr fixup`.

### Task 3: op `hii question add` (движок + RPC + CLI)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (op + оркестрация),
  `crates/uefi-engine/src/hii/schema.rs` (QuestionAddSchema),
  `crates/uefi-engine/src/hii/values.rs` (переиспользование без правок,
  при необходимости — `pub(crate)` хелпер ширин),
  `crates/uefi-engine/src/rpc/*` (метод), `crates/uefi-cli/src/client.rs`,
  `crates/uefi-cli/src/commands/hii.rs`, `crates/uefi-cli/src/main.rs`,
  `crates/uefi-cli/src/output.rs`.
- Test: модульные в mod.rs/schema.rs + интеграционный hii-тест.

**Interfaces:**
- Produces:
  - `QuestionAddSchema` (serde, `deny_unknown_fields`):
    `{ "form_id": u16, "prompt": String, "help": String, "question_id": u16,
    "var_store_id": u16, "var_offset": u16, "size": u8,
    "options": [ {"text": String, "value": u64, "default": "optimized"?} ],
    "defaults": {"optimized": u64} }` — одно QuestionAddSchema = один вопрос;
    CLI принимает **файл со списком** `{"questions": [ … ]}`.
  - `pub fn add_question(image: &mut Image, item_id: &str, schema: &QuestionAddSchema) -> Result<AddQuestionResult, HiiError>`:
    `AddQuestionResult { question_id: u16, string_ids: HashMap<String, u16>, spf_record_offset: usize }`.
    Оркестрация (по образцу form_hijack: цель Setup-PE, режим Write,
    LZMA-слоты пересобираются существующим механизмом mark_rebuild_to_root):
    (1) валидации: qid уникален в формсете, var_offset не пересекает
    существующие вопросы того же варстора (ширины из values.rs), size<=1
    для one_of-семантики опций u8; (2) `string_pack::add_strings_to_resource`
    — prompt/help/options тексты; (3) `IfrBuilder` собирает one_of+options+
    default+END; (4) `splice_question_ops` (Task 1) — форма `form_id`;
    (5) `$SPF` (секция SetupData — правка по факту Task 4: `pfs_payload_path
    (image, None)` НЕ резолвит LIVE-геометрию — UI "AMITSESetupData" внук
    за GUID_DEFINED-обёрткой, fallback проверяет только прямых детей;
    marker-fallback требует body%72==0, LIVE body 49860. form_hijack
    резолвил секцию явным GUID из hijack-схемы; S3-схема и CLI-контракт
    GUID не содержат → add_question обязан находить SetupData сам —
    поиском в глубину через GUID_DEFINED-детей по UI-имени или
    $SPF-сигнатуре в декомпрессированном теле):
    `fixup_record_ifr_offsets(threshold=insert_at, delta)` — правка по
    LIVE-факту Task 4: fixup ИЗБИРАТЕЛЬНЫЙ — до IFR-мутации снять карту
    Setup-резолвящихся записей (32 на LIVE), после сплайса сдвинуть только
    их; 355 foreign-записей (2-й класс, видимы сканеру, ifr не декодирован)
    не трогать →
    `append_question_record` (клон записи первого one_of-вопроса целевой
    формы; если нет F8-образца в её записях — клон первой F8-записи
    контейнера) + `append_string_control` (клон первого строкового
    контроля страницы) ×2 вопроса → `clone_page_with_controls` (клон
    страницы формы, новые контроли указывают на новые записи) →
    `repoint_page_slot` → `append`-рост контейнера →
    `bump_container_length`; (6) пересборка слотов; (7) марк rebuild.
    Counter новых записей: `(0x0001 << 16) | (1 + max(counter_low))` по
    всем записям контейнера.
  - CLI: `uefi-cli hii question add <item_id> --file schema.json`
    (item_id = `3/28/1/0#10019`); вывод — таблица qid + string ids.

- [ ] **Step 1: schema-тесты** (schema.rs): парсинг списка вопросов,
  unknown-field rejection, пустой список — ошибка.
- [ ] **Step 2:** тест-красный, реализация `QuestionAddSchema`.
- [ ] **Step 3: интеграционный failing-тест** (mod.rs, синт-образ из тестов
  form_hijack — flash-образ с Setup-PE (ресурсный канал, forms-пакет с формой
  10019 и q35-образцом, LZMA-слот) + секция SetupData с синт-$SPF):
  после `add_question` — (а) find_question находит новый qid с varstore/
  offset/options; (б) новый текст резолвится через строковый пакет;
  (в) $SPF: запись с qid/help/prompt/ifr существует, слот страницы указывает
  на клон с cnt+1, строковый контроль с новым help-id существует;
  (г) ifr-fixup: записи, чьи ifr были >= точки вставки, увеличены;
  (д) re-parse образа проходит, повторный add_question того же qid — ошибка.
- [ ] **Step 4:** Реализация op + RPC + client + CLI + output (по образцу
  `hii_form_hijack` цепочкой: proto-метод → server → client → commands →
  main enum → output).
- [ ] **Step 5:** `cargo test -p uefi-engine -p uefi-cli`; clippy оба; fmt.
- [ ] **Step 6:** Commit: `feat: hii question add — insert OneOf question into live form (IFR splice + strings + additive SPF record/page clone)`.

### Task 4: real-image гейт `real_image_ops_insert_serial_s3`

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новый `#[ignore]`-тест),
  `crates/uefi-engine/tests/data/` — новый JSON `serial/s3_questions.json`
  (схема списка из 2 вопросов — контракты «Схемы данных», коммитится).

**Содержимое s3_questions.json (дословно):**

```json
{
  "questions": [
    {
      "form_id": 10019,
      "prompt": "Baud Rate",
      "help": "Serial console port speed (applies on next boot)",
      "question_id": 512,
      "var_store_id": 1,
      "var_offset": 93,
      "size": 1,
      "options": [
        {"text": "115200", "value": 0, "default": "optimized"},
        {"text": "57600", "value": 1},
        {"text": "38400", "value": 2},
        {"text": "19200", "value": 3},
        {"text": "9600", "value": 4}
      ]
    },
    {
      "form_id": 10019,
      "prompt": "Terminal Type",
      "help": "Serial console terminal type (applies on next boot)",
      "question_id": 513,
      "var_store_id": 1,
      "var_offset": 94,
      "size": 1,
      "options": [
        {"text": "VT-UTF8", "value": 0, "default": "optimized"},
        {"text": "VT-100+", "value": 1},
        {"text": "VT-100", "value": 2},
        {"text": "ANSI", "value": 3}
      ]
    }
  ]
}
```

- [ ] **Step 1: failing-тест** `real_image_ops_insert_serial_s3` (копия
  структуры `real_image_ops_insert_serial_s2`, путь к образу тот же):
  база → S2-вставка тройки (артефакты из tests/data/serial) →
  `add_question` ×2 (schema из s3_questions.json) → save → инварианты:
  (1) find_question(10019, 512/513) — varstore 1, offsets 93/94, по 5/4
  опции, дефолты 0; (2) новые строки в пакете (тексты дословно);
  (3') $SPF: записи qid 512/513 (help-ids = новые), слот страницы формы
  10019 → клон, cnt 6→8; entries[0..6] байт-в-байт == базовой странице;
  entries[6..8] = spf_record_offset новых записей — прямые указатели на
  72Б F8-записи с qid 512/513 (правка по LIVE-факту Task 4: 6 старых
  элементов страницы 10019 указывают на пул-контрол-блоки — двухуровневая
  конвенция tse-bridge §3; прямая конвенция доминирует в контейнере,
  4736 элементов таблиц указывают на записи напрямую; семантика TSE не
  декодирована — риск снимает дискриминатор E31-(b) с контингенси-картой
  E29); (4') ifr-инвариант (правка по LIVE-факту Task 4: из 389 базовых
  F8-записей Setup-формсету принадлежат 34 — резолвятся (qid, ifr) в
  forms-пакете Setup; 355 foreign не резолвятся ни в одном пакете образа):
  (a) R_base = резолвящиеся записи базового образа (live = 32, вычислять,
  не хардкодить; опционально пинировать |R_base| как регресс-гард);
  (b) каждая r ∈ R_base после пересборки резолвится на base_ifr + Σdelta;
  (c) записи 512/513 резолвятся на точных позициях сплайса;
  (d) foreign-записи НЕ изменены (final_ifr == base_ifr — избирательный
  fixup; live-числа fix-раунда 2: 32 Setup-резолвящихся / 357 foreign,
  предикат `spf_record_resolves` — общий для движка и гейта); (5) дифф образа confined: окно S2 [0xB63B18, 0xB81C18) +
  слоты файлов Setup (3/28) и SetupData (3/208) — вне этих трёх зон 0
  изменённых байт; (6) state-байты тройки 0xF8; SerialDxe/TerminalDxe
  вставленные тела байт-идентичны файлам tests/data/serial (sha256-сверка
  распакованных тел против файлов); (7) повторный rebuild стабилен
  (save → open → save, побайтово).
- [ ] **Step 2:** Запуск `UEFIPATCHER_TEST_IMAGE=… cargo test -p uefi-engine --test real_image real_image_ops_insert_serial_s3 -- --ignored` (или путь по прецеденту S2-теста) — красный.
- [ ] **Step 3:** Реализация теста + фикстуры s3_questions.json (commit'ится).
- [ ] **Step 4:** Прогон зелёный; полный `cargo test -p uefi-engine` —
  регресс (425+ nuevos / 0 failed); clippy; fmt.
- [ ] **Step 5:** Commit: `test(uefi-engine): real_image_ops_insert_serial_s3 gate — S2 triple + two Setup questions, full invariants`.

### Task 5: config-aware glue (пересборка, S1-стенд)

**Files:**
- Modify: `docker/edk2/UefiPatcherSerialPkg/SerialConsoleGlue/SerialConsoleGlue.c`,
  при необходимости `SerialConsoleGlue.inf` (протоколы: gEfiRuntimeArch — нет,
  используется lib: UefiRuntimeServicesTableLib уже в зависимостях DXE-минимума;
  проверить INF [LibraryClasses] и [Protocols] — добавлений не ожидается).
- Замена артефакта: `crates/uefi-engine/tests/data/serial/SerialConsoleGlue.ffs`
  (новый sha256 в отчёт Task 6; SerialDxe/TerminalDxe не трогаются).

- [ ] **Step 1:** Правка SerialConsoleGlue.c по контракту «Схема данных»:
  статический helper `ReadConfigBytes()` (GetVariable Setup → байты 93/94 c
  fallback 0; GetVariable PNP0501_0_NV → байт 0 c fallback 1), таблицы
  `kBaudMap[5] = {115200, 57600, 38400, 19200, 9600}` и терминал-GUID таблица;
  в существующем пути: enable==0 → ранний return (до LocateProtocol/
  ConnectController/Con*-append); baud → `SerialIo->SetAttributes(baud, 0, 0,
  NoParity, 8, OneStopBit)` после получения SerialIo (перед
  ConnectController терминала); term → выбор vendor-GUID в существующем месте
  конструирования device path (оба места: найденный child и сконструированный
  путь — R-S1.2). Маркеры и порядок сообщений не меняются. Комментариев нет
  (кроме file:line на r-efi GUID-константы при необходимости).
- [ ] **Step 2:** Сборка двойная (каждая — свежий контейнер + свежий
  git-archive, по S1-методу): `docker/edk2/build_serial.sh` ×2 → все три .ffs:
  SerialDxe/TerminalDxe байт-идентичны текущим в tests/data/serial (sha256),
  glue — `reproducible (identical)` между сборками.
- [ ] **Step 3:** `hack/edk2_build_check.py` — прогон (3/3 инварианта типа/
  machine/subsystem/size24); `cargo test -p uefi-engine --test serial_ffs` —
  зелёный (гейт согласованности хелперов).
- [ ] **Step 4:** Замена SerialConsoleGlue.ffs в tests/data/serial (коммитится
  новый бинарь — прецедент fix-волны S1: то же место); обновление sha256 в
  Task-6 отчёте; `cargo test -p uefi-engine` — зелёный (real_image s3-гейт из
  Task 4 прогоняется с новым glue-артефактом — вставляется его новое тело).
- [ ] **Step 5:** Commit: `feat(serial): config-aware glue — reads Setup[0x5D/0x5E] + PNP0501_0_NV[0], applies baud/terminal type, enable=0 skips attach (artifact rebuilt, SerialDxe/TerminalDxe untouched)`.

### Task 6: E31-pack — кандидат + протокол приёмки + статусы дуги

**Files:**
- Create: `docs/reports/2026-09-06-serial-s3-e31-pack.md`
- Modify: `docs/superpowers/specs/2026-09-05-serial-console-ladder-design.md` (§7-строка «S3 готова к E31»), `roadmap.md`.

- [ ] **Step 1:** Сборка кандидата транскриптом (сохранить дословно в отчёт;
  окружение как S2): `UEFIPATCHER_SOCK=/tmp/serial-s3/engine.sock`;
  `rm -f "$UEFIPATCHER_SOCK"`; запуск engine (nohup, лог); `session init
  --force`; `image open --mode write refs/fw/HNX99TF_200525_original_E5C88C6F.bin`;
  `node insert 3/215 --file tests/data/serial/SerialDxe.ffs --mode after` →
  `3/216`; TerminalDxe → `3/217`; SerialConsoleGlue (новый) → `3/218`;
  `hii question add 3/28/1/0#10019 --file crates/uefi-engine/tests/data/serial/s3_questions.json`;
  `image save /tmp/serial-s3/E31-candidate.bin`; sha256sum базы и кандидата;
  `pkill -x engine`. Скопировать кандидата в `~/E31/E31-candidate-<sha8>.bin`
  ДО любых ребутов (прецедент E30).
- [ ] **Step 2:** Независимая валидация: `hack/fv_audit.py` (FV1 files 219,
  used/free_tail ±123 140 против базы; FV0/прочие тома идентичны) + байтовый
  python-дифф: изменения только в [окно S2] + слот Setup + слот SetupData;
  счёт изменённых байт в отчёт.
- [ ] **Step 3:** Отчёт E31-pack по скелету E30-pack: §1 стенд+транскрипт,
  §2 артефакт (sha256, состав), §3 инварианты (гейт-тест + fv_audit +
  байтовый авторитет), §4 протокол E31: (a) дефолт-бут — консоль на COM1
  115200 8N1, поведение = E30 (никаких регрессий); (b) Setup → Advanced →
  Super IO Configuration → Serial Port 1 Configuration: видны «Serial Port
  [Enabled]», «Change Settings», НОВЫЕ «Baud Rate [115200]» и «Terminal Type
  [VT-UTF8]» + хелпы; (c) Baud Rate → 57600 → Save & Exit → ребут → весь
  вывод и Setup-навигация на 57600 (терминал перенастроить!); (d) возврат
  115200 → ребут → 115200; (e) Terminal Type → VT-100 → ребут → экран
  рендерится в VT-100-эмуляции; (f) Serial Port → Disabled → ребут → UEFI-
  консоли на COM1 нет (OS-вывод после загрузки ОС остаётся — Linux програм-
  мирует UART сам); (g) Load Optimized Defaults → Baud/Terminal возвращаются
  к [115200]/[VT-UTF8]. Контингенси: вопросы не рендерятся → карта каналов
  E29 (запись s14/пул-контроль/counter/type) — итерация полей записи
  движком (без прошивки-перебора: единый диагностический образ с печатью
  статуса glue в COM при enable=1: маркер SC-S1 уже даёт «драйверы живы»);
  0xB2-quirk из E30 — отдельный открытый вопрос, не гейт S3. §5 инструкция
  (копия из /tmp до ребута; откат — рефлеш E30-candidate 09f5e897… или базы),
  §6 открытое (вердикт за владельцем; закрытие S3 — аддендум по вердикту).
- [ ] **Step 4:** Спека §7: строка «S3 готова к E31 (дата, кандидат sha256,
  состав)»; roadmap: S3 → «в ожидании E31» с указателем на отчёт.
- [ ] **Step 5:** `cargo test --all` (с cleaned env) + `cargo clippy --all -- -D warnings` + `cargo fmt --all -- --check` — всё зелёное; Commit:
  `docs(s3): E31 flashpack — candidate built, protocol, spec/roadmap ready-to-flash state`.
- [ ] **Step 6:** Push dsevosty serial-console; финальный ответ владельцу:
  кандидат готов к прошивке, протокол в отчёте §4, вердикт E31 — за владельцем.

## Self-Review (выполнен при написании)

- Покрытие спеки §3-S3: «пункты serial-настроек механикой hijack/append» —
  add-question op (Tasks 1–3); «varstore — существующая семья, новых не
  вводить» — Setup id 1 (pre-check §9-3/4); «драйверы читают переменную» —
  config-aware glue (Task 5); «гейт: пункты видимы, смена baud меняет
  скорость» — протокол E31 (b)–(c) (Task 6). Ветка «enable» бесплатно —
  существующий q34.
- Placeholder-скан: код тестов приведён для Task 1–2; Task 3–5 — поэлементные
  интерфейсы с точными сигнатурами/файлами (уровень детализации планов цикла 6).
- Типы: сигнатуры Task 1/2 согласованы с употреблением в Task 3; схема JSON
  синхронна между Task 3 (парсер) и Task 4 (файл-фикстура).

## Риски (из пре-check, не блокеры)

1. **Первая живая аддитивная вставка $SPF** — геометрия доказана статикой
   (tse-bridge §4, S0 §3.6) и клонированием образца целевой платы, но не
   железом; дискриминатор E31-(b) + карта каналов E29 для итерации полей.
2. **IFR-сдвиг** — компенсируется fixup записей; инвариант-тест 389+2 (Task 4).
3. **Timing GetVariable в DXE** (до материализации StdDefaults) — fallback =
   дефолтные значения, поведение = E30; изменений пользователь не видит.
4. **0xB2-quirk E30** — вне гейта S3, парковка в TODO сохраняется.
