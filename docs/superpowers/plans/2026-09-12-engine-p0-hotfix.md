# Engine P0 Hotfix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Починить два P0 движка, найденных на живом образе 450x: no-op `hii_unlock` не должен пересобирать/перезаписывать артефакт; `build_image` должен собирать верхний уровень образа по абсолютным offset'ам (round-trip на образах с перекрывающимися регионами). Плюс P1-защита: flush валидирует билд до записи на диск.

**Architecture:** (1) RPC-хендлер `hii_unlock` вызывает `flush_image` только при непустом `outcome.applied`; (2) `build_node` для Image/Capsule/Root собирает каждого ребёнка в отдельный буфер и кладёт его в выход по полю `FfsNode.offset` (random-access placement, fill 0xFF), а не конкатенацией — на непересекающихся образах это тождественно старому поведению, на перекрывающихся устраняет дублирование ~11.6MB; (3) `flush_image` до `atomic_write` прогоняет билд через `parse_image` + сравнение `count_files` и отказывает в записи при коллапсе.

**Tech Stack:** Rust workspace, uefi-engine (binrw/r-efi уже в зависимостях), tonic RPC, tmp существующие тест-харнессы `spawn_engine_on` (server.rs tests) и `real_image.rs`.

**Spec:** аддендум (2026-09-12) к `docs/superpowers/specs/2026-08-13-full-flash-round-trip-design.md` (Goals 2/4 + no-op flush триггер, кросс-ссылка на `2026-09-03-hii-unlock-op-design.md`). Первоисточник находок: `TODO.md`, раздел «Находки live-сессии 450x: unlock формсета, порча артефакта (2026-09-12)» — записи `no-op hii_unlock уничтожает артефакт` [P0] и `build_image round-trip ломает образ 450x` [P0]. Дополнительно (закрывается попутно, не отдельной задачей): guard-заметка «рост не ловит» — закрывается Task 3.

## Диагноз (первоисточник, воспроизведено 2026-09-12)

**P0-1.** `hii::unlock` при 0 гейтах корректно не мутирует дерево (early return `hii/mod.rs:374-380`), но RPC-хендлер `rpc/server.rs:963` вызывает `self.flush_image(...)` безусловно. flush = build → atomic_write на диск → re-parse → замена in-memory. На живом 450x: артефакт 16777216 → 28409856 байт, `hii form list` = 0 строк.

**P0-2.** Верхний уровень 450x (по `node list`): Descriptor (off 0, 4096), **Dev Expansion 2 (off 0, 4349952 — перекрывает всё начало)**, Dev Expansion 1 (off 4096, 65536), ME (off 69632, 8318976), **IE (off 1146880, 7282688 — залезает в ME и в BIOS-область до 8431552)**, дальше тома/паддинги 8388608..16777216. Вендорский flash-дескриптор кривой ( regions перекрываются; отсюда же WARN «region outside image, skipped»). `build_node` для Root конкатенирует детей: сумма тел ≈ 29.4MB → выход 28409856, re-parse: volumes 13→6, files 311→0. У образов со смежными регионами (HNX99TF/kot: Descriptor 0..4096, ME 4096..8388608, тома 8388608..16777216) конкатенация тождественна оригиналу, поэтому round-trip зелёный.

**P1.** `flush_image` пишет на диск ДО re-parse (`server.rs:110-122`): если билд не перепарсивается, диск уже испорчен. Валидация до записи закрывает весь класс «билд сломан» независимо от root-cause.

## Global Constraints

- AGENTS.md: TDD-порядок (тест падает → реализация → тест проходит → commit); один коммит на шаг с `git commit`; после каждой задачи `cargo test -p uefi-engine` и `cargo clippy -p uefi-engine -- -D warnings`.
- Правило 11 (AGENTS.md): расхождение плана с реальностью — отдельный коммит `docs: fix Task N in engine-p0-hotfix plan (...)` ДО реализации.
- Никаких narration-комментариев в коде; только короткие rustdoc `///` на pub/pub(crate)-контрактах.
- Реальные образы — только в `#[ignore]`-тестах с путём через `env!("CARGO_MANIFEST_DIR")/../../../refs/...` и env-override (паттерн `fw_path()` в `tests/real_image.rs:9-15`).
- Команда прогона real-тестов: `cargo test -p uefi-engine -- --ignored` (плюс обычный `cargo test -p uefi-engine` на каждый шаг).

---

### Task 1: `hii_unlock` — no-op не flush'ит артефакт

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs:952-970` (хендлер `hii_unlock`)
- Test: `crates/uefi-engine/src/rpc/server.rs` (tests mod, рядом с `flush_image_writes_bytes_to_data_dir` ~2257)

**Interfaces:**
- Consumes: существующие тест-харнессы `setup()` (server.rs:1333), `create_session` (server.rs:2293), layout артефакта `sessions/<sid>/images/<image_id>.bin`.
- Produces: поведение RPC «unlock с 0 flips не трогает диск»; Task 3 добавляет валидацию внутрь `flush_image` — сигнатуры не меняются.

- [ ] **Step 1: Написать падающий real-image тест**

В tests mod `server.rs` добавить (использует живой образ, у которого форма #1 IntelRCSetup заведомо без гейтов — воспроизведено 2026-09-12):

```rust
fn amibcp_450x_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("UEFIPATCHER_TEST_AMIBCP") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../refs/amibcp/450x — копия.bin")
}
```

```rust
#[tokio::test]
#[ignore = "requires real AMI image under refs/amibcp/ (gitignored)"]
async fn hii_unlock_noop_keeps_artifact_untouched() {
    let (td, mut client) = setup().await;
    let orig = amibcp_450x_path();
    let orig_bytes = std::fs::read(&orig).unwrap();
    let session = create_session(&mut client).await;
    let opened = client
        .image_open(ImageOpenRequest {
            session_id: session.clone(),
            path: orig.to_string_lossy().to_string(),
            mode: ImageMode::Write as i32,
            name: "450x".into(),
        })
        .await
        .unwrap()
        .into_inner();
    let resp = client
        .hii_unlock(HiiUnlockRequest {
            image_id: opened.image_id.clone(),
            item_id: "abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0#1".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(resp.applied_flips.is_empty(), "форма #1 без гейтов: flips нет");
    let img_path = td
        .path()
        .join("sessions")
        .join(&session)
        .join("images")
        .join(format!("{}.bin", opened.image_id));
    let stored = std::fs::read(&img_path).unwrap();
    assert_eq!(
        stored.len(),
        orig_bytes.len(),
        "no-op unlock не должен переписывать артефакт"
    );
    assert_eq!(stored, orig_bytes);
    let _ = client
        .session_destroy(SessionDestroyRequest {
            session_id: session,
        })
        .await;
}
```

Если у `image_open`/`hii_unlock` в тестовом харнессе есть auth-требования (существующие тесты зовут RPC напрямую без metadata — ожидается, что нет), поправить по образцу соседних тестов (правило 11).

- [ ] **Step 2: Прогнать, убедиться в падении**

Run: `cargo test -p uefi-engine hii_unlock_noop -- --ignored`
Expected: FAIL — `stored.len()` = 28409856 ≠ 16777216.

- [ ] **Step 3: Реализация — условный flush**

`crates/uefi-engine/src/rpc/server.rs`, хендлер `hii_unlock` (сейчас строка 963 — безусловный вызов):

```rust
        self.flush_image(&r.image_id).await?;
```

заменить на:

```rust
        if !outcome.applied.is_empty() {
            self.flush_image(&r.image_id).await?;
        }
```

Семантика: `applied` пуст ⇒ `hii::unlock` не мутировал дерево (early return при `absolute_flips.is_empty()`, `hii/mod.rs:374-380`) ⇒ пересборка и запись не нужны. Гейты есть, но все уже разлочены (`plan_gates_skip_unlocked`) — тоже `applied` пуст, диск не трогаем.

- [ ] **Step 4: Прогнать, убедиться в прохождении**

Run: `cargo test -p uefi-engine hii_unlock_noop -- --ignored`
Expected: PASS.

- [ ] **Step 5: Полный прогон + lint**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: все существующие тесты зелёные (хендлер меняет только no-op путь), clippy чист.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(engine): no-op hii_unlock больше не flush'ит артефакт (P0-1, TODO 2026-09-12)"
```

---

### Task 2: `build_image` кладёт верхний уровень по offset'ам

**Files:**
- Modify: `crates/uefi-engine/src/builder/mod.rs:33-48` (`build_node`, ветки Image/Capsule/Root)
- Test: `crates/uefi-engine/src/builder/mod.rs` (tests mod — новые тесты + обновление offset'ов в существующих фикстурах)
- Test: `crates/uefi-engine/tests/real_image.rs` (новый `#[ignore]`-тест)

**Interfaces:**
- Consumes: `FfsNode.offset: u32` (types.rs:153, абсолютный offset — парсер заполняет всегда), `Action::NoAction` у нетронутых нод.
- Produces: инвариант «build нетронутого дерева воспроизводит вход по размещению детей»; Task 3 полагается на то, что валидация ловит остаточные случаи.

Семантика placement: каждый ребёнок Image/Capsule/Root собирается в отдельный буфер (рекурсия как раньше) и пишется в выход по `child.offset`; размер выхода = `max(offset + built_len)`; дыры заполняются 0xFF; при перекрытии поздний ребёнок перезаписывает раннего (на реальных образцах перекрытия — срезы одних и тех же байтов, так что порядок не меняет результат). У существующих тестов фикстуры стоят `offset: 0` всем детям — их надо довести до накопительных offset'ов, иначе все дети лягут в ноль.

- [ ] **Step 1: Написать падающие юнит-тесты placement**

В tests mod `builder/mod.rs` добавить хелпер и два теста (структура `FfsNode` — 13 полей, types.rs:149-163):

```rust
    fn region_node(offset: u32, body: Vec<u8>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type: FfsType::Region,
            subtype: 0,
            offset,
            header: vec![],
            body,
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::Region(RegionParsingData {
                kind: FlashRegionKind::Me,
            }),
            fixed: true,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn image_with_children(children: Vec<FfsNode>) -> Image {
        Image {
            image_id: "t".into(),
            session_id: "s".into(),
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
            mode: ImageMode::Write,
        }
    }
```

(Если `ParsingData::None` не существует — посмотреть реальный вариант enum `ParsingData` в `types.rs` для «нет данных» и использовать его; `FlashRegionKind`/`RegionParsingData` импортировать сверху tests mod по образцу импортов файла.)

```rust
    #[test]
    fn build_image_places_top_level_children_by_offset() {
        let img = image_with_children(vec![
            region_node(0, vec![0x11; 8]),
            region_node(4, vec![0x22; 8]),
        ]);
        let out = build_image(&img).unwrap();
        assert_eq!(
            out,
            vec![0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22],
            "перекрытие: поздний ребёнок перезаписывает раннего, размер = max(offset+len)"
        );
    }

    #[test]
    fn build_image_fills_gaps_with_ff() {
        let img = image_with_children(vec![
            region_node(0, vec![0x11; 4]),
            region_node(8, vec![0x22; 4]),
        ]);
        let out = build_image(&img).unwrap();
        assert_eq!(out.len(), 12);
        assert_eq!(&out[4..8], &[0xFF; 4]);
        assert_eq!(&out[0..4], &[0x11; 4]);
        assert_eq!(&out[8..12], &[0x22; 4]);
    }
```

- [ ] **Step 2: Прогнать, убедиться в падении**

Run: `cargo test -p uefi-engine build_image_places build_image_fills`
Expected: FAIL — текущая конкатенация даёт 16 байт без fill (первый тест) и 8 байт (второй).

- [ ] **Step 3: Реализация — placement вместо конкатенации**

В `builder/mod.rs` заменить ветку Image/Capsule/Root в `build_node` (сейчас строки 35-38 — цикл `build_node(child, out)?`):

```rust
        FfsType::Image | FfsType::Capsule | FfsType::Root => {
            debug_assert!(out.is_empty());
            let mut placed: Vec<(usize, Vec<u8>)> = Vec::new();
            let mut total = 0usize;
            for child in &node.children {
                let mut buf = Vec::new();
                build_node(child, &mut buf)?;
                total = total.max(child.offset as usize + buf.len());
                placed.push((child.offset as usize, buf));
            }
            out.resize(total, 0xFF);
            for (off, buf) in &placed {
                out[*off..*off + buf.len()].copy_from_slice(buf);
            }
        }
```

Остальные ветки (Volume/File/Section/Padding/FreeSpace/Region) не трогать. Если `debug_assert!` падает в каком-то тесте (вложенный Image с непустым out) — заменить на `out.clear()` перед циклом и зафиксировать в коммите плана (правило 11).

- [ ] **Step 4: Обновить offset'ы в существующих фикстурах builder-тестов**

Прогнать `cargo test -p uefi-engine` — упасть должны тесты, чьи фикстуры ставят всем детям `offset: 0` (сайты: `builder/mod.rs` ~289, 304, 319, 340, 417, 452, 539, 616, 631, 655 — актуализировать по коду). Правило: ребёнку выставляется накопительный offset = сумма `header+body+tail` (плюс выравнивания, если тест их ожидает) всех предыдущих детей. Пример диффа для фикстуры с Volume(256) + Padding(128):

```rust
// было
let vol = FfsNode { offset: 0, /* body 256 */ .. };
let pad = FfsNode { offset: 0, /* body 128 */ .. };
// стало
let vol = FfsNode { offset: 0, /* body 256 */ .. };
let pad = FfsNode { offset: 256, /* body 128 */ .. };
```

Каждый сайт правится по данным его фикстуры; ожидания тестов (байты выхода) не меняются — на смежных фикстурах placement тождественен старой конкатенации. Если какой-то тест после выставления offset'ов всё ещё красный — разбрать отдельно, при расхождении с этим шагом зафиксировать в плане (правило 11), не подгоняя тест молча.

Run: `cargo test -p uefi-engine`
Expected: PASS (весь крейт).

- [ ] **Step 5: Написать real-image round-trip тест**

`crates/uefi-engine/tests/real_image.rs` — рядом с `fw_path()` (строки 9-15) добавить:

```rust
fn amibcp_path() -> PathBuf {
    if let Ok(p) = std::env::var("UEFIPATCHER_TEST_AMIBCP") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../refs/amibcp/450x — копия.bin")
}
```

и тест (локальный хелпер подсчёта — `count_files` в парсере приватный):

```rust
fn count_files(node: &FfsNode) -> usize {
    let mut n = if node.node_type == FfsType::File { 1 } else { 0 };
    for child in &node.children {
        n += count_files(child);
    }
    n
}

#[test]
#[ignore = "requires external real AMI image under refs/amibcp/ (gitignored)"]
fn real_amibcp_450x_build_round_trip() {
    let data = std::fs::read(amibcp_path()).unwrap();
    let img = parse_image(&data, ImageMode::Write, "t", "s").unwrap();
    let before = count_files(&img.root);
    assert_eq!(before, 311);
    let built = uefi_engine::builder::build_image(&img).unwrap();
    assert_eq!(built.len(), data.len(), "перекрывающиеся регионы не должны раздувать образ");
    let reparsed = parse_image(&built, ImageMode::Write, "t2", "s").unwrap();
    let after = count_files(&reparsed.root);
    assert_eq!(after, before, "round-trip не должен терять файлы");
}
```

- [ ] **Step 6: Прогнать real-тест**

Run: `cargo test -p uefi-engine real_amibcp_450x -- --ignored`
Expected: PASS (до Task 2 этот тест давал 28409856/0 файлов).

- [ ] **Step 7: Полный прогон + lint**

Run: `cargo test -p uefi-engine && cargo test -p uefi-engine -- --ignored && cargo clippy -p uefi-engine -- -D warnings`
Expected: всё зелёное (включая `real_image_*` на HNX99TF — смежные регионы, placement тождествен).

- [ ] **Step 8: Commit**

```bash
git add crates/uefi-engine/src/builder/mod.rs crates/uefi-engine/tests/real_image.rs
git commit -m "fix(engine): build_image кладёт верхний уровень по offset'ам детей — round-trip на перекрывающихся регионах 450x (P0-2)"
```

---

### Task 3: `flush_image` валидирует билд до записи на диск

**Files:**
- Modify: `crates/uefi-engine/src/parser/image.rs:134` (`count_files` → `pub(crate)`)
- Modify: `crates/uefi-engine/src/rpc/server.rs:84-132` (`flush_image`)
- Test: `crates/uefi-engine/src/rpc/server.rs` (tests mod)

**Interfaces:**
- Consumes: `parse_image(&bytes, mode, image_id, session_id)` (уже вызывается в `flush_image` после записи — переиспользуем до записи), `crate::parser::image::count_files`.
- Produces: `fn validate_flush_bytes(current: &Image, bytes: &[u8]) -> Result<Image, Status>` (private, server.rs) — возвращает перепарсенный образ; отказ `FailedPrecondition` если байт не парсится или файлов стало меньше.

- [ ] **Step 1: Написать падающий юнит-тест валидатора**

В tests mod `server.rs` (fixture_volume уже есть, server.rs:2246-2255):

```rust
    fn parsed_fixture_image() -> Image {
        let bytes = fixture_volume();
        crate::parser::image::parse_image(&bytes, ImageMode::Write, "t", "s").unwrap()
    }

    #[test]
    fn validate_flush_bytes_rejects_unparseable_and_collapsed() {
        let img = parsed_fixture_image();
        let err = validate_flush_bytes(&img, b"garbage not an image".as_slice()).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("refusing write"));

        let ok_bytes = fixture_volume();
        let before = crate::parser::image::count_files(&img.root);
        let refreshed = validate_flush_bytes(&img, &ok_bytes).unwrap();
        assert!(crate::parser::image::count_files(&refreshed.root) >= before);
    }
```

Если `fixture_volume()` парсится с 0 файлов (пустой FV без файлов) — коллапс-ветку проверить на «0 → 0 не отказывает» (валидатор отказывает только при `after < before`), что уже покрыто вызовом с `ok_bytes`.

- [ ] **Step 2: Прогнать, убедиться в падении**

Run: `cargo test -p uefi-engine validate_flush_bytes`
Expected: FAIL — `validate_flush_bytes` не определена.

- [ ] **Step 3: Реализация валидатора и проводка в flush_image**

`parser/image.rs:134`: `fn count_files(` → `pub(crate) fn count_files(`.

В `server.rs` (над `flush_image`) добавить:

```rust
    fn validate_flush_bytes(current: &Image, bytes: &[u8]) -> Result<Image, Status> {
        let refreshed =
            parse_image(bytes, current.mode, &current.image_id, &current.session_id).map_err(
                |e| {
                    Status::failed_precondition(format!(
                        "build output does not re-parse ({e}); refusing write to prevent data loss"
                    ))
                },
            )?;
        let before = crate::parser::image::count_files(&current.root);
        let after = crate::parser::image::count_files(&refreshed.root);
        if after < before {
            return Err(Status::failed_precondition(format!(
                "build output reparses with {after} files vs {before} stored; \
                 refusing write to prevent data loss"
            )));
        }
        Ok(refreshed)
    }
```

В `flush_image` (server.rs:84-132) переставить порядок: валидация ДО `atomic_write`, замена in-memory — после. Тело после size-guard (строки 99-109, guard оставить как есть):

```rust
        let refreshed = {
            let images = self.images.lock().await;
            let img = images
                .get(image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            validate_flush_bytes(img, &bytes)?
        };
        atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
        let mut images = self.images.lock().await;
        if let Some(img) = images.get_mut(image_id) {
            *img = refreshed;
        }
```

(старый блок `match parse_image(...)` в строках 111-123 удалить — парс уже случился в валидаторе; `touch_image` и хвост без изменений). Существующие тесты `flush_image_rejects_when_build_output_smaller_than_stored` (server.rs:2814) и `flush_image_rejects_zero_byte_build_output` (2871) должны остаться зелёными — их сценарии отсекаются раньше (size-guard) или тем же кодом отказа.

- [ ] **Step 4: Прогнать, убедиться в прохождении**

Run: `cargo test -p uefi-engine validate_flush_bytes && cargo test -p uefi-engine`
Expected: PASS (весь крейт; успешные flush-сценарии в тестах проходят валидацию — их фикстуры смежные и round-trip'ятся).

- [ ] **Step 5: Lint**

Run: `cargo clippy -p uefi-engine -- -D warnings`
Expected: чисто.

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/rpc/server.rs crates/uefi-engine/src/parser/image.rs
git commit -m "fix(engine): flush_image валидирует билд (re-parse + count_files) до записи на диск (P1, защита от порчи артефактов)"
```

---

### Task 4: Закрыть TODO-записи и зафиксировать вердикт

**Files:**
- Modify: `TODO.md` (раздел «Находки live-сессии 450x…»)

**Interfaces:**
- Consumes: результаты Tasks 1-3 (все тесты зелёные, real-прогон на 450x).
- Produces: закрытые чекбоксы P0-1/P0-2 + пометка о P1-защите.

- [ ] **Step 1: Обновить TODO.md**

В разделе «Находки live-сессии 450x…»:
- `* [ ] **uefi-engine [P0]: no-op hii_unlock уничтожает артефакт**` → `* [x]` + строка «закрыто дугой engine-p0-hotfix (2026-09-12): условный flush + validate-before-persist».
- `* [ ] **uefi-engine [P0]: build_image round-trip ломает образ 450x**` → `* [x]` + строка «закрыто: placement по offset'ам; real-тест `real_amibcp_450x_build_round_trip`».
- В записи про guard («рост не ловит») дописать: «закрыто validate-before-persist (Task 3)».
- Записи REF5/дуга/скролл Forms View остаются открытыми (не входят в эту дугу).

- [ ] **Step 2: Финальная верификация дуги**

Run: `cargo test --all && cargo clippy --all -- -D warnings && cargo test -p uefi-engine -- --ignored`
Expected: всё зелёное.

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs(todo): закрыть P0-записи live-сессии 450x (no-op unlock flush, round-trip) дугой engine-p0-hotfix"
```

---

## Self-Review

1. **Spec coverage:** TODO P0-1 → Task 1; TODO P0-2 → Task 2 (+ real-тест); P1-защита/guard-заметка → Task 3; audit-заметка «прогнать по всем HII-хендлерам» — проверено при планировании: `hii_set_form_visibility` при отсутствии изменений возвращает `Err(NoSuppressScope)` ДО flush (hii/mod.rs:126-129), т.е. не виновник; `hii_set_value` при Ok всегда мутирует (значение+флипы) — flush оправдан. Открытыми остаются REF5/дуга/скролл — вне скоупа, чекбоксы не трогаем.
2. **Placeholder-сканирование:** код во всех шагах конкретный; два места с «посмотреть по образцу соседних» (auth в тесте Task 1, вариант `ParsingData` в Task 2) — явные развилки с инструкцией и правилом 11, не TBD.
3. **Консистентность типов:** `validate_flush_bytes(&Image, &[u8]) -> Result<Image, Status>` определена в Task 3 и там же использована; `count_files` поднимается до `pub(crate)` в Task 3 (integration-тест Task 2 использует собственный локальный хелпер — намеренно, без расширения public API).
