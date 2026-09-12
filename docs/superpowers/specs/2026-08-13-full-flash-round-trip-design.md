# Full-flash round-trip в билдере — Design

**Date:** 2026-08-13
**Scope:** `uefi-engine` (parser, builder, rpc/server)
**Related:** `TODO.md` строки 434-487 («CRITICAL: Builder не round-tripит полный
flash-образ (issue V, ревизия 2026-08-13)»), `2026-07-22-uefi-engine-design.md`.
**Severity:** high — потенциальная порча данных / кирпич при прошивке.

## Motivation

`build_image(parse_image(<16MB образ>))` → **7.8MB** вместо 16MB. Замерено на
`refs/fw/HNX99TF_200525_original_E5C88C6F.bin` (16,777,216 байт): `build_image`
даёт `7,798,784` байта во **всех** кейсах, даже без мутаций.

### Root cause

`parse_image` (`parser/image.rs:12-72`) сканирует буфер по `EFI_FVH_SIGNATURE` и
кладёт в `root.children` **только** найденные FV (на реальном образе — 3 тома:
`0x800000`, `0x890000`, `0xda0000`). Всё остальное — Intel Flash Descriptor
(первые `0x1000` байт), ME-регион, межтомные gap'ы, padding в конце — **не
моделируется** как узел.

`build_image` (`builder/mod.rs:18-22`) эммитит только `root.children` (FV) →
~половина образа теряется.

### Impact

`flush_image` (`rpc/server.rs:31-55`) на **любой** мутации (remove/insert/
replace/rebuild) вызывает `build_image` и пишет результат на диск через
`atomic_write`. После первой мутации хранимый файл становится **обрезанным**
(7.8MB вместо 16MB). На рестарте `get_or_load_image` (`server.rs:57-86`) читает
уже 7.8MB → дерево меньше оригинала (нет ME/descriptor). Прошивка такого образа
= потеря ME/IFD = **кирпич**.

Существующий тест `real_image_builder_round_trip` (`tests/real_image.rs:422-464`)
проверяет round-trip только **отдельных FV-слайсов** (`parse_image(slice)`, где
slice = один FV), не полного образа. Поэтому регрессия не ловилась.

## Goals

1. **`parse_image` полного образа создает дерево, покрывающее весь буфер** —
   каждый байт исходного образа принадлежит ровно одному узлу (Volume или
   Padding).
2. **`build_image` на немодифицированном дереве = byte-identical round-trip** —
   `build_image(parse_image(buf)) == buf`.
3. **Re-patch stability** — после `build_image → re-parse → build_image` дерево
   стабильно (идемпотентность).
4. **Safety-guard в `flush_image`** — отказывать в write-through с понятной
   ошибкой, если build-output усечён относительно файла на диске.
5. **Регрессионный тест** на полном 16MB образе.

## Non-goals (перенесены в TODO.md)

- **Парсинг Intel Flash Descriptor** → Region-узлы (ME/BIOS/GbE) с subtype.
  Сейчас non-FV байты — opaque Padding. Структурный IFD-парсинг — отдельная
  задача (большой объём, `descriptor.cpp` из UEFITool).
- **Рекомпрессия LZMA/GUID_DEFINED** — issue IV, отдельная спека.
- **Volume-level Remove → erase-byte Padding** — известное ограничение (см.
  «Known limitations»), фикс отдельной задачей.
- **ME/FTPR region parsing** — отдельный распознаватель, не нужен для round-trip.

## Архитектура

### Подход: Gap-aware (minimal)

Парсер создаёт `FfsType::Padding`-узлы для сырых байт между и вокруг FV.
Builder **уже умеет** Padding — `build_node` (`mod.rs:34`):
`FfsType::Padding | FfsType::FreeSpace => out.extend_from_slice(&node.body)`.

Это согласуется с моделью UEFITool: на уровне Image `buildRawArea`
(`ffsbuilder.cpp:249-326`) строит children (Volume + Padding), не-FFV регионы
эммитятся verbatim. Отличие: мы не различаем Region subtypes (ME/GbE/…) — все
non-FV байты = Padding. Достаточно для round-trip.

Альтернативы (patch-overlay, full IFD parse) — отклонены: patch-overlay вводит
двойную модель (дерево + overlay-buffer), full IFD parse — избыточен для
round-trip и несёт высокий риск (сложный парсинг descriptor + ME).

### Parser change: gap capture (`parser/image.rs`)

Функция `parse_image` модифицируется: между находимыми FV (и перед первым, и
после последнего) создаются Padding-узлы для байтовых диапазонов, не покрытых
FV.

```rust
pub fn parse_image(buf: &[u8], mode: ImageMode, image_id: &str, session_id: &str)
    -> Result<Image, ParserError>
{
    let mut children = vec![];
    let mut off = 0usize;
    let mut last_end = 0usize;  // конец предыдущего узла (абсолютный offset)

    while off + 44 <= buf.len() {
        let sig = u32::from_le_bytes([buf[off+40], buf[off+41], buf[off+42], buf[off+43]]);
        if sig != EFI_FVH_SIGNATURE {
            off += FVH_SCAN_STEP;
            continue;
        }
        match parse_volume(buf, off as u32) {
            Ok(vol) => {
                let vol_size = vol.header.len() + vol.body.len();
                if vol_size == 0 {
                    off += FVH_SCAN_STEP;
                    continue;
                }
                // Gap capture: [last_end .. off] — байты до этого FV
                if off > last_end {
                    children.push(make_padding_node(buf, last_end, off));
                }
                // ... существующая логика parse_volume_files ...
                children.push(vol_with_files);
                last_end = off + vol_size;
                off += vol_size;
            }
            Err(e) => {
                tracing::debug!("not a volume at {off:#x}: {e}");
                off += FVH_SCAN_STEP;
            }
        }
    }

    // Trailing gap: [last_end .. buf.len()]
    if buf.len() > last_end {
        children.push(make_padding_node(buf, last_end, buf.len()));
    }

    Ok(Image { root: FfsNode { node_type: FfsType::Image, children, ... }, ... })
}
```

Где `make_padding_node`:

```rust
fn make_padding_node(buf: &[u8], start: usize, end: usize) -> FfsNode {
    FfsNode {
        guid: None,
        node_type: FfsType::Padding,
        subtype: 0,
        offset: start as u32,
        header: vec![],
        body: buf[start..end].to_vec(),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    }
}
```

**Ключевые свойства:**
- `children` остаются отсортированными по offset (gaps вставляются между FV в
  порядке обхода).
- Zero-size gaps не создают узел (guard `if off > last_end` / `if buf.len() >
  last_end`).
- Padding node `offset` = абсолютная позиция в буфере (консистентно с
  `Volume.offset`).
- Padding node `header` = пустой (header не нужен — build_node для Padding
  эммитит только `body`).
- На образе без gaps (single-FV slice, как в существующих тестах) — new behavior
  = old behavior (zero Padding nodes created).

### Builder: без изменений

`build_node` (`mod.rs:24-37`) уже корректно обрабатывает новое дерево:

```rust
FfsType::Image | FfsType::Capsule | FfsType::Region | FfsType::Root => {
    for child in &node.children {
        build_node(child, out)?;
    }
}
FfsType::Padding | FfsType::FreeSpace => out.extend_from_slice(&node.body),
```

С деревом `[Padding, Volume, Padding, Volume, Padding, Volume, Padding]` builder
эммитит каждый узел по порядку → `sum(header+body+tail для всех children) ==
len(orig_buf)`.

**Для `Action::NoAction`** (нет мутаций): каждый узел эммитит свои байты verbatim
→ `build_image(parse_image(buf)) == buf`. Byte-identical round-trip.

**Для мутаций** (например, remove файла внутри volume):
- Volume → Rebuild → файлы перестраиваются, padding до `old_total` с `empty_byte`
  (`build_volume:63-67`).
- Смежные Padding-узлы → verbatim (не меняются).
- Total size сохранён.

### flush_image safety-guard (`rpc/server.rs`)

Belt-and-suspenders проверка перед записью на диск. Если `build_image` вернул
меньше байт, чем файл на диске — отказать в write и вернуть понятную ошибку
вместо тихой порчи.

```rust
async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
    let (bytes, session_id) = {
        let images = self.images.lock().await;
        let img = images.get(image_id)
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes = crate::builder::build_image(img)
            .map_err(|e| Status::internal(e.to_string()))?;
        (bytes, img.session_id.clone())
    };

    // Safety guard: сравнить размер build-output с файлом на диске
    let path = self.data_dir.join("sessions").join(&session_id)
        .join("images").join(format!("{image_id}.bin"));
    if let Ok(metadata) = std::fs::metadata(&path) {
        let existing_size = metadata.len() as usize;
        if !bytes.is_empty() && bytes.len() < existing_size {
            return Err(Status::failed_precondition(format!(
                "build_image output ({}) is smaller than stored file ({}); \
                 refusing write to prevent data loss",
                bytes.len(), existing_size
            )));
        }
    }

    atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
    // ... existing touch_image logic ...
    Ok(())
}
```

**Условие срабатывания**: только при усечении (`bytes.len() < existing_size`).
Не блокирует первый write (файла ещё нет) и не блокирует рост (insert). Если
build-output корректно меньше (теоретически невозможно при gap-aware, но
защита от будущих регрессий) — пользователь получает ошибку вместо молчаливой
порчи.

## Re-patch stability analysis

Сценарий: patch → flush → reload → patch → flush.

1. Open 16MB → parse: `[Padding(0..0x800000), Volume(0x800000), Padding(gap),
   Volume(0x890000), Padding(gap), Volume(0xda0000), Padding(trailing)]`
2. Remove file `1/13/0` → flush → build_image:
   - Padding(0..0x800000) → verbatim ✓
   - Volume(0x800000) → NoAction → verbatim ✓
   - Padding(gap) → verbatim ✓
   - Volume(0x890000) → Rebuild → файлы перестроены (минус удалённый), padded to
     fixed size with 0xFF ✓
   - Padding(gap) → verbatim ✓
   - Volume(0xda0000) → NoAction → verbatim ✓
   - Padding(trailing) → verbatim ✓
   - Total: 16MB ✓
3. Reload → parse 16MB → дерево: та же структура (минус один файл) ✓
4. Remove ещё файл → flush → тот же процесс → 16MB ✓

**Стабильно.** Padding-узлы verbatim, Volume-узлы либо verbatim (NoAction), либо
rebuilt to fixed size (Rebuild). Total size всегда сохранён.

## Тестирование

### Unit-тесты в `parser/image.rs` (без реального образа)

```rust
#[test]
fn parse_image_captures_leading_gap_as_padding() {
    // [0xAA * 64] + [valid FV 256 bytes]
    // → root.children[0] = Padding(offset=0, len=64)
    // → root.children[1] = Volume(offset=64)
}

#[test]
fn parse_image_captures_gap_between_volumes_as_padding() {
    // [FV 256] + [0xAA * 32 gap] + [FV 256]
    // → children[0]=Volume(0), children[1]=Padding(256, len=32), children[2]=Volume(288)
}

#[test]
fn parse_image_captures_trailing_gap_as_padding() {
    // [FV 256] + [0xBB * 64 trailing]
    // → children[0]=Volume(0), children[1]=Padding(256, len=64)
}

#[test]
fn parse_image_no_gap_when_fv_at_start() {
    // [FV 256] (no leading gap)
    // → children[0]=Volume(0), len(children)==1 (no Padding)
}

#[test]
fn parse_image_padding_node_fields() {
    // Проверить: node_type=Padding, offset правильный, body=сырые байты,
    // header=пустой, action=NoAction
}
```

### Регрессионный тест в `tests/real_image.rs` (с `#[ignore]`)

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/"]
fn real_image_full_flash_round_trip() {
    let data = load_fw();  // 16MB
    assert_eq!(data.len(), 0x0100_0000);

    let img = parse_image(&data, ImageMode::Read, "img1", "s1").unwrap();
    let rebuilt = build_image(&img).unwrap();

    assert_eq!(rebuilt.len(), data.len(),
        "full-flash round-trip: size mismatch ({} != {})",
        rebuilt.len(), data.len());
    assert_eq!(rebuilt, data,
        "full-flash round-trip: byte mismatch");
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/"]
fn real_image_full_flash_repatch_stability() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").unwrap();
    let built1 = build_image(&img).unwrap();
    // Re-parse the built output and build again — must be identical
    let img2 = parse_image(&built1, ImageMode::Read, "img1", "s1").unwrap();
    let built2 = build_image(&img2).unwrap();

    assert_eq!(built2, built1,
        "re-patch instability: second build differs from first");
    assert_eq!(built2, data,
        "re-patch instability: second build differs from original");
}
```

### Тест flush_image safety-guard (в `rpc/server.rs` tests)

```rust
#[tokio::test]
async fn flush_image_rejects_truncated_output() {
    // Setup: write a 16MB file to data_dir, then flush with a tree
    // that produces < 16MB (simulated via mock or corrupted tree).
    // Expect: Status::failed_precondition, file unchanged.
}
```

## Known limitations (обновление TODO.md)

Эти пункты добавляются в TODO.md после строки 487 (блок issue V), как
результат ревизии gap-aware фикса:

1. **Volume-level Remove на full-flash** — `build_volume` (`mod.rs:40-42`)
   эммитит 0 байт для `Action::Remove`. На полном образе это сдвигает все
   последующие регионы → ломает IFD layout (absolute region boundaries). Future
   fix: при Remove Volume → emit erase-byte Padding того же размера (preserve
   total flash size). Пока: Volume-level Remove на полном flash-образе
   **опасен**, не использовать без ручной проверки.

2. **Padding node `Action::Remove` игнорируется** — `build_node` (`mod.rs:34`)
   всегда эммитит `body` для Padding, не проверяя action. Safe для round-trip
   (нет data loss), но пользовательский intent (удалить padding) молча
   игнорируется. Low priority — Padding-манипуляция нестандартна.

3. **ME/IFD регионы opaque** — captured как raw Padding bytes, не structured.
   Нет ME version display, нет IFD region labeling. Future: IFD parser upgrade
   (Padding → Region nodes with subtype), отдельный цикл.

## Ссылки

- `refs/UEFITool-ai-fork/common/ffsbuilder.cpp:153-247` — `buildIntelImage`
  (descriptor + regions + padding).
- `refs/UEFITool-ai-fork/common/ffsbuilder.cpp:249-326` — `buildRawArea`
  (BIOS region rebuild, pad-to-old-size).
- `refs/UEFITool-ai-fork/common/ffsbuilder.cpp:389-489` — `buildVolume`
  (fixed volume size, FreeSpace absorption).
- `crates/uefi-engine/src/parser/image.rs:12-72` — текущий `parse_image` (scan
  for FVH, skip non-FV bytes).
- `crates/uefi-engine/src/builder/mod.rs:24-37` — `build_node` dispatch (Padding
  → emit body).
- `crates/uefi-engine/src/builder/mod.rs:39-70` — `build_volume` (fixed-size
  pad to `old_total`).
- `crates/uefi-engine/src/rpc/server.rs:31-55` — `flush_image` (atomic_write
  build output).
- `crates/uefi-engine/tests/real_image.rs:422-464` — существующий
  `real_image_builder_round_trip` (FV-slice only, не ловил баг).

## Аддендум (2026-09-12): перекрывающиеся регионы, no-op flush, validate-before-persist

Источник: live-сессия 2026-09-12 на образе `refs/amibcp/450x — копия.bin`
(16,777,216 байт, вендорский AMI/450x). Попытка no-op `hii_unlock` формы #1
IntelRCSetup (0 гейтов) уничтожила артефакт: 16777216 → 28409856 байт,
re-parse: volumes 13→6, files 311→0. План фикса:
`docs/superpowers/plans/2026-09-12-engine-p0-hotfix.md`. Три правки к целям
этой спеки:

1. **Goal 2 (byte-identical round-trip) выполнялся только на смежных
   регионах.** IFD-парсинг (Known limitation #3) был реализован позже как
   Region-узлы, но `build_node` для Image/Root **конкатенирует** детей —
   это тождественно оригиналу только если регионы смежные (HNX99TF/kot:
   Descriptor 0..4096, ME 4096..8388608, тома до 16777216). Дескриптор 450x
   кривой: Dev Expansion 2 (off 0, size 4349952) перекрывает начало образа,
   IE (off 1146880, size 7282688) перекрывает ME и заходит в BIOS-область —
   конкатенация дублирует ~11.6MB. **Правка:** placement по offset'ам —
   каждый ребёнок верхнего уровня собирается в отдельный буфер и пишется по
   `FfsNode.offset` (парсер заполняет всегда), дыры — 0xFF, размер выхода =
   `max(offset + len)`, при перекрытии поздний ребёнок перезаписывает
   раннего (перекрывающиеся тела — срезы одних и тех же байтов оригинала,
   порядок не меняет результат). На смежных образах placement тождественен
   конкатенации — существующие round-trip тесты не меняют ожиданий.
   Референс: `ffsbuilder.cpp:153-247` (`buildIntelImage` placement).

2. **Goal 4 (safety-guard) усилен с size-only до validate-before-persist.**
   Текущий guard сравнивает только длины и срабатывает уже ПОСЛЕ записи
   (re-parse в строках после `atomic_write`): билд, который раздувает образ
   (28MB > 16MB), guard'ом проходил, а диск уже испорчен. **Правка:** до
   `atomic_write` — `parse_image(bytes)` + сравнение `count_files` с
   хранимым деревом; отказ `FailedPrecondition` («refusing write to prevent
   data loss») если байт не парсится или файлов стало меньше. Закрывает
   весь класс «билд сломан» независимо от root-cause.

3. **Триггер происшествия — вне скоупа спеки, но в скоупе аддендума:**
   RPC `hii_unlock` вызывал `flush_image` безусловно, даже когда
   `hii::unlock` не мутировал дерево (0 гейтов, early return). Спека
   unlock-op (`2026-09-03-hii-unlock-op-design.md`) регламентирует флипы,
   но не условность flush. **Правка:** flush только при непустом
   `outcome.applied`. Аудит остальных HII-хендлеров при планировании:
   `hii_set_form_visibility` при отсутствии изменений возвращает
   `Err(NoSuppressScope)` до flush — не виновник; `hii_set_value` при Ok
   всегда мутирует — flush оправдан.

Секция «Builder: без изменений» этой спеки — по-прежнему верна для Volume и
ниже; правка касается только верхнего уровня (Image/Capsule/Root).
