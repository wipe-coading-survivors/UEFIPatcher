# TUI Revival (цикл A) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** вернуть TUI в статус полноценного рабочего инструмента: читаемые метки узлов, честное дерево после мутаций, FIND/GO, tab-completion, регионы flash-дескриптора (полная FLREG + ME/$FPT), `ImageUpload` RPC + `:upload`, снапшоты образа.

**Architecture:** engine-фиксы первыми (name-lift, prune, регионы), затем TUI-фичи; proto-расширения (`Node.region`, `ImageUpload`, снапшоты) — отдельными задачами с догоном всех `Node`-литералов в тестовых моках (урок `9933f59`: per-crate прогоны не компилируют чужие тест-таргеты — финал каждой proto-задачи проверяем `cargo test --all`).

**Tech Stack:** Rust edition 2024 (ratatui/crossterm/tonic/rusqlite), существующие крейты workspace; новых зависимостей нет.

**Spec:** `docs/superpowers/specs/2026-09-11-tui-revival-design.md` **включая аддендум от 2026-09-11** (A8' = снапшоты, A5/A1-уточнения). Спека путешествует с планом: исполнители читают обе.

## Global Constraints

- Ветка `feat/tui-revival` от `master`.
- TDD-порядок (AGENTS.md): тесты → красный → реализация → зелёный → commit; module-first rule (`pub mod x;` в том же шаге, что и создание файла).
- Комментариев в коде нет (кроме `file:line`-ссылок на референс и коротких rustdoc на pub-API).
- После каждой задачи: `cargo test -p <crate>`, `cargo clippy -p <crate> -- -D warnings`; в задачах с proto-правками дополнительно `cargo test --all` (mock-серверы чужих крейтов компилируются только в full-workspace прогоне).
- Real-image гейты: `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, `#[ignore]`-тесты, путь из крейта `../../../refs/fw/...`; образ не коммитится.
- Референс регионов/дескриптора: `../refs/UEFITool-ai-fork/common/descriptor.{h,cpp}` (FLVALSIG/FLMAP/FLREG, `calculateRegionOffset = base*0x1000`, `calculateRegionSize = (limit+1-base)*0x1000`, `calculateAddress8 = base*0x10`), `common/me.h` ($FPT, `FPT_HEADER`/`FPT_HEADER_ENTRY`), `common/meparser.cpp:39-170` (обход $FPT, rom-bypass 0x10), `common/ffsparser.cpp:343-470` (порядок регионов, паддинги между ними).
- Дефекты плана — отдельный `docs:`-коммит ДО реализации (правило-11).
- Сообщения коммитов — из задач.

## Контракты между задачами (схема данных)

| Контракт | Задаётся | Потребители |
|---|---|---|
| `fn find_lifted_name(children: &[FfsNode], depth: usize) -> Option<String>` (private, `parser/image.rs`) | Task 1 | — (внутренний) |
| `App::node_label(&self, node: &TreeNode) -> String` | Task 2 | `ui/tree.rs` (рендер), Task 8 (регион-метки приходят через `TreeNode.name` — уже сегодня заполняется из `Node.name`) |
| `OpsError::{MutationBehindCompression, ImmutableRegion}`; контракт `flush_image` = re-parse кэша после записи | Task 3 (barrier+re-parse), Task 6 (ImmutableRegion) | мутационные хендлеры `rpc/server.rs` |
| `App::goto_path(&mut self, target: &str) -> Result<(), String>` | Task 4 | `commands.rs` `:goto` |
| `uefi_common::cli::{NodeCmdArgs, Source, parse_node_flags}` | Task 5 | TUI `commands.rs` (insert/replace) |
| `commands::complete(app: &App, cmdline: &str) -> (Option<String>, Vec<String>)` | Task 5 | `main.rs` Tab |
| `types::{FlashRegionKind, RegionParsingData, ParsingData::Region}` | Task 6 | `parser/region.rs`, `node_name`, Task 7 ($FPT-партиции — Region-узлы), Task 8 (`Node.region`) |
| `parser::region::{parse_flash_regions, make_region_node, parse_fpt}` | Task 6/7 | `parse_image` |
| proto `Node.region` (string, поле 9) | Task 8 | TUI `TreeNode`, все прото-Node-литералы |
| proto `ImageUpload` + `ImageUploadRequest` | Task 9 | Task 10 (TUI client + `:upload`) |
| storage: таблица `image_snapshots`, `Db::{insert_image_snapshot, list_image_snapshots, get_image_snapshot}`, `storage::image::{store_snapshot_file, read_snapshot_file}` | Task 11 | Task 12 (restore) |
| proto снапшоты (`ImageSnapshotCreate/List/Restore`) | Task 11 | Task 12, Task 13 (TUI) |

Раскладка снапшотов на диске: `data_dir/sessions/<session_id>/images/<image_id>.snapshots/<snapshot_id>.bin` (внутри сессионного каталога — умирает с сессией при purge, как артефакты).

---

### Task 1: (A2) engine — name-lift имён файлов через GUIDED/compression-обёртки

**Files:**
- Modify: `crates/uefi-engine/src/parser/image.rs` (`node_name`, ~273-290)
- Test: там же, `mod tests`

**Interfaces:**
- Consumes: `FfsNode`, `FfsType`, `EFI_SECTION_UI`, `EFI_SECTION_VERSION`, `EFI_SECTION_COMPRESSION`, `EFI_SECTION_GUID_DEFINED` (все уже в scope файла через `use crate::ffs::*` / `crate::types::*`).
- Produces: поведение `node_name` — имя файла поднимается с глубины ≤ 8 через секции-обёртки. Ничего нового на публику.

- [ ] **Step 1: написать падающие тесты** (в `mod tests` файла `image.rs`, рядом с `node_name_lifts_ui_for_ffs_file`):

```rust
fn guided_with_ui_child() -> FfsNode {
    let ui = FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_UI,
        offset: 0,
        header: vec![0; 4],
        body: encode_utf16le_null("DeepSetup"),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_GUID_DEFINED,
        offset: 0,
        header: vec![0; 4],
        body: vec![],
        tail: vec![],
        children: vec![ui],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    }
}

#[test]
fn node_name_lifts_ui_through_guided_wrapper() {
    let mut file = FfsNode {
        guid: None,
        node_type: FfsType::File,
        subtype: 0x07,
        offset: 0,
        header: vec![0; 24],
        body: vec![],
        tail: vec![],
        children: vec![guided_with_ui_child()],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    assert_eq!(node_name(&file), "DeepSetup");
    let mut wrapper = guided_with_ui_child();
    wrapper.subtype = EFI_SECTION_COMPRESSION;
    file.children = vec![wrapper];
    assert_eq!(node_name(&file), "DeepSetup");
}

#[test]
fn node_name_lift_stops_at_depth_limit() {
    let mut node = guided_with_ui_child();
    for _ in 0..10 {
        let mut w = guided_with_ui_child();
        w.children = vec![node];
        node = w;
    }
    let file = FfsNode {
        guid: None,
        node_type: FfsType::File,
        subtype: 0x07,
        offset: 0,
        header: vec![0; 24],
        body: vec![],
        tail: vec![],
        children: vec![node],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    assert_eq!(node_name(&file), "");
}
```

- [ ] **Step 2: красный** — `cargo test -p uefi-engine node_name` → FAIL (`DeepSetup` vs `""`).

- [ ] **Step 3: реализация** — в `image.rs` заменить ветку `FfsType::File` в `node_name` и добавить хелпер:

```rust
fn node_name(node: &FfsNode) -> String {
    match node.node_type {
        FfsType::Section
            if node.subtype == EFI_SECTION_UI || node.subtype == EFI_SECTION_VERSION =>
        {
            decode_utf16le_body(&node.body)
        }
        FfsType::File => find_lifted_name(&node.children, 0).unwrap_or_default(),
        _ => String::new(),
    }
}

fn find_lifted_name(children: &[FfsNode], depth: usize) -> Option<String> {
    if depth >= 8 {
        return None;
    }
    for child in children {
        if child.node_type == FfsType::Section
            && (child.subtype == EFI_SECTION_UI || child.subtype == EFI_SECTION_VERSION)
        {
            return Some(decode_utf16le_body(&child.body));
        }
    }
    for child in children {
        if child.node_type == FfsType::Section
            && (child.subtype == EFI_SECTION_COMPRESSION
                || child.subtype == EFI_SECTION_GUID_DEFINED)
            && let Some(name) = find_lifted_name(&child.children, depth + 1)
        {
            return Some(name);
        }
    }
    None
}
```

- [ ] **Step 4: зелёный** — `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`.

- [ ] **Step 5: commit** — `feat(uefi-engine): node_name lifts UI/Version through compression wrappers (depth 8)`

---

### Task 2: (A1-core) TUI — метки безымянных узлов

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (новый метод `node_label`)
- Modify: `crates/uefi-tui/src/ui/tree.rs:35` (рендер использует `node_label`)
- Test: `app.rs` `mod tests`

**Interfaces:**
- Consumes: `App.registry.images: Vec<ImageInfo>` (`image_id`, `name`), `uefi_common::names::{file_type_name_or_raw, section_type_name_or_raw}`, константы `TYPE_*` из `theme.rs`.
- Produces: `App::node_label(&self, node: &TreeNode) -> String`.

- [ ] **Step 1: падающие тесты** (в `mod tests` файла `app.rs`):

```rust
#[test]
fn node_label_image_from_registry_name() {
    let mut app = App::new();
    app.active_image_id = Some("img-9".into());
    app.registry.images = vec![ImageInfo {
        image_id: "img-9".into(),
        name: "HNX99TF.bin".into(),
        ..Default::default()
    }];
    assert_eq!(app.node_label(&node("", 0)), "HNX99TF.bin");
}

#[test]
fn node_label_image_fallback_when_no_registry_match() {
    let mut app = App::new();
    app.active_image_id = Some("img-9".into());
    assert_eq!(app.node_label(&node("", 0)), "img-9");
    app.active_image_id = None;
    assert_eq!(app.node_label(&node("", 0)), "Image");
}

#[test]
fn node_label_volume_and_subtype_fallback() {
    let app = App::new();
    let mut vol = node("0", 1);
    vol.node_type = 65;
    assert_eq!(app.node_label(&vol), "Volume");
    let mut file = node("1/0", 2);
    file.node_type = 66;
    file.subtype = 0x07;
    assert_eq!(app.node_label(&file), "DXE driver");
    let mut sec = node("1/0/0", 3);
    sec.node_type = 67;
    sec.subtype = 0x77;
    assert_eq!(app.node_label(&sec), "Unknown 77h");
    let mut unk = node("3", 1);
    unk.node_type = 99;
    unk.subtype = 0x42;
    assert_eq!(app.node_label(&unk), "0x42");
}
```

- [ ] **Step 2: красный** — `cargo test -p uefi-tui node_label` → FAIL (метода нет).

- [ ] **Step 3: реализация** — в `impl App` (`app.rs`):

```rust
pub fn node_label(&self, node: &TreeNode) -> String {
    if !node.name.is_empty() {
        return node.name.clone();
    }
    match node.node_type {
        crate::theme::TYPE_IMAGE => match &self.active_image_id {
            Some(id) => self
                .registry
                .images
                .iter()
                .find(|i| &i.image_id == id)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| id.clone()),
            None => "Image".into(),
        },
        crate::theme::TYPE_VOLUME => "Volume".into(),
        66 => uefi_common::names::file_type_name_or_raw(node.subtype),
        67 => uefi_common::names::section_type_name_or_raw(node.subtype),
        _ => format!("0x{:02X}", node.subtype),
    }
}
```

(`*_name_or_raw` никогда не возвращает голых цифр — неизвестный код даёт `"Unknown 77h"`-формат, `names.rs:60-73`.)

- [ ] **Step 4: рендер** — `ui/tree.rs:35` заменить `format!("{} ", node.name)` на `format!("{} ", app.node_label(node))`.

- [ ] **Step 5: зелёный** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings`.

- [ ] **Step 6: commit** — `feat(uefi-tui): readable labels for unnamed nodes (image name, Volume, subtype fallback)`

---

### Task 3: (A3) engine — prune применённых операций + барьер-гард в ops

> **Rule-11 фикс (2026-09-11, по ходу исполнения):** обнаружены два дефекта шага.
> (1) Серверный ассерт `nodes.iter().all(|n| n.path != "0/0")` непроходим на собственной фикстуре задачи: после prune выживший файл смещается в индекс 0 и занимает путь "0/0" (пути позиционные). Ассерт заменён на «остался ровно один File-узел и это выживший (offset 88)».
> (2) Механизм `prune_applied` в `flush_image` порождает регрессию «воскресения» байтов: `build_volume`/`build_file` при `NoAction` сериализуют `header+body+tail` дословно (builder/mod.rs:52-57, 85-89), ops-мутации не синхронизируют `body` контейнеров — значит ЛЮБОЙ повторный flush после prune (поздний save или мутация в другом поддереве) emit'ит stale body и удалённый файл возвращается в записанные байты. До prune pending-маркеры Rebuild случайно защищали от этого. Корректный механизм: после успешного `atomic_write` заменять кэшированный образ re-parse'ом записанных байт (тот же путь, что `get_or_load_image` на cache-miss) — дерево и байты совпадают по построению. `prune_applied` как pub-API и его ops-тест исключаются; ensure_mutable/OpsError/сессионные ассерты остаются как были.

**Files:**
- Modify: `crates/uefi-engine/src/ops.rs` (`OpsError`, гард в `remove`/`replace`/`rebuild`/`insert`)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`flush_image`: re-parse кэша после успешного `atomic_write`)
- Test: оба файла, `mod tests`

**Interfaces:**
- Consumes: `Action`, `FfsType`, `ParsingData::GuidedSection`, `crate::ffs::is_recompressable_lzma_guid`.
- Produces: `OpsError::MutationBehindCompression` (RPC-маппинг: `Status::failed_precondition`, добавить в обработку ошибок мутационных хендлеров — сегодня они маппят `OpsError` через `Status::internal`; заменить на матч по вариантам); контракт `flush_image` — после записи кэш-образ == parse(записанные байты).

- [ ] **Step 1: падающий тест в `ops.rs`**:

```rust
#[test]
fn remove_behind_tiano_compression_refused() {
    let buf = make_simple_image();
    let mut img = parse_image(&buf, ImageMode::Write, "i", "s").unwrap();
    let mut file = parse_ffs_bytes(&make_ffs_file()).unwrap();
    let mut inner = FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: 0x19,
        offset: 0,
        header: vec![0u8; 4],
        body: vec![0xAA; 8],
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    };
    inner.action = Action::Rebuild;
    file.children.push(FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: EFI_SECTION_COMPRESSION,
        offset: 0,
        header: vec![0u8; 4],
        body: vec![0xBB; 16],
        tail: vec![],
        children: vec![inner],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    });
    img.root.children[0].children.push(file);
    let t = parse_target("0/0/0/0").unwrap();
    assert!(matches!(
        remove(&mut img.root, &t),
        Err(OpsError::MutationBehindCompression)
    ));
}
```

(`EFI_SECTION_COMPRESSION` импортировать из `crate::ffs` в тест-модуль.)

- [ ] **Step 2: красный** — `cargo test -p uefi-engine remove_behind` → FAIL.

- [ ] **Step 3: реализация в `ops.rs`** (prune_applied исключён rule-11-фиксом — см. шапку задачи):

```rust
#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("not found")]
    NotFound,
    #[error("invalid parent")]
    InvalidParent,
    #[error("invalid FFS data")]
    InvalidFfs,
    #[error("mutation behind non-recompressable compression barrier")]
    MutationBehindCompression,
}

fn ensure_mutable(root: &FfsNode, path: &[usize], include_target: bool) -> Result<(), OpsError> {
    let mut node = root;
    let n = if include_target {
        path.len()
    } else {
        path.len().saturating_sub(1)
    };
    for &i in &path[..n] {
        let Some(child) = node.children.get(i) else {
            break;
        };
        if child.node_type == FfsType::Section && child.subtype == EFI_SECTION_COMPRESSION {
            return Err(OpsError::MutationBehindCompression);
        }
        if child.node_type == FfsType::Section
            && child.subtype == EFI_SECTION_GUID_DEFINED
            && !matches!(&child.parsing_data, ParsingData::GuidedSection(d)
                if crate::ffs::is_recompressable_lzma_guid(&d.guid))
        {
            return Err(OpsError::MutationBehindCompression);
        }
        node = child;
    }
    Ok(())
}
```

Вызовы: в `remove`, `replace`, `rebuild` — после `target_path` и до мутации: `ensure_mutable(root, &path, true)?;`. В `insert`: для `InsertMode::Into` — `ensure_mutable(root, &parent_path, true)?;`; для Before/After — `ensure_mutable(root, &parent_path, false)?;`.

- [ ] **Step 4: зелёный ops** — `cargo test -p uefi-tui` не трогаем; `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`.

- [ ] **Step 5: server-интеграция** — в `rpc/server.rs::flush_image` после успешного `atomic_write(...)` и до `touch_image` заменить кэшированный образ re-parse'ом записанных байт (rule-11-фикс: честность байтов, см. шапку задачи):

```rust
let mut images = self.images.lock().await;
if let Some(img) = images.get_mut(image_id) {
    *img = parse_image(&bytes, img.mode, image_id, &img.session_id)
        .map_err(|e| Status::internal(e.to_string()))?;
}
drop(images);
```

(поля `img.mode`/`img.session_id` — фактические имена полей `Image` в types.rs; `bytes` — буфер, отданный в `atomic_write`. Если parse упал — образ уже записан корректно, но кэш протух: вернуть internal и инвалидировать (`images.remove(image_id)`) — следующий `get_or_load_image` перепарсит с диска.)

Маппинг ошибок ops в мутационных хендлерах (`image_node_insert/remove/replace/rebuild`): заменить `Status::internal(e.to_string())` на хелпер над `crate::ops::OpsError`:

```rust
fn ops_error_status(e: crate::ops::OpsError) -> Status {
    match e {
        crate::ops::OpsError::MutationBehindCompression => Status::failed_precondition(e.to_string()),
        _ => Status::internal(e.to_string()),
    }
}
```

Тест в `mod tests` файла `server.rs` (стенд `setup()` уже есть). ВАЖНО: удаляем **файл внутри FV** (`0/0`), не том — удаление тома уменьшает вывод, и `flush_image` правомерно откажет по data-loss-гварду (`server.rs:87-97`); файл-удаление пересобирает FV с паддингом до прежнего размера. Фикстура — FV с одним файлом внутри:

```rust
fn fv_image_with_two_files() -> Vec<u8> {
    let mut buf = vec![0xFFu8; 256];
    buf[32..40].copy_from_slice(&256u64.to_le_bytes());
    buf[40..44].copy_from_slice(&crate::ffs::EFI_FVH_SIGNATURE.to_le_bytes());
    buf[44..48].copy_from_slice(&crate::ffs::EFI_FVB2_ERASE_POLARITY.to_le_bytes());
    buf[48..50].copy_from_slice(&56u16.to_le_bytes());
    buf[55] = 2;
    let guid = uuid::Uuid::new_v4();
    for (k, at) in [56usize, 88].iter().enumerate() {
        let mut f = vec![0u8; 32];
        f[0..16].copy_from_slice(guid.as_bytes());
        f[18] = 0x01;
        f[20..23].copy_from_slice(&crate::ffs::size_to_uint24(32));
        f[24] = [0xAA, 0xBB][k];
        buf[*at..*at + 32].copy_from_slice(&f);
    }
    buf
}

#[tokio::test]
async fn remove_rpc_leaves_clean_tree() {
    let (_td, mut client) = setup().await;
    let sid = create_session(&mut client).await;
    let dir = _td.path().join("src.bin");
    std::fs::write(&dir, fv_image_with_two_files()).unwrap();
    let open = client
        .image_open(tonic::Request::new(ImageOpenRequest {
            session_id: sid.clone(),
            path: dir.display().to_string(),
            mode: 1,
            name: "t".into(),
        }))
        .await
        .unwrap()
        .into_inner();
    client
        .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
            image_id: open.image_id.clone(),
            target: "0/0".into(),
        }))
        .await
        .unwrap();
    let nodes = client
        .image_nodes_list(tonic::Request::new(ImageNodesListRequest {
            image_id: open.image_id.clone(),
            filter: String::new(),
        }))
        .await
        .unwrap()
        .into_inner()
        .nodes;
    assert!(
        nodes.iter().all(|n| n.action == 50),
        "no pending markers after write-through flush"
    );
    let files: Vec<&_> = nodes.iter().filter(|n| n.node_type == 66).collect();
    assert_eq!(files.len(), 1, "removed file must disappear from the tree");
    assert_eq!(files[0].offset, 88, "survivor is the second fixture file");
}
```

(Rule-11-фикс ассерта: пути позиционные — после удаления выживший файл занимает "0/0"; проверяем «остался один File и это выживший».)

Дополнительный гейт воскрешения (rule-11-фикс №2 — регрессия stale-body, которую prune не ловил): после `remove "0/0"` повторно сохранить образ (мутация в другом поддереве или явный save — что даёт второй flush) и проверить записанные байты уровня данных, а не дерева:

```rust
#[tokio::test]
async fn second_flush_does_not_resurrect_removed_file() {
    // тот же стенд/фикстура, что remove_rpc_leaves_clean_tree;
    // файлы фикстуры несут разные маркеры: f[24]=0xAA (файл @56), f[25]=0xBB (файл @88)
    // 1) remove "0/0" (первый flush — FV ещё Rebuild, пишет корректно)
    // 2) триггер второго flush без мутаций в этом FV: подойдёт вторая мутация
    //    в другом поддереве (двух-FV фикстура, remove "1/0") ИЛИ повторный
    //    save/rebuild существующего RPC-путя — выбрать доступный на месте;
    //    маркер 0xAA из тела удалённого файла обязан отсутствовать в
    //    прочитанных с диска байтах образа (data_dir/sessions/<sid>/images/<iid>.bin).
}
```

(тело заполнить по фактическим RPC; суть — байтовый ассерт, не дерево: `!bytes_after.contains(&0xAA)` в зоне тела файла @56; с re-parse-фиксом проходит, со stale-body — падает.)

(`fv_image_with_two_files` кладёт два выровненных FFS-файла в тело FV @56 и @88; GUID-байты и `size_to_uint24` — по образцу `ops.rs::tests::make_ffs_file`; `create_session`-хелпер взять из существующего теста `flush_image_writes_bytes_to_data_dir` — там сессия создаётся через `sm.create_session`/RPC, переиспользовать тот же путь).

- [ ] **Step 6: зелёный всё** — `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`.

- [ ] **Step 7: commit** — `feat(uefi-engine): honest tree+bytes after flush (cache re-parse) + refuse mutations behind compression barrier`

---

### Task 4: (A4) TUI — FIND/GO

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (`goto_path`)
- Modify: `crates/uefi-tui/src/commands.rs` (`"goto" | "g"` в `execute_command`)
- Modify: `crates/uefi-tui/src/main.rs` (`handle_normal`: клавиша `/`)
- Test: `app.rs` `mod tests`, `commands.rs` `mod tests`

**Interfaces:**
- Consumes: `crate::tree::segments`, `App::visible`, `App::enter_insert_mode`.
- Produces: `App::goto_path(&mut self, target: &str) -> Result<(), String>`; ex-команда `:goto PATH`; `/`-промпт.

- [ ] **Step 1: падающие тесты в `app.rs`**:

```rust
#[test]
fn goto_path_expands_ancestors_and_moves_cursor() {
    let mut app = App::new();
    app.tree = vec![
        node("", 0),
        node("0", 1),
        node("0/0", 2),
        node("0/0/0", 3),
    ];
    app.cursor = 0;
    app.goto_path("0/0/0").unwrap();
    assert!(app.tree[1].expanded);
    assert!(app.tree[2].expanded);
    assert_eq!(app.selected_path().as_deref(), Some("0/0/0"));
}

#[test]
fn goto_path_accepts_leading_slash_and_errors_on_miss() {
    let mut app = App::new();
    app.tree = vec![node("", 0), node("0", 1)];
    app.goto_path("/0").unwrap();
    assert_eq!(app.selected_path().as_deref(), Some("0"));
    assert!(app.goto_path("9/9").is_err());
}
```

- [ ] **Step 2: красный** — `cargo test -p uefi-tui goto` → FAIL.

- [ ] **Step 3: реализация `App::goto_path`** (в `impl App`):

```rust
pub fn goto_path(&mut self, target: &str) -> Result<(), String> {
    let norm = target.trim_start_matches('/');
    let idx = self
        .tree
        .iter()
        .position(|n| n.path == norm)
        .ok_or_else(|| format!("no node at path {target}"))?;
    let want = crate::tree::segments(norm);
    for node in &mut self.tree {
        let segs = crate::tree::segments(&node.path);
        if segs.len() < want.len()
            && segs.iter().zip(want.iter()).all(|(a, b)| a == b)
            && node.has_children
        {
            node.expanded = true;
        }
    }
    let vis = self.visible();
    self.cursor = vis
        .iter()
        .position(|&v| v == idx)
        .ok_or("node hidden after expand")?;
    Ok(())
}
```

- [ ] **Step 4: ex-команда** — в `execute_command` (`commands.rs`), рядом с `"refresh"`:

```rust
"goto" | "g" => {
    let target = parts
        .get(1)
        .ok_or("usage: :goto PATH (e.g. 1/28/1)")?;
    app.goto_path(target)?;
    Ok(format!("→ {target}"))
}
```

`main.rs`, в `handle_normal` (до catch-all `_ => {}`):

```rust
AppEvent::Key('/') => {
    app.enter_insert_mode("goto", "goto ".into());
}
```

`enter_insert_mode` уже умеет prompt `goto>` через `insert_cmd` — проверить `ui/cmdline.rs` рендерит `{insert_cmd}> ` (да, из миграционного цикла).

- [ ] **Step 5: зелёный** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings`.

- [ ] **Step 6: commit** — `feat(uefi-tui): :goto PATH + /-prompt with ancestor auto-expand`

---

### Task 5: (A5) общий парсер флагов в uefi-common + tab-completion

**Files:**
- Create: `crates/uefi-common/src/cli.rs`
- Modify: `crates/uefi-common/src/lib.rs` (`pub mod cli;`)
- Modify: `crates/uefi-tui/src/commands.rs` (переход на общий парсер; `complete`)
- Modify: `crates/uefi-tui/src/input.rs` (`AppEvent::Tab`)
- Modify: `crates/uefi-tui/src/main.rs` (`handle_command`: Tab)
- Test: `cli.rs` `mod tests`, `commands.rs` `mod tests`

**Interfaces:**
- Produces:
  - `uefi_common::cli::{NodeCmdArgs, Source}`; `NodeCmdArgs::source(&self) -> Result<Option<Source>, String>`; `parse_node_flags(parts: &[&str]) -> NodeCmdArgs`.
  - `uefi_tui::commands::complete(app: &App, cmdline: &str) -> (Option<String>, Vec<String>)` — `replacement` = новая cmdline целиком, `options` = варианты для status_msg.
  - `AppEvent::Tab`.

- [ ] **Step 1: module-first + тесты общего парсера** — создать `crates/uefi-common/src/cli.rs`:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeCmdArgs {
    pub target: Option<String>,
    pub file: Option<String>,
    pub artifact_id: Option<String>,
    pub mode: Option<String>,
    pub body_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    File,
    Artifact,
}

impl NodeCmdArgs {
    pub fn source(&self) -> Result<Option<Source>, String> {
        match (&self.file, &self.artifact_id) {
            (Some(_), None) => Ok(Some(Source::File)),
            (None, Some(_)) => Ok(Some(Source::Artifact)),
            (None, None) => Ok(None),
            (Some(_), Some(_)) => {
                Err("exactly one of --file / --artifact-id is required".into())
            }
        }
    }
}

pub fn parse_node_flags(parts: &[&str]) -> NodeCmdArgs {
    let mut a = NodeCmdArgs::default();
    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "--file" => {
                a.file = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--artifact-id" => {
                a.artifact_id = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--mode" => {
                a.mode = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--body-only" => {
                a.body_only = true;
                i += 1;
            }
            other if !other.starts_with("--") && a.target.is_none() => {
                a.target = Some(other.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_and_target() {
        let a = parse_node_flags(&["insert", "0/3", "--file", "/tmp/x.bin", "--mode", "after"]);
        assert_eq!(a.target.as_deref(), Some("0/3"));
        assert_eq!(a.file.as_deref(), Some("/tmp/x.bin"));
        assert_eq!(a.mode.as_deref(), Some("after"));
        assert_eq!(a.source().unwrap(), Some(Source::File));
    }

    #[test]
    fn both_sources_rejected() {
        let a = parse_node_flags(&["insert", "--file", "a", "--artifact-id", "b"]);
        assert!(a.source().is_err());
    }

    #[test]
    fn none_source_ok_and_body_only() {
        let a = parse_node_flags(&["replace", "0/3", "--body-only"]);
        assert_eq!(a.source().unwrap(), None);
        assert!(a.body_only);
    }
}
```

В `lib.rs` в ТОМ ЖЕ ШАГЕ добавить `pub mod cli;`. Красный → зелёный: `cargo test -p uefi-common`.

- [ ] **Step 2: TUI переходит на общий парсер** — в `commands.rs`: удалить локальные `NodeCmdArgs` и `parse_node_cmd_args`, добавить `use uefi_common::cli::{parse_node_flags, NodeCmdArgs, Source};`. Веткам `"insert"`/`"replace"` заменить самодельные проверки `(Some(p), None)/(None, Some(id))` на:

```rust
match a.source() {
    Ok(Some(Source::File)) => { /* существующий --file путь */ }
    Ok(Some(Source::Artifact)) => { /* существующий --artifact-id путь */ }
    Ok(None) => return Err("exactly one of --file / --artifact-id is required".into()),
    Err(e) => return Err(e),
}
```

Существующие тесты `parse_node_cmd_target_and_flags` и соседи переписать на `parse_node_flags` (переехали в uefi-common; в TUI оставить только поведенческие тесты insert/replace если есть).

- [ ] **Step 3: тесты completion** (в `commands.rs` `mod tests`; `App` с деревом/registry из фикстур):

```rust
#[test]
fn complete_first_token_to_unique_command() {
    let app = crate::app::App::new();
    let (rep, opts) = complete(&app, "rebui");
    assert_eq!(rep.as_deref(), Some("rebuild"));
    assert!(opts.is_empty());
}

#[test]
fn complete_artifact_ids_after_flag() {
    let mut app = crate::app::App::new();
    app.registry.artifacts = vec![
        uefi_proto::ArtifactInfo { artifact_id: "art-1".into(), ..Default::default() },
        uefi_proto::ArtifactInfo { artifact_id: "art-2".into(), ..Default::default() },
    ];
    let (rep, _) = complete(&app, "insert 0/3 --artifact-id art-");
    assert_eq!(rep.as_deref(), Some("insert 0/3 --artifact-id art-"));
    let (_, opts) = complete(&app, "insert 0/3 --artifact-id ");
    assert_eq!(opts, vec!["art-1".to_string(), "art-2".to_string()]);
}

#[test]
fn complete_flags_of_insert() {
    let app = crate::app::App::new();
    let (_, opts) = complete(&app, "insert 0/3 --");
    assert_eq!(opts, vec!["--file".into(), "--artifact-id".into(), "--mode".into()]);
}

#[test]
fn complete_target_from_visible_tree() {
    let mut app = crate::app::App::new();
    let mk = |path: &str, node_type: u8| crate::app::TreeNode {
        path: path.into(),
        depth: 1,
        node_type,
        subtype: 0,
        guid: None,
        name: String::new(),
        action: crate::theme::ACTION_NO,
        expanded: true,
        has_children: false,
    };
    app.tree = vec![mk("1", 65), mk("1/28", 66)];
    let (rep, _) = complete(&app, "remove 1/2");
    assert_eq!(rep.as_deref(), Some("remove 1/28"));
}
```

(после Task 8 у `TreeNode` появится поле `region` — литерал `mk` дополнить `region: String::new()` при переходе; `ArtifactInfo` реализует `Default` — прецедент `app.rs:379`.)

- [ ] **Step 4: красный** → реализация в `commands.rs`:

```rust
const COMMANDS: &[&str] = &[
    "open", "o", "save", "s", "extract", "export", "import", "artifacts", "insert",
    "replace", "remove", "rebuild", "image", "refresh", "goto", "g", "upload",
    "snapshot", "snapshots", "restore", "help", "h", "quit", "q",
];

pub fn complete(app: &App, cmdline: &str) -> (Option<String>, Vec<String>) {
    let ends_space = cmdline.ends_with(' ');
    let mut parts: Vec<&str> = cmdline.split_whitespace().collect();
    let token = if ends_space || parts.is_empty() {
        String::new()
    } else {
        parts.pop().unwrap().to_string()
    };
    let head: Vec<&str> = parts.clone();
    let candidates: Vec<String> = if head.is_empty() {
        COMMANDS
            .iter()
            .filter(|c| c.starts_with(token.as_str()))
            .map(|s| s.to_string())
            .collect()
    } else {
        context_candidates(app, head[0], &head, &token)
    };
    if candidates.is_empty() {
        return (None, vec![]);
    }
    let prefix = common_prefix(&candidates);
    let mut out = head.join(" ");
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(&prefix);
    let opts = if candidates.len() > 1 { candidates } else { vec![] };
    (Some(out), opts)
}

fn common_prefix(items: &[String]) -> String {
    let mut p = items[0].clone();
    for s in items {
        p.truncate(p.chars().zip(s.chars()).take_while(|(a, b)| a == b).count());
    }
    p
}

fn context_candidates(app: &App, cmd: &str, head: &[&str], token: &str) -> Vec<String> {
    if head.last() == Some(&"--mode") {
        return ["into", "before", "after"]
            .iter()
            .filter(|c| c.starts_with(token))
            .map(|s| s.to_string())
            .collect();
    }
    if head.last() == Some(&"--artifact-id") {
        return app
            .registry
            .artifacts
            .iter()
            .map(|a| a.artifact_id.clone())
            .filter(|c| c.starts_with(token))
            .collect();
    }
    if token.starts_with("--") {
        let flags: &[&str] = match cmd {
            "insert" => &["--file", "--artifact-id", "--mode"],
            "replace" => &["--file", "--artifact-id", "--body-only"],
            _ => &[],
        };
        return flags
            .iter()
            .filter(|c| c.starts_with(token))
            .map(|s| s.to_string())
            .collect();
    }
    let has_positional = head[1..]
        .iter()
        .any(|s| !s.starts_with("--"));
    let target_cmds = ["insert", "replace", "remove", "rebuild", "extract", "goto", "restore"];
    if target_cmds.contains(&cmd) && !has_positional {
        return app
            .visible()
            .iter()
            .map(|&i| app.tree[i].path.clone())
            .filter(|p| p.starts_with(token))
            .collect();
    }
    vec![]
}
```

(Позиционный фильтр упростить при реализации до «первый non-flag после cmd», если полная форма шума даст; тесты — критерий.)

> **Rule-11 фикс (2026-09-11, по ходу исполнения):** исходный хвост `complete` (`if prefix.len() > token.len() … else (None, candidates)`) противоречил собственным тестам задачи: `complete_artifact_ids_after_flag` ожидает `Some(...)` при неоднозначных кандидатах (общий префикс == токену) и непустые `opts` при пустом токене. Контракт по тестам: уникальный кандидат → `(Some(полный), [])`; неоднозначные → `(Some(общий префикс), кандидаты)` — вызывающий применяет префикс И показывает варианты. Хендлер `main.rs` соответственно `if`/`if` (не `else if`).

`input.rs`: добавить вариант `Tab` в `AppEvent` и `KeyCode::Tab => AppEvent::Tab`. `main.rs::handle_command`:

```rust
AppEvent::Tab => {
    let (rep, opts) = commands::complete(app, &app.cmdline.clone());
    if let Some(r) = rep {
        app.cmdline = r;
    }
    if !opts.is_empty() {
        app.status_msg = format!("options: {}", opts.join(" "));
    }
}
```

- [ ] **Step 5: зелёный** — `cargo test -p uefi-common -p uefi-tui && cargo clippy -p uefi-common -p uefi-tui -- -D warnings`.

- [ ] **Step 6: commit** — `feat(uefi-common,uefi-tui): shared parse_node_flags + cmdline tab-completion`

---

### Task 6: (A6a) engine — flash-дескриптор, регионы FLREG, Region-узлы

**Files:**
- Modify: `crates/uefi-engine/src/types.rs` (`FlashRegionKind`, `RegionParsingData`, вариант `ParsingData::Region`)
- Create: `crates/uefi-engine/src/parser/region.rs`
- Modify: `crates/uefi-engine/src/parser/mod.rs` (`pub mod region;`)
- Modify: `crates/uefi-engine/src/parser/image.rs` (`parse_image`: дескрипторный путь; `node_name`: Region-ветка)
- Modify: `crates/uefi-engine/src/builder/mod.rs:33-46` (Region → raw-body arm)
- Modify: `crates/uefi-engine/src/ops.rs` (`OpsError::ImmutableRegion` + проверка в `ensure_mutable`)
- Test: `parser/region.rs` `mod tests`, `parser/image.rs` `mod tests`, `builder/mod.rs` tests

**Interfaces:**
- Produces:
  - `types::FlashRegionKind` (16 вариантов, `from_flreg_index(usize) -> Option<Self>`, `label() -> &'static str`: "Descriptor", "BIOS", "ME", "GbE", "PDR", "Dev Expansion 1", "Secondary BIOS", "Microcode", "EC", "Dev Expansion 2", "IE", "10GbE 1", "10GbE 2", "Reserved 1", "Reserved 2", "PTT").
  - `types::RegionParsingData { pub kind: FlashRegionKind }`; `ParsingData::Region(RegionParsingData)`.
  - `parser::region::parse_flash_regions(buf: &[u8]) -> Option<Vec<FlashRegion>>`; `FlashRegion { kind, offset: usize, size: usize }`.
  - `parser::region::make_region_node(buf: &[u8], kind: FlashRegionKind, offset: usize, size: usize) -> FfsNode`.
- Consumes (Task 7): `make_region_node` для $FPT-партиций; `parse_fpt`.

- [ ] **Step 1: types** — в `types.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashRegionKind {
    Descriptor,
    Bios,
    Me,
    Gbe,
    Pdr,
    DevExpansion1,
    SecondaryBios,
    Microcode,
    Ec,
    DevExpansion2,
    Ie,
    Tgbe1,
    Tgbe2,
    Reserved1,
    Reserved2,
    Ptt,
}

impl FlashRegionKind {
    pub fn from_flreg_index(i: usize) -> Option<Self> {
        Some(match i {
            0 => Self::Descriptor,
            1 => Self::Bios,
            2 => Self::Me,
            3 => Self::Gbe,
            4 => Self::Pdr,
            5 => Self::DevExpansion1,
            6 => Self::SecondaryBios,
            7 => Self::Microcode,
            8 => Self::Ec,
            9 => Self::DevExpansion2,
            10 => Self::Ie,
            11 => Self::Tgbe1,
            12 => Self::Tgbe2,
            13 => Self::Reserved1,
            14 => Self::Reserved2,
            15 => Self::Ptt,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Descriptor => "Descriptor",
            Self::Bios => "BIOS",
            Self::Me => "ME",
            Self::Gbe => "GbE",
            Self::Pdr => "PDR",
            Self::DevExpansion1 => "Dev Expansion 1",
            Self::SecondaryBios => "Secondary BIOS",
            Self::Microcode => "Microcode",
            Self::Ec => "EC",
            Self::DevExpansion2 => "Dev Expansion 2",
            Self::Ie => "IE",
            Self::Tgbe1 => "10GbE 1",
            Self::Tgbe2 => "10GbE 2",
            Self::Reserved1 => "Reserved 1",
            Self::Reserved2 => "Reserved 2",
            Self::Ptt => "PTT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegionParsingData {
    pub kind: FlashRegionKind,
}
```

и вариант в `enum ParsingData`: `Region(RegionParsingData),`.

- [ ] **Step 2: module-first + парсер** — создать `parser/region.rs`, в `parser/mod.rs` ТОТ ЖЕ ШАГ добавить `pub mod region;`:

```rust
use crate::types::{FfsNode, FlashRegionKind, RegionParsingData, Action, FfsType, ParsingData};

pub const FLASH_DESCRIPTOR_SIGNATURE: u32 = 0x0FF0A55A;
pub const FLASH_DESCRIPTOR_SIZE: usize = 0x1000;
const FLMAP0_OFFSET: usize = 0x10;
const REGION_SECTION_ALIGN: usize = 0x10;
const REGION_GRANULARITY: usize = 0x1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashRegion {
    pub kind: FlashRegionKind,
    pub offset: usize,
    pub size: usize,
}

pub fn parse_flash_regions(buf: &[u8]) -> Option<Vec<FlashRegion>> {
    if buf.len() < FLASH_DESCRIPTOR_SIZE {
        return None;
    }
    if u32::from_le_bytes(buf[0..4].try_into().unwrap()) != FLASH_DESCRIPTOR_SIGNATURE {
        return None;
    }
    let flmap0 = u32::from_le_bytes(buf[FLMAP0_OFFSET..FLMAP0_OFFSET + 4].try_into().unwrap());
    let region_base = ((flmap0 >> 16) & 0xFF) as usize * REGION_SECTION_ALIGN;
    if region_base + 64 > FLASH_DESCRIPTOR_SIZE {
        return None;
    }
    let mut out = vec![FlashRegion {
        kind: FlashRegionKind::Descriptor,
        offset: 0,
        size: FLASH_DESCRIPTOR_SIZE,
    }];
    for i in 1..=15usize {
        let base = u16::from_le_bytes(
            buf[region_base + i * 4..region_base + i * 4 + 2].try_into().unwrap(),
        );
        let limit = u16::from_le_bytes(
            buf[region_base + i * 4 + 2..region_base + i * 4 + 4].try_into().unwrap(),
        );
        if limit == 0 || (base == 0xFFFF && limit == 0xFFFF) {
            continue;
        }
        let Some(kind) = FlashRegionKind::from_flreg_index(i) else {
            continue;
        };
        let offset = base as usize * REGION_GRANULARITY;
        let size = (limit as usize + 1 - base as usize) * REGION_GRANULARITY;
        if offset + size > buf.len() {
            tracing::warn!(kind = kind.label(), "region outside image, skipped");
            continue;
        }
        out.push(FlashRegion { kind, offset, size });
    }
    if !out.iter().any(|r| r.kind == FlashRegionKind::Bios) {
        tracing::warn!("descriptor has no BIOS region, ignoring descriptor");
        return None;
    }
    Some(out)
}

pub fn make_region_node(
    buf: &[u8],
    kind: FlashRegionKind,
    offset: usize,
    size: usize,
) -> FfsNode {
    FfsNode {
        guid: None,
        node_type: FfsType::Region,
        subtype: 0,
        offset: offset as u32,
        header: vec![],
        body: buf[offset..offset + size].to_vec(),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::Region(RegionParsingData { kind }),
        fixed: true,
        compressed: false,
        alignment_bytes: vec![],
    }
}
```

- [ ] **Step 3: тесты региона-парсера** (в `region.rs` `mod tests`; фикстура-конструктор дескриптора):

```rust
fn descriptor_image(region_specs: &[(usize, u16, u16)], total: usize) -> Vec<u8> {
    let mut buf = vec![0xFFu8; total];
    buf[0..4].copy_from_slice(&super::FLASH_DESCRIPTOR_SIGNATURE.to_le_bytes());
    buf[FLMAP0_OFFSET..FLMAP0_OFFSET + 4]
        .copy_from_slice(&0x0040_0000u32.to_le_bytes());
    for &(i, base, limit) in region_specs {
        let at = 0x400 + i * 4;
        buf[at..at + 2].copy_from_slice(&base.to_le_bytes());
        buf[at + 2..at + 4].copy_from_slice(&limit.to_le_bytes());
    }
    buf
}

#[test]
fn parse_flash_regions_layout() {
    let buf = descriptor_image(&[(1, 4, 7), (2, 1, 3), (3, 8, 8)], 0x10000);
    let rs = parse_flash_regions(&buf).unwrap();
    assert_eq!(rs.len(), 4);
    assert_eq!(rs[0], FlashRegion { kind: FlashRegionKind::Descriptor, offset: 0, size: 0x1000 });
    assert_eq!(rs[1], FlashRegion { kind: FlashRegionKind::Bios, offset: 0x4000, size: 0x4000 });
    assert_eq!(rs[2], FlashRegion { kind: FlashRegionKind::Me, offset: 0x1000, size: 0x3000 });
    assert_eq!(rs[3], FlashRegion { kind: FlashRegionKind::Gbe, offset: 0x8000, size: 0x1000 });
}

#[test]
fn no_signature_returns_none() {
    assert!(parse_flash_regions(&[0u8; 0x2000]).is_none());
    assert!(parse_flash_regions(&[0xFFu8; 0x2000]).is_none());
}

#[test]
fn region_outside_image_skipped() {
    let buf = descriptor_image(&[(1, 0, 3), (2, 0xF000, 0xFFFF)], 0x10000);
    let rs = parse_flash_regions(&buf).unwrap();
    assert!(rs.iter().all(|r| r.kind != FlashRegionKind::Me));
}
```

Красный → зелёный: `cargo test -p uefi-engine region`.

- [ ] **Step 4: интеграция в `parse_image`** — дескрипторный путь перед существующим сканом (существующий цикл скана вынести в хелпер `scan_volumes(buf, window: Range<usize>, children: &mut Vec<FfsNode>, last_end: &mut usize)` без изменения поведения; не-дескрипторный путь вызывает его с `0..buf.len()`):

```rust
pub fn parse_image(...) -> Result<Image, ParserError> {
    let mut children = vec![];
    if let Some(regions) = super::region::parse_flash_regions(buf) {
        let mut last_end = 0usize;
        for r in &regions {
            if r.offset > last_end {
                children.push(make_padding_node(buf, last_end, r.offset));
            }
            match r.kind {
                FlashRegionKind::Bios => {
                    scan_volumes(buf, r.offset..r.offset + r.size, &mut children, &mut last_end);
                }
                _ => {
                    children.push(super::region::make_region_node(buf, r.kind, r.offset, r.size));
                    last_end = r.offset + r.size;
                }
            }
        }
        if buf.len() > last_end {
            children.push(make_padding_node(buf, last_end, buf.len()));
        }
    } else {
        let mut last_end = 0usize;
        scan_volumes(buf, 0..buf.len(), &mut children, &mut last_end);
    }
    ...
}
```

`scan_volumes` — тело сегодняшнего while-цикла, но границы `off + 44 <= window.end`, старт `off = window.start`, паддинги только внутри окна. `use crate::types::FlashRegionKind;` в `image.rs`. `node_name` — добавить ветку:

```rust
FfsType::Region => match &node.parsing_data {
    ParsingData::Region(rd) => format!("{} region", rd.kind.label()),
    _ => String::new(),
},
```

Тесты в `image.rs`:

```rust
#[test]
fn parse_image_descriptor_path_regions_and_padding() {
    use crate::parser::region::FlashRegion;
    use crate::types::FlashRegionKind;
    let mut buf = vec![0xFFu8; 0x10000];
    buf[0..4].copy_from_slice(&0x0FF0_A55Au32.to_le_bytes());
    buf[0x10..0x14].copy_from_slice(&0x0040_0000u32.to_le_bytes());
    buf[0x400 + 1 * 4..0x400 + 1 * 4 + 2].copy_from_slice(&1u16.to_le_bytes());
    buf[0x400 + 1 * 4 + 2..0x400 + 1 * 4 + 4].copy_from_slice(&3u16.to_le_bytes());
    buf[0x400 + 2 * 4..0x400 + 2 * 4 + 2].copy_from_slice(&4u16.to_le_bytes());
    buf[0x400 + 2 * 4 + 2..0x400 + 2 * 4 + 4].copy_from_slice(&15u16.to_le_bytes());
    let img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
    let kinds: Vec<&str> = img
        .root
        .children
        .iter()
        .filter(|c| c.node_type == FfsType::Region)
        .map(|c| match &c.parsing_data {
            ParsingData::Region(rd) => rd.kind.label(),
            _ => "",
        })
        .collect();
    assert!(kinds.contains(&"Descriptor"));
    assert!(kinds.contains(&"ME"));
    let bios_volumes = img
        .root
        .children
        .iter()
        .filter(|c| c.node_type == FfsType::Volume)
        .count();
    assert!(bios_volumes >= 1, "BIOS window scanned for FVs");
    let rebuilt = crate::builder::build_image(&img).unwrap();
    assert_eq!(rebuilt, buf, "descriptor round-trip byte-identical");
}
```

- [ ] **Step 5: builder + ops** — `builder/mod.rs` arm:

```rust
FfsType::Image | FfsType::Capsule | FfsType::Root => {
    for child in &node.children {
        build_node(child, out)?;
    }
}
FfsType::Padding | FfsType::FreeSpace | FfsType::Region => out.extend_from_slice(&node.body),
```

(дети Region-узлов не сериализуются — тело уже покрывает весь диапазон; ME-партиции Task 7 добавит как детей, контракт не меняется).

`ops.rs`: `OpsError` добавить `#[error("flash region is read-only")] ImmutableRegion`; в `ensure_mutable` первой проверкой внутри цикла:

```rust
if child.node_type == FfsType::Region {
    return Err(OpsError::ImmutableRegion);
}
```

и в `ops_error_status` (server.rs, Task 3): `ImmutableRegion => Status::failed_precondition(...)`. Тесты:

```rust
#[test]
fn region_nodes_immutable() {
    let buf = descriptor_image(&[(1, 4, 7), (2, 1, 3)], 0x10000);
    let mut img = parse_image(&buf, ImageMode::Write, "i", "s").unwrap();
    let me_idx = img
        .root
        .children
        .iter()
        .position(|c| {
            matches!(
                &c.parsing_data,
                ParsingData::Region(rd) if rd.kind == FlashRegionKind::Me
            )
        })
        .unwrap();
    let t = parse_target(&me_idx.to_string()).unwrap();
    assert!(matches!(
        remove(&mut img.root, &t),
        Err(OpsError::ImmutableRegion)
    ));
}
```

(`descriptor_image` — фикстура из Step 3 тестов `region.rs`; здесь нужен её дубль или переиспользование через `crate::parser::region::tests`-экспорт — на месте задачи решить, где жить хелперу; главное — образ, где ME-регион реально распознан. Удаление пути ВНУТРИ региона (`me_idx/0`) проверяется в Task 7 — детям ME ($FPT-партициям) даёт смысл только он.)

- [ ] **Step 6: зелёный всё** — `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`. Существующие round-trip/padding-тесты обязаны остаться зелёными (не-дескрипторный путь не меняется).

- [ ] **Step 7: commit** — `feat(uefi-engine): flash descriptor regions (full FLREG table) as Region nodes, immutable, raw round-trip`

---

### Task 7: (A6b) engine — $FPT-партиции ME-региона

**Files:**
- Modify: `crates/uefi-engine/src/parser/region.rs` (`parse_fpt`, подключение в `make_region_node`-путь ME)
- Test: `region.rs` `mod tests`

**Interfaces:**
- Produces: `pub struct FptPartition { pub name: String, pub offset: u32, pub size: u32 }`; `pub fn parse_fpt(body: &[u8]) -> Option<Vec<FptPartition>>`; `pub fn fpt_children(buf: &[u8], region_offset: usize, region_size: usize) -> Vec<FfsNode>` (Region-узлы партиций, `RegionParsingData` с kind от родителя не подходит — для партиций `node_name` должен дать имя партиции; поэтому партиции получают `ParsingData::None` и имя через отдельный контракт — см. реализацию ниже: имя партиции кладётся в `node_name`-ветку по `subtype`? Нет: добавить `FfsNode`-независимый путь — партиции получают `guid: None`, `subtype: 0` и имя резолвится в `node_name` через `ParsingData::FptPartition(FptParsingData { name: String })`. Итог: `types::FptParsingData { pub name: String }` + вариант `ParsingData::FptPartition(FptParsingData)`; `node_name`-ветка `FfsType::Region` отдаёт `rd.kind.label()` для региона и `name.clone()` для партиции.)

- [ ] **Step 1: types + тесты** — `types.rs`:

```rust
#[derive(Debug, Clone)]
pub struct FptParsingData {
    pub name: String,
}
```

вариант `ParsingData::FptPartition(FptParsingData)`.

Тесты в `region.rs`:

```rust
fn me_body_with_fpt() -> Vec<u8> {
    let mut body = vec![0u8; 0x2000];
    body[0..4].copy_from_slice(&0x5450_4624u32.to_le_bytes());
    body[4..8].copy_from_slice(&2u32.to_le_bytes());
    body[8] = 0x10;
    body[10] = 0x20;
    body[0x20..0x24].copy_from_slice(b"FTPR");
    body[0x28..0x2C].copy_from_slice(&0x0000u32.to_le_bytes());
    body[0x2C..0x30].copy_from_slice(&0x0800u32.to_le_bytes());
    body[0x40..0x44].copy_from_slice(b"NFTP");
    body[0x48..0x4C].copy_from_slice(&0x1000u32.to_le_bytes());
    body[0x4C..0x50].copy_from_slice(&0x0400u32.to_le_bytes());
    body
}

#[test]
fn parse_fpt_entries() {
    let parts = parse_fpt(&me_body_with_fpt()).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "FTPR");
    assert_eq!(parts[0].offset, 0);
    assert_eq!(parts[0].size, 0x800);
    assert_eq!(parts[1].name, "NFTP");
    assert_eq!(parts[1].offset, 0x1000);
}

#[test]
fn parse_fpt_bypass_vector_offset() {
    let mut body = me_body_with_fpt();
    body.copy_within(0..0x40, 0x10);
    body[0..4].copy_from_slice(&0u32.to_le_bytes());
    assert!(parse_fpt(&body).is_some());
}

#[test]
fn parse_fpt_graceful_on_garbage() {
    assert!(parse_fpt(&[0xFFu8; 0x2000]).is_none());
    let mut body = me_body_with_fpt();
    body[4..8].copy_from_slice(&1000u32.to_le_bytes());
    assert!(parse_fpt(&body).is_none());
}

#[test]
fn me_region_gets_fpt_children() {
    let children = fpt_children(&me_body_with_fpt(), 0, 0x2000);
    assert_eq!(children.len(), 2);
    assert!(children.iter().all(|c| c.node_type == FfsType::Region));
    assert!(children.iter().all(|c| matches!(&c.parsing_data, ParsingData::FptPartition(_))));
}

#[test]
fn fpt_partition_inside_me_immutable() {
    let mut buf = descriptor_image(&[(1, 4, 7), (2, 1, 3)], 0x10000);
    let me = me_body_with_fpt();
    buf[0x1000..0x1000 + me.len()].copy_from_slice(&me);
    let mut img = parse_image(&buf, ImageMode::Write, "i", "s").unwrap();
    let me_idx = img
        .root
        .children
        .iter()
        .position(|c| {
            matches!(
                &c.parsing_data,
                ParsingData::Region(rd) if rd.kind == FlashRegionKind::Me
            )
        })
        .unwrap();
    assert!(!img.root.children[me_idx].children.is_empty(), "ME has $FPT children");
    let inside = format!("{me_idx}/0");
    assert!(matches!(
        crate::ops::remove(&mut img.root, &crate::parser::target::parse_target(&inside).unwrap()),
        Err(crate::ops::OpsError::ImmutableRegion)
    ));
}
```

- [ ] **Step 2: красный** — `cargo test -p uefi-engine fpt` → FAIL.

- [ ] **Step 3: реализация** (в `region.rs`; обход — референс `meparser.cpp:138-170`, структуры `me.h:38-87`):

```rust
pub const FPT_SIGNATURE: u32 = 0x5450_4624;
const FPT_MAX_ENTRIES: u32 = 32;
const FPT_HEADER_LEN: usize = 0x20;
const FPT_ENTRY_LEN: usize = 0x20;
const ME_ROM_BYPASS_VECTOR_SIZE: usize = 0x10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FptPartition {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

pub fn parse_fpt(body: &[u8]) -> Option<Vec<FptPartition>> {
    let base = if body.len() >= 4
        && u32::from_le_bytes(body[0..4].try_into().unwrap()) == FPT_SIGNATURE
    {
        0
    } else if body.len() >= ME_ROM_BYPASS_VECTOR_SIZE + 4
        && u32::from_le_bytes(
            body[ME_ROM_BYPASS_VECTOR_SIZE..ME_ROM_BYPASS_VECTOR_SIZE + 4]
                .try_into()
                .unwrap(),
        ) == FPT_SIGNATURE
    {
        ME_ROM_BYPASS_VECTOR_SIZE
    } else {
        return None;
    };
    let hdr = &body[base..];
    let num = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
    let mut hdr_len = hdr[10] as usize;
    if hdr_len == 0 {
        hdr_len = FPT_HEADER_LEN;
    }
    if num > FPT_MAX_ENTRIES || base + hdr_len + num as usize * FPT_ENTRY_LEN > body.len() {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..num as usize {
        let e = &hdr[hdr_len + i * FPT_ENTRY_LEN..];
        let name: String = e[0..4]
            .iter()
            .map(|&b| b as char)
            .filter(|c| c.is_ascii_graphic())
            .collect();
        let offset = u32::from_le_bytes(e[8..12].try_into().unwrap());
        let size = u32::from_le_bytes(e[12..16].try_into().unwrap());
        out.push(FptPartition { name, offset, size });
    }
    Some(out)
}

pub fn fpt_children(body: &[u8], region_offset: usize, region_size: usize) -> Vec<FfsNode> {
    let Some(parts) = parse_fpt(body) else {
        tracing::warn!(region_offset, "ME region has no $FPT, no partition children");
        return vec![];
    };
    parts
        .iter()
        .filter(|p| p.size > 0 && (p.offset as usize) < region_size)
        .map(|p| {
            let end = (p.offset as usize + p.size as usize).min(region_size);
            FfsNode {
                guid: None,
                node_type: FfsType::Region,
                subtype: 0,
                offset: (region_offset + p.offset as usize) as u32,
                header: vec![],
                body: body[p.offset as usize..end].to_vec(),
                tail: vec![],
                children: vec![],
                action: Action::NoAction,
                parsing_data: ParsingData::FptPartition(FptParsingData { name: p.name.clone() }),
                fixed: true,
                compressed: false,
                alignment_bytes: vec![],
            }
        })
        .collect()
}
```

`parse_image` (Task 6) — в не-BIOS ветке для `FlashRegionKind::Me` после `make_region_node` добавить детей:

```rust
_ => {
    let mut node =
        super::region::make_region_node(buf, r.kind, r.offset, r.size);
    if r.kind == FlashRegionKind::Me {
        node.children = super::region::fpt_children(&node.body, r.offset, r.size);
    }
    children.push(node);
    last_end = r.offset + r.size;
}
```

`node_name` в `image.rs`, ветка Region:

```rust
FfsType::Region => match &node.parsing_data {
    ParsingData::Region(rd) => format!("{} region", rd.kind.label()),
    ParsingData::FptPartition(pd) => pd.name.clone(),
    _ => String::new(),
},
```

Round-trip: дети Region не сериализуются (builder arm из Task 6) — байты не меняются; добавить в тест `me_region_gets_fpt_children` утверждение `build` через существующий дескрипторный round-trip-тест (расширив его ME-часть телом с $FPT).

- [ ] **Step 4: зелёный** — `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`.

- [ ] **Step 5: commit** — `feat(uefi-engine): ME region $FPT partitions as named Region children`

---

### Task 8: (A6c) proto Node.region + TUI read-only-индикация

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (`Node` поле 9 + rpc-хендлеры не меняются)
- Modify: `crates/uefi-engine/src/parser/image.rs` (`list_recursive`/`search_recursive` заполняют `region`)
- Modify: все прото-`Node`-литералы (см. Step 2)
- Modify: `crates/uefi-tui/src/theme.rs` (`TYPE_REGION`, `immutable_style`)
- Modify: `crates/uefi-tui/src/ui/tree.rs` (dim-рендер)
- Modify: `crates/uefi-tui/src/app.rs` (`details_text`: Region-строка)
- Test: `image.rs` tests, `theme.rs` tests, `app.rs` tests

**Interfaces:**
- Produces: `Node { ..., string region = 9; }`; TUI `TreeNode.region: String` (новое поле, заполняется в `tree.rs::build_tree`); `theme::TYPE_REGION: u8 = 63`; `theme::immutable_style() -> Style`.

- [ ] **Step 1: proto** — `engine.proto`:

```proto
message Node {
  string path = 1;
  uint32 type = 2;
  uint32 subtype = 3;
  string guid = 4;
  uint64 offset = 5;
  uint64 size = 6;
  string name = 7;
  uint32 action = 8;
  string region = 9;
}
```

`list_recursive`/`search_recursive` (`image.rs`): поле

```rust
region: match &node.parsing_data {
    ParsingData::Region(rd) => rd.kind.label().to_string(),
    ParsingData::FptPartition(_) => "ME".to_string(),
    _ => String::new(),
},
```

- [ ] **Step 2: догнать все прото-Node-литералы** — найти: `grep -rn 'Node {' crates/ --include='*.rs' | grep -v FfsNode | grep -v TreeNode`. Ожидаемые места: `uefi-engine/src/parser/image.rs` (2), `uefi-tui/tests/mock_server.rs` (6), `uefi-cli/tests/mock_server.rs`, `uefi-gateway/tests/mock_server.rs`, `uefi-engine/tests/real_image.rs`, тест-моды CLI при наличии. В каждый литерал добавить `region: String::new()`. Проверка: `cargo test --all` компилирует все тест-таргеты (урок `9933f59`).

- [ ] **Step 3: TUI** — `theme.rs`:

```rust
pub const TYPE_REGION: u8 = 63;

pub fn immutable_style() -> ratatui::style::Style {
    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)
}
```

`type_icon`: `TYPE_REGION => "\u{F492}"`; `type_color`: `TYPE_REGION => Color::DarkGray`.

`tree.rs` (модуль данных TUI `src/tree.rs`) — `TreeNode` получает `pub region: String`, `build_tree` заполняет из `Node.region`. `ui/tree.rs` рендер: если `node.node_type == TYPE_REGION` — имя/иконка/expand через `immutable_style()` (переопределить `color`):

```rust
let immutable = node.node_type == TYPE_REGION;
let name_style = if immutable {
    immutable_style()
} else {
    Style::default().fg(color)
};
```

и использовать `name_style` в Span'е имени; Span иконки аналогично.

`app.rs::details_text` — в конец формата, при непустом `node.region`:

```rust
let region_part = if node.region.is_empty() {
    String::new()
} else {
    format!("\nRegion:   {} (read-only)", node.region)
};
```

Тесты: `theme` (`immutable_style` → DarkGray; иконка региона отлична от прочих), `app` (`details_text` содержит `Region:   ME (read-only)` для узла с `region: "ME"`), `tree.rs` (`build_tree` переносит region).

- [ ] **Step 4: зелёный full-workspace** — `cargo test --all && cargo clippy --all -- -D warnings`.

- [ ] **Step 5: commit** — `feat(uefi-proto,uefi-tui): Node.region + dim read-only styling for flash regions`

---

### Task 9: (A7a) proto ImageUpload + server + лимиты сообщений

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`image_upload`, рефактор `image_open` → `open_from_bytes`; `.max_decoding_message_size` на обоих serve-сайтах: основной ~1066 и тест-setup ~1106)
- Test: `server.rs` `mod tests`

**Interfaces:**
- Produces: `rpc ImageUpload(ImageUploadRequest) returns (ImageOpenResponse)`; `ImageUploadRequest { string session_id = 1; bytes data = 2; ImageMode mode = 3; string name = 4; }`.

- [ ] **Step 1: proto** (после `rpc ImageOpen`):

```proto
rpc ImageUpload(ImageUploadRequest)     returns (ImageOpenResponse);

message ImageUploadRequest {
  string session_id = 1;
  bytes data = 2;
  ImageMode mode = 3;
  string name = 4;
}
```

- [ ] **Step 2: рефактор + хендлер** — тело `image_open` после чтения `bytes` вынести в приватный метод (та же логика: parse → `store_image_file` → `insert_image` → ответ; `path`-колонка для upload = `name`, для open = реальный путь):

```rust
#[tracing::instrument(skip(self, req), err)]
async fn image_upload(&self, req: Request<ImageUploadRequest>) -> RpcResult<ImageOpenResponse> {
    let r = req.into_inner();
    let mode = match r.mode {
        0 => ImageMode::Read,
        1 => ImageMode::Write,
        _ => return Err(Status::invalid_argument("bad mode")),
    };
    let name = if r.name.is_empty() {
        "upload".to_string()
    } else {
        r.name.clone()
    };
    self.open_from_bytes(&r.session_id, r.data, mode, &name, &name)
        .await
}

async fn open_from_bytes(
    &self,
    session_id: &str,
    bytes: Vec<u8>,
    mode: ImageMode,
    name: &str,
    path: &str,
) -> RpcResult<ImageOpenResponse> {
    let image_id = Uuid::new_v4().to_string();
    let img = parse_image(&bytes, mode, &image_id, session_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    store_image_file(&self.data_dir, session_id, &image_id, &bytes)
        .map_err(|e| Status::internal(e.to_string()))?;
    self.sm
        .db
        .lock()
        .unwrap()
        .insert_image(
            &image_id,
            session_id,
            name,
            path,
            mode as i64,
            bytes.len() as i64,
        )
        .map_err(|e| Status::internal(e.to_string()))?;
    let root_guid = img
        .root
        .guid
        .map(|g| crate::guid_to_upper_string(&g))
        .unwrap_or_default();
    self.images.lock().await.insert(image_id.clone(), img);
    let _ = self.sm.touch(session_id);
    tracing::info!(name = %name, size = bytes.len(), image_id = %image_id, "image uploaded");
    Ok(Response::new(ImageOpenResponse {
        image_id,
        root_guid,
        name: name.to_string(),
    }))
}
```

`image_open` ужимается до: parse mode → `fs::read` → default name из file_name → `open_from_bytes(&r.session_id, bytes, mode, &name, &r.path)`.

serve-сайты:

```rust
.add_service(
    EngineServiceServer::new(server)
        .max_decoding_message_size(64 * 1024 * 1024),
)
```

- [ ] **Step 3: тесты** (server.rs `mod tests`):

```rust
#[tokio::test]
async fn image_upload_roundtrip() {
    let (_td, mut client) = setup().await;
    let sid = create_session(&mut client).await;
    let resp = client
        .image_upload(tonic::Request::new(ImageUploadRequest {
            session_id: sid,
            data: fv_image_with_two_files(),
            mode: 0,
            name: "up.bin".into(),
        }))
        .await
        .unwrap()
        .into_inner();
    let nodes = client
        .image_nodes_list(tonic::Request::new(ImageNodesListRequest {
            image_id: resp.image_id.clone(),
            filter: String::new(),
        }))
        .await
        .unwrap()
        .into_inner()
        .nodes;
    assert!(!nodes.is_empty());
}

#[tokio::test]
async fn image_upload_rejected_over_small_limit() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("small.sock");
    let db = crate::storage::open_db(&td.path().join("db.sqlite")).unwrap();
    let sm = Arc::new(SessionManager::new(
        db,
        td.path().to_path_buf(),
        Duration::from_secs(864000),
        Duration::from_secs(3600),
        false,
    ));
    let server = EngineServer {
        sm,
        images: Arc::new(Mutex::new(HashMap::new())),
        data_dir: td.path().to_path_buf(),
    };
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();
    let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(
                EngineServiceServer::new(server)
                    .max_decoding_message_size(1024 * 1024),
            )
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let channel = Endpoint::try_from("http://localhost")
        .unwrap()
        .connect_with_connector(tower::service_fn({
            let s = sock.to_string_lossy().to_string();
            move |_: http::Uri| {
                let s = s.clone();
                async move {
                    Ok::<_, std::io::Error>(
                        hyper_util::rt::TokioIo::new(
                            tokio::net::UnixStream::connect(s).await?,
                        ),
                    )
                }
            }
        }))
        .await
        .unwrap();
    let mut client = EngineServiceClient::new(channel)
        .max_encoding_message_size(64 * 1024 * 1024);
    let result = client
        .image_upload(tonic::Request::new(ImageUploadRequest {
            session_id: "s".into(),
            data: vec![0u8; 2 * 1024 * 1024],
            mode: 0,
            name: "big".into(),
        }))
        .await;
    assert!(result.is_err(), "server with 1MB decode limit must reject 2MB upload");
}
```

- [ ] **Step 4: зелёный full-workspace** — `cargo test --all` (mock-серверы TUI/CLI/gateway получат новый трейт-метод `image_upload` → добавить заглушки по прецеденту `9933f59`/`f970bf6`, если компиляция потребует; заглушка = `Ok(Response::new(ImageOpenResponse { image_id: "mock".into(), root_guid: String::new(), name: "mock.bin".into() }))`).

- [ ] **Step 5: commit** — `feat(uefi-proto,uefi-engine): ImageUpload RPC (bytes open) + 64MB message limits`

---

### Task 10: (A7b) TUI — `:upload PATH`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (`"upload"`-ветка; клиентский метод; `connect` → `max_encoding_message_size`)
- Modify: `crates/uefi-tui/tests/mock_server.rs` (`image_upload` заглушка)
- Test: `commands.rs` tests / `mock_server.rs` integration

**Interfaces:**
- Consumes: Task 9 RPC.
- Produces: ex-команда `:upload PATH [--mode write]`; `Client::image_upload(&mut self, session_id: &str, data: Vec<u8>, mode: i32, name: &str) -> Result<ImageOpenResponse, String>`.

- [ ] **Step 1: клиент** — в `connect` (`commands.rs:40-43`):

```rust
Ok(Client {
    inner: EngineServiceClient::new(channel)
        .max_encoding_message_size(64 * 1024 * 1024),
    state,
})
```

Метод на `Client`:

```rust
impl Client {
    pub async fn image_upload(
        &mut self,
        session_id: &str,
        data: Vec<u8>,
        mode: i32,
        name: &str,
    ) -> Result<ImageOpenResponse, String> {
        self.inner
            .image_upload(auth_req(
                &self.state,
                ImageUploadRequest {
                    session_id: session_id.into(),
                    data,
                    mode,
                    name: name.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())
            .map(|r| r.into_inner())
    }
}
```

- [ ] **Step 2: команда** (в `execute_command`, рядом с `"open"`):

```rust
"upload" => {
    let path = parts
        .get(1)
        .ok_or("usage: :upload PATH [--mode read|write]")?;
    let mode = if parts.contains(&"write") { 1 } else { 0 };
    let bytes =
        std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let sid = client.state.session_id.clone().ok_or("no session")?;
    let resp = client.image_upload(&sid, bytes, mode, &name).await?;
    app.active_image_id = Some(resp.image_id.clone());
    client.state.active_image_id = Some(resp.image_id.clone());
    app.image_loaded = true;
    app.cursor = 0;
    refresh_tree(app, client).await?;
    refresh_registry(app, client).await?;
    app.status_msg = format!("uploaded {} ({})", resp.name, resp.image_id);
    Ok(resp.image_id)
}
```

(если постлудия `open`-ветки отличается — привести к её виду, переиспользуя те же вызовы.)

- [ ] **Step 3: mock + тест** — `tests/mock_server.rs` заглушка `image_upload` (вернуть image_id "mock-upload"); integration-тест:

```rust
#[tokio::test]
async fn upload_sets_active_image_and_tree() {
    let (sock, _handle) = spawn_mock().await;
    let state = mock_state(&sock).await;
    let mut client = commands::connect(None, state.clone()).await.unwrap();
    let mut app = App::new();
    let out = std::env::temp_dir().join("tui-upload-test.bin");
    std::fs::write(&out, b"mock").unwrap();
    let r = commands::execute_command(
        &mut app,
        &format!("upload {}", out.display()),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(app.active_image_id.as_deref(), Some(r.as_str()));
    assert!(!app.tree.is_empty());
}
```

(хелперы `spawn_mock`/`mock_state` — из существующих тестов `mock_server.rs`.)

- [ ] **Step 4: зелёный** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings && cargo test --all`.

- [ ] **Step 5: commit** — `feat(uefi-tui): :upload PATH — open image bytes through ImageUpload RPC`

---

### Task 11: (A8a) снапшоты — proto + storage + create/list

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto`
- Modify: `crates/uefi-proto/build.rs` (serde-атрибут `engine.ImageSnapshotInfo`)
- Modify: `crates/uefi-engine/src/storage/schema.rs` (таблица)
- Modify: `crates/uefi-engine/src/storage/mod.rs` (`ImageSnapshotRow`, Db-методы)
- Modify: `crates/uefi-engine/src/storage/image.rs` (`store_snapshot_file`, `read_snapshot_file`)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`image_snapshot_create`, `image_snapshots_list`)
- Test: `storage/mod.rs` tests, `server.rs` tests

**Interfaces:**
- Produces (см. спека-аддендум «A8'»): RPC `ImageSnapshotCreate/List`; `Db::{insert_image_snapshot, list_image_snapshots}`; `storage::image::{store_snapshot_file, read_snapshot_file}`; `ImageSnapshotRow { id, image_id, name, size, created_at }`.

- [ ] **Step 1: proto** (в конец HII-блока):

```proto
rpc ImageSnapshotCreate(ImageSnapshotCreateRequest)   returns (ImageSnapshotCreateResponse);
rpc ImageSnapshotsList(ImageSnapshotsListRequest)     returns (ImageSnapshotsListResponse);

message ImageSnapshotCreateRequest  { string image_id = 1; string name = 2; }
message ImageSnapshotCreateResponse { string snapshot_id = 1; int64 created_at = 2; }
message ImageSnapshotInfo {
  string snapshot_id = 1;
  string name = 2;
  int64 created_at = 3;
  uint64 size = 4;
}
message ImageSnapshotsListRequest  { string image_id = 1; }
message ImageSnapshotsListResponse { repeated ImageSnapshotInfo snapshots = 1; }
```

`build.rs`: `.message_attribute("engine.ImageSnapshotInfo", "#[derive(serde::Serialize)]")`.

- [ ] **Step 2: storage-тесты** (`storage/mod.rs` tests):

```rust
#[test]
fn image_snapshots_roundtrip_and_cascade() {
    let db = open_test_db();
    db.insert_session("s1", "tok", "n").unwrap();
    db.insert_image("i1", "s1", "n", "p", 1, 16).unwrap();
    db.insert_image_snapshot("sn1", "i1", "before", 16).unwrap();
    let rows = db.list_image_snapshots("i1").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "before");
    assert_eq!(db.get_image_snapshot("sn1").unwrap().unwrap().image_id, "i1");
    db.delete_session_metadata("s1").unwrap();
    assert!(db.list_image_snapshots("i1").unwrap().is_empty());
}
```

(`open_test_db` — существующий хелпер тестов storage или `open_db` на tempfile.)

- [ ] **Step 3: реализация storage** — `schema.rs` добавить:

```sql
CREATE TABLE IF NOT EXISTS image_snapshots (
    id         TEXT PRIMARY KEY,
    image_id   TEXT NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_image_snapshots_image ON image_snapshots(image_id);
```

`storage/mod.rs`:

```rust
#[derive(Debug, Clone)]
pub struct ImageSnapshotRow {
    pub id: String,
    pub image_id: String,
    pub name: String,
    pub size: i64,
    pub created_at: i64,
}
```

Db-методы (по образцу соседних; `insert_image_snapshot(&self, id, image_id, name, size)`, `list_image_snapshots(&self, image_id) -> Result<Vec<ImageSnapshotRow>>` c `ORDER BY created_at`, `get_image_snapshot(&self, id) -> Result<Option<ImageSnapshotRow>>`).

`storage/image.rs` (по образцу `store_image_file`/`read_image_file`):

```rust
pub fn store_snapshot_file(
    data_dir: &Path,
    session_id: &str,
    image_id: &str,
    snapshot_id: &str,
    bytes: &[u8],
) -> Result<()> {
    let dir = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.snapshots"));
    fs::create_dir_all(&dir)?;
    atomic_write(&dir.join(format!("{snapshot_id}.bin")), bytes)
}

pub fn read_snapshot_file(
    data_dir: &Path,
    session_id: &str,
    image_id: &str,
    snapshot_id: &str,
) -> Result<Vec<u8>> {
    let p = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.snapshots"))
        .join(format!("{snapshot_id}.bin"));
    fs::read(p).map_err(Into::into)
}
```

(если `atomic_write` живёт в другом модуле — поправить импорт по факту; он уже используется в `server.rs` из `storage::image`.)

- [ ] **Step 4: хендлеры** (`server.rs`):

```rust
#[tracing::instrument(skip(self, req), err)]
async fn image_snapshot_create(
    &self,
    req: Request<ImageSnapshotCreateRequest>,
) -> RpcResult<ImageSnapshotCreateResponse> {
    let r = req.into_inner();
    let row = self
        .sm
        .db
        .lock()
        .unwrap()
        .get_image(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("image not found"))?;
    let bytes = read_image_file(&self.data_dir, &row.session_id, &r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    let snapshot_id = Uuid::new_v4().to_string();
    let name = if r.name.is_empty() {
        format!("snap-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs())
    } else {
        r.name
    };
    store_snapshot_file(&self.data_dir, &row.session_id, &r.image_id, &snapshot_id, &bytes)
        .map_err(|e| Status::internal(e.to_string()))?;
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    self.sm
        .db
        .lock()
        .unwrap()
        .insert_image_snapshot(&snapshot_id, &r.image_id, &name, bytes.len() as i64)
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(ImageSnapshotCreateResponse {
        snapshot_id,
        created_at,
    }))
}

#[tracing::instrument(skip(self, req), err)]
async fn image_snapshots_list(
    &self,
    req: Request<ImageSnapshotsListRequest>,
) -> RpcResult<ImageSnapshotsListResponse> {
    let r = req.into_inner();
    let rows = self
        .sm
        .db
        .lock()
        .unwrap()
        .list_image_snapshots(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(Response::new(ImageSnapshotsListResponse {
        snapshots: rows
            .into_iter()
            .map(|s| ImageSnapshotInfo {
                snapshot_id: s.id,
                name: s.name,
                created_at: s.created_at,
                size: s.size as u64,
            })
            .collect(),
    }))
}
```

Импорт `read_image_file`/`store_snapshot_file` расширить в шапке `server.rs`.

Тест server: `create → list` (строка с именем), `create` на несуществующем образе → not_found. Догнать трейт-заглушки в трёх mock_server.rs (TUI/CLI/gateway): `snapshot_create` → `ImageSnapshotCreateResponse { snapshot_id: "mock-snap".into(), created_at: 0 }`, `snapshots_list` → пустой список, `snapshot_restore` → `Empty {}` (restore появится в Task 12 — заглушки добавлять по мере появления методов в трейте).

- [ ] **Step 5: зелёный full-workspace** — `cargo test --all && cargo clippy --all -- -D warnings`.

- [ ] **Step 6: commit** — `feat(uefi-proto,uefi-engine): image snapshots — create/list RPC + storage`

---

### Task 12: (A8b) снапшоты — restore

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (`rpc ImageSnapshotRestore(...) returns (Empty);` + `message ImageSnapshotRestoreRequest { string image_id = 1; string snapshot_id = 2; }`)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (`image_snapshot_restore`)
- Test: `server.rs` `mod tests`

**Interfaces:**
- Consumes: Task 11 storage.
- Produces: RPC `ImageSnapshotRestore`.

- [ ] **Step 1: хендлер**:

```rust
#[tracing::instrument(skip(self, req), err)]
async fn image_snapshot_restore(
    &self,
    req: Request<ImageSnapshotRestoreRequest>,
) -> RpcResult<Empty> {
    let r = req.into_inner();
    let snap = self
        .sm
        .db
        .lock()
        .unwrap()
        .get_image_snapshot(&r.snapshot_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("snapshot not found"))?;
    if snap.image_id != r.image_id {
        return Err(Status::invalid_argument("snapshot belongs to another image"));
    }
    let row = self
        .sm
        .db
        .lock()
        .unwrap()
        .get_image(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("image not found"))?;
    let bytes = read_snapshot_file(&self.data_dir, &row.session_id, &r.image_id, &r.snapshot_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    let path = self
        .data_dir
        .join("sessions")
        .join(&row.session_id)
        .join("images")
        .join(format!("{}.bin", r.image_id));
    atomic_write(&path, &bytes).map_err(|e| Status::internal(e.to_string()))?;
    self.images.lock().await.remove(&r.image_id);
    self.sm
        .db
        .lock()
        .unwrap()
        .touch_image(&r.image_id)
        .map_err(|e| Status::internal(e.to_string()))?;
    tracing::info!(image_id = %r.image_id, snapshot_id = %r.snapshot_id, "snapshot restored");
    Ok(Response::new(Empty {}))
}
```

- [ ] **Step 2: тест гейта** (спека-аддендум: open(write) → remove → snapshot → ещё правка → restore → дерево как на момент снапшота; «рестарт» = новый EngineServer на том же data_dir):

```rust
#[tokio::test]
async fn snapshot_restore_rolls_tree_back() {
    let td = tempfile::TempDir::new().unwrap();
    let (mut client, _keep) = spawn_engine_on(td.path()).await;
    let sid = create_session(&mut client).await;
    let src = td.path().join("img.bin");
    std::fs::write(&src, fv_image_with_two_files()).unwrap();
    let open = client
        .image_open(tonic::Request::new(ImageOpenRequest {
            session_id: sid,
            path: src.display().to_string(),
            mode: 1,
            name: "t".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    client
        .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
            image_id: open.image_id.clone(),
            target: "0/0".into(),
        }))
        .await
        .unwrap();
    let nodes_at_snapshot = list_nodes(&mut client, &open.image_id).await;
    let snap = client
        .image_snapshot_create(tonic::Request::new(ImageSnapshotCreateRequest {
            image_id: open.image_id.clone(),
            name: "before-second-cut".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    client
        .image_node_remove(tonic::Request::new(ImageNodeRemoveRequest {
            image_id: open.image_id.clone(),
            target: "0/1".into(),
        }))
        .await
        .unwrap();

    client
        .image_snapshot_restore(tonic::Request::new(ImageSnapshotRestoreRequest {
            image_id: open.image_id.clone(),
            snapshot_id: snap.snapshot_id.clone(),
        }))
        .await
        .unwrap();
    let nodes_after = list_nodes(&mut client, &open.image_id).await;
    assert_eq!(
        nodes_at_snapshot, nodes_after,
        "restore must roll the tree back to snapshot state"
    );

    drop(client);
    let (mut client2, _keep2) = spawn_engine_on(td.path()).await;
    let listed = client2
        .image_snapshots_list(tonic::Request::new(ImageSnapshotsListRequest {
            image_id: open.image_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner()
        .snapshots;
    assert_eq!(listed.len(), 1, "snapshots survive engine restart");
    assert_eq!(listed[0].name, "before-second-cut");

    let bad = client2
        .image_snapshot_restore(tonic::Request::new(ImageSnapshotRestoreRequest {
            image_id: open.image_id.clone(),
            snapshot_id: "nope".into(),
        }))
        .await;
    assert!(bad.is_err());
}
```

(`spawn_engine_on(dir)` — вынести установку сервера из `setup()` в параметризуемый хелпер: data_dir+новый сокет, возвращает `(client, Vec<JoinHandle<()>>)` — второй элемент держит сервер живым; `list_nodes` — хелпер-обёртка над `image_nodes_list`. «Ещё правка» = remove `0/1` того же образа — семантика гейта сохраняется.)

- [ ] **Step 3: зелёный full-workspace** — `cargo test --all && cargo clippy --all -- -D warnings` (+ заглушки `snapshot_restore` в трёх mock_server.rs).

- [ ] **Step 4: commit** — `feat(uefi-engine): image_snapshot_restore — rollback bytes + cache invalidation`

---

### Task 13: (A8c) TUI — `:snapshot` / `:snapshots` / `:restore`

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (клиентские методы + три ветки)
- Modify: `crates/uefi-tui/tests/mock_server.rs` (заглушки с реалистичными данными)
- Test: `mock_server.rs` integration

**Interfaces:**
- Consumes: Task 11/12 RPC.
- Produces: `Client::{image_snapshot_create, image_snapshots_list, image_snapshot_restore}`; ex-команды `:snapshot [NAME]`, `:snapshots`, `:restore <snapshot_id>`.

- [ ] **Step 1: клиентские методы** (по образцу `image_upload` из Task 10; `active_image` через `client.state.active_image_id.clone().ok_or("no active image")?`).

- [ ] **Step 2: ветки `execute_command`**:

```rust
"snapshot" => {
    let iid = client
        .state
        .active_image_id
        .clone()
        .ok_or("no active image")?;
    let name = parts.get(1).copied().unwrap_or_default().to_string();
    let resp = client.image_snapshot_create(&iid, &name).await?;
    app.status_msg = format!("snapshot {} ({})", resp.snapshot_id, name);
    Ok(resp.snapshot_id)
}
"snapshots" => {
    let iid = client
        .state
        .active_image_id
        .clone()
        .ok_or("no active image")?;
    let snaps = client.image_snapshots_list(&iid).await?;
    app.status_msg = if snaps.is_empty() {
        "no snapshots".into()
    } else {
        snaps
            .iter()
            .map(|s| format!("{}  {}  {}B", s.snapshot_id, s.name, s.size))
            .collect::<Vec<_>>()
            .join("  ·  ")
    };
    Ok(snaps.iter().map(|s| s.snapshot_id.clone()).collect::<Vec<_>>().join(","))
}
"restore" => {
    let iid = client
        .state
        .active_image_id
        .clone()
        .ok_or("no active image")?;
    let snap_id = parts
        .get(1)
        .ok_or("usage: :restore SNAPSHOT_ID (see :snapshots)")?;
    client.image_snapshot_restore(&iid, snap_id).await?;
    refresh_tree(app, client).await?;
    refresh_registry(app, client).await?;
    app.status_msg = format!("restored {snap_id}");
    Ok(snap_id.to_string())
}
```

- [ ] **Step 3: mock + тест** — mock-заглушки: create → `snapshot_id: "snap-1"`, list → один `ImageSnapshotInfo { snapshot_id: "snap-1", name: "before", created_at: 1, size: 16 }`, restore → `Empty`. Тест: `snapshot` возвращает id; `snapshots` кладёт строку в `status_msg`; `restore` вызывает refresh (дерево непустое, статус «restored»).

- [ ] **Step 4: зелёный** — `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings && cargo test --all`.

- [ ] **Step 5: commit** — `feat(uefi-tui): :snapshot/:snapshots/:restore — snapshot workflow in TUI`

---

### Task 14: real-image гейты + TODO-бухгалтерия + финальный прогон

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (три `#[ignore]`-теста)
- Modify: `TODO.md` (закрыть потреблённые пункты)
- Modify: `roadmap.md` (статус цикла A)

**Interfaces:**
- Consumes: всё предыдущее.

- [ ] **Step 1: гейты** (по образцу существующих `#[ignore]`-тестов real_image.rs; путь `../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin`):

```rust
#[tokio::test]
#[ignore = "needs real image (AGENTS.md refs/fw)"]
async fn real_image_name_lift_dxe_setup() {
    // open(write) → найти File-узел, чей первый ребёнок — GUIDED/compression
    // и внутри есть UI-секция (path 1/28-класс) → Node.name непусто.
    // Дополнительно: хотя бы 20 файлов DXE имеют непустые имена.
}

#[tokio::test]
#[ignore = "needs real image"]
async fn real_image_remove_save_clean_tree() {
    // open(write) → remove первого DXE-файла → nodes_list:
    // ни одного action != 50; узла нет. save → повторный open → то же дерево.
}

#[tokio::test]
#[ignore = "needs real image"]
async fn real_image_flash_regions_layout() {
    // parse: есть Region-узлы Descriptor/ME/GbE?; ME имеет детей-партиций ($FPT,
    // FTPR среди имён); build_image == исходные байты (round-trip); путь
    // count DXE/PEI-файлов не изменился против фиксированного числа (снять
    // константой при написании — это и есть регрессия FV-части).
    // ops: remove на ME-ребёнке → ImmutableRegion (failed_precondition).
}
```

Заполнить тела по фактическим хелперам файла; константу числа файлов зафиксировать при первом прогоне (живой образ — критерий). Прогон: `cargo test -p uefi-engine --test real_image -- --ignored` при наличии файла; зелёный — обязателен для закрытия цикла.

- [ ] **Step 2: TODO-бухгалтерия** — в `TODO.md` закрыть `[x]` с указанием коммитов: «ТUI: метка для узла Image/Volume», «fallback-метка по subtype», «Имя файла не поднимается из UI-секции за GUIDED/LZMA», «Том ME показывает только одну секцию», пункты раздела «Stale-tree» (prune), FIND/GO, tab-completion, «TUI: `:upload PATH`», «Server: ImageUpload RPC» (server-часть); добавить остаточные заметки при наличии (маркеры действий в TUI мертвы после prune — отложенная косметика).

- [ ] **Step 3: roadmap** — цикл A: «реализован (дата, коммиты)».

- [ ] **Step 4: финальный прогон** — `cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check`.

- [ ] **Step 5: commit** — `test(uefi-engine): real-image gates for cycle A (name-lift, prune, flash regions) + docs bookkeeping`

---

## Self-Review (выполнен при написании)

- **Spec coverage:** A1→Task 2(+8 для региона-меток/dim), A2→Task 1(+гейт Task 14), A3→Task 3(+гейт), A4→Task 4, A5→Task 5, A6→Tasks 6-8(+гейты), A7→Tasks 9-10, A8'→Tasks 11-13. Non-goals не нарушены (мутации регионов запрещены Task 6/7; визардов нет; GbE/PDR — raw-узлы Task 6).
- **Placeholders:** код шагей полный; отсылки «по образцу» — только на уже существующие хелперы (`setup()`, `spawn_mock`, `open_test_db`, `create_session`), которые исполнитель читает в тех же файлах.
- **Type consistency:** `FlashRegionKind`/`RegionParsingData`/`FptParsingData` заданы в Task 6/7 и переиспользуются; `NodeCmdArgs` переезжает в Task 5 целиком; proto-поля без коллизий (Node.region=9; новые rpc — после `HiiPageAdd`).
- **Нюансы, пойманные ревью плана:** (1) серверный тест remove обязан удалять файл внутри FV, не том — data-loss-гвард `flush_image` отказывает на уменьшение вывода; (2) `TreeNode` без `Default` — литералы полные; (3) `*_name_or_raw` всегда читаем (`Unknown 77h`), fallback-цепочка A1 без hedging; (4) удаление пути внутри региона тестируется только после Task 7 ($FPT-дети); (5) фикстура немутируемости партиций = дескриптор + $FPT-тело по ME-офсету.

## Риски

- Дескрипторный путь `parse_image` меняет парсинг живых образов — round-trip-гейты (юнит Task 6 + real-image Task 14) обязательны до слияния; регресс FV-части ловится числом файлов.
- Прото-расширения ломают чужие тест-моки — каждый proto-таск заканчивать `cargo test --all` (урок `9933f59`).
- `ensure_mutable` может отказать там, где раньше всё «работало» до save (Tiano-барьеры) — это честная ошибка, но если real-image-гейт Task 14 упрётся в образ, где ВСЁ за барьерами, рассмотреть separate docs-коммит с ослаблением (сначала диагностика, не молча).
