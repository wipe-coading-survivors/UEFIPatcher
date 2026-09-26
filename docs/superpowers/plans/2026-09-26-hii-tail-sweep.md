# hii-tail-sweep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Закрыть 6 хвостовых позиций TODO (87, 1416, 1422, 4506, 4509, 4512) fix-волной по спеке `docs/superpowers/specs/2026-09-26-hii-tail-sweep-design.md`.

**Architecture:** Три независимых движковых правки (порядок ошибок add_varstores; cold-load хелпер ensure_image_loaded; усиление write-through теста), одна правка тестового кода real_image и один guard в TUI-рендере. Никаких изменений proto/RPC-контрактов.

**Tech Stack:** Rust workspace (tonic RPC, tokio, ratatui TestBackend для TUI-тестов).

## Global Constraints

- Спека: `docs/superpowers/specs/2026-09-26-hii-tail-sweep-design.md` — порядок ошибок `InvalidItemId → NotFound → NotWritable` (§1), Non-goals не трогать.
- Ветка кода: `fix/hii-tail-sweep`; спека/план уже в master docs-коммитами.
- После каждой задачи: `cargo test -p <crate>` + `cargo clippy -p <crate> -- -D warnings`.
- Комментарии в коде не добавлять (кроме rustdoc-контрактов на pub-функции).
- Игнор-тесты пачки (450x, rk3588) прогоняются явно — образы в `refs/`.

---

### Task 1: add_varstores — контракт порядка ошибок

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs:1864-1876` (ранний mode-гейт)
- Test: `crates/uefi-engine/src/hii/mod.rs` (tests mod, рядом с `set_item_visibility_error_order_contract` ~3584)

**Interfaces:**
- Consumes: `add_varstores(image, item_id, varstores)` (сигнатура не меняется), фикстуры `read_mode_image()`, `vendor_image_with(0x19, vendor_forms_pkg())`, константы `MALFORMED_ITEM`, `UNKNOWN_TARGET_ITEM`, `VENDOR_FORM_ITEM`.
- Produces: поведение §1 спеки; тест `add_varstores_error_order_contract`.

- [ ] **Step 1: Написать красный контракт-тест**

В tests mod `hii/mod.rs`, сразу после `set_item_visibility_error_order_contract` (конец ~3607):

```rust
    #[test]
    fn add_varstores_error_order_contract() {
        let vs = schema::VarStoreSchema {
            id: 0x7F01,
            guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
            size: 0x1670,
            name: "IntelSetup".into(),
            var_type: schema::VarStoreType::Efi,
            attributes: 7,
        };
        let mut r = read_mode_image();
        assert!(
            matches!(
                add_varstores(&mut r, MALFORMED_ITEM, &[vs.clone()]),
                Err(HiiError::InvalidItemId(_))
            ),
            "malformed item_id must not be hidden behind NotWritable"
        );
        assert!(
            matches!(
                add_varstores(&mut r, UNKNOWN_TARGET_ITEM, &[vs.clone()]),
                Err(HiiError::NotFound)
            ),
            "unknown target must surface before mode check"
        );
        assert!(matches!(
            add_varstores(&mut r, VENDOR_FORM_ITEM, &[vs]),
            Err(HiiError::NotWritable)
        ));
        let mut w = vendor_image_with(0x19, vendor_forms_pkg());
        assert!(matches!(
            add_varstores(&mut w, MALFORMED_ITEM, &[schema::VarStoreSchema {
                id: 0x7F01,
                guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 0x1670,
                name: "IntelSetup".into(),
                var_type: schema::VarStoreType::Efi,
                attributes: 7,
            }]),
            Err(HiiError::InvalidItemId(_))
        ));
        assert!(
            add_varstores(&mut w, VENDOR_FORM_ITEM, &[]).is_ok(),
            "empty varstores keep the early Ok return"
        );
    }
```

Если `VarStoreSchema` не `Clone` — заменить `[vs.clone()]` на повторный литерал (как в последнем ассерте).

- [ ] **Step 2: Прогнать — ожидать RED**

Run: `cargo test -p uefi-engine add_varstores_error_order_contract`
Expected: FAIL — первые два ассерта падают (сейчас NotWritable прячет InvalidItemId/NotFound).

- [ ] **Step 3: Убрать ранний mode-гейт**

В `add_varstores` (mod.rs:1872-1874) удалить:

```rust
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
```

(остаётся `let (target, _form_id, _qid) = parse_item_id(item_id)?;` затем `resolve_writable_path` — NotFound → NotWritable → барьер).

- [ ] **Step 4: Прогнать — ожидать GREEN**

Run: `cargo test -p uefi-engine add_varstores && cargo test -p uefi-engine`
Expected: PASS (включая `add_varstores_declares_efi_varstore_and_shifts_spf_records` и question_add-гейты — путь write-режима не менялся).

- [ ] **Step 5: clippy + коммит**

```bash
cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "fix(uefi-engine): add_varstores — порядок ошибок InvalidItemId → NotFound → NotWritable (TODO:1416)"
```

---

### Task 2: ensure_image_loaded — холодный кэш unlock, set_value без клона

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs:207-236` (get_or_load_image → выделить хелпер), `:1070-1091` (hii_unlock), `:1127-1151` (hii_set_value)
- Test: `crates/uefi-engine/src/rpc/server.rs` tests mod

**Interfaces:**
- Produces: `async fn ensure_image_loaded(&self, image_id: &str) -> Result<(), Status>` (private); `get_or_load_image` — прежняя сигнатура/поведение; `hii_unlock`/`hii_set_value` — контракт RPC не меняется.

- [ ] **Step 1: Красный синтетический тест холодного кэша**

В tests mod (после `hii_unlock_malformed_item_is_invalid_argument` ~3005):

```rust
    #[tokio::test]
    async fn hii_unlock_survives_engine_restart_cold_cache() {
        let (td, mut client) = setup().await;
        let orig = td.path().join("v.bin");
        std::fs::write(&orig, fixture_volume()).unwrap();
        let session = client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner();
        let opened = client
            .image_open(ImageOpenRequest {
                session_id: session.session_id.clone(),
                path: orig.to_string_lossy().to_string(),
                mode: ImageMode::Write as i32,
                name: "v.bin".into(),
            })
            .await
            .unwrap()
            .into_inner();
        drop(client);

        let (mut client2, _keep2) = spawn_engine_on(td.path()).await;
        let err = client2
            .hii_unlock(HiiUnlockRequest {
                image_id: opened.image_id.clone(),
                item_id: "garbage".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(
            err.code(),
            tonic::Code::InvalidArgument,
            "cold cache must load the image from disk and reach item_id parsing, got: {err}"
        );
    }
```

- [ ] **Step 2: Прогнать — ожидать RED**

Run: `cargo test -p uefi-engine hii_unlock_survives_engine_restart_cold_cache`
Expected: FAIL — код `NotFound` (image not found), не InvalidArgument.

- [ ] **Step 3: Выделить ensure_image_loaded, переключить оба хендлера**

Заменить тело `get_or_load_image` (server.rs:207-236) на пару:

```rust
    async fn ensure_image_loaded(&self, image_id: &str) -> Result<(), Status> {
        {
            let images = self.images.lock().await;
            if images.contains_key(image_id) {
                return Ok(());
            }
        }
        let row = self
            .sm
            .db
            .lock()
            .unwrap()
            .get_image(image_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("image not found"))?;
        let bytes = read_image_file(&self.data_dir, &row.session_id, image_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        let mode = if row.mode == 1 {
            ImageMode::Write
        } else {
            ImageMode::Read
        };
        let img = crate::parser::image::parse_image(&bytes, mode, image_id, &row.session_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        self.images.lock().await.insert(image_id.into(), img);
        Ok(())
    }

    async fn get_or_load_image(&self, image_id: &str) -> Result<Image, Status> {
        self.ensure_image_loaded(image_id).await?;
        let images = self.images.lock().await;
        images
            .get(image_id)
            .cloned()
            .ok_or_else(|| Status::not_found("image not found"))
    }
```

`hii_unlock` (1070): первой строкой после `into_inner()` добавить
`self.ensure_image_loaded(&r.image_id).await?;`

`hii_set_value` (1127-1144): заменить
`let img = self.get_or_load_image(&r.image_id).await?;`
на `self.ensure_image_loaded(&r.image_id).await?;`, блок мутации — взять session_id из слота (зеркало unlock), `sm.touch(&session_id)` вместо `&img.session_id`:

```rust
        let r = req.into_inner();
        self.ensure_image_loaded(&r.image_id).await?;
        let (outcome, session_id) = {
            let mut images = self.images.lock().await;
            let img_slot = images
                .get_mut(&r.image_id)
                .ok_or_else(|| Status::not_found("image not found"))?;
            let session_id = img_slot.session_id.clone();
            let outcome = crate::hii::set_value(img_slot, &r.item_id, r.value)
                .map_err(|e| hii_error_status_ctx(e, &r.item_id))?;
            (outcome, session_id)
        };
```

- [ ] **Step 4: Прогнать — ожидать GREEN (весь крейт)**

Run: `cargo test -p uefi-engine`
Expected: PASS — новый тест + `hii_set_value_noop_keeps_artifact_untouched` (ignore) и прочие не задеты.

- [ ] **Step 5: Ignore-гейт на 450x (рестарт → no-op unlock)**

Добавить рядом с `hii_unlock_noop_keeps_artifact_untouched` (2806):

```rust
    #[tokio::test]
    #[ignore = "requires real AMI image under refs/amibcp/ (gitignored)"]
    async fn hii_unlock_cold_cache_450x_noop() {
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
        drop(client);
        let (mut client2, _keep2) = spawn_engine_on(td.path()).await;
        let resp = client2
            .hii_unlock(HiiUnlockRequest {
                image_id: opened.image_id.clone(),
                item_id: "abbce13d-e25a-4d9f-a1f9-2f7710786892:0x10:0#1".into(),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(resp.applied_flips.is_empty());
        let img_path = td
            .path()
            .join("sessions")
            .join(&session)
            .join("images")
            .join(format!("{}.bin", opened.image_id));
        assert_eq!(std::fs::read(&img_path).unwrap(), orig_bytes);
    }
```

Прогнать явно: `cargo test -p uefi-engine hii_unlock_cold_cache_450x_noop -- --ignored` (образ в refs/amibcp присутствует). Expected: PASS.

- [ ] **Step 6: clippy + коммит**

```bash
cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "fix(uefi-engine): ensure_image_loaded — hii_unlock переживает рестарт движка, set_value без клона ради session_id (TODO:1422)"
```

---

### Task 3: write-through тест — реальная мутация + холодный ре-открытие

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs:3446-3491` (`write_through_persists_mutation_to_disk`)
- Test: тот же тест.

**Interfaces:**
- Consumes: `fixture_volume()`, `image_node_insert` (ffs_path, mode 0=Into), `spawn_engine_on`, `list_nodes` (тестовый хелпер), `FfsType`.

- [ ] **Step 1: Переписать тест**

Полностью заменить тело `write_through_persists_mutation_to_disk`:

```rust
    #[tokio::test]
    async fn write_through_persists_mutation_to_disk() {
        let (td, mut client) = setup().await;
        let orig = td.path().join("v.bin");
        std::fs::write(&orig, fixture_volume()).unwrap();
        let session = client
            .session_create(SessionCreateRequest::default())
            .await
            .unwrap()
            .into_inner();
        let opened = client
            .image_open(ImageOpenRequest {
                session_id: session.session_id.clone(),
                path: orig.to_string_lossy().to_string(),
                mode: ImageMode::Write as i32,
                name: "v.bin".into(),
            })
            .await
            .unwrap()
            .into_inner();

        let img_path = td
            .path()
            .join("sessions")
            .join(&session.session_id)
            .join("images")
            .join(format!("{}.bin", opened.image_id));
        let before = std::fs::read(&img_path).unwrap();

        let mut ffs = vec![0u8; 32];
        let guid = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        ffs[0..16].copy_from_slice(&guid.to_bytes());
        ffs[18] = 0x01;
        ffs[20..23].copy_from_slice(&crate::ffs::size_to_uint24(32));
        ffs[24] = 0xAB;
        let ffs_path = td.path().join("insert.ffs");
        std::fs::write(&ffs_path, &ffs).unwrap();
        client
            .image_node_insert(ImageNodeInsertRequest {
                image_id: opened.image_id.clone(),
                target: "0".into(),
                ffs_path: ffs_path.to_string_lossy().to_string(),
                artifact_id: String::new(),
                mode: 0,
            })
            .await
            .unwrap();

        let after = std::fs::read(&img_path).unwrap();
        assert_ne!(
            after, before,
            "mutation must change persisted bytes, not just mtime"
        );
        assert!(after.len() > before.len());

        drop(client);
        let (mut client2, _keep2) = spawn_engine_on(td.path()).await;
        let nodes = list_nodes(&mut client2, &opened.image_id).await;
        assert!(
            nodes
                .iter()
                .any(|n| n.r#type == FfsType::File as u32),
            "inserted file must be visible from the disk copy after engine restart"
        );
    }
```

Сверить поля `ImageNodeInsertRequest` по факту (если `artifact_id`/`mode` названы иначе — взять реальные имена из proto).

- [ ] **Step 2: Прогнать**

Run: `cargo test -p uefi-engine write_through_persists_mutation_to_disk`
Expected: PASS на текущем коде (тест-гигиена; усиление — в самих ассертах). Если ассерты упали — это находка класса write-path, стоп и разбор (rule: дефект плана — docs-коммит до правки).

- [ ] **Step 3: clippy + коммит**

```bash
cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "test(uefi-engine): write-through — байт-мутация insert + холодный ре-открытие вместо mtime-тафтологии (TODO:87)"
```

---

### Task 4: rk3588-гейт — try_from и абсолютная подсказка

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs:1105` (ignore-подсказка), `:1114` (u32→u16 cast)

- [ ] **Step 1: Правки**

Строка 1105:

```rust
#[ignore = "requires rk3588 image; run from repo root with UEFIPATCHER_TEST_FW=$PWD/refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img"]
```

Строки 1113-1117 заменить:

```rust
    for f in &forms {
        let Ok(form_id) = u16::try_from(f.form_id_ifr) else {
            continue;
        };
        let Ok(qs) = uefi_engine::hii::list_questions(&img, &f.form_id, form_id) else {
            continue;
        };
```

- [ ] **Step 2: Компиляция тестов + явный прогон ignore-гейта**

Run: `cargo test -p uefi-engine --test real_image -- --list | grep rk3588`
затем из корня репо:
`UEFIPATCHER_TEST_FW=$PWD/refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img cargo test -p uefi-engine --test real_image real_image_rk3588_bare_questions_resolve -- --ignored`
Expected: PASS (ok).

- [ ] **Step 3: clippy (--all-targets) + коммит**

```bash
cargo clippy -p uefi-engine --all-targets -- -D warnings
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(uefi-engine): rk3588-гейт — u16::try_from со skip, $PWD в ignore-подсказке (TODO:4506, 4512)"
```

---

### Task 5: TUI strings-браузер — титул только для visible-курсора

**Files:**
- Modify: `crates/uefi-tui/src/ui/forms.rs:176-183` (render_strings)
- Test: `crates/uefi-tui/src/ui/forms.rs` tests mod

**Interfaces:**
- Consumes: `app.strings_visible()`, `app.forms.strings`, `app.forms.strings_filter`, `app.forms.strings_cursor`, `app.forms.show_strings`, `uefi_proto::StringInfo { language, string_id, text, source }`, TestBackend-хелперы `panel_text`/`row_of`.

- [ ] **Step 1: Красный тест**

В tests mod forms.rs:

```rust
    #[test]
    fn strings_title_hides_source_of_filtered_out_row() {
        let mut app = crate::app::App::new();
        app.forms.show_strings = true;
        let src = |g: &str| uefi_proto::StringInfo {
            language: "en-US".into(),
            string_id: 1,
            text: String::new(),
            source: g.into(),
        };
        app.forms.strings = vec![
            uefi_proto::StringInfo {
                text: "Alpha".into(),
                ..src("11111111-1111-1111-1111-111111111111/res")
            },
            uefi_proto::StringInfo {
                text: "Beta".into(),
                ..src("22222222-2222-2222-2222-222222222222/res")
            },
        ];
        app.forms.strings_filter = "beta".into();
        app.forms.strings_cursor = 0;
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(
            !text.contains("11111111"),
            "title must not show source of a row hidden by filter"
        );
        assert!(text.contains("filter: beta"));

        app.forms.strings_cursor = 1;
        t.draw(|f| render(f, f.area(), &mut app)).unwrap();
        let text = panel_text(&t, 24);
        assert!(
            text.contains("22222222"),
            "visible cursor row source stays in the title"
        );
    }
```

Если `uefi_proto::StringInfo` не поддерживает `..src(...)`-композицию (нет нужных полей/Default) — заменить на два полных литерала.

- [ ] **Step 2: Прогнать — ожидать RED**

Run: `cargo test -p uefi-tui strings_title_hides`
Expected: FAIL — первый ассерт (титул содержит source скрытой строки).

- [ ] **Step 3: Guard в render_strings**

forms.rs:176-183 обернуть source-добавление проверкой видимости:

```rust
    if visible.contains(&app.forms.strings_cursor) {
        if let Some(s) = app
            .forms
            .strings
            .get(app.forms.strings_cursor)
            .filter(|s| !s.source.is_empty())
        {
            title.push_str(&format!(" — {}", s.source));
        }
    }
```

- [ ] **Step 4: Прогнать — ожидать GREEN**

Run: `cargo test -p uefi-tui`
Expected: PASS (новый + все существующие strings-тесты).

- [ ] **Step 5: clippy + коммит**

```bash
cargo clippy -p uefi-tui -- -D warnings
git add crates/uefi-tui/src/ui/forms.rs
git commit -m "fix(uefi-tui): strings-титул — source только когда cursor внутри :filter (TODO:4509)"
```

---

### Task 6: Финальный гейт + TODO-бухгалтерия

**Files:**
- Modify: `TODO.md` (позиции 87, 1416, 1422, 4506, 4509, 4512 → `[x]` + «Закрыто: цикл hii-tail-sweep (<коммиты>)»)

- [ ] **Step 1: Финальные гейты**

```bash
cargo test --all
cargo clippy --all --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: все зелёные (включая новые тесты).

- [ ] **Step 2: Явные ignore-прогоны пачки**

```bash
cargo test -p uefi-engine hii_unlock_noop -- --ignored
UEFIPATCHER_TEST_FW=$PWD/refs/fw/orange-pi-5-plus-uefi-edk2-rk3588.img cargo test -p uefi-engine --test real_image real_image_rk3588_bare_questions_resolve -- --ignored
```

- [ ] **Step 3: Закрыть TODO-позиции**

Каждую из шести позиций пометить `[x]`, добавив строку «Закрыто: цикл hii-tail-sweep (спека 2026-09-26-hii-tail-sweep-design.md, коммиты Task 1-5)».

- [ ] **Step 4: Коммит**

```bash
git add TODO.md
git commit -m "docs(todo): hii-tail-sweep — закрыты 87/1416/1422/4506/4509/4512"
```

- [ ] **Step 5: Push + PR**

```bash
git push -u origin fix/hii-tail-sweep
```

Создать PR `fix/hii-tail-sweep → master` (gh pr create), описание — по спеке.
