# Full-flash round-trip (issue V) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Заставить `build_image(parse_image(buf)) == buf` для полного flash-образа (16MB) — устранить тихую усечку до 7.8MB при `flush_image`.

**Architecture:** Gap-aware подход: `parse_image` создаёт `FfsType::Padding`-узлы для сырых байт между и вокруг FV (Intel Flash Descriptor, ME-регион, межтомные gaps, trailing padding). Builder не меняется — `build_node` уже умеет Padding (`mod.rs:34`: `out.extend_from_slice(&node.body)`). Дополнительно: safety-guard в `flush_image` для отказа при усечении.

**Tech Stack:** Rust workspace, `uefi-engine` крейт. Без новых зависимостей.

**Spec:** `docs/superpowers/specs/2026-08-13-full-flash-round-trip-design.md`

## Global Constraints

- **TDD порядок:** (1) написать тест → (2) `cargo test` (падает) → (3) реализация → (4) `cargo test` (проходит) → (5) commit.
- **Без комментариев в коде** (кроме ссылок на референс `file:line`).
- После каждой задачи: `cargo test -p uefi-engine` и `cargo clippy -p uefi-engine -- -D warnings`.
- Тестовый образ: `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` (16MB), путь из крейта `../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin`. `#[ignore]`-тесты запускаются явно.
- `FfsType::Padding` уже определён в `types.rs:14` (`Padding = 64`).
- `build_node` для Padding уже работает: `builder/mod.rs:34` `FfsType::Padding | FfsType::FreeSpace => out.extend_from_slice(&node.body)`.

---

## Файлы плана

| Файл | Действие | Назначение |
|---|---|---|
| `crates/uefi-engine/src/parser/image.rs` | Modify | gap-capture в `parse_image` + `make_padding_node` helper + unit-тесты |
| `crates/uefi-engine/tests/real_image.rs` | Modify | регрессионные `#[ignore]`-тесты на полном 16MB образе |
| `crates/uefi-engine/src/rpc/server.rs` | Modify | safety-guard в `flush_image` + тест |
| `TODO.md` | Modify | known limitations после issue V |

---

### Task 1: Gap-capture в `parse_image` + unit-тесты

**Files:**
- Modify: `crates/uefi-engine/src/parser/image.rs` (функция `parse_image` строки 12-72, `mod tests` строки 250-568)
- Test: `crates/uefi-engine/src/parser/image.rs` (`mod tests`)

**Interfaces:**
- Consumes: `FfsType::Padding` (`types.rs:14`), `FfsNode` (`types.rs:72-86`), `Action::NoAction` (`types.rs:24`), `ParsingData::None` (`types.rs:64`)
- Produces: `parse_image` теперь создаёт `FfsNode { node_type: FfsType::Padding, ... }` для non-FV байт. `make_padding_node(buf, start, end) -> FfsNode` — приватный хелпер.

- [ ] **Step 1: Написать failing-тесты для gap-capture**

Добавить в конец `mod tests` в `parser/image.rs` (после теста `search_skips_non_section_nodes`, перед закрывающей `}`):

```rust
    #[test]
    fn parse_image_captures_leading_gap_as_padding() {
        use crate::builder::build_image;
        let fv = make_image_with_volume();
        let mut buf = vec![0xAAu8; 64];
        buf.extend_from_slice(&fv);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 2, "expected Padding + Volume");
        let pad = &img.root.children[0];
        assert_eq!(pad.node_type, FfsType::Padding);
        assert_eq!(pad.offset, 0);
        assert_eq!(pad.body.len(), 64);
        assert_eq!(&pad.body, &[0xAAu8; 64]);
        let vol = &img.root.children[1];
        assert_eq!(vol.node_type, FfsType::Volume);
        assert_eq!(vol.offset, 64);
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(rebuilt, buf, "leading gap round-trip must be byte-identical");
    }

    #[test]
    fn parse_image_captures_gap_between_volumes_as_padding() {
        use crate::builder::build_image;
        let fv1 = make_image_with_volume();
        let fv2 = make_image_with_volume();
        let mut buf = fv1.clone();
        buf.extend_from_slice(&[0xBBu8; 32]);
        buf.extend_from_slice(&fv2);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 3, "expected Volume + Padding + Volume");
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
        assert_eq!(img.root.children[0].offset, 0);
        assert_eq!(img.root.children[1].node_type, FfsType::Padding);
        assert_eq!(img.root.children[1].offset, 256);
        assert_eq!(img.root.children[1].body.len(), 32);
        assert_eq!(img.root.children[2].node_type, FfsType::Volume);
        assert_eq!(img.root.children[2].offset, 288);
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(rebuilt, buf, "inter-volume gap round-trip must be byte-identical");
    }

    #[test]
    fn parse_image_captures_trailing_gap_as_padding() {
        use crate::builder::build_image;
        let fv = make_image_with_volume();
        let mut buf = fv;
        buf.extend_from_slice(&[0xCCu8; 64]);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 2, "expected Volume + trailing Padding");
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
        assert_eq!(img.root.children[1].node_type, FfsType::Padding);
        assert_eq!(img.root.children[1].offset, 256);
        assert_eq!(img.root.children[1].body.len(), 64);
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(rebuilt, buf, "trailing gap round-trip must be byte-identical");
    }

    #[test]
    fn parse_image_no_padding_when_fv_fills_buffer() {
        let buf = make_image_with_volume();
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 1, "no gaps expected");
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
    }

    #[test]
    fn parse_image_padding_node_fields() {
        let fv = make_image_with_volume();
        let mut buf = vec![0xDDu8; 48];
        buf.extend_from_slice(&fv);
        buf.extend_from_slice(&[0xEEu8; 16]);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        let lead = &img.root.children[0];
        assert_eq!(lead.node_type, FfsType::Padding);
        assert_eq!(lead.guid, None);
        assert_eq!(lead.subtype, 0);
        assert_eq!(lead.offset, 0);
        assert!(lead.header.is_empty());
        assert_eq!(lead.body.len(), 48);
        assert!(lead.tail.is_empty());
        assert!(lead.children.is_empty());
        assert_eq!(lead.action, Action::NoAction);
        assert!(matches!(lead.parsing_data, ParsingData::None));
        let trail = &img.root.children[2];
        assert_eq!(trail.node_type, FfsType::Padding);
        assert_eq!(trail.offset, 304);
        assert_eq!(trail.body.len(), 16);
    }
```

- [ ] **Step 2: Запустить тесты — должны упасть**

Run: `cargo test -p uefi-engine parse_image_captures`
Expected: FAIL — тесты падают (либо `children.len()` не равен ожидаемому, либо нет Padding-узла). Существующий тест `parse_image_no_padding_when_fv_fills_buffer` должен PASS (т.к. он проверяет существующее поведение).

- [ ] **Step 3: Добавить `make_padding_node` helper**

Добавить приватную функцию `make_padding_node` в `parser/image.rs`, **перед** `fn parse_volume_files` (после закрывающей `}` функции `parse_image`, примерно строка 73):

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

- [ ] **Step 4: Модифицировать `parse_image` — gap capture**

Заменить тело функции `parse_image` (строки 12-72). Добавить `last_end` tracking и gap-capture перед каждым Volume и после последнего.

Старый код (строки 12-72):
```rust
pub fn parse_image(
    buf: &[u8],
    mode: ImageMode,
    image_id: &str,
    session_id: &str,
) -> Result<Image, ParserError> {
    let mut children = vec![];
    let mut off = 0usize;
    while off + 44 <= buf.len() {
        let sig = u32::from_le_bytes([buf[off + 40], buf[off + 41], buf[off + 42], buf[off + 43]]);
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
                let (erase, rev) = match &vol.parsing_data {
                    ParsingData::Volume(vd) => (vd.empty_byte, vd.revision),
                    _ => (0xFF, 2),
                };
                let mut vol_with_files = vol.clone();
                let header_len = vol.header.len();
                let body_start = off + header_len;
                let body_end = off + vol_size;
                vol_with_files.children =
                    parse_volume_files(&buf[body_start..body_end], body_start, erase, rev);
                children.push(vol_with_files);
                off += vol_size;
            }
            Err(e) => {
                tracing::debug!("not a volume at {off:#x}: {e}");
                off += FVH_SCAN_STEP;
            }
        }
    }
    Ok(Image {
        image_id: image_id.into(),
        session_id: session_id.into(),
        root: FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        },
        mode,
    })
}
```

Новый код (замена целиком):
```rust
pub fn parse_image(
    buf: &[u8],
    mode: ImageMode,
    image_id: &str,
    session_id: &str,
) -> Result<Image, ParserError> {
    let mut children = vec![];
    let mut off = 0usize;
    let mut last_end = 0usize;
    while off + 44 <= buf.len() {
        let sig = u32::from_le_bytes([buf[off + 40], buf[off + 41], buf[off + 42], buf[off + 43]]);
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
                if off > last_end {
                    children.push(make_padding_node(buf, last_end, off));
                }
                let (erase, rev) = match &vol.parsing_data {
                    ParsingData::Volume(vd) => (vd.empty_byte, vd.revision),
                    _ => (0xFF, 2),
                };
                let mut vol_with_files = vol.clone();
                let header_len = vol.header.len();
                let body_start = off + header_len;
                let body_end = off + vol_size;
                vol_with_files.children =
                    parse_volume_files(&buf[body_start..body_end], body_start, erase, rev);
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
    if buf.len() > last_end {
        children.push(make_padding_node(buf, last_end, buf.len()));
    }
    Ok(Image {
        image_id: image_id.into(),
        session_id: session_id.into(),
        root: FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        },
        mode,
    })
}
```

- [ ] **Step 5: Запустить тесты — должны пройти**

Run: `cargo test -p uefi-engine --lib parser::image`
Expected: PASS — все 5 новых тестов проходят + существующие тесты не сломаны.

- [ ] **Step 6: Запустить полный набор тестов крейта + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — все тесты проходят, clippy чистый.

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-engine/src/parser/image.rs
git commit -m "fix(uefi-engine): parse_image captures non-FV bytes as Padding nodes (issue V)

parse_image now creates FfsType::Padding nodes for byte ranges between
and around FVs (Intel Flash Descriptor, ME region, inter-volume gaps,
trailing padding). This fixes build_image truncating full flash images
from 16MB to 7.8MB — every byte of the input buffer is now represented
in the FfsNode tree and emitted by the builder."
```

---

### Task 2: Регрессионные тесты на полном 16MB образе

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (добавить тесты в конец файла)
- Test: `crates/uefi-engine/tests/real_image.rs` (`#[ignore]`-тесты)

**Interfaces:**
- Consumes: `parse_image` (`parser::image`), `build_image` (`builder`), `ImageMode` (`types`), `load_fw()` / `fw_path()` (existing helpers в `real_image.rs:11-22`)

- [ ] **Step 1: Добавить тест `real_image_full_flash_round_trip`**

Добавить в конец файла `tests/real_image.rs` (после теста `real_image_search_finds_utf8_string_in_pe32`):

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/"]
fn real_image_full_flash_round_trip() {
    use uefi_engine::builder::build_image;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");

    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let rebuilt = build_image(&img).expect("build_image");

    assert_eq!(
        rebuilt.len(),
        data.len(),
        "full-flash round-trip: size mismatch (rebuilt {} != orig {})",
        rebuilt.len(),
        data.len()
    );
    assert_eq!(rebuilt, data, "full-flash round-trip: byte mismatch");
}
```

- [ ] **Step 2: Добавить тест `real_image_full_flash_repatch_stability`**

Добавить сразу после предыдущего теста:

```rust
#[test]
#[ignore = "requires external real BIOS image under refs/fw/"]
fn real_image_full_flash_repatch_stability() {
    use uefi_engine::builder::build_image;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let built1 = build_image(&img).expect("build_image");
    assert_eq!(built1, data, "first build must equal original");

    let img2 = parse_image(&built1, ImageMode::Read, "img1", "s1").expect("re-parse");
    let built2 = build_image(&img2).expect("second build_image");

    assert_eq!(
        built2, built1,
        "re-patch instability: second build differs from first"
    );
    assert_eq!(
        built2, data,
        "re-patch instability: second build differs from original"
    );
}
```

- [ ] **Step 3: Запустить тесты (если есть образ)**

Run: `cargo test -p uefi-engine -- --ignored real_image_full_flash`
Expected: PASS (если файл `refs/fw/HNX99TF_200525_original_E5C88C6F.bin` существует).

Если файл отсутствует — проверить компиляцию:
Run: `cargo test -p uefi-engine --no-run`
Expected: компилируется без ошибок.

- [ ] **Step 4: Запустить clippy**

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — чисто.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): full-flash round-trip + re-patch stability (issue V)

Regression tests on 16MB real BIOS image:
- real_image_full_flash_round_trip: parse_image(build_image(buf)) == buf
- real_image_full_flash_repatch_stability: parse→build→parse→build is
  idempotent (re-patching the same image is safe)"
```

---

### Task 3: Safety-guard в `flush_image`

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (функция `flush_image` строки 31-55, `mod tests` строки 675-853)
- Test: `crates/uefi-engine/src/rpc/server.rs` (`mod tests`)

**Interfaces:**
- Consumes: `build_image` (`builder`), `atomic_write` (`storage::image`), `EngineServer.data_dir` (`server.rs:25`), `fs::metadata` (`std::fs` уже импортирован в `server.rs:2`)
- Produces: `flush_image` теперь возвращает `Status::failed_precondition` если build-output меньше существующего файла.

- [ ] **Step 1: Написать failing-тест для safety-guard**

Тест вызывает `flush_image` напрямую на `EngineServer` с in-memory деревом,
построенным из 256-байтного буфера, тогда как файл на диске = 512 байт.
Guard должен обнаружить `256 < 512` и отказать.

Добавить в `mod tests` в `server.rs`, перед закрывающей `}` (после теста
`write_through_persists_mutation_to_disk`):

```rust
    #[tokio::test]
    async fn flush_image_rejects_when_build_output_smaller_than_stored() {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
        let sm = Arc::new(SessionManager::new(
            db,
            td.path().to_path_buf(),
            Duration::from_secs(864000),
            Duration::from_secs(3600),
            false,
        ));
        let (session_id, _tok) = sm.create_session("test").unwrap();
        let image_id = "img-test";

        let stored_buf = {
            use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};
            let mut buf = vec![0xFFu8; 512];
            buf[32..40].copy_from_slice(&512u64.to_le_bytes());
            buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
            buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
            buf[48..50].copy_from_slice(&56u16.to_le_bytes());
            buf[55] = 2;
            buf
        };

        let img_path = td
            .path()
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        std::fs::create_dir_all(img_path.parent().unwrap()).unwrap();
        std::fs::write(&img_path, &stored_buf).unwrap();

        let small_buf = vec![0xFFu8; 256];
        let img = parse_image(&small_buf, ImageMode::Write, image_id, &session_id).unwrap();

        let images = Arc::new(Mutex::new(HashMap::from([(image_id.to_string(), img)])));
        let server = EngineServer {
            sm,
            images,
            data_dir: td.path().to_path_buf(),
        };

        let result = server.flush_image(image_id).await;

        assert!(
            result.is_err(),
            "flush_image must reject when build output (256) < stored file (512)"
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);

        let preserved = std::fs::read(&img_path).unwrap();
        assert_eq!(preserved, stored_buf, "stored file must be unchanged");
    }
```

- [ ] **Step 2: Запустить тест — должен упасть**

Run: `cargo test -p uefi-engine --lib flush_image_rejects_when_build_output_smaller`
Expected: FAIL — `flush_image` не имеет safety-guard, `build_image` возвращает 256 байт, файл перезаписывается 256 байтами (или тест падает т.к. `result.is_err()` не выполняется — write проходит успешно).

- [ ] **Step 3: Реализовать safety-guard в `flush_image`**

Заменить тело функции `flush_image` в `server.rs` (строки 31-55).

Старый код:
```rust
    async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
        let (bytes, session_id) = {
            let images = self.images.lock().await;
            let img = images
                .get(image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let bytes =
                crate::builder::build_image(img).map_err(|e| Status::internal(e.to_string()))?;
            (bytes, img.session_id.clone())
        };
        let path = self
            .data_dir
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        self.sm
            .db
            .lock()
            .unwrap()
            .touch_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(())
    }
```

Новый код (добавить safety-guard после вычисления `path`, перед `atomic_write`):
```rust
    async fn flush_image(&self, image_id: &str) -> Result<(), Status> {
        let (bytes, session_id) = {
            let images = self.images.lock().await;
            let img = images
                .get(image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let bytes =
                crate::builder::build_image(img).map_err(|e| Status::internal(e.to_string()))?;
            (bytes, img.session_id.clone())
        };
        let path = self
            .data_dir
            .join("sessions")
            .join(&session_id)
            .join("images")
            .join(format!("{image_id}.bin"));
        if let Ok(metadata) = fs::metadata(&path) {
            let existing_size = metadata.len() as usize;
            if bytes.len() < existing_size {
                return Err(Status::failed_precondition(format!(
                    "build_image output ({}) is smaller than stored file ({}); \
                     refusing write to prevent data loss",
                    bytes.len(),
                    existing_size
                )));
            }
        }
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        self.sm
            .db
            .lock()
            .unwrap()
            .touch_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(())
    }
```

> Примечание: `!bytes.is_empty()` убран — empty build output должен триггерить guard (0 < existing_size), а не обходить его.

- [ ] **Step 4: Запустить тест — должен пройти**

Run: `cargo test -p uefi-engine --lib flush_image_rejects_when_build_output_smaller`
Expected: PASS — guard возвращает `FailedPrecondition`, файл не изменён.

- [ ] **Step 5: Запустить все тесты крейта + clippy**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS — все тесты (включая существующие `flush_image_writes_bytes_to_data_dir` и `write_through_persists_mutation_to_disk`) проходят.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(uefi-engine): flush_image rejects truncated build output (issue V)

Safety-guard in flush_image: compares build_image output size against
the existing file on disk. If output is smaller, returns
FailedPrecondition instead of silently corrupting the file. Belt-and-
suspenders protection against future regressions in the gap-aware
parser."
```

---

### Task 4: Обновить TODO.md — known limitations

**Files:**
- Modify: `TODO.md` (добавить секцию после строки 487, конец блока issue V)

- [ ] **Step 1: Добавить known limitations в TODO.md**

Добавить в конец файла `TODO.md` (после строки 487):

```markdown
### Known limitations: gap-aware round-trip (после фикса issue V)

> Результаты ревизии gap-aware фикса. Фикс (issue V) устраняет усечку
> полного образа, но следующие ограничения остаются:

* [ ] **Volume-level Remove на full-flash** — `build_volume`
  (`builder/mod.rs:40-42`) эммитит 0 байт для `Action::Remove`. На полном
  flash-образе это сдвигает все последующие регионы → ломает IFD layout
  (absolute region boundaries). Future fix: при Remove Volume → emit
  erase-byte Padding того же размера (preserve total flash size). Пока:
  Volume-level Remove на полном flash-образе **опасен**, не использовать
  без ручной проверки output-байтов.
* [ ] **Padding node `Action::Remove` игнорируется** — `build_node`
  (`builder/mod.rs:34`) всегда эммитит `body` для Padding, не проверяя
  action. Safe для round-trip (нет data loss), но пользовательский intent
  (удалить padding) молча игнорируется. Low priority — Padding-манипуляция
  нестандартна.
* [ ] **ME/IFD регионы opaque** — captured как raw Padding bytes, не
  structured. Нет ME version display, нет IFD region labeling. Future:
  IFD parser upgrade (Padding → Region nodes with subtype), отдельный цикл.
  Референс: `refs/UEFITool-ai-fork/common/descriptor.cpp`,
  `refs/UEFITool-ai-fork/common/meparser.cpp`.
```

- [ ] **Step 2: Commit**

```bash
git add TODO.md
git commit -m "docs(todo): known limitations after gap-aware round-trip fix (issue V)

Volume-level Remove shifts regions on full-flash; Padding Action::Remove
silently ignored; ME/IFD regions captured as opaque bytes (no structured
parsing yet)."
```
