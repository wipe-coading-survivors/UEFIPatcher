# HII PE-resource extraction — дизайн фазы 6 (read-only)

Дата: 2026-08-14. Ветка: `fix/cycle6-reimplent`. Предыдущие фазы:
`2026-08-14-hii-phase3-string-reader` … `2026-08-14-hii-phase5-targeting-validation`.
Следующая фаза (рекомпрессия): findings-документ
`docs/plans/2026-08-14-предпосылки-для-рекомпрессии.md`.

## 1. Контекст и мотивация

Фаза 5 показала: ридеры фаз 3–4 (`collect_forms`/`collect_strings`) проверяют
только тела секций (`node.body`) на bare-пакеты — и на реальном образе
HNX99TF (AMI Aptio V, Lenovo) возвращают пусто. Два acceptance-теста в
`crates/uefi-engine/tests/real_image.rs` красные:
`real_image_hii_forms_and_strings`,
`real_image_hii_form_visibility_round_trip`.

Реальное хранение HII (подтверждено живыми байтами, см. §2): package lists
внутри PE-ресурсов типа `L"HII"` (AMI + EDK2) и, у EDK2, FORM-пакеты
bare-массивами в теле PE.

Решение пользователя (вариант C): эта фаза — **read-only** извлечение с
честным отказом на мутации за compression barrier; рекомпрессия — отдельная
следующая фаза со своей спекой. Критерий приёмки — реальные образы
(HNX99TF + EDK2), не синтетика.

## 2. Эталон — разведка на живых байтах (2026-08-14)

Probe-скрипт (Python, `/tmp/hii_probe.py`): raw-MZ-скан + распаковка
GUIDed-LZMA (все 4 LZMA-GUID из `ffs.rs`) + MZ-скан внутри распакованного +
обход resource directory. Образцы: HNX99TF_200525 (Lenovo AMI Aptio V),
OVMF 4MB ×2, edk2-rk3588 1.9MB (Orange Pi 5 Plus) + капсула 6.9MB.

Зафиксированные факты:

1. **Тип PE-ресурса — именованный `L"HII"`** (UCS-2). У AMI и EDK2
   одинаково. Отчёт фазы 5 («'H'») читал имя неполно.
2. **Package list**: `16 B list GUID + u32 total + конкатенация пакетов`.
   Пакет: `u24 длина + u8 тип` (типы `r_efi::hii::PACKAGE_*`), терминатор
   `PACKAGE_END = 0xDF`. Инвариант AMI: `list_guid == GUID FFS-файла`-
   владельца.
3. **HNX99TF: 6 списков, 4 формсета** — Setup `899407D7` (FORM 10 082,
   formset `7B59104A-C00D-4158-87FF-F04D6396A915` + 2 STRING), Platform
   `ABBCE13D` (FORM 207 249, formset `EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9`
   + 2 STRING), `70E1A818` (formset `80E1202E`), `CDC1C80D` (formset
   `932D37B0`), + 2 строковых списка. Тест `formsets.len() >= 2` корректен.
4. **Заголовок строкового пакета** (HNX99TF и edk2-rk3588 идентичны):
   `+0 u24 len + u8 type(0x04)`, `+4 HdrSize u32`, `+8 StringInfoOffset
   u32`, `+12 LanguageWindow CHAR16[16]` (32 B, обычно нули), `+44
   LanguageName u16` (string-id отображаемого имени языка), `+46 Language
   CHAR8` NUL-terminated, переменной длины («en-US» → HdrSize 52, длиннее
   → 57). Первый SIBT-блок — по `StringInfoOffset` (обычно имя языка
   UCS-2, напр. «English»). **`strings.rs` (`LANGUAGE_OFFSET=46`, граница
   `info_off`) уже корректен** — переделка не нужна.
5. **EDK2 хранит HII двойственно**: STRING/IMAGES-пакеты — в 'HII'-
   ресурсах; FORM-пакеты — **bare-конст-массивами в теле PE** (VFR→массив,
   HiiAddPackages в рантайме). Ресурс-канал один покроет AMI, но не формы
   EDK2 → нужен второй канал (§4.3).
6. `object` 0.39.1 (workspace-версия) содержит read-API ресурсов:
   `object::read::pe::{ResourceDirectoryTable, ResourceDirectory,
   ResourceName, ResourceDirectoryEntryData, ResourceNameOrId}`.

## 3. Scope

**Входит** (согласовано с пользователем):

1. PE-resource extraction ('HII') + package-list сплиттер + wiring в
   `collect_forms`/`collect_strings`;
2. bare-канал FORM-пакетов в телах PE32 (глубокая валидация, дедуп);
3. per-file скопинг титулов (вместо глобальной карты string-id);
4. резолюция таргета `guid:subtype:idx` по вложенным секциям (DFS через
   guided/compressed-обёртки) + зелёный тест `section_index: Some(n>0)`;
5. обязательная граница заявленной длины пакета в IFR-walker (бывший
   отложенный TODO фазы 4 — без него bare-канал небезопасен);
6. `set_item_visibility`: Write-гейт + честный отказ за compression
   barrier;
7. выравнивание синтетических фикстур фаз 3–4 под реальный layout строкового
   заголовка (язык @+46);
8. косметика `real_image.rs`: единая ignore-строка, сообщение language-
   assert.

**Не входит** (явно): рекомпрессия LZMA (следующая фаза, findings-документ);
TUI HII-view (TODO); bare-скан STRING-пакетов (у EDK2 строки в ресурсах;
добавим, если живые данные покажут обратное); SIBT EXT1/2/4 и прочие
отложенные миноры фаз 3–4; изменения proto/RPC/CLI-вывода (FormInfo/
StringInfo не меняются).

## 4. Архитектура: модули

Новые модули в `crates/uefi-engine/src/hii/`; дерево/builder/proto не
меняются (подход 1 — ленивое извлечение на чтении).

### 4.1 `hii/pe_resource.rs` — ресурс-канал

```rust
pub fn hii_resource_blobs(pe: &[u8]) -> Vec<&[u8]>
```

Через `object` (добавить в зависимости `uefi-engine`; в workspace уже
объявлен `object = "0.39"` c features `read_core, pe`): `PeFile64::parse`
(fallback `PeFile32`) → data directory №2 (RESOURCE) → обход
`ResourceDirectoryTable` (type → name → lang) → на уровне TYPE именованный
`"HII"` → все листы-данные. Не-PE / нет .rsrc / кривой каталог — не ошибка:
пустой возврат + `tracing::debug`.

### 4.2 `hii/package_list.rs` — сплиттер

```rust
pub struct HiiPackage<'a> { pub kind: u8, pub bytes: &'a [u8] }   // bytes = весь пакет, с заголовком
pub struct HiiPackageList<'a> { pub guid: Guid, pub packages: Vec<HiiPackage<'a>> }
pub fn parse_package_list(bytes: &[u8]) -> Option<HiiPackageList>
```

Правила: `len >= 20`; GUID 16 B + u32 total; пакеты с `+20`, стоп на
`PACKAGE_END (0xDF)`; границы проверяются (`plen >= 4`, `pos + plen <=
len`); malformed → `None` + `warn` (строгий отказ, ничего не фабрикуем).
`total` — только подсказка (на AMI встречается `total == len - 1`), жёстко
не валидируется. Диспетч по `kind`: `PACKAGE_FORMS` → `parse_form_package`,
`PACKAGE_STRINGS` → `parse_string_package`; прочие типы пропускаются.

### 4.3 bare-канал FORM-пакетов

```rust
pub fn bare_form_packages(pe: &[u8], exclude: &[(usize, usize)]) -> Vec<&[u8]>
```

Скан тела PE32 на паттерн `[u24 len][0x02][IFR_FORM_SET_OP=0x0E]`; границы;
диапазоны, покрытые ресурсными блобами (`exclude`), отбрасываются;
кандидат обязан пройти **глубокую валидацию**: `parse_form_package` на
срезе ровно в заявленную длину успешно парсится (§4.4). Без валидации
нельзя: на реальных образах паттерн даёт ложные срабатывания (мусорные
«формсеты» вида `0203852A…` в данных PE).

### 4.4 Граница длины пакета в IFR-walker (`hii/ifr.rs`)

`parse_form_package` обязан потреблять опкоды только в пределах заявленной
u24-длины пакета (балансный END, без чтения за границей); хвост за
заявленной длиной игнорируется. Это закрывает TODO фазы 4 и делает
bare-канал безопасным.

### 4.5 Перестройка обхода (`hii/strings.rs`, `hii/forms.rs`)

Единый DFS по дереву с контекстом файла. Для каждой секции внутри файла X:

- subtype `0x19` + bare-пакет в теле → старый путь (сохранён);
- subtype `0x10` (PE32) → `hii_resource_blobs` → `parse_package_list` →
  диспетч STRING/FORM; затем `bare_form_packages` с exclude-диапазонами.

`collect_forms`: титулы — карта `u16 → String` **только из string-пакетов
этого файла** (first-wins), закрывает cross-file коллизии StringId.
`collect_strings`: строки из ресурсных STRING-пакетов добавляются к
существующим (язык читается уже корректным кодом @+46).

### 4.6 Нумерация form_id (единая семантика)

`form_id = "{file_guid}:{subtype:#04x}:{idx}"` остаётся. Для форм из
PE-ресурсов/bare — subtype `0x10` (PE32-секция). `idx` — **pre-order DFS
номер среди секций того же subtype под файлом, включая спуск внутрь
guided/compressed-обёрток**. На образах без обёрток нумерация байт-в-байт
совпадает с текущей → синтетические тесты фаз 3–5 остаются зелёными.
Несколько формсетов внутри одной PE32-секции дают один target (FormInfo —
на форму, не на пакет).

## 5. Таргетинг по вложенным секциям

GuidSection-arm в `find_item` и `find_item_path` (`parser/target.rs`):
вместо сканирования прямых детей файла — тот же DFS через обёртки с тем же
счётчиком (зеркало нумерации §4.6). Изменение семантики (было:
direct-children-only) — документируется здесь. Тесты: резолюция PE32
внутри LZMA (`Some(0)`), `Some(n>0)`, round-trip «form_id из collect_forms
→ find_item → та же секция».

## 6. `set_item_visibility` — гейты

Новые варианты `HiiError`:

- `NotWritable` — `image.mode != ImageMode::Write`;
- `MutationBehindCompression` — среди предков цели (по пути из
  `find_item_path`) есть Section subtype `0x01` (COMPRESSION) или `0x02`
  (GUID_DEFINED). Сообщение упоминает необходимость рекомпрессии
  (следующая фаза).

Bare-путь (несжатая секция 0x19) — существующее поведение (unsuppress +
Rebuild-каскад), тесты фаз 4–5 зелёные.

**Поправка acceptance-теста** `real_image_hii_form_visibility_round_trip`:
резолюция form_id → PE32-секция остаётся; вместо `Ok(())` ожидается
`Err(MutationBehindCompression)`. Обоснование: решение вариант C
(пользователь, 2026-08-14) — честный отказ вместо тихой потери мутации за
барьером. Поправка оформляется отдельным шагом плана с отсылкой к этому
пункту спеки.

## 7. Ошибки

- Ошибки extraction — не ошибки: отсутствие .rsrc, не-PE, отсутствие 'HII'
  — норма (`debug`); malformed package list → `warn` + пропуск.
- `parse_target`/`find_item` — существующие ошибки; новая семантика DFS —
  только расширение.
- Гейты §6 — новые варианты `HiiError`; CLI-вывод сообщений без изменений
  инфраструктуры.

## 8. Фикстуры

Источник — **edk2-rk3588** (открытая лицензия; Lenovo-байты в репо не
коммитим):

- `tests/fixtures/hii_rk3588_string_res.bin` — 'HII'-ресурс-список
  (list `A487A478`, STRING 6 852 B, язык en-US) — ресурс-канал + реальный
  строковый заголовок;
- `tests/fixtures/hii_rk3588_bare_form.bin` — bare FORM-пакет (2 186 B,
  formset `5DAF50A5-EA81-4DE2-8F9B-CABDA9CF5C14`) — bare-канал;
- PE-обёртки: минимальный синтетический PE с .rsrc 'HII' собирается в
  тесте (без бинарной фикстуры); либо коммитится маленький настоящий PE из
  rk3588 — финализируется в задаче-0 плана;
- Lenovo-подобный список FORM+STRING (list_guid = file GUID) синтезируется
  в тесте из существующих форм-фикстур (заголовок списка тривиален).

Real-image `#[ignore]`-тесты остаются на внешнем файле HNX99TF.
`UEFIPATCHER_TEST_FW` может указывать на edk2-rk3588 для ручной
перекрёстной проверки (формы — через bare-канал, строки — через
ресурсы); OVMF для этого не годится: в нём probe нашел только
images-список (строк-ресурсов нет) — строковый ассерт может упасть.
Acceptance-критерий фазы — HNX99TF.

## 9. Тесты и приёмка

- **Unit**: сплиттер (happy/END/обрезка/мусор → None); `hii_resource_blobs`
  на синтетическом PE; bare-канал (валидный принимается, мусор
  `0203852A`-подобный отвергается, дедуп по диапазонам); walker с границей
  длины (хвост за длиной игнорируется, обрыв → None).
- **Integration (синтетика)**: образ с PE32+'HII' → формы/строки видны;
  два файла с коллизией string-id → титулы per-file; вложенная резолюция
  таргета; Write-гейт; отказ за барьером; несжатый bare-путь всё ещё
  мутирует.
- **Real-image** (`#[ignore]`, внешний HNX99TF):
  `real_image_hii_forms_and_strings` зеленеет **без правки ассертов**
  (4 формсета ≥ 2, язык `en*`); `real_image_hii_form_visibility_round_trip`
  — с поправкой §6.
- **CLI**: e2e (mock) без изменений, зелёный; `cargo test --all`,
  `clippy -D warnings`, `fmt --check`.

### Скелет задач для плана (ориентир, ~9)

0. Извлечение фикстур из rk3588-образа + проверка object-API (recon-first);
1. `package_list.rs` (TDD);
2. `pe_resource.rs` (TDD);
3. walker: граница длины (TDD);
4. bare-канал (TDD);
5. перестройка обхода + wiring + фикстуры lang@46 (TDD);
6. `find_item`/`find_item_path` DFS-зеркало (TDD);
7. гейты `set_item_visibility` + поправка visibility-теста (docs-шаг);
8. косметика `real_image.rs`;
9. финальная проверка: real-image тесты зелёные, CLI e2e.

## 10. Риски

1. **Ложные срабатывания bare-канала** на данных, похожих на валидный IFR.
   Митигация: глубокая валидация + граница длины + дедуп; наблюдение на
   реальных образах в тестах. Остаточный риск — задокументирован.
2. **object-API ресурсов** — проверен по docs.rs 0.39.1 (§2.6); если
   практическое использование выявит пробелы — fallback: ручной обход
   сырых структур `object::pe::ImageResourceDirectory*` (решение в задаче-0).
3. **Стоимость extraction** при каждом `collect_*` — миллисекунды на
   сотнях КБ, кэш не заводим (YAGNI).
4. **Мутации за барьером честно отказаны** до фазы рекомпрессии — осознанное
   решение (вариант C), сообщение об ошибке объясняет причину.

## 11. Референсы

- Живые байты: probe-разведка 2026-08-14 (TODO, коммит `ffa1a46`);
  скрипт `/tmp/hii_probe.py` (вне репо).
- `docs/plans/2026-08-14-предпосылки-для-рекомпрессии.md` — следующая фаза.
- `refs/UEFITool-ai-fork/common/ffsparser.cpp` — layout guided-секций;
  `refs/IFRExtractor-RS/src/main.rs` — сигнатурный скан (отвергнут как
  основной канал; идеологический предок bare-канала с валидацией).
- `r_efi::hii` — `PACKAGE_FORMS/PACKAGE_STRINGS/PACKAGE_END`, IFR-типы.
- docs.rs `object` 0.39.1 `read::pe` — ресурсное read-API.
