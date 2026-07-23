# UEFIPatcher — Дизайн цикла 6 (Расширенный Setup)

## Краткое описание

Расширение функционала Setup-меню UEFI BIOS: генерация новых IFR-форм/пунктов с привязкой к NVRAM-переменным, установка значений по умолчанию (Optimized/Failsafe), авто-добавление строк в HII String-пакет, обязательный AMI-патчинг (setupdataBin + amitseSct). Вход — JSON-схема; результат — отдельный FFS-модуль с новым FormSet, вставленный в DXE-том образа.

## Цели цикла 6

- Реализовать генерацию IFR-байтов (FormSet/Form/VarStore/OneOf/CheckBox/Numeric/Ref/Text/Default/DefaultStore/End) по edk2-структурам
- Реализовать авто-добавление строк (prompt/help/options) в существующий HII String-пакет образа с возвратом маппинга текст→StringId
- Реализовать сборку отдельного FFS-модуля с новым FormSet + String-пакет (аналог IntelRCSetup — не правит родной Setup FFS)
- Реализовать обязательный AMI-патчинг: setupdataBin (accessLevel/failsafe/optimal для новых QuestionId) + amitseSct (регистрация FormId в порядке форм)
- Реализовать новый gRPC-метод `AddSetupFormSet(image_id, schema_json, target_ffs_guid)` в `EngineService`
- Поддержать значения по умолчанию через `EFI_IFR_DEFAULT` (DefaultId: 0=Optimized/Standard, 1=Failsafe/Manufacturing)

## Архитектура

### Зависимости (крейт `uefi-engine`)

- `serde` / `serde_json` — JSON-схема FormSet
- `uguid = "2.2.1"` (features: `serde`) — `uguid::Guid` (замена ручного `Guid` с `data1/data2/data3/data4`); API: `Guid::try_parse(...)`, `g.to_bytes()`, `Guid::from_bytes(arr)`
- `r-efi = "7.0"` — модуль `r_efi::hii::*` со всеми IFR-структурами (`IfrFormSet`, `IfrForm`, `IfrCheckbox`, `IfrNumeric`, `IfrOneOf`, `IfrOneOfOption`, `IfrDefault`, `IfrVarstore`, `IfrVarstoreEfi`, `IfrText`, `IfrSubtitle`, `IfrEnd`, `IfrOpHeader`, `IfrQuestionHeader`, ...) и константами опкодов (`IFR_FORM_SET_OP`, `IFR_NUMERIC_OP`, `IFR_CHECKBOX_OP`, и т.д.)
- `binrw = "0.15"` — декларативный бинарный парсинг (заголовки FFS, секции, String-пакет)
- `tonic` (gRPC), существующие модули цикла 1 (`parser`, `builder`, `ops`, `types`, `ffs`)

### Расширение `uefi-engine` (цикл 1) + `uefi-proto`

```
crates/uefi-engine/src/setup_advanced/
├── mod.rs              # координатор add_setup_formset
├── schema.rs          # JSON-схема (serde): FormSetSchema, FormSchema, ItemSchema, DefaultSchema
├── ifr_builder.rs      # генерация IFR-байтов
├── string_pack.rs      # поиск HII String-пакета, добавление StringId
├── ami_patcher.rs      # патч setupdataBin + amitseSct
└── ffs_assembler.rs    # сборка FFS с FormSet
```

### Новый gRPC-метод

`AddSetupFormSet(AddSetupFormSetRequest) -> AddSetupFormSetResponse`:
- Request: `image_id`, `schema_json` (строка JSON), `target_ffs_guid` (GUID для поиска String-пакета, или пусто — авто-поиск)
- Response: `new_ffs_id` (GUID нового FFS), `inserted_form_ids[]`, `string_ids` (map ключ→StringId)

### Поток add_setup_formset

1. Парсинг JSON-схемы → `FormSetSchema` (валидация через serde).
2. Поиск в образе HII String-пакета по `target_ffs_guid` или авто-поиск.
3. Добавление строк (prompt/help/options/varstore-name/formset-title) в String-пакет → маппинг `StringId`.
4. Генерация IFR-байтов (`ifr_builder`): FormSet → VarStore → DefaultStore → Form → Items (с вложенными Default/OneOfOption) → End.
5. Сборка FFS-файла (`ffs_assembler`): заголовок FFSv2 + body = IFR + обновлённый String-пакет.
6. AMI-патчинг: найти `setupdataBin` и `amitseSct` в образе. Для каждого нового QuestionId — генерация 108-байтной записи в setupdataBin. Для каждой новой формы — регистрация FormId в amitseSct. Если файлы не найдены — ошибка `AmiFilesNotFound` (обязательный патчинг).
7. Вставка нового FFS в DXE-том образа через `ops::insert` (цикл 1).

## Фронтенды

- [x] gRPC API: `AddSetupFormSet` (см. раздел «API»)
- [ ] CLI: команда `setup add-formset <SCHEMA_JSON>` — цикл 2 (после реализации цикла 6)
- [ ] TUI — цикл 3 (roadmap)

## API/Контракты

### AddSetupFormSet
- Метод: gRPC `EngineService/AddSetupFormSet`
- Параметры:
  - `image_id`: string, обязательный — ID открытого образа
  - `schema_json`: string, обязательный — JSON со схемой FormSet (включая опциональные `setupdata_guid`/`amitse_guid` для прямого поиска AMI-модулей)
  - `target_ffs_guid`: string, необязательный — GUID FFS для поиска String-пакета (пусто = авто-поиск)
- Ответ:
  - Успех: `{new_ffs_id: string, inserted_form_ids: [u16], string_ids: map<string, u16>}`
  - Ошибки: `INVALID_ARGUMENT` (невалидный JSON/схема), `NOT_FOUND` (String-пакет/setupdataBin/amitseSct), `INTERNAL` (ошибка сборки)
- Пример: `uefi-cli setup add-formset myformset.json` (через CLI цикла 2)

## JSON-схема

### Структура

| Тип | Поля | Описание |
|---|---|---|
| `FormSetSchema` | `formset_guid: string`, `title: string`, `help: string`, `class_guids: [string]`, `varstores: [VarStoreSchema]`, `default_stores: [DefaultStoreSchema]`, `forms: [FormSchema]`, `setupdata_guid: string?`, `amitse_guid: string?` | Корень; опциональные `setupdata_guid`/`amitse_guid` — GUID'ы AMI-модулей для прямого поиска (иначе авто-поиск по имени/эвристике) |
| `VarStoreSchema` | `id: u16`, `guid: string`, `size: u16`, `name: string`, `type: "buffer"\|"efi"` | NVRAM-хранилище |
| `DefaultStoreSchema` | `name: string`, `id: u16` (0=Optimized, 1=Failsafe) | Профиль дефолтов |
| `FormSchema` | `id: u16`, `title: string`, `items: [ItemSchema]` | Форма (меню) |
| `ItemSchema` | enum: `OneOf`, `CheckBox`, `Numeric`, `Text`, `Ref`, `String`, `Action`, `OrderedList` | Пункт меню |
| `OneOfItem` | `prompt`, `help`, `question_id: u16`, `var_store_id: u16`, `var_offset: u16`, `size: 1\|2\|4\|8`, `display: "int_dec"\|"uint_dec"\|"uint_hex"`, `options: [OptionSchema]`, `defaults: Defaults` | Выпадающий список |
| `OptionSchema` | `text: string`, `value: u64`, `default: "optimized"\|"failsafe"\|null` | Опция OneOf |
| `CheckBoxItem` | `prompt`, `help`, `question_id`, `var_store_id`, `var_offset`, `defaults: Defaults` | Чекбокс |
| `NumericItem` | `prompt`, `help`, `question_id`, `var_store_id`, `var_offset`, `size`, `min`, `max`, `step`, `display`, `defaults: Defaults` | Числовое поле |
| `RefItem` | `prompt`, `help`, `question_id`, `form_id: u16` (целевая форма) | Ссылка на форму |
| `TextItem` | `prompt`, `help`, `text_two: string` | Статичный текст |
| `Defaults` | `optimized: u64?`, `failsafe: u64?` | Дефолты (маппинг на EFI_IFR_DEFAULT DefaultId=0/1) |

### Пример

```json
{
  "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
  "title": "My Custom Setup",
  "help": "Custom UEFI settings",
  "class_guids": ["A3C4B5D6-..."],
  "setupdata_guid": "12345678-90AB-CDEF-1234-567890ABCDEF",
  "amitse_guid": "87654321-FEDC-BA09-8765-432109FEDCBA",
  "varstores": [
    { "id": 1, "guid": "11111111-2222-3333-4444-555555555555", "size": 256, "name": "MySetupVar", "type": "buffer" }
  ],
  "default_stores": [
    { "name": "Optimized", "id": 0 },
    { "name": "Failsafe", "id": 1 }
  ],
  "forms": [
    {
      "id": 1, "title": "Main",
      "items": [
        {
          "type": "one_of", "prompt": "PCI-E Bifurcation", "help": "Select bifurcation mode",
          "question_id": 256, "var_store_id": 1, "var_offset": 0, "size": 1,
          "display": "uint_dec",
          "options": [
            { "text": "x4", "value": 0, "default": "optimized" },
            { "text": "x2x2", "value": 1, "default": "failsafe" },
            { "text": "x1x1x1x1", "value": 2 }
          ]
        },
        {
          "type": "check_box", "prompt": "Serial Console", "help": "Enable serial port output",
          "question_id": 257, "var_store_id": 1, "var_offset": 1,
          "defaults": { "optimized": 1, "failsafe": 0 }
        }
      ]
    },
    {
      "id": 2, "title": "Advanced",
      "items": [
        { "type": "ref", "prompt": "Back to Main", "help": "", "question_id": 258, "form_id": 1 }
      ]
    }
  ]
}
```

## IFR-билдер

`IfrBuilder { buf: Vec<u8>, string_ids: HashMap<String, u16> }`:
- `emit_form_set(guid, title_id, help_id, class_guids)` — 0x0E + scope
- `emit_var_store(id, guid, size, name_id)` — 0x24
- `emit_default_store(name_id, default_id)` — 0x5C
- `emit_form(id, title_id)` — 0x01 + scope
- `emit_one_of(prompt_id, help_id, qid, vsid, voff, flags, min, max, step)` — 0x05 + scope
- `emit_one_of_option(text_id, value, type, flags)` — 0x09
- `emit_check_box(prompt_id, help_id, qid, vsid, voff, flags)` — 0x06 + scope
- `emit_numeric(prompt_id, help_id, qid, vsid, voff, flags, min, max, step)` — 0x07 + scope
- `emit_ref(prompt_id, help_id, qid, form_id)` — 0x0F
- `emit_text(prompt_id, help_id, text_two_id)` — 0x03
- `emit_string(prompt_id, help_id, qid, vsid, voff, min_size, max_size, flags)` — 0x1C + scope
- `emit_action(prompt_id, help_id, qid, config_id)` — 0x0C + scope
- `emit_ordered_list(prompt_id, help_id, qid, vsid, voff, max_containers, flags)` — 0x23 + scope
- `emit_default(default_id, type, value)` — 0x5B
- `emit_end()` — 0x29
- `build() -> Vec<u8>`

Каждый `emit_*` пишет `OpCode(u8) + Length:7bit|Scope:1bit + data` по структурам edk2 `UefiInternalFormRepresentation.h`. IFR-структуры **брать из `r_efi::hii::*`** (`IfrFormSet`, `IfrForm`, `IfrCheckbox`, `IfrNumeric`, `IfrOneOf`, `IfrOneOfOption`, `IfrDefault`, `IfrVarstore`, `IfrVarstoreEfi`, `IfrText`, `IfrSubtitle`, `IfrEnd`, `IfrOpHeader`, `IfrQuestionHeader`, ...), не определять свои; константы опкодов — `IFR_FORM_SET_OP`, `IFR_NUMERIC_OP`, `IFR_CHECKBOX_OP` и т.п. Референс размеров/scope: `BaseTools/Source/C/VfrCompile/VfrFormPkg.cpp:2255-2357`.

## String-пакет

`string_pack.rs`:
- Поиск HII String Package в образе: итерация по FFS → секциям → поиск `HII_STRING_PACKAGE`. Референс: `../refs/IFRExtractor-RS/src/uefi_parser.rs:167` (`hii_string_package`), `:312` (`sibt_string_scsu`), `:355` (`sibt_string_ucs2`).
- Парсинг существующего пакета: `string_id_map: HashMap<u16, String>`, `max_string_id`.
- `add_strings(image: &mut Image, ffs_guid: Option<uguid::Guid>, strings: &[String]) -> Result<HashMap<String, u16>>`:
  - Alloc новых StringId (от `max+1`).
  - Запись SIBT-блоков для каждой строки (SCSU для ASCII, UCS2 для unicode).
  - Пересборка String-пакета: обновление `Length` заголовка, пересчёт checksum.
  - Возврат маппинга текст→StringId.

## AMI-патчинг

`ami_patcher.rs`:
- Поиск `setupdataBin` и `amitseSct` в образе. Приоритет методов поиска (применяются по очереди до успеха):
  1. **По GUID FFS**: если в JSON-схеме указаны `setupdata_guid`/`amitse_guid` — прямой поиск FFS по GUID через `find_item` (цикл 1). Самый надёжный способ.
  2. **По имени FFS**: поиск FFS с UI-секцией имени "setupdata"/"AMITSE" (AMI-конвенция имён).
  3. **Эвристика по содержимому**: сканирование тела FFS на наличие QuestionId-маркеров (паттерн AMI-записи с recognizable offset-структурой) — запасной метод для неизвестных имён/GUID.
- Опциональные поля JSON-схемы: `setupdata_guid: Option<string>`, `amitse_guid: Option<string>` — GUID'ы соответствующих FFS-модулей в образе для прямого поиска. Если не указаны — используются авто-методы 2/3.
- Для каждого нового QuestionId:
  - Генерация 108-байтной AMI-записи в `setupdataBin`:
    - `[+0] QuestionId (u16 LE)` — маркер
    - `[+24] pageId (u16)` — только для Ref (целевая форма)
    - `[+32] accessLevel (u8)` — 0x05 (видимый)
    - `[+104] failsafe (u8)` — значение по умолчанию Failsafe
    - `[+106] optimal (u8)` — значение по умолчанию Optimized
    - Остальные байты — 0x00 (AMI-специфичные, не критичные)
  - Дополнение записи в конец `setupdataBin`.
- Регистрация форм в `amitseSct`: для каждой новой формы — запись `FormId (u16 LE)` после FormSetId-маркера (хвост GUID FormSet `formSet[4]+formSet[5]`). Референс: `../refs/UEFI-Editor/src/components/scripts/scripts.ts:242-272`.
- `patch_ami(image: &mut Image, formset_guid: uguid::Guid, form_ids: &[u16], questions: &[QuestionAmiRecord], setupdata_guid: Option<uguid::Guid>, amitse_guid: Option<uguid::Guid>) -> Result<()>` — поиск setupdataBin/amitseSct по приоритету: GUID (если передан) → имя FFS → эвристика.
- `QuestionAmiRecord { question_id: u16, page_id: Option<u16>, access_level: u8, failsafe: u8, optimal: u8 }`
- Если `setupdataBin`/`amitseSct` не найдены — `Err(SetupAdvancedError::AmiFilesNotFound)` (обязательный патчинг).

## Ошибки

`SetupAdvancedError`:
- `InvalidSchema(String)` — JSON невалиден/нарушает схему
- `StringPackageNotFound` — HII String-пакет не найден
- `AmiFilesNotFound` — setupdataBin/amitseSct не найдены
- `IfrBuildError(String)` — ошибка генерации IFR
- `FfsAssemblyError(String)` — ошибка сборки FFS

## Тестирование

### Unit-тесты
- `ifr_builder`: генерация каждого опкода → сравнение с эталонными байтами (edk2 размеры/scope); round-trip emit→parse.
- `string_pack`: парсинг синтетического String-пакета, добавление строк, пересчёт checksum.
- `ami_patcher`: генерация 108-байтной записи, проверка offset'ов (accessLevel=+32, failsafe=+104, optimal=+106), вставка FormId в amitseSct; поиск setupdataBin/amitseSct по GUID (прямое совпадение), по имени FFS, эвристика по содержимому (fallback).
- `schema`: валидация JSON (типы, обязательные поля, диапазоны, enum-values).

### Интеграционные тесты
- `AddSetupFormSet` на синтетическом образе (FFS со String-пакетом + setupdataBin + amitseSct) — проверка вставки FFS, строк, AMI-записей.

### E2E-тесты
- Реальный BIOS-образ (из `../refs`) — `AddSetupFormSet` → `SaveImage` → UEFITool: новый FormSet виден, формы в дереве.

## Тонкости имплементации

- **IFR-структуры** использовать из `r_efi::hii::*` (IfrFormSet, IfrForm, IfrCheckbox, IfrNumeric, IfrOneOf, IfrDefault, IfrVarstoreEfi, IfrText, IfrEnd, etc.), не определять свои.
- **GUID** — `uguid::Guid` с `.to_bytes()` / `Guid::from_bytes()` вместо ручного `guid_bytes()`.
- **Module-first rule:** `pub mod X;` объявляется ДО `cargo test`.

## Этапы реализации

1. **schema.rs**: JSON-схема (serde), валидация. Unit-тесты.
2. **ifr_builder.rs**: генерация всех опкодов. Unit-тесты с эталонами edk2.
3. **string_pack.rs**: поиск/парсинг/добавление строк, пересборка. Unit-тесты.
4. **ami_patcher.rs**: поиск setupdataBin/amitseSct, генерация записей, патч. Unit-тесты.
5. **ffs_assembler.rs**: сборка FFS из IFR+String-пакета. Unit-тесты.
6. **mod.rs**: координатор `add_setup_formset`. Integration-тест.
7. **rpc**: метод `AddSetupFormSet` в `EngineService`. Integration-тест через mock.
8. **CLI** (цикл 2): команда `setup add-formset <SCHEMA_JSON>`. E2E-тест.
9. **Документация**: обновление PLANNING.md/IMPLEMENTATION.md.

## Риски и ограничения

- **AMI-формат не задокументирован** — восстановлен из regex UEFI-Editor. Мера: тестировать на реальных BIOS; сверяться с `../refs/edk2` для стандартных полей; если AMI-запись не подходит — не падать, а WARN (но патч обязателен по решению).
- **String-пакет может быть сжат** (LZMA/Tiano) — добавление строк требует декомпрессии/компрессии. Мера: переиспользовать `decompress` из цикла 1; сжатие — через lzma-rs (compress есть? если нет — оставлять несжатым, WARN).
- **Несколько String-пакетов в образе** —哪个 именно править. Мера: `target_ffs_guid` или авто-поиск по FormSet GUID (язык STRING, UEFI-стандарт: первый подходящий).
- **QuestionId/VarOffset коллизии** — новые ID могут совпасть с существующими. Мера: валидация в schema.rs (пользователь отвечает за уникальность); опционально авто-alloc высоких ID (0x8000+).
- **Обязательный AMI-патчинг при отсутствии AMI-файлов** — на не-AMI BIOS упадёт. Мера: документировать, что цикл 6 ориентирован на AMI Aptio; для не-AMI — отдельный цикл (стандартный IFR без AMI).
- **Отдельный FFS может не подхватиться** — без AMITSE-записи форма не появится. Мера: обязательная регистрация в amitseSct (уже в дизайне).

## Решения (фиксация)

- Scope: полный (IFR-генерация + AMI setupdataBin/amitseSct патчинг).
- Формат описания: JSON-схема.
- Встраивание: отдельный FFS-модуль с новым FormSet (аналог IntelRCSetup, не правит родной Setup FFS).
- String-пакет: авто-добавление в существующий HII String-пакет.
- AMI-патчинг: обязательный (setupdataBin accessLevel/failsafe/optimal + amitseSct FormId).
- Дефолты: EFI_IFR_DEFAULT (DefaultId 0=Optimized, 1=Failsafe) + AMI setupdataOptimized/Failsafe.
- DefaultId-константы (edk2): STANDARD=0x0000 (Optimized), MANUFACTURING=0x0001 (Failsafe).
- Архитектура: модуль `setup_advanced` в `uefi-engine`, новый gRPC-метод `AddSetupFormSet`.
- Референсы: edk2 `UefiInternalFormRepresentation.h` + `VfrFormPkg.{h,cpp}` для IFR-структур/генерации; IFRExtractor-RS для парсинга String-пакета; UEFI-Editor для AMI-формата setupdataBin/amitseSct.