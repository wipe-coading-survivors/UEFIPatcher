# Formset-Unlock / перенос IIO-бифуркации — дизайн (U1–U4)

Дата: 2026-09-12. Статус: согласовано (план `2026-09-12-formset-unlock.md`).
Первоисточник: live-сессия 450x 2026-09-12 (`TODO.md`, раздел «Находки
live-сессии 450x»), P0-фиксы вводной части закрыты дугой engine-p0-hotfix
(PR #15).

## 1. Контекст и цель

Владелец на `refs/amibcp/450x — копия.bin` попытался открыть секцию PCI-e
бифуркации: весь формсет IntelRCSetup (`EC87D643-EBA4-…`, FFS
`ABBCE13D-…`:0x10:0, 161 форма) скрыт. `hii unlock …#1` ответил «no
flippable gates (0 gates)» — честно: AMI прячет формсет-уровневое меню не
внутри секции цели, а suppress-гейтом вокруг GOTO в СЕКЦИИ корневого Setup
(`899407D7-…`:0x10:0, формсет `7B59104A-…`), которую unlock не сканирует.

Цель дуги: дать формсет-уровневый unlock и перенос отдельных вопросов
(бифуркация IIO) в видимый Setup. Ступени:

- **U1** — REF-грамматика: length-дискриминация REF-вариантов,
  кросс-формсетные гейты (gates) и рёбра (ref_tree).
- **U2** — formset-unlock: (a) флип suppress-гейта вокруг REF3/REF4 в
  секции-доноре; (b) fallback — emit собственного REF3 из видимой формы.
- **U3** — перенос вопросов: декларация varstore (IfrVarStoreEfi) в чужом
  Setup-пакете при question add.
- **U4** — карта NVRAM-эффекта + live-гейт на 450x.

## 2. REF-грамматика: факты (сверено с первоисточниками 2026-09-12)

UEFI 2.10 §33.3.8.3.59 ([spec](https://uefi.org/specs/UEFI/2.10/33_Human_Interface_Infrastructure.html))
и EDK2 `MdePkg/Include/Uefi/UefiInternalFormRepresentation.h:955-988`:

- **Отдельных опкодов у REF2–REF5 нет.** Все варианты — opcode `0x0F`,
  различаются length-байтом («several forms of this opcode which are
  distinguished by the length of the opcode»). Константы `IFR_REF2_OP`…
  не существуют ни в UEFI, ни в EDK2; r-efi 7.0 экспортирует структуры
  `IfrRef2..IfrRef5` (`src/hii.rs:954-987`) при единственной
  `IFR_REF_OP = 0x0F`.
- Question-header (11 байт, `+2..+13` от начала стейтмента):
  `Prompt@+2, Help@+4, QuestionId@+6, VarStoreId@+8, VarStoreInfo@+10,
  Flags@+12` — соответствует `EFI_IFR_QUESTION_HEADER = StatementHeader
  (Prompt, Help) + QuestionId + VarStoreId + VarStoreInfo + Flags`.
  Текущий код движка (values.rs/gates.rs/questions.rs/ifr_builder.rs)
  уже читает/пишет ровно этот порядок — расхождений нет.

| Вариант | length | Формат после question-header (с +13) | Статическая цель |
|---------|--------|--------------------------------------|------------------|
| REF     | 15     | FormId@13                            | форма своего формсета |
| REF2    | 17     | FormId@13, QuestionId@15             | форма+вопрос своего формсета |
| REF3    | 33     | FormId@13, QuestionId@15, **FormSetGuid@17** | форма чужого формсета |
| REF4    | 35     | + DevicePath@33                      | форма чужого формсета на устройстве |
| REF5    | 13     | — (Header+Question)                  | **динамическая**: цель приходит из runtime-value вопроса (`EFI_IFR_TYPE_REF`) |

Следствия для кода:

- Текущее чтение `op == 0x0F && length >= 15 → FormId@13` корректно
  покрывает REF/REF2/REF3/REF4 на уровне form_id — «0 gates» на 450x был
  пробелом **скоупа сканирования**, а не парсинга.
- REF3/REF4 необходимо отличать от intra-formset REF по FormSetGuid@17:
  иначе (а) гейт чужого формсета ложно матчится как «свой», (б) ref_tree
  рисует ребро не в тот формсет.
- REF5 исключаем из статического анализа (цель неразрешима без NVRAM);
  parse помечает его как `Dynamic` — ни гейтов, ни рёбер.
- Emit кросс-формсетного GOTO = REF3 (QuestionId = 0xFFFF =
  `EFI_QUESTION_ID_INVALID`, паттерн EDK2 `CIfrRef3`).

## 3. Модель и решения

### U1a. Парсер вариантов (`hii/ref_variant.rs`, новый)

`parse_ref(op, stmt) -> Option<RefTarget>` где
`RefTarget = Form{form_id} | FormQuestion{form_id, question_id} |
Formset{formset_guid, form_id, question_id} | Dynamic`. Точные длины
{13, 15, 17, 33, 35}; прочие — `None` (сегодняшний код молча принимал
любые `length >= 15`).

### U1b. Кросс-гейты (`hii/gates.rs`)

`GateTarget` расширяется полем `formset_guid: Option<Guid>` (None —
существующее поведение). `Wraps` получает вариант
`CrossFormsetRef { form_id, host_form_id, formset_guid }`: emit при
`parse_ref → Formset{guid == target.formset_guid, form_id == target.form_id}`.
Гейт-выражение/флипы — без изменений (та же plan/apply-механика).

### U2a. Формсет-unlock флипом (`hii/cross_formset.rs`, новый + `hii/mod.rs`)

1. Из секции самой цели читаем GUID её формсета (`parse_form_package`).
2. Обход образа (паттерн `ref_tree::collect_edges`): все form-пакеты,
   кроме собственной секции; `find_gates` с расширенным GateTarget.
3. Найденные сайты: `{path: Vec<usize>, source_ffs: String (upper GUID),
   pkg_start, gates}`. Мутация донора — через `Target::Path(path)`
   (уходим от семантики индексов GuidSection-таргетов), флипы —
   `pkg_start + flip.offset` по телу донорской секции, затем
   `mark_rebuild_to_root_by_path` по пути донора.
4. `resolve_writable_path` для каждого донора (режим write + барьер
   сжатия — как у hijack/unlock).
5. `GateInfo.source_target` (новое поле proto, строка) — upper GUID FFS
   донора; пусто для гейтов собственной секции. CLI/TSV/TUI печатают
   суффиксом.
6. No-op семантика P0-фикса сохранена: ноль флипов ⇒ ноль записей.

`gates list` получает тот же кросс-скоуп (read-only).

### U2b. Fallback — inject REF3 (`schema.rs`, `ifr_builder.rs`, `mod.rs`)

`QuestionAddRefSchema` + необязательное `formset_guid: String` →
`emit_ref3(prompt, help, qid, form_id, &Guid)`: total 33, FormId@13,
QuestionId 0xFFFF@15, FormSetGuid@17. Валидация GUID — `try_parse`,
ошибка → `InvalidSchema`. Применяется, если в корневом Setup нет
флипабельного скрытого GOTO (живой гейт Task 8 определяет, что реально
на 450x).

### U1c. Кросс-рёбра (`hii/ref_tree.rs`, proto `FormEdge`)

`package_edges` возвращает тройку `(parent, child, Option<formset_guid>)`;
`FormEdge.target_formset_guid` (строка, пусто = intra). TUI Forms View:
ребро с непустым target резолвится в `FormKey { formset_guid: target,
form_id_ifr: child }`; ненайденное — DanglingRef-строка (паттерн
существующих висячих REF).

### U3. Varstore-декларации при question add (`schema.rs`, `ifr_builder.rs`, `ifr.rs`, `mod.rs`)

- `QuestionAddList` + `varstores: Vec<VarStoreSchema>` (default `[]`);
  `VarStoreSchema` + `attributes: u32` (default `7` =
  NV|BS|RT; сверяется по живым байтам на гейте Task 8).
- `emit_var_store_efi(id, guid, size, name, attributes)` по EDK2-раскладке
  `EFI_IFR_VARSTORE_EFI`: `VarStoreId@2, Guid@4, Attributes@20, Size@24,
  Name@26` (UCS-2 + NUL). Читающая сторона (`values::varstore_map`) уже
  парсит оба типа varstore — двигаем только emission.
- Вставка — в пролог формсета (перед первым `IFR_FORM_OP`): новая
  `locate_formset_prelude_end` + `splice_varstore_ops` (bare) +
  resource-вариант (зеркало `splice_question_ops_into_resource`, рост
  .rsrc через существующий reloc-aware механизм).
- **$SPF-сдвиги**: вставка в пролог сдвигает IFR-offset'ы всех вопросов
  пакета → существующие `$SPF`-записи, резолвящиеся в этот пакет,
  сдвигаются на delta (переиспользуем
  `spf::fixup_selected_record_ifr_offsets`; selected = записи, для
  которых `spf_record_resolves(pkg_before, …)`). Затем обычный
  add_question работает в уже-сдвинутых координатах.
- Коллизии: id из схемы не должен совпадать с существующими
  (`varstore_map`); var_store_id вопросов должен быть объявлен (в схеме
  или существующим varstore) — сейчас проверка отсутствует.

### U4. Карта NVRAM-эффекта + live-гейт

- Real-тест `real_amibcp_450x_formset_unlock` (#[ignore]): кросс-гейты
  для `ABBCE13D…:0x10:0#1`; при наличии — unlock + билд (round-trip
  байт-точность на базовой механике engine-p0-hotfix) + re-parse; при
  отсутствии — путь U2b (inject REF3 в форму Chipset 10008 корневого
  Setup) + ребро в `collect_edges`. Тест детерминирован для конкретного
  образа, ветка фиксируется фактическими данными (гипотеза скрытого
  REF3 байтами пока не подтверждена — только косвенно строкой 64).
- Карта (живые данные 2026-09-12, вопрос → varstore/offset) — §6;
  дополняется прогоном question-info на гейте.
- Семантика для документации: `IfrVarStoreEfi` — UEFI-переменная
  (IntelSetup/EC87D643…); в образе правки меняют IFR (декларации,
  default-опции one_of, сами вопросы), рантайм-NVRAM инициализируется
  из этих default'ов при загрузке/сбросе.

## 4. Границы (out of scope)

- REF5-эмит и динамические GOTO — никогда (статическая цель
  неразрешима).
- Перенос formset-декларации целиком (varstore/default-store копирование
  при form-add) — только минимальные varstore-декларации question-add.
- TUI-скролл Forms View, lost-update flush_image, «форма под формой» —
  отдельные записи TODO, не эта дуга.
- Кросс-гейты за compression-барьером без рекомпрессии — честный отказ
  (существующая семантика `MutationBehindCompression`).

## 5. Acceptance

1. Синтетика: length-дискриминация REF-вариантов; кросс-гейт находится и
   флипается в донорской секции; source_target в GateInfo; REF3-эмит
   (байты 33, layout §2); varstore-декларация в прологе + сдвиг $SPF;
   коллизии/валидация — InvalidSchema.
2. Регрессия: существующие тесты gates/unlock/hijack/question-add/ref_tree
   не падают (GateTarget-литералы обновляются механически).
3. Real-гейт: `real_amibcp_450x_formset_unlock` (ветка a или b) + полный
   прогон `cargo test -p uefi-engine -- --ignored`.
4. Владельческий live-прогон TUI/CLI на 450x (вердикт-аддендум в §7).

## 6. Карта NVRAM-эффекта бифуркации (450x, живые данные)

| Вопрос | Форма | varstore | offset | Опции |
|--------|-------|----------|--------|-------|
| 0x243 | 118 (IIO 0) | IntelSetup (EC87D643…, id 1, 0x1670) | 0x531 | 0=x4x4x4x4 … 4=x16, 0xFF=Auto |
| 0x242/0x244 | 118 | IntelSetup id 1 | заполняется на гейте U4 | — |
| 0x257–0x259 | 119 (IIO 1) | IntelSetup id 1 | заполняется | — |
| 0x26b–0x26d | 422 (IIO 2) | IntelSetup id 1 | заполняется | — |
| 0x27f–0x281 | 423 (IIO 3) | IntelSetup id 1 | заполняется | — |

Гейт формы 118 внутри IntelRCSetup: `suppress ref host 5 qid 0 expr
'0x0215 == 0x0000'`, flip `pkg+0x5614: 00 00 → ff ff` (флипается
существующей механикой).

## 7. Вердикт владельца

(заполняется после live-гейта)
