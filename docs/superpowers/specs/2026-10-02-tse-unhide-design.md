# Спека мини-цикла `tse-unhide`: операция снятия hide-маркера AMITSE + $SPF recon + универсальность

Дата: 2026-10-02 · Статус: дизайн одобрен владельцем, ждёт плана
Первоисточник: TODO «мини-цикл "TSE unhide"» (f2f5bc1, запрос владельца
2026-09-22). База знаний: спека positional-insert, аддендум «Фронт 1» и
живой гейт v3 (2026-09-21-positional-insert-design.md:497-605);
`docs/analyzer-patterns.md` P1 ($SPF), P2 (stride-таблицы), P3 (каталог
переменных).

Решения владельца (брейншторм 2026-10-02): (1) состав — все три пункта
TODO: операция + $SPF recon-команда + универсальность на образах;
(2) поверхность — engine + RPC + CLI + TUI, WebUI не трогаем;
(3) приёмка — байт-паритет с артефактом v3.

Правка контракта дискриминации (2026-10-02, после прогона прототипа
сканера на живом PE 450x): премисса «запись hide встречается в единственном
блоке» ложна — `{EC87D643#1}` есть и в hide @0x1b40, и в бар-блоке
@0x1bc0 (зануление в баре даёт обратный эффект — вкладка пропадает).
Отделяющий признак: hide-таблица состоит ТОЛЬКО из корневых форм
формсетов (минимальный form_id по IFR), бар/фаза-1 содержат некорневые
(10001, 2049). Контракт: кандидат = блок содержит запись И все записи
блока корневые; ровно один → патч, иначе refuse + экспертный
`--block-offset`. Подтверждено владельцем.

## 1. Контекст

Живой гейт v3 (2026-09-22, ПРОЙДЕН) доказал механизм: вкладка
IntelRCSetup в корневом баре появляется после зануления 32 байт записи
`{EC87D643, form 1}` hide-таблицы `@0x1b40` PE-тела файла AMITSE
(B1DA0ADF-4F77-4070-A88E-BFFE1C60529A). Артефакт v3 собран temp-тестом
поверх движкового API (parse → guard → зануление → Action::Rebuild по
цепочке предков → LZMA-рестрим guided-секции → пересчёт FFS-чексумм) —
ядро уже всё умеет, пользовательской операции нет. Долг: `tse-unhide`
как команда + $SPF/stride-инвентарь как recon для любых Aptio-образов.

Структуры (P2): в TSE PE несколько stride-блоков записей
`{GUID(16), u64 form-id, u64 0}` шагом 0x20 с полностью нулевым
терминатором; на 450x их четыре: `@0x1a80` фаза-1 «показывать»,
`@0x1b40` **hide-таблица** (единственный потребитель — фаза-2 TSE),
`@0x1ba0` порядок корневого бара, `@0x1c00` динамические формсеты.
Структурно блоки неразличимы — различие известно только из дизасма.

## 2. Ядро: модуль `crates/uefi-engine/src/hii/tse.rs`

### 2.1 Скан stride-блоков

`scan_stride_blocks(pe: &[u8]) -> Vec<StrideBlock>`; `StrideBlock {
pe_offset, entries: Vec<StrideEntry> }`, `StrideEntry { guid, form_id:
u64 }`. Критерии блока (P2): ≥2 записей шагом 0x20, каждая
`{GUID(16), u64 form-id < 0x10000, u64 0}` (хвост записи нулевой),
терминатор — 32 нулевых байта. Дополнительно: **все** GUID-ы записей
блока ∈ формсеты образа (union: IFR `collect_forms` + $SPF
formset-blob) — отсекает случайные stride-серии в чужом коде/данных.
Оффсетная арифметика — checked, выход за `pe.len()` → конец блока.

### 2.2 Операция `tse_unhide`

`tse_unhide(image: &mut Image, formset_guid, form_id: u16) ->
Result<UnhideOutcome, HiiError>`; `UnhideOutcome { pe_offset,
entry_pe_offset, formset_guid, form_id }`:

1. Файл `B1DA0ADF-…` → первый PE32-лист. Нет файла/PE → `NotFound`
   («no AMITSE setup browser module»).
2. `image.mode != Write` → `NotWritable` (write-path, чек-лист AGENTS.md).
3. Скан блоков (§2.1); **root-фильтр**: корневая форма формсета =
   минимальный form_id формсета по IFR (`collect_forms`, группировка по
   formset_guid). Кандидат = блок, который (а) содержит запись
   `guid == formset_guid && form_id as u64 == entry.form_id` И
   (б) ВСЕ записи блока — корневые формы своих формсетов. На 450x: hide
   @0x1b40 проходит ({EC87D643#1, 7B59104A#10000} — оба корни), бар
   @0x1bc0 — нет (10001/2049 некорневые), фаза-1 @0x1a80 — нет.
4. Кандидатов 0 → `NotFound` («entry not found in any hide-candidate
   block», с инвентарём — видимые вкладки отсекаются здесь); >1 →
   `InvalidSchema` «ambiguous: entry in N hide-candidate blocks @…» —
   fail-closed, эксперт разбирается recon-ом.
5. Byte-guard: по адресу записи лежат в точности
   `{guid_le(16), form_id_le(8), нули(8)}` — иначе `InvalidSchema`
   (guard-mismatch).
6. Зануление 32 байт записи; `ops::mark_rebuild_to_root_by_path` по
   пути узла; обёртка `with_rollback` (snapshot-rollback, чек-лист §7).

Экспертный режим: `block_pe_offset: Option<usize>` в API — явный оффсет
записи в PE (из recon-инвентаря) минует подбор кандидатов (шаги 3-4),
byte-guard (шаг 5) остаётся. Для неоднозначных чужих образов.

Сравнение form_id: аргумент u16, поле записи u64 — match по точному
равенству `entry.form_id == u64::from(form_id)`; записи с form_id ≥
0x10000 отсеяны ещё сканом.

### 2.3 Recon `tse_report`

`tse_report(image: &Image) -> Result<TseReport, HiiError>`:

- `blocks: Vec<StrideBlock>` — инвентарь (offset + записи `{guid,
  form_id}`);
- `spf: Option<SpfSummary>` — если у B1DA0ADF есть FREEFORM-секция
  subtype FE612B72-… с magic `$SPF`: число страниц, формсеты (GUID,
  u32-поле записи, число страниц по fsIdx из таблицы страниц), каталог
  переменных P3 (`{guid, имя UCS-2, attrs, size}`), число
  string-controls. Образец структуры — P1/P3 analyzer-patterns.

Слоты хедера $SPF (пинн по живому дампу 450x, оффсеты от magic):
+0x28 → 0x48, **+0x2c → 0x60 (таблица страниц: u32 count, u32 offs[])**,
+0x30 → 0x83c4 (контролы), **+0x34 → 0x7659c (каталог переменных)**
, +0x38/+0x3c → 0x79320/0x79350, **+0x40 → 0x79bc8 (блоб формсетов)**,
+0x44 → 0x79c74. Записи формсетов: GUID(16) + u32 @+0x10, stride 0x14,
оффсеты относительны началу блоба. Записи каталога переменных: длина
0x7C, имя UCS-2 @+0x10..+0x60 (40 симв.), attrs u32 @+0x60, u32 @+0x64,
size u32 @+0x78; оффсеты относительны начала блоба. Поля u32 записи
формсета (значения 450x: 0,1,2,1,121,64,20) семантика неясна —
репортить как raw.

Недостающие читатели (formset-blob @header, var-catalog @header) —
добавить в `hii/spf.rs` рядом с существующими `scan_*` (по одному
read-хелперу, bounds — checked, мусорные хвосты → пустой список +
warn-строка в отчёте, не паника).

## 3. Поверхности

### 3.1 Proto/RPC

`uefi-proto/proto/engine.proto`: `TseReport(TseReportRequest) →
TseReportResponse` и `TseUnhide(TseUnhideRequest) →
TseUnhideResponse`; сообщения — зеркала структур §2. Ошибки —
существующая таксономия HiiError → status (NotFound /
InvalidSchema / NotWritable).

### 3.2 CLI

- `uefi-cli tse report <image-id>` — text/TSV/JSON по конвенциям
  крейта (блоки + $SPF-сводка);
- `uefi-cli tse unhide <image-id> <formset-guid> <form-id>
  [--block-offset <hex>]` — мутация, печатает pe_offset/entry offset
  результата; `--block-offset` — экспертный обход (§2.2).

### 3.3 TUI

- `:tse` — инвентарь блоков (+ $SPF-сводка) в существующем
  list-паттерне;
- `:tse-unhide <formset-guid> <form-id>` — мутация по образцу
  `:hii set-value` (confirm через существующий пре-филл, статус
  результата).

### 3.4 WebUI

Не трогаем. Доступ после цикла — generic bridge
`POST /api/v1/rpc/TseUnhide` (gateway-кода ноль, descriptor pool).
**WebUI-кнопка TseUnhide отложена** — по прецеденту
`HiiFormAdd`/`HiiFormsetAdd` (TODO:543): там интерактивные обвязки были
отложены из основного цикла (план фазы C) и исполнены позже отдельным
циклом webui-parity; здесь тот же порядок.

## 4. Моки

`mock_server.rs` (uefi-tui, uefi-cli): стабы `tse_report`/`tse_unhide`
(фикстура: один блок с записью; вариант «нет AMITSE» → NotFound).

## 5. Тесты

- **Синтетика (TDD)**: скан находит блоки/терминаторы/GUID-фильтр
  (чужой GUID → блок отброшен); root-фильтр (блок с некорневой
  записью → не кандидат); refuse неоднозначности (два root-only блока с
  записью); NotFound (записи нет / запись только в некорневых блоках —
  «видимая вкладка»); guard-mismatch; mode-guard (Read → NotWritable);
  `--block-offset` обход + его guard; rebuild-метки по цепочке предков
  (guided-секция → билд рестримит).
- **Байт-паритет v3** (#[ignore], presence-gated): `refs/fw/450x.bin`
  → `tse_unhide(EC87D643, 1)` → `build_image` → sha256 ==
  `9d5f6b553de1b7586f119bd7bebc83d5c95cfa44a5b003bc3da59ca7bc7b5b0e`
  (артефакт `refs/amibcp/450x-intelrcsetup-tse-unhide-v3.bin`).
- **HNX99TF** (#[ignore]): `tse_report` работает (файл B1DA0ADF в
  образе есть — используется тестами form_hijack).
- **Универсальность** (#[ignore], presence-gated): `C275D4I3.20`
  (asrock), `mz32-ar0-RBU.rom`, `226D2IL3.30`, `X10DRH1_816` —
  `tse_report` не падает; фактический инвентарь (есть ли блоки, GUID
  AMITSE, $SPF) — в отчёт цикла, не в ассерты. Образы все в
  refs/amibcp + refs/fw (поправка: asrock/mz32 доступны, вопреки
  ранней заметке).

## 6. Гейты

`cargo test --all`; `cargo clippy --all --all-targets -- -D warnings`;
`cargo fmt --all -- --check`. Живой гейт не требуется (пройден v3,
байт-паритет его пиннит); владелец вправе прогнать TMM-прошивку
повторно по желанию.

## 7. За рамками

- Обратная операция hide (восстановление/добавление записи).
- WebUI-кнопка — отложена (§3.4, прецедент TODO:543).
- Правка динамического списка `@0x1c00` и бар-порядка `@0x1ba0`.
- Кросс-валидация «запись в бар-блоке ⇒ уже видима» (требует
  per-image реверса различий блоков — не делаем).

## 8. Вердикт

Ждёт реализации.
