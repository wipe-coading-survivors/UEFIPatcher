# Спека: рекомпрессия LZMA GUIDed-секций в билдере (фаза 7)

> Дизайн-документ, 2026-08-14. Статус: дизайн утверждён пользователем
> (подход A — публичный компрессор lzma-rs; brainstorm-цикл 2026-08-14).
> Закрывает issue IV (вариант «Полный (рекомпрессия)» + «Минимум») и
> мини-фикс issue II-ter из TODO.md.
>
> Предпосылки: `docs/plans/2026-08-14-предпосылки-для-рекомпрессии.md`
> (включая верификацию 2026-08-14 — коммит `000b6bd`, probe
> `hack/recompress_probe.py`). Референс:
> `refs/UEFITool-ai-fork/common/ffsbuilder.cpp:685-990` (compressData,
> GUIDed LZMA rebuild + round-trip check, пересчёт размеров). Связанная
> спека: `2026-08-14-hii-pe-resource-extraction-design.md` (фаза 6) — её
> §6 корректируется настоящей спекой (см. §5).

## 1. Контекст и проблема

`build_section` (`crates/uefi-engine/src/builder/mod.rs:105-131`) для
COMPRESSED/GUID_DEFINED-секций эммитит оригинальный `body` **дословно** и
не ре-сериализует распакованные children — парсер строит детей
(`parser/section.rs:34-50`), билдер обратно не упаковывает. Поэтому
`Action::Remove/Replace/Insert` на узле внутри сжатия не влияет на
выходные байты (issue IV): write-through пишет неизменный блоб, после
рестарта парсинг восстанавливает узел. Кейс воспроизведения на
`refs/fw/HNX99TF_200525_original_E5C88C6F.bin`: `remove 1/13/2/1`
(UI-секция внутри GUIDed-LZMA обёртки файла `1/13`) — секция остаётся.

Прочие элементы проблемы (issue IV, «Связанные баги»): `ops::rebuild`
перетирает `Action::Remove` (`ops.rs:105`) — rebuild «воскрешает»
удалённый узел; replace body-only на самой сжатой секции молча портит
образ (stale header-size).

## 2. Цели и не-цели

**Цели:**

1. Мутации узлов **внутри** GUIDed plain-LZMA секций (LZMA / LZMA_HP /
   LZMA_MS GUID из `ffs.rs`) материализуются в выходных байтах: билдер
   пересобирает payload детей и переупаковывает LZMA-стрим.
2. Честные ошибки вместо silent-verbatim для dirty-обёрток, которые
   пересжать нельзя: Tiano-GUID, LZMAF86, standard COMPRESSION (0x01),
   unknown-GUID, а также LZMA-обёртки без распакованных детей.
3. `ops::rebuild` не перетирает `Action::Remove` (мини-фикс issue II-ter).
4. `replace body_only` на самой сжатой секции перестаёт портить образ —
   становится честной ошибкой (дети очищены, пересобирать не из чего).
5. Человекочитаемые сообщения об ошибке барьера в CLI/TUI (через Display
   `BuilderError` → `e.to_string()` в `flush_image`).
6. Real-image acceptance: удаление UI-секции внутри LZMA на HNX99TF
   материализуется, длина образа сохраняется, чистые регионы
   байт-идентичны.

**Не-цели (вне рамок фазы):**

- Tiano/Efi-компрессор (decompress-поддержки нет — `decompress.rs:23-25`).
- LZMAF86: исключён из пересжимаемых. EDK2-декодер F86 применяет
  BCJ-unfilter, plain-LZMA стрим был бы испорчен на железе (заявление
  референса «most firmware accepts plain LZMA» не проверяемо без железа);
  на HNX99TF F86-секций ноль (probe). Симметрично наш парсер трактует F86
  как plain LZMA — если такой образ появится, это отдельная задача.
- Рекомпрессия standard COMPRESSION (0x01), включая type-2 (LZMA):
  dirty → ошибка. Механически почти бесплатное расширение (префикс из
  5 байт + UncompressedLength), но на живых образах AMI не встречается —
  YAGNI, фиксированная точка расширения.
- Prune Remove-узлов после flush (issue II-bis), моделирование
  file-alignment (`alignment_bytes`/`fixed` парсером не заполняются),
  PE-resident IFR-мутации (гейт 3 фазы 6), WebUI.

## 3. Верификационные данные (обоснование решений)

Верификация 2026-08-14 (коммит `000b6bd`, детали в findings-документе):

- **Энкодер**: `lzma_rs::lzma_compress_with_options` (публичен в 0.3.0 —
  той же версии, что в Cargo.lock; `src/lib.rs:70`) с опцией
  `UnpackedSize::WriteToHeader(Some(len))` пишет формат EDK2 LzmaCompress:
  13-байтный alone-заголовок [props=0x5D, dict=0x800000 LE, known size
  u64 LE], без EOS-маркера. Энкодер literal-only (контроль на случайных
  байтах: ratio 1.014), реальные firmware-блобы жмёт до 0.346–0.426,
  2–12 мс на секцию.
- **Валидация стрима двумя независимыми декодерами**: lzma-rs (Rust) и
  liblzma (python `lzma.FORMAT_ALONE`) — оба декодируют байт-идентично
  исходу.
- **Props/Dict**: все секции HNX99TF — props 0x5D (совпадает с выходом
  энкодера), dict 0x1000000 против 0x800000 у энкодера. Разница
  безопасна: стрим без дистанций (literal-only), header-dict задаёт лишь
  размер окна/скрэтча декодера — требования снижаются, а не растут.
- **Ёмкость**: FV0 free=0, FV1 free=2 033 KiB, FV2 free=0. Рост секции
  1/13 после пересжатия: 37.0 KB против 26.0 KB (+11 KB, запас ~180x);
  гипотетический «пересжать все LZMA-секции FV1» ≈ +0.85 MB < 2.03 MB.

## 4. Дизайн

### 4.1 Примитив `compress.rs` (новый модуль uefi-engine)

```rust
pub enum CompressError {
    EmptyInput,
    CompressFailed,
    RoundTripFailed,
}

pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError>;
```

- Пустой вход → `Err(EmptyInput)` (guard до кодека).
- `lzma_rs::lzma_compress_with_options(&mut Cursor::new(input), &mut out,
  &Options { unpacked_size: UnpackedSize::WriteToHeader(Some(input.len()
  as u64)) })` → `Err(CompressFailed)` при io-ошибке.
- **Round-trip self-check внутри примитива**: `lzma_rs::lzma_decompress`
  выхода; результат != вход → `Err(RoundTripFailed)`; вывод в `warn`-лог.
  Зеркалит референс `ffsbuilder.cpp:902-913`. Стоимость — одна лишняя
  декомпрессия на секцию (мс).
- Энкодер изолирован в этой функции: замена (например, на match-based
  энкодер) в будущем = правка одного файла, контракт не меняется.

### 4.2 `subtree_dirty` (builder/mod.rs)

```rust
fn subtree_dirty(node: &FfsNode) -> bool;
```

true, если `node.action != Action::NoAction` или любой потомок dirty
(рекурсивно). Дублирующий детект к каскаду `mark_rebuild_to_root_by_path`
(`ops.rs:111-125`): ops помечает предков Rebuild всегда, но прямые правки
дерева (тесты, будущие ops) каскада не проходят — детект по факту
состояния надёжнее.

### 4.3 Предикат пересжимаемых GUID (ffs.rs)

```rust
pub fn is_recompressable_lzma_guid(g: &Guid) -> bool;
```

`lzma_guid() || lzma_hp_guid() || lzma_ms_guid()` — **без**
`lzmaf86_guid()` (основание — §2, не-цели). Существующий `is_lzma_guid`
(декодерная семантика, включает F86) не меняется.

### 4.4 Ветка recompress в `build_section`

Новый порядок dispatch (полная замена тела `builder/mod.rs:105-131`):

```
action == Remove                                  → skip (как сейчас)
is_compressed_or_guided(node):
  action == NoAction && !subtree_dirty(node)      → verbatim (как сейчас;
                                                     clean-дерево НИКОГДА
                                                     не пересжимается —
                                                     byte-identical
                                                     round-trip сохранён)
  GUID_DEFINED && is_recompressable_lzma_guid &&
  !children.is_empty()                            → recompress (ниже)
  иначе                                           → Err(Recompression-
                                                     Unsupported)
не-сжатая секция:
  action == NoAction                              → verbatim (как сейчас)
  иначе                                           → rebuild из детей
                                                     (как сейчас, без
                                                     изменений)
```

Ветка recompress:

1. Сериализовать детей в `children_stream` — тем же циклом, что
   сегодняшний rebuild-путь (каждый ребёнок через `build_node`, затем
   `pad_to(align4, 0x00)` после каждого, включая последнего).
2. `stream = compress_lzma(&children_stream)?` (ошибка — наверх через
   `From<CompressError>`).
3. Префикс guided-заголовка — дословно из `node.body`:
   `data_offset = u16 LE из body[16..18]`; `prefix_len = data_offset - 4`
   (DataOffset секционно-относителен, тело начинается после 4/8-байтного
   header секции — см. `parser/section.rs:81-84`). Guard:
   `prefix_len >= 20 && prefix_len <= body.len()`, иначе
   `RecompressionUnsupported`. Префикс (GUID + DataOffset + Attributes)
   не пересобирается — остаётся байт-идентичным, DataOffset остаётся
   валидным (длина префикса не меняется).
4. `new_body = body[..prefix_len] ++ stream`;
   `set_section_size(header, header.len() + new_body.len())` (обычный и
   section2-вариант уже поддержаны, `builder/mod.rs:151-164`).
5. Эммит `header ++ new_body`. Tail у секций пуст (парсер не заполняет).

Пересчёт размеров/чексуммов по цепочке (set_ffs_size,
recompute_ffs_checksums, volume reflow с паддингом до исходной длины и
честным `SizeMismatch` при переполнении) уже существует
(`builder/mod.rs:41-72, 137-183`) и не меняется.

### 4.5 Guard в `ops::rebuild` (issue II-ter)

`ops.rs:102-109`: до `node.action = Action::Rebuild` добавить
`if node.action == Action::Remove { return Ok(()); }` — rebuild не
воскрешает удалённый узел. С рекомпрессией молчаливое воскрешение стало
бы хуже: пересборка реально пере-упаковала бы удалённое в новый стрим.
`ops::replace` сознательно не трогаем: replace осмысленно «воскрешает»
узел новым содержимым (TODO issue II-ter — «как минимум задокументировать»;
семантика фиксируется в матрице §6).

### 4.6 Маппинг в `flush_image` (rpc/server.rs)

`build_image(...).map_err(...)` (`server.rs:38`): для
`BuilderError::RecompressionUnsupported` → `Status::failed_precondition(
e.to_string())` (клиентская, actionable — симметрично size-guard'у),
остальные — `internal`, как сейчас. Display-текст доезжает до CLI/TUI
без дополнительной проводки.

## 5. Поправка гейта фазы 6 (end-state)

Фаза 6 (спека `2026-08-14-hii-pe-resource-extraction-design.md`, §6)
вводит в `set_item_visibility` второй гейт: любой предок-Section
subtype 0x01/0x02 по `find_item_path` → `HiiError::MutationBehindCompression`.
End-state после настоящей фазы: гейт сужается — `MutationBehindCompression`
возвращается только если среди предков 0x01/0x02 есть обёртка, **не**
являющаяся GUID_DEFINED с `is_recompressable_lzma_guid`. За LZMA-обёрткой
мутация разрешена: билдер пересожмёт (движок-wide инвариант совпадает с
билдером).

Следствие для HNX99TF (формы в PE32-ресурсах за LZMA): ожидаемая ошибка
меняется с `MutationBehindCompression` на `NotASetupItem` третьего гейта
(цель — PE32-секция 0x10, не bare-форм). PE-resident IFR-патчинг —
будущая фаза, которая сузит и третий гейт; до неё ошибка остаётся честным
отказом, меняется только причина.

Порядок реализации фаз не влияет на end-state:

- **Фаза 6 раньше**: фаза 6 реализуется по утверждённому плану (тест
  ожидает `MutationBehindCompression`); настоящая фаза затем сужает гейт
  в коде и меняет ожидание реал-теста на `NotASetupItem` (один flip).
- **Настоящая фаза раньше**: до реализации фазы 6 — docs-коммит с
  правкой её спеки §6 и плана Task 7 (гейт-2 формулируется сразу
  сужённо, тест ожидает `NotASetupItem`).

## 6. Семантика мутаций (матрица end-state)

Обновление матрицы issue IV. «Внутри» = узел-потомок обёртки; «сама» =
узел-обёртка.

| Мутация | Вне сжатия | Сама plain-LZMA guided | Внутри plain-LZMA | Tiano/F86/COMPRESSION/unknown |
|---|---|---|---|---|
| Remove | ✅ | ✅ (дроп блоба целиком) | ✅ **recompress, материализуется** | сама ✅; внутри ❌ `RecompressionUnsupported` |
| Rebuild | cosmetic | ✅ recompress | ✅ recompress | ❌ ошибка |
| Insert Into | ✅ | ✅ recompress с новым ребёнком | — | ❌ ошибка |
| Replace body-only | ✅ | ❌ **честная ошибка** (дети очищены; раньше — corrupt) | ❌ ошибка | ❌ ошибка |
| Replace whole | `parse_file`-семантика (вне рамок) | ✅ recompress из распарсенных детей | ❌ ошибка | ❌ ошибка |

Примечания: Replace whole на сжатой секции (`ops.rs:84-94`) кладёт
распакованных детей от `parse_file` — recompress-ветка сериализует их и
переупаковывает; стрим отличается от пользовательского входа, контент
эквивалентен. Каскад `mark_rebuild_to_root_by_path` помечает предков
Rebuild — file/volume пересчитывают размеры и чексуммы автоматически.
Удаление последнего оставшегося ребёнка обёртки → отказ
`Compression(CompressError::EmptyInput)` (см. §8).

## 7. Ошибки и сообщения

- `CompressError` (compress.rs, thiserror): `EmptyInput`, `CompressFailed`,
  `RoundTripFailed` — Display на английском, по стилю существующих
  (`"size mismatch: ..."`).
- `BuilderError` (builder/mod.rs): два новых варианта —
  `Compression(#[from] CompressError)` и
  `RecompressionUnsupported`; Display последнего:
  `"compressed/guided section cannot be rebuilt: unsupported algorithm (Tiano, LZMAF86, standard compression, unknown GUID) or no decompressed children"`.
- RPC: `RecompressionUnsupported` → `failed_precondition` (§4.6); текст
  виден в CLI/TUI как статус ошибки flush.

## 8. Тестирование и acceptance

**Unit (внутрикрейтовые):**

- `compress_lzma`: round-trip на синтетике и на
  `tests/fixtures/lzma_guided_section.decompressed.bin`; точные байты
  заголовка (props 0x5D, dict LE 0x800000, size LE == input.len());
  `EmptyInput` на `&[]`.
- `subtree_dirty`: чистое дерево / dirty-потомок / dirty-сам.
- `is_recompressable_lzma_guid`: 3 GUID — true; f86 и tiano — false.
- `ops::rebuild` guard: remove → rebuild → action остаётся `Remove` →
  `build_image` узел отсутствует.

**Fixture-уровень (пара `lzma_guided_section{,.decompressed}.bin`; факт:
распакованный буфер = одна RAW-секция 0x19, x86-код, 210 байт):**

- parse → `replace body_only` на RAW-ребёнке (новое тело иной длины) →
  build → re-parse: у ребёнка новое тело; байты `body[..prefix_len]`
  (GUID/DataOffset/Attributes) байт-идентичны оригиналу;
  `decompress(new payload)` == сериализации детей; section size в header
  == фактической длине.
- `remove` единственного ребёнка → `Err(BuilderError::Compression(
  CompressError::EmptyInput))` — осознанный отказ: пустой payload за
  обёрткой не поддерживаем (поведение EDK2-декодера на size-0 стриме не
  верифицировано).
- dirty tiano-guided узел (синтетика: guided с tiano GUID, own Rebuild)
  → `build_image` → `Err(RecompressionUnsupported)`.
- dirty COMPRESSION-узел (синтетика) → та же ошибка.
- clean guided-узел с детьми → байт-в-байт verbatim (регрессия
  существующего поведения).

**Real-image `#[ignore]` (HNX99TF, acceptance issue IV):**

- Динамический поиск цели (path-таргеты на full-flash нестабильны —
  TODO «Positional Target::Path нестабилен»): первый по обходу
  GUID_DEFINED-узел с `is_recompressable_lzma_guid` и UI-ребёнком;
  target = UI-ребёнок.
- `remove` target → `build_image`:
  - `built.len() == orig.len()` (паддинг FV поглощает рост);
  - re-parse `built`: выбранная UI-секция отсутствует (по строке имени);
  - байты вне FV1 (`[..0x890000]` и `[0xd60000..]`) байт-идентичны
    оригиналу.

**Регрессия (обязательна зелёной):** `real_image_full_flash_round_trip`,
`real_image_full_flash_repatch_stability` — clean-дерево не пересжимается
никогда; полный flash остаётся байт-идентичным без мутаций.

**RPC:** flush с `RecompressionUnsupported` → статус `failed_precondition`
(unit на маппинг в server.rs-тестах).

## 9. Риски

1. **Совместимость с EDK2-декодером прошивки** (главный, переносится из
   findings): единственное окончательное доказательство — прошивка на
   железе. Смягчения: формат known-size/props 0x5D байт-идентичен
   параметрам образца; round-trip self-check на каждом стриме;
   валидация двух независимых декодеров на probe-данных. Фиксируется как
   acceptance-риск: перед реальной прошивкой модифицированного образа —
   ручная проверка на совместимом ридере/UEFITool.
2. **FV без свободного места** (FV0/FV2 на HNX99TF): рост пересжатого
   файла → `SizeMismatch` от `build_volume`, flush отказывает честно.
   Документируемое ограничение, не блокер.
3. **Рост размеров** (ratio 0.35–0.43 против оригинальных 0.14–0.24):
   одиночные мутации поглощаются свободным местом FV1 с запасом ~180x;
   массовые — до исчерпания, затем честная ошибка. Энкодер изолирован в
   `compress_lzma` — апгрейд до match-based энкодера точечен.

## 10. Скелет задач (детализируется в writing-plans)

| # | Задача | Объём |
|---|--------|-------|
| 1 | `compress.rs`: `compress_lzma` + `CompressError` + round-trip self-check; тесты заголовка/round-trip/empty | ядро |
| 2 | `subtree_dirty` + `is_recompressable_lzma_guid` + тесты | маленькая |
| 3 | `build_section` recompress-ветка + варианты `BuilderError` + fixture-тесты (recompress, verbatim-регрессия, честные ошибки) | ядро |
| 4 | `ops::rebuild` Remove-guard + тест | маленькая |
| 5 | `flush_image` маппинг статуса + unit-тест | маленькая |
| 6 | Real-image acceptance + прогон full-flash регрессий | средняя |
| 7 | Docs: правка фазы 6 (§5 настоящей спеки, порядок зависим от
    последовательности фаз) + TODO-апдейты (закрыть issue IV «Полный» +
   «Минимум» ×2, II-ter; снять пометку «Спека: рекомпрессия») | маленькая |

## 11. Ограничения и отложенное

- **Section2-апгрейд при >0xFFFFFF**: пересжатая секция не может
  превысить 16 MB (FV меньше образа); 4-байтный header достаточен.
  Апгрейд header 4→8 байт при переполнении не реализуется (YAGNI).
- **Alignment файлов**: `build_volume` выравнивает на 8; рост файла
  сдвигает последующие файлы (свойство существует и сегодня для
  remove/insert; `alignment_bytes`/`fixed` парсером не заполняются).
  Без изменений; фиксированная точка будущего цикла.
- **F86-декод**: парсер трактует LZMAF86 как plain LZMA (без BCJ) —
  если реальный образ с BCJ-потоком появится, дети уже некорректны;
  отдельная задача парсера (в TODO не заводится до живого прецедента).
- **Компрессия standard COMPRESSION type-2**: см. §2 — точка расширения
  (префикс 5 байт + UncompressedLength), не реализуется.
