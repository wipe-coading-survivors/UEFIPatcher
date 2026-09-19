# Varstore-контракт: дискаверибельность, валидация, экспозиция — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Экспонировать varstore-карту формсета (RPC `HiiListVarstores` + CLI + TUI), закрыть молчаливые мисбиндинги `form_add` валидациями, прочесть зондом занятость varstore ids 21–30 на 450x, закрыть тестовые долги value-op и два микро-фикса (сокет-дефолт, node-list-легенда).

**Architecture:** Подход B спеки: точечный read-only RPC над существующим walker'ом `values::varstore_map` (движковая база уже есть); валидации — зеркала существующих проверок `check_question_add`, вставляются в `add_form` до первой мутации; клиентские поверхности (CLI/TUI) потребляют тот же RPC. Form-export не трогаем — для него только готовим данные (§6 спеки = контракт).

**Tech Stack:** Rust workspace (edition 2024), tonic/gRPC (`uefi-proto`, build.rs генерация), r-efi (`IFR_VARSTORE_*` константы), ratatui (TUI), существующие test-фикстуры движка.

**Spec:** `docs/superpowers/specs/2026-09-19-varstore-contract-design.md` — план аргументируется от спеки; исполнитель читает оба документа.

## Global Constraints

- Крейты: `uguid` (Guid без data1/data2/data3 полей), `r-efi` (IFR-константы; НЕ определять свои). Не писать byte-offset парсинг вручную там, где есть walker'ы.
- Module-first rule: `pub mod`/`mod` объявление — в том же шаге, что создание файла, ДО `cargo test`.
- TDD: тест падает → реализация → тест проходит → коммит. Один коммит на шаг с `git commit`.
- Без narration-комментариев; допустимы rustdoc `///`-контракты на pub/pub(crate)-функциях и ссылки `file:line`.
- После каждой задачи: `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`.
- Расхождения плана с реальностью — отдельный docs-коммит `docs: fix Task N in varstore-contract plan (…)` ДО правки кода.
- Реализация — в ветке `varstore-contract` (создать первой задачей); docs-коммиты спеки/TODO — в master.
- `#[ignore]` real-image тесты запускаются явно при наличии образов; пути — от `CARGO_MANIFEST_DIR` крейта uefi-engine (`../../../refs/…`).

---

### Task 0: Ветка цикла

**Files:** — (только git)

- [ ] **Step 1: Создать ветку от master**

```bash
git checkout master && git pull --ff-only 2>/dev/null; git checkout -b varstore-contract
```

Все последующие кодовые коммиты — в этой ветке. Docs-коммиты (правки спеки/TODO/плана) — тоже в ней: цикл мержится целиком, история docs-правок едет вместе (отступление от прецедента form-export оправдано: правки спеки по находкам реализации должны быть видимы в ветке).

---

### Task 1: values::varstore_map — name-value декларации (спека §1)

**Files:**
- Modify: `crates/uefi-engine/src/hii/values.rs` (impl `varstore_map` ~:182, тесты мод ниже)
- Test: тот же файл, `mod tests`

**Interfaces:**
- Consumes: `r_efi::hii::IFR_VARSTORE_NAME_VALUE_OP` (=0x25), существующие тест-хелперы `opcode`, `form_set`, `package`, `end`.
- Produces: `VarStoreMap { id: u16, guid: Option<Guid> /* None для name-value */, size: u16 /* 0 для name-value */, name: String }` — расширенная карта, используемая задачами 2, 3, 7.

Формат IFR_VARSTORE_NAME_VALUE_OP: Header(2) + VarStoreId u16@+2 + Name UCS-2z@+4; минимальная длина 6.

- [ ] **Step 1: Failing test — фикстура + чтение 0x25**

В `mod tests` (values.rs), рядом с `varstore_buffer`/`varstore_efi`:

```rust
    fn varstore_name_value(id: u16, name_ucs2: &str) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        for u in name_ucs2.encode_utf16().chain(std::iter::once(0)) {
            p.extend_from_slice(&u.to_le_bytes());
        }
        opcode(IFR_VARSTORE_NAME_VALUE_OP, false, &p)
    }

    #[test]
    fn varstore_map_reads_name_value_declarations() {
        let mut ifr = form_set(7);
        ifr.extend(varstore_name_value(9, "NV"));
        ifr.extend(end());
        let map = varstore_map(&package(&ifr));
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].id, 9);
        assert_eq!(map[0].guid, None);
        assert_eq!(map[0].size, 0);
        assert_eq!(map[0].name, "NV");
    }
```

Импорт `IFR_VARSTORE_NAME_VALUE_OP` в use-список r_efi::hii наверху values.rs.

- [ ] **Step 2: Run — падает**

Run: `cargo test -p uefi-engine varstore_map_reads_name_value`
Expected: FAIL — карта пуста (0 assertions about len 1).

- [ ] **Step 3: Implement — третья ветка в varstore_map**

В `pub fn varstore_map` добавить arm (по образцу существующих):

```rust
        IFR_VARSTORE_NAME_VALUE_OP if len >= 6 => out.push(VarStoreMap {
            id: u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]),
            guid: None,
            size: 0,
            name: ucs2_strz(&pkg[off + 4..off + len]),
        }),
```

- [ ] **Step 4: Run — проходит + регресс соседей**

Run: `cargo test -p uefi-engine --lib hii::values`
Expected: PASS (все, вкл. `varstore_map_stops_on_truncated_package`).

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/values.rs
git commit -m "feat(engine): varstore_map читает IFR_VARSTORE_NAME_VALUE (guid=None, size=0, занятость id) — спека varstore-contract §1"
```

---

### Task 2: engine — list_varstores (спека §3, движковая половина)

**Files:**
- Modify: `crates/uefi-engine/src/hii/mod.rs` (новая pub fn рядом с `question_info`/set_value-зоной)
- Test: тот же файл, тестовый мод (переиспользовать `question_add_flash_image`)

**Interfaces:**
- Consumes: `values::varstore_map` (Task 1), `form_add::resource_forms_package`, `crate::parser::target::{parse_target, find_item}`, `uefi_proto::VarStoreInfo { id: u32, guid: String, size: u32, name: String }`.
- Produces: `pub fn list_varstores(image: &Image, item_id: &str) -> Result<Vec<uefi_proto::VarStoreInfo>, HiiError>` — её зовут RPC-хендлер (Task 4) и real-image гейты (Task 7). Ошибки: NotFound (bad target / нет `#`-цифры), NotASetupItem.

- [ ] **Step 1: Failing test**

В тестовый mod.rs-модуль (там, где живут `ITEM_FORMSET`-фикстуры; если `question_add_flash_image()` недоступна из этого мода — использовать тот же источник образа, что в `form_varstore_tests`):

```rust
    #[test]
    fn list_varstores_returns_formset_declarations() {
        let (flash, _, _) = question_add_flash_image();
        let img = parse_image(&flash, ImageMode::Read, "i", "s").unwrap();
        let stores = list_varstores(&img, ITEM_FORMSET).unwrap();
        assert!(stores.iter().any(|v| v.id == 1 && v.size > 0),
            "базовый формсет фикстуры декларирует varstore 1, got {stores:?}");
        assert!(stores.iter().all(|v| {
            v.guid.parse::<crate::types::Guid>().is_ok() || v.guid.is_empty()
        }));
        assert_eq!(list_varstores(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x01:0")
            .unwrap_err(),
            HiiError::NotASetupItem);
        assert_eq!(list_varstores(&img, "00000000-0000-0000-0000-000000000000:0x10:0")
            .unwrap_err(),
            HiiError::NotFound);
    }
```

Точный набор деклараций фикстуры проверить прогоном (первый запуск теста покажет фактическую карту — ассерт на конкретный id/size уточнить по факту; это пиннинг, не гадание).

- [ ] **Step 2: Run — падает**

Run: `cargo test -p uefi-engine list_varstores`
Expected: FAIL — функция не существует (compile error).

- [ ] **Step 3: Implement**

```rust
/// Карта varstore-деклараций формсета (спека varstore-contract §3):
/// item_id — грамматика `hii form add` (`<target>` или `<target>#<n>`;
/// карта не зависит от formset-ординала — это пакет формсета).
/// Read-only, образ в любом режиме. guid="" — name-value декларация.
pub fn list_varstores(image: &Image, item_id: &str) -> Result<Vec<uefi_proto::VarStoreInfo>, HiiError> {
    let (target_str, _formset_idx) = match item_id.rsplit_once('#') {
        Some((t, n)) => match n.parse::<u16>() {
            Ok(idx) => (t, idx as usize),
            Err(_) => return Err(HiiError::NotFound),
        },
        None => (item_id, 0),
    };
    let target = crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    let node = crate::parser::target::find_item(&image.root, &target)
        .map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let pkg: &[u8] = if node.subtype == EFI_SECTION_RAW && ifr::is_form_package(&node.body) {
        &node.body
    } else if node.subtype == EFI_SECTION_PE32 {
        let (off, len) = form_add::resource_forms_package(&node.body)
            .ok_or(HiiError::NotASetupItem)?;
        node.body.get(off..off + len).ok_or(HiiError::InvalidIfr)?
    } else {
        return Err(HiiError::NotASetupItem);
    };
    Ok(values::varstore_map(pkg)
        .into_iter()
        .map(|m| uefi_proto::VarStoreInfo {
            id: u32::from(m.id),
            guid: m.guid.map(crate::guid_to_upper_string).unwrap_or_default(),
            size: u32::from(m.size),
            name: m.name,
        })
        .collect())
}
```

- [ ] **Step 4: Run — проходит**

Run: `cargo test -p uefi-engine list_varstores && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS, clippy чист.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): list_varstores — карта деклараций формсета по item_id form add (RAW bare / PE32 resource, name-value guid=empty) — спека §3"
```

---

### Task 3: engine — валидации add_form (спека §2, закрывает TODO:496)

**Files:**
- Modify: `crates/uefi-engine/src/hii/form_add.rs` (начало `add_form`, после bare-детекта ~:91, до сбора строк :93)
- Test: `crates/uefi-engine/src/hii/mod.rs`, `mod form_varstore_tests`

**Interfaces:**
- Consumes: `values::varstore_map`, `question_forms_package(&image.root, &target, bare_channel)`, `schema::ItemSchema`.
- Produces: два новых отказа `InvalidSchema`: `"var store id {:#x} is not declared (add it to schema varstores)"` (per-item), `"varstore id {:#x} already exists in the formset"` (дубль декларации; тот же текст для дубля внутри пакета). Хелпер `fn item_var_store_id(item: &schema::ItemSchema) -> Option<u16>` — приватный в form_add.rs.

- [ ] **Step 1: Failing tests — 5 кейсов**

В `mod form_varstore_tests` (фикстуры `varstore_form_schema`, `ITEM_FORMSET`, паттерн «rejection must not mutate» из соседних модов):

```rust
    fn one_of_item(var_store_id: u16) -> schema::ItemSchema {
        schema::ItemSchema::OneOf(schema::OneOfItem {
            prompt: "P".into(),
            help: "H".into(),
            question_id: 0x7F01,
            var_store_id,
            var_offset: 0,
            size: 1,
            display: schema::DisplayMode::UintDec,
            options: vec![
                schema::OptionSchema { text: "Off".into(), value: 0, default: None },
                schema::OptionSchema { text: "On".into(), value: 1, default: None },
            ],
            defaults: schema::Defaults::default(),
        })
    }

    #[test]
    fn add_form_rejects_item_on_undeclared_varstore_without_mutation() {
        let (flash, _, _) = question_add_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let mut schema = varstore_form_schema();
        schema.forms[0].items.push(one_of_item(0x7F7F)); // не в формсете и не в пакете
        let err = add_form(&mut img, ITEM_FORMSET, &schema).unwrap_err();
        assert!(matches!(err, HiiError::InvalidSchema(ref m)
            if m.contains("var store id 0x7f7f is not declared")), "got {err:?}");
        assert_eq!(build_image(&img).unwrap(), flash, "rejection must not mutate");
    }

    #[test]
    fn add_form_rejects_duplicate_declared_varstore_id() {
        // id 7 уже декларирован пакетом varstore_form_schema; второй раз = дубль внутри пакета
        let (flash, _, _) = question_add_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let mut schema = varstore_form_schema();
        schema.varstores.push(schema.varstores[0].clone());
        let err = add_form(&mut img, ITEM_FORMSET, &schema).unwrap_err();
        assert!(matches!(err, HiiError::InvalidSchema(ref m)
            if m.contains("varstore id 0x7 already exists")), "got {err:?}");
        assert_eq!(build_image(&img).unwrap(), flash);
    }

    #[test]
    fn add_form_rejects_declared_id_existing_in_formset() {
        // выяснить фактический занятый id базового формсета фикстуры (см. Step 2),
        // объявить его же в пакете → дубль с формсетом
        let (flash, _, _) = question_add_flash_image();
        let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let busy = list_varstores(&img, ITEM_FORMSET).unwrap()[0].id as u16;
        drop(img);
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let mut schema = varstore_form_schema();
        schema.varstores[0].id = busy; // теперь конфликтует с формсетом
        let err = add_form(&mut img, ITEM_FORMSET, &schema).unwrap_err();
        assert!(matches!(err, HiiError::InvalidSchema(ref m)
            if m.contains("already exists in the formset")), "got {err:?}");
        assert_eq!(build_image(&img).unwrap(), flash);
    }

    #[test]
    fn add_form_accepts_item_on_package_declared_varstore() {
        let (flash, _, _) = question_add_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let mut schema = varstore_form_schema();
        schema.varstores[0].id = 7;
        schema.forms[0].items.push(one_of_item(7)); // объявлен пакетом
        let res = add_form(&mut img, ITEM_FORMSET, &schema);
        assert!(res.is_ok(), "got {:?}", res.unwrap_err());
    }

    #[test]
    fn add_form_accepts_item_on_existing_formset_varstore() {
        let (flash, _, _) = question_add_flash_image();
        let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let busy = list_varstores(&img, ITEM_FORMSET).unwrap()[0].id as u16;
        drop(img);
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let mut schema = varstore_form_schema();
        schema.varstores.clear(); // не декларируем ничего — item на существующий id
        schema.forms[0].items.push(one_of_item(busy));
        let res = add_form(&mut img, ITEM_FORMSET, &schema);
        assert!(res.is_ok(), "got {:?}", res.unwrap_err());
    }
```

- [ ] **Step 2: Run — падают**

Run: `cargo test -p uefi-engine form_varstore_tests`
Expected: два reject-теста FAIL (добавление проходит молча), accept-тесты могут пройти уже (это фиксация контракта). Фактический занятый id фикстуры узнать отсюда же (первый прогон покажет; при расхождении с `busy = …[0].id` — docs-фикс плана по правилу 11).

- [ ] **Step 3: Implement — чеки до мутаций в add_form**

В form_add.rs, сразу после `let bare_channel = { … };` (:91) и `let owner_file_guid…` — ДО сбора strings:

```rust
    validate_form_varstores(image, &target, bare_channel, schema)?;
```

Новые приватные функции в form_add.rs:

```rust
fn item_var_store_id(item: &schema::ItemSchema) -> Option<u16> {
    match item {
        schema::ItemSchema::OneOf(i) => Some(i.var_store_id),
        schema::ItemSchema::CheckBox(i) => Some(i.var_store_id),
        schema::ItemSchema::Numeric(i) => Some(i.var_store_id),
        schema::ItemSchema::String(i) => Some(i.var_store_id),
        schema::ItemSchema::OrderedList(i) => Some(i.var_store_id),
        schema::ItemSchema::Text(_) | schema::ItemSchema::Ref(_) | schema::ItemSchema::Action(_) => None,
    }
}

/// Спека varstore-contract §2: per-item ссылки и декларации пакета
/// проверяются против карты формсета ДО первой мутации образа.
fn validate_form_varstores(
    image: &Image,
    target: &crate::types::Target,
    bare_channel: bool,
    schema: &schema::FormSetSchema,
) -> Result<(), HiiError> {
    let pkg = question_forms_package(&image.root, target, bare_channel)?.to_vec();
    let existing = super::values::varstore_map(&pkg);
    let mut seen: Vec<u16> = Vec::new();
    for vs in &schema.varstores {
        if existing.iter().any(|m| m.id == vs.id) || seen.contains(&vs.id) {
            return Err(HiiError::InvalidSchema(format!(
                "varstore id {:#x} already exists in the formset",
                vs.id
            )));
        }
        seen.push(vs.id);
    }
    for form in &schema.forms {
        for item in &form.items {
            let Some(id) = item_var_store_id(item) else { continue };
            if id != 0
                && !seen.contains(&id)
                && !existing.iter().any(|m| m.id == id)
            {
                return Err(HiiError::InvalidSchema(format!(
                    "var store id {:#x} is not declared (add it to schema varstores)",
                    id
                )));
            }
        }
    }
    Ok(())
}
```

`question_forms_package` — pub(crate) в hii/mod.rs (сейчас без `pub`): поднять видимость одной правкой в mod.rs (это не дефект плана — conscious API-расширение крейта).

- [ ] **Step 4: Run — все проходят + весь crate**

Run: `cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS. Если существующие тесты add_form/formset_add/page_add упали на новых отказах — прочитать каждый: это осознанные отказы спеки §2; фикстуры этих тестов поправить (добавить декларацию в varstores или существующий id) В ЭТОМ ЖЕ коммите с пояснением в теле commit-message. Если падение не объясняется §2 — стоп, docs-фикс плана.

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/hii/form_add.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "feat(engine): add_form валидирует var_store_id items и дубли деклараций против карты формсета до мутаций (зеркало check_question_add) — спека §2, TODO:496"
```

---

### Task 4: proto + RPC HiiListVarstores (спека §3)

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (service + messages рядом с HiiListForms:28/:168)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (хендлер рядом с `hii_list_forms` :772)

**Interfaces:**
- Consumes: `crate::hii::list_varstores` (Task 2), `hii_error_status_ctx`.
- Produces: `HiiListVarstoresRequest { image_id, target }` / `HiiListVarstoresResponse { repeated VarStoreInfo varstores }`, RPC-метод `HiiListVarstores` — потребляется Task 5 (CLI client), Task 6 (TUI), моками.

- [ ] **Step 1: Proto-правка**

В engine.proto, service-блок (после строки 28 `rpc HiiListForms…`):

```proto
  rpc HiiListVarstores(HiiListVarstoresRequest)             returns (HiiListVarstoresResponse);
```

Message-блок (рядом с `HiiListFormsResponse` :175):

```proto
message HiiListVarstoresRequest { string image_id = 1; string target = 2; }
message HiiListVarstoresResponse { repeated VarStoreInfo varstores = 1; }
```

`VarStoreInfo` (:269) уже существует и уже получает `#[derive(serde::Serialize)]` в build.rs — новых message_attribute не нужно (CLI сериализует Vec<VarStoreInfo>).

- [ ] **Step 2: Хендлер**

server.rs, после `hii_list_forms`:

```rust
    #[tracing::instrument(skip(self, req), err)]
    async fn hii_list_varstores(
        &self,
        req: Request<HiiListVarstoresRequest>,
    ) -> RpcResult<HiiListVarstoresResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let varstores = crate::hii::list_varstores(&img, &r.target)
            .map_err(|e| hii_error_status_ctx(e, &r.target))?;
        let _ = self.sm.touch(&img.session_id);
        tracing::info!(image_id = %r.image_id, target = %r.target, varstores = varstores.len(), "hii list varstores");
        Ok(Response::new(HiiListVarstoresResponse { varstores }))
    }
```

Import `HiiListVarstoresRequest/Response` в use-список uefi_proto::* (там wildcard — ничего не делать, проверить).

- [ ] **Step 3: Build + engine green**

Run: `cargo test -p uefi-proto && cargo test -p uefi-engine && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (хендлер тонкий, покрывается моками Task 5/6; компиляция = контракт trait).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-proto/proto/engine.proto crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(proto,rpc): HiiListVarstores — read-only карта varstore-деклараций формсета по target — спека §3"
```

---

### Task 5: CLI `hii varstore list` (спека §4)

**Files:**
- Modify: `crates/uefi-common/src/format.rs` (HiiLegendCmd + hii_legend + тест)
- Modify: `crates/uefi-cli/src/client.rs` (обёртка), `main.rs` (HiiCmd), `commands/hii.rs` (varstore_list), `output.rs` (print_varstores)
- Test: format.rs tests; `crates/uefi-cli/tests/mock_server.rs` (мок-метод), `crates/uefi-cli/tests/e2e.rs` (tsv-гейт)

**Interfaces:**
- Consumes: `HiiListVarstoresRequest/Response` (Task 4), `hii_legend`/`HiiLegendCmd`, `target_section_codes`.
- Produces: `Client::hii_list_varstores(&mut self, image_id: &str, target: &str) -> Result<Vec<VarStoreInfo>, AppError>`; `HiiLegendCmd::VarstoreList`; `commands::hii::varstore_list(item_id, cli_sock, format)`; `output::print_varstores(&[VarStoreInfo], OutputFormat)`.

- [ ] **Step 1: Failing test — легенда (uefi-common)**

format.rs, рядом с `hii_legend_questions_variant`:

```rust
    #[test]
    fn hii_legend_varstore_variant() {
        let leg = hii_legend(HiiLegendCmd::VarstoreList, &[0x10]);
        assert!(leg.contains("item_id = <ffs-file-guid>:<section-type>:<index>"));
        assert!(leg.contains("guid '-' = name-value declaration (no GUID, no bounds)"));
        assert!(leg.contains("10 = PE32"));
    }
```

- [ ] **Step 2: Run — падает**

Run: `cargo test -p uefi-common hii_legend_varstore`
Expected: FAIL — варианта VarstoreList нет (compile error).

- [ ] **Step 3: Implement — enum + ветка**

```rust
pub enum HiiLegendCmd {
    FormList,
    QuestionList,
    VarstoreList,
}
```

В `hii_legend` добавить match-arm после QuestionList-ветки (точный вид существующих веток скопировать по месту):

```rust
        HiiLegendCmd::VarstoreList => out.push_str(
            "  columns: id / guid / size / name\n  guid '-' = name-value declaration (no GUID, no bounds)\n",
        ),
```

- [ ] **Step 4: Run — проходит**

Run: `cargo test -p uefi-common && cargo clippy -p uefi-common -- -D warnings`
Expected: PASS.

- [ ] **Step 5: CLI — client + команда + вывод**

client.rs (по образцу `hii_list_forms` :278):

```rust
    pub async fn hii_list_varstores(
        &mut self,
        image_id: &str,
        target: &str,
    ) -> Result<Vec<VarStoreInfo>, AppError> {
        let req = HiiListVarstoresRequest {
            image_id: image_id.into(),
            target: target.into(),
        };
        Ok(self
            .inner
            .hii_list_varstores(auth_req(&self.state, req))
            .await?
            .into_inner()
            .varstores)
    }
```

main.rs — в `enum HiiCmd` (:204) добавить вариант и матч-руку диспетчера (:427):

```rust
    Varstore {
        item_id: String,
    },
```

```rust
                HiiCmd::Varstore { item_id } => commands::hii::varstore_list(&item_id, sock, format).await,
```

(имена полей/формат — выровнять по соседним HiiCmd-вариантам; about-строка `hii varstore list <item_id>`.)

commands/hii.rs (по образцу `form_list`):

```rust
pub async fn varstore_list(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let target = item_id.rsplit_once('#').map(|(t, _)| t).unwrap_or(item_id);
    let stores = client.hii_list_varstores(&image_id, target).await?;
    if format == OutputFormat::Text {
        let codes = target_section_codes(std::iter::once(target));
        eprint!(
            "{}",
            uefi_common::format::hii_legend(uefi_common::format::HiiLegendCmd::VarstoreList, &codes)
        );
    }
    crate::output::print_varstores(&stores, format);
    Ok(())
}
```

output.rs:

```rust
pub fn print_varstores(stores: &[VarStoreInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(stores).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("id\tguid\tsize\tname");
            for s in stores {
                println!("{}\t{}\t{}\t{}", s.id, s.guid, s.size, s.name);
            }
        }
        OutputFormat::Text => {
            for s in stores {
                println!(
                    "id={:#06x} guid={} size={:#06x} name=\"{}\"",
                    s.id,
                    if s.guid.is_empty() { "-".to_string() } else { s.guid.clone() },
                    s.size,
                    s.name
                );
            }
        }
    }
}
```

`VarStoreInfo` — в use uefi_proto::* (wildcard уже есть).

- [ ] **Step 6: Мок + e2e**

tests/mock_server.rs — impl метода трейта (по образцу `hii_list_forms` :190):

```rust
    async fn hii_list_varstores(
        &self,
        _req: Request<HiiListVarstoresRequest>,
    ) -> Result<Response<HiiListVarstoresResponse>, Status> {
        Ok(Response::new(HiiListVarstoresResponse {
            varstores: vec![VarStoreInfo {
                id: 2,
                guid: "EC87D643-99DC-4D14-B25D-8AC6D5C7B27A".into(),
                size: 0x94,
                name: "Setup".into(),
            }],
        }))
    }
```

tests/e2e.rs (по образцу tsv-подкейса question info):

```rust
#[tokio::test]
async fn hii_varstore_list_tsv() {
    let out = run_cli(&["--format", "tsv", "hii", "varstore", "list",
        "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0"]).await;
    out.assert_success();
    out.assert_stdout_contains("id\tguid\tsize\tname");
    out.assert_stdout_contains("2\tEC87D643-99DC-4D14-B25D-8AC6D5C7B27A\t148\tSetup");
}
```

(имена хелперов `run_cli`/`assert_*` — по фактическим в e2e.rs; если структура другая — зеркалить ближайший tsv-тест.)

- [ ] **Step 7: Run + clippy**

Run: `cargo test -p uefi-cli && cargo clippy -p uefi-cli -- -D warnings`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/uefi-common/src/format.rs crates/uefi-cli/src/client.rs crates/uefi-cli/src/main.rs crates/uefi-cli/src/commands/hii.rs crates/uefi-cli/src/output.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-cli/tests/e2e.rs
git commit -m "feat(cli): hii varstore list <item_id> — карта формсета (text+легенда в stderr/json/tsv), VarstoreList-легенда — спека §4"
```

---

### Task 6: TUI — панель varstores 'V' в Forms View (спека §5)

**Files:**
- Modify: `crates/uefi-tui/src/app.rs` (FormsData поля, cursor-хелперы, ListState)
- Modify: `crates/uefi-tui/src/commands.rs` (refresh_varstores), `main.rs` (клавиши 'V'/Esc/j/k), `ui/mod.rs` (рендер-ветка), `ui/forms.rs` (render_varstores)
- Test: `crates/uefi-tui/tests/mock_server.rs` (мок RPC); юниты в app.rs

**Interfaces:**
- Consumes: `HiiListVarstoresRequest/Response` (Task 4); паттерн strings-панели: `FormsData.show_strings/strings/strings_cursor`, `App.strings_list_state`, `commands::refresh_strings`, `AppEvent::Key('S')` (main.rs:304).
- Produces: `FormsData { show_varstores: bool, varstores: Vec<VarStoreInfo>, varstores_target: Option<String>, varstores_cursor: usize }`, `App::varstores_cursor_down/up`, `App.varstores_list_state: ListState`, `commands::refresh_varstores(app, client, target) -> Result<(), String>`, `ui::forms::render_varstores`.

Клавиша `'V'` (Shift+v) свободна — нижняя `'v'` занята toggle-visibility (main.rs:313), конфликта нет (map_key даёт разные Char).

- [ ] **Step 1: Failing tests — состояние и курсор (app.rs tests)**

```rust
    #[test]
    fn varstores_panel_toggles_and_clamps_cursor() {
        let mut app = App::default();
        app.forms.varstores = vec![
            VarStoreInfo { id: 1, guid: "A".into(), size: 4, name: "One".into() },
            VarStoreInfo { id: 2, guid: "B".into(), size: 8, name: "Two".into() },
        ];
        assert!(!app.forms.show_varstores);
        app.forms.show_varstores = true;
        app.varstores_cursor_down();
        assert_eq!(app.forms.varstores_cursor, 1);
        app.varstores_cursor_down();
        assert_eq!(app.forms.varstores_cursor, 1, "clamp at last");
        app.varstores_cursor_up();
        assert_eq!(app.forms.varstores_cursor, 0);
    }
```

(конструктор VarStoreInfo — проверить фактические поля proto-структуры; u32 id/size.)

- [ ] **Step 2: Run — падает**

Run: `cargo test -p uefi-tui varstores`
Expected: FAIL — полей/методов нет (compile error).

- [ ] **Step 3: Implement — App/FormsData**

app.rs FormsData (по образцу strings-полей :83–86):

```rust
    pub show_varstores: bool,
    pub varstores: Vec<VarStoreInfo>,
    pub varstores_target: Option<String>,
    pub varstores_cursor: usize,
```

App: `pub varstores_list_state: ratatui::widgets::ListState,` + init в Default. Cursor-методы (копия strings_cursor_down/up :585–603 с заменой поля; без фильтра — список мал):

```rust
    pub fn varstores_cursor_down(&mut self) {
        if self.forms.varstores.is_empty() { return; }
        self.forms.varstores_cursor = (self.forms.varstores_cursor + 1)
            .min(self.forms.varstores.len() - 1);
    }
    pub fn varstores_cursor_up(&mut self) {
        self.forms.varstores_cursor = self.forms.varstores_cursor.saturating_sub(1);
    }
```

- [ ] **Step 4: commands + клавиши + рендер**

commands.rs (по образцу refresh_strings :1753; target — цель формы под курсором, `"<ffs>:<type>:<idx>"` из FormKey):

```rust
pub async fn refresh_varstores(
    app: &mut App,
    client: &mut Client,
    target: &str,
) -> Result<(), String> {
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let resp = client
        .inner
        .hii_list_varstores(auth_req(
            &client.state,
            HiiListVarstoresRequest { image_id, target: target.into() },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.forms.varstores = resp.varstores;
    app.forms.varstores_target = Some(target.to_string());
    app.forms.varstores_cursor = 0;
    Ok(())
}
```

main.rs — рядом с `AppEvent::Key('S')` (:304). Как получить target: у соседних обработчиков используется `commands::selected_form_item_id(app)`; взять его target-часть `rsplit_once('#')` (если хелпер не отдаёт target — добавить `pub fn selected_form_target(app) -> Option<String>` рядом, по образцу):

```rust
        AppEvent::Key('V') => {
            let need_fetch = match (&app.forms.varstores_target, commands::selected_form_target(app)) {
                (Some(cached), Some(cur)) if cached == &cur => false,
                (_, Some(_)) => true,
                _ => false,
            };
            app.forms.show_varstores = !app.forms.show_varstores;
            if app.forms.show_varstores
                && need_fetch
                && let Some(target) = commands::selected_form_target(app)
                && let Some(c) = client.as_mut()
                && let Err(e) = commands::refresh_varstores(app, c, &target).await
            {
                app.status_msg = format!("error: {e}");
            }
        }
        AppEvent::Key('j') | AppEvent::Down if app.forms.show_varstores => app.varstores_cursor_down(),
        AppEvent::Key('k') | AppEvent::Up if app.forms.show_varstores => app.varstores_cursor_up(),
        AppEvent::Esc if app.forms.show_varstores => app.forms.show_varstores = false,
```

Существующие j/k-руки Forms View ограничить `if !app.forms.show_varstores` (по образцу show_strings-ограничений :242–291 — у большинства уже есть `&& !app.forms.show_strings`, дополнить до `&& !app.forms.show_varstores`).

ui/mod.rs (:53 — ветка show_strings) добавить симметричную ветку show_varstores → `ui::forms::render_varstores(f, area, app)`.

ui/forms.rs (по образцу render_strings :159; скролл — `compute_scrolled_offset` из ui/scroll.rs + varstores_list_state):

```rust
pub fn render_varstores(f: &mut Frame, area: Rect, app: &mut App) {
    let target = app.forms.varstores_target.clone().unwrap_or_default();
    let rows: Vec<ListItem> = std::iter::once(ListItem::new(format!("Varstores: {target}")))
        .chain(app.forms.varstores.iter().map(|v| {
            ListItem::new(format!(
                "id={:#06x} guid={} size={:#06x} \"{}\"",
                v.id,
                if v.guid.is_empty() { "-".into() } else { v.guid.clone() },
                v.size,
                v.name
            ))
        }))
        .collect();
    /* List + ListState-скролл по образцу render_strings */
}
```

- [ ] **Step 5: Мок TUI**

tests/mock_server.rs — тот же мок-метод, что в Task 5 Step 6 (VarStoreInfo Setup id 2).

- [ ] **Step 6: Run + clippy**

Run: `cargo test -p uefi-tui && cargo clippy -p uefi-tui -- -D warnings`
Expected: PASS (вкл. существующие strings-тесты — их guard'ы не сломаны).

- [ ] **Step 7: Commit**

```bash
git add crates/uefi-tui/src/app.rs crates/uefi-tui/src/commands.rs crates/uefi-tui/src/main.rs crates/uefi-tui/src/ui/mod.rs crates/uefi-tui/src/ui/forms.rs crates/uefi-tui/tests/mock_server.rs
git commit -m "feat(tui): панель 'V' в Forms View — varstore-карта формсета под курсором (ленивый RPC + кэш per target, j/k/Esc) — спека §5"
```

---

### Task 7: Real-image гейты — HNX-карта + 450x-зонд ids 21–30 (спека §7, закрывает TODO:3029)

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новые `#[ignore]`-тесты; образы-константы уже есть: HNX `refs/fw/HNX99TF_…`, 450x `450x — копия.bin` :28)

**Interfaces:**
- Consumes: `uefi_engine::hii::list_varstores` (Task 2), `uefi_engine::hii::check_question_add` + `schema::QuestionAddSchema` (существующие; сигнатура — hii/mod.rs:1513), `uefi_engine::parser::image::…` (по образцу соседних real-тестов открытия образа).

- [ ] **Step 1: Тест-гейт HNX-карты**

```rust
/// Спека varstore-contract §7: карта деклараций корневого Setup на живом
/// образе — полнота walker'а по всем трём видам опкодов.
#[test]
#[ignore = "real image required"]
fn real_hii_varstore_map_hnx() {
    let img = open_hnx_read(); // хелпер-паттерн соседних real-тестов (по образцу real_image_hii_*)
    let stores = uefi_engine::hii::list_varstores(
        &img,
        "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0",
    )
    .unwrap();
    assert!(stores.iter().any(|v| v.id == 2
        && v.guid.starts_with("EC87D643")
        && v.size == 0x94
        && v.name == "Setup"), "HNX Setup varstore 2/EC87D643/0x94, got {stores:?}");
    assert!(stores.iter().map(|v| v.id).collect::<std::collections::BTreeSet<_>>().len() == stores.len(),
        "ids unique");
    println!("HNX Setup varstores: {stores:#?}");
}
```

`open_hnx_read` — если такого хелпера нет, открыть образ напрямую (паттерн строк 28–45: `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(...)` + parse_image Read-режим).

- [ ] **Step 2: Зонд 450x 21–30**

```rust
/// Спека varstore-contract §7: занятость varstore ids 21–30 на 450x.
/// Метод TODO:3029 — check_question_add (без мутаций) с var_offset 0xFFFE:
/// «not declared» = id свободен, «exceeds var store size» = занят.
#[test]
#[ignore = "real image required"]
fn real_450x_varstore_ids_21_30_probe() {
    let img = open_450x_write(); // образ «450x — копия.bin», ImageMode::Write (check требует Write)
    let target = format!("{RC_SETUP_FFS}:0x10:0"); // RC-формсет: известные 1..20 заняты
    let mut report = Vec::new();
    for id in 21u16..=30 {
        let schema = uefi_engine::hii::schema::QuestionAddSchema {
            // поля — по фактической структуре (numeric, question_id 0x7F7F,
            // var_store_id = id, var_offset 0xFFFE, size 1, min 0, max 0xFF, step 1)
            ..numeric_probe_schema(id)
        };
        let err = uefi_engine::hii::check_question_add(&img, &format!("{target}#118"), &[schema], &[])
            .unwrap_err();
        let status = if err.to_string().contains("is not declared") {
            "free"
        } else if err.to_string().contains("exceeds var store size") {
            "busy"
        } else {
            panic!("id {id}: unexpected error {err:?}")
        };
        report.push((id, status));
    }
    println!("450x varstore ids 21-30: {report:?}");
}
```

Детали (форма-таргет `#118`, поля QuestionAddSchema, вспомогательный конструктор `numeric_probe_schema`) — выровнять по фактическим сигнатурам; подходящая форма — та, где check доходит до declared-size-проверки (пустая форма с ctrl-шаблоном; при NotFound ctrl → попробовать соседние формы, список даст `hii`-тестов real-файла). Если check падает раньше по несвязанным причинам ($SPF-страница и пр.) — docs-фикс плана с фактической причиной и корректировкой метода.

- [ ] **Step 3: Прогон с образами**

Run: `cargo test -p uefi-engine --test real_image real_ -- --ignored --nocapture`
Expected: оба PASS; в выводе — таблица 21–30 (free/busy) и карта HNX.

- [ ] **Step 4: Фиксация знания — docs (в ветке)**

Результат зонда внести в спеку §7.1 (таблица) — коммит в ветке (спека едет в master при merge цикла):

```bash
# правка docs/superpowers/specs/2026-09-19-varstore-contract-design.md §7.1
git add docs/superpowers/specs/2026-09-19-varstore-contract-design.md
git commit -m "docs(spec): varstore-contract §7.1 — таблица занятости ids 21–30 по прогону зонда (450x)"
```

- [ ] **Step 5: Commit (код — в ветке)**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(engine): real-image гейты varstore-карты HNX + зонд занятости ids 21–30 на 450x — спека §7, TODO:3029"
```

---

### Task 8: Тест-долги value-op: width/DEFAULT покрытие + 7 веток set_value (спека §8: TODO:2673, TODO:2676)

**Files:**
- Test: `crates/uefi-engine/src/hii/values.rs` (mod tests — фикстуры `numeric_op`, `default_op` уже есть)
- Test: `crates/uefi-engine/src/hii/mod.rs` (mod tests — фикстуры `image_with_nvar_stores`, `value_forms_pkg`, `g_*`-билдеры уже есть)

**Interfaces:**
- Consumes: `r_efi::hii::IFR_NUMERIC_SIZE_2/4` (константы есть; SIZE_1=0x00, SIZE_2=0x01, SIZE_4=0x02, 8 = `IFR_NUMERIC_SIZE` =0x03), `find_question`/`questions` walker, `validate_set_value`, `set_value`.

- [ ] **Step 1: values.rs — параметризованный width/DEFAULT-набор**

```rust
    #[test]
    fn numeric_width_2_4_8_read_from_size_flags() {
        for (flags, width) in [
            (IFR_NUMERIC_SIZE_2, 2u8),
            (IFR_NUMERIC_SIZE_4, 4),
            (r_efi::hii::IFR_NUMERIC_SIZE, 8),
        ] {
            let pkg = package(&[form_set(7), numeric_op(0x11, flags, 0, 0xFF, 1), end()].concat());
            let qs = questions(&pkg, 7);
            assert_eq!(qs[0].width, width, "flags {flags:#x}");
        }
    }

    #[test]
    fn default_types_2_to_4_read() {
        for t in 2u8..=4 {
            let pkg = package(&[form_set(7), g_one_of_q(), default_op(0, t, &[0x01]), end()].concat());
            let qs = questions(&pkg, 7);
            assert_eq!(qs[0].defaults.len(), 1, "type {t}");
            assert_eq!(qs[0].defaults[0].type_, t);
        }
    }
```

(`g_one_of_q` — минимальный one_of-вопрос; если такого хелпера нет — собрать из `one_of_4g()` с `default_op` перед ним. `questions`/`find_question` — фактическое имя walker-функции values.rs.)

- [ ] **Step 2: mod.rs — 7 веток ValueOpUnsupported**

Используя `vendor_image_with` + `image_with_nvar_stores` + map-фабрику (литерал `values::QuestionMap` — по образцу теста :3782):

| кейс | конструкция | assert contains |
|---|---|---|
| kind Other | map kind=Other | "question kind is not value-settable" |
| width 0 / width 9 | map width=0 / 9 | "width {n} is not settable" |
| value не влезает | width=1, value=0x100 | "does not fit in 8 bits" |
| CheckBox>1 | kind=CheckBox width=1 value=2 | "checkbox accepts only 0 or 1" |
| Numeric вне диапазона | kind=Numeric min=0 max=1 value=5 | "outside numeric range" |
| nameless varstore | пакет: one_of var_store_id=1 без декларации (тест :3770 уже есть — этот кейс ДОПОЛНИТЬ проверкой «varstore has no name»: декларация с пустым именем `varstore_buffer(1, 4, "")`) | "has no name" |
| var_offset+width>size | декларация size=4, вопрос var_offset=4 width=1 | "exceeds varstore size" |
| record-missing-in-store | `image_with_nvar_stores`, вопрос со стором «Setup» size 6 (запись есть) НО var_offset, выводящий за запись — либо стор без «Setup»-записи: собрать `nvar_entry(Some("StdDefaults"), &nvar_entry(Some("Other"), …))` | "has no record" |

Каждый кейс — отдельный `#[test]` (не макро-таблица): чтение ошибки важнее компактности; имена `set_value_refuses_<case>`.

- [ ] **Step 3: Run**

Run: `cargo test -p uefi-engine set_value_refuses && cargo test -p uefi-engine --lib hii::values && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (новые тесты зелёные сразу, кроме случаев где факт. поведение расходится — тогда читать ветку и править тест по факту; расхождение с TODO-списком семи веток = docs-фикс).

- [ ] **Step 4: Commit**

```bash
git add crates/uefi-engine/src/hii/values.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "test(engine): покрытие numeric width 2/4/8, DEFAULT types 2..4 и 7 веток ValueOpUnsupported — спека §8, TODO:2673+2676"
```

---

### Task 9: NVAR-пиннинг 2730 + is_store_body leaf 2681 (спека §8)

**Files:**
- Modify: `crates/uefi-engine/src/hii/nvar.rs` (rustdoc на `find_varstore_record` :102 + пиннинг-тест в mod tests)
- Modify: `crates/uefi-engine/src/hii/mod.rs` (замыкание `is_store_body` :720 + тест)

**Interfaces:**
- Consumes: `nvar::tests::{entry, inner_fixture}` (приватные тест-фикстуры nvar.rs), `mk_node` (mod.rs tests).

- [ ] **Step 1: Failing test — пиннинг first-match**

nvar.rs tests:

```rust
    #[test]
    fn find_varstore_record_pins_first_match_on_duplicate_name_and_len() {
        // Дискриминатор (имя + data_len) осознанно матчит ПЕРВУЮ запись —
        // пиннинг контракта, смена отдельной правкой (спека §8, TODO:2730).
        let inner = nvar_dup_records_fixture(); // две записи "Setup" по 6 байт, данные различаются (0x11*6 / 0x22*6)
        let store = entry(Some("StdDefaults"), &inner, 0x82, Some(0));
        let (off, data) = find_varstore_record(&store, "Setup", 6).unwrap();
        assert_eq!(data, &[0x11u8; 6], "first record wins");
        assert_eq!(&store[off..off + 6], &[0x11u8; 6]);
    }
```

`nvar_dup_records_fixture` — по образцу `inner_fixture()`/`nvar_entry` из mod.rs-тестов (:3568 — логика `nvar_entry(Some("Setup"), &[0x11u8; 6], …)` дважды).

- [ ] **Step 2: rustdoc-контракт**

```rust
/// Первая NVAR-запись, совпавшая по (имя, data_len). Дубликаты имени и
/// длины в одном сторе не дискриминируются (осознанный контракт; на
/// живых образах AMI уникальны — спека varstore-contract §8).
pub fn find_varstore_record<'a>(
```

- [ ] **Step 3: Failing test — is_store_body leaf (mod.rs)**

```rust
    #[test]
    fn collect_std_defaults_hits_descends_into_section_with_children() {
        // Спека §8 TODO:2681: Section с детьми и телом-стором НЕ является
        // хитом — обход спускается внутрь (сторы — листья).
        let leaf_store = mk_node(FfsType::Section, nvar_store_body(), vec![]);
        let mut parent = mk_node(FfsType::Section, nvar_store_body(), vec![leaf_store]);
        parent.subtype = 0x19;
        let mut root = mk_node(FfsType::Volume, vec![], vec![parent]);
        root.node_type = FfsType::Image; // по фактической сборке фикстур
        let image = Image { image_id: "i".into(), session_id: "s".into(), root, mode: ImageMode::Write };
        let mut hits = Vec::new();
        collect_std_defaults_hits(&image.root, &mut Vec::new(), false, None, "Setup", 6, &mut hits).unwrap();
        assert_eq!(hits.len(), 1, "only the leaf store is a hit, got {hits:?}");
    }
```

(mk_node-сигнатуру/сборку образа выровнять по `image_with_nvar_stores` :3573.)

- [ ] **Step 4: Implement — ужесточение is_store_body**

mod.rs :720:

```rust
    let is_store_body = |n: &FfsNode| {
        matches!(n.node_type, FfsType::File | FfsType::Section)
            && n.children.is_empty()
            && nvar::is_std_defaults(&n.body)
    };
```

(убрана File-специфика `node_type != File || children.is_empty()` → единый leaf-критерий).

- [ ] **Step 5: Run**

Run: `cargo test -p uefi-engine --lib && cargo clippy -p uefi-engine -- -D warnings`
Expected: PASS (существующие set_value-тесты не задеты — их сторы листья).

- [ ] **Step 6: Commit**

```bash
git add crates/uefi-engine/src/hii/nvar.rs crates/uefi-engine/src/hii/mod.rs
git commit -m "test(engine): пиннинг find_varstore_record first-match + rustdoc-контракт; is_store_body ужесточён до leaf-sections — спека §8, TODO:2730+2681"
```

---

### Task 10: Микро-фиксы — сокет-дефолт (§9.1, TODO:1353) + node list text-легенда (§9.2)

**Files:**
- Modify: `crates/uefi-engine/src/bin/engine.rs` (:37–39 дефолт сокета), `crates/uefi-gateway/src/config.rs` (:14–17) + `crates/uefi-gateway/Cargo.toml` (+dep `uefi-common`)
- Modify: `crates/uefi-cli/src/output.rs` (`print_nodes` text-ветка :33–46)
- Test: uefi-gateway config-тест (если тестового модуля нет — добавить в config.rs)

**Interfaces:**
- Consumes: `uefi_common::state::default_sock()` (существует, покрыт state.rs-тестом), `uefi_common::format::format_legend`.

- [ ] **Step 1: Failing test — gateway config дефолт**

config.rs (мод tests внизу, env-очистка по образцу state.rs `env_guard`; если в крейте нет дев-зависимости на temp-env — проверять только явный путь и композицию через `default_sock()`):

```rust
    #[test]
    fn sock_default_follows_uefi_common_default_sock() {
        std::env::remove_var("UEFIPATCHER_SOCK");
        let cfg = load_config().unwrap();
        assert_eq!(cfg.sock_path, uefi_common::state::default_sock());
    }
```

- [ ] **Step 2: Implement — engine + gateway**

Cargo.toml gateway: `uefi-common = { path = "../uefi-common" }`.

engine.rs :37:

```rust
    let sock = args.sock.unwrap_or_else(uefi_common::state::default_sock);
```

+ после `std::fs::create_dir_all(&data_dir)?;` — `if let Some(parent) = sock.parent() { std::fs::create_dir_all(parent)?; }` (существующий remove_file-блок остаётся).

config.rs :16: `.unwrap_or_else(|_| uefi_common::state::default_sock())` (uefi-engine уже зависит от uefi-common — Cargo.toml:9; gateway — dep добавлен выше).

- [ ] **Step 3: node list легенда**

output.rs `print_nodes` text-ветка: легенда уже вычислима из тех же rows — перед `print!`:

```rust
            eprint!("{}", uefi_common::format::format_legend(&rows));
            print!("{}", uefi_common::format::format_tree(&rows));
```

- [ ] **Step 4: Run**

Run: `cargo test -p uefi-engine --bin engine && cargo test -p uefi-gateway && cargo test -p uefi-cli && cargo clippy --all -- -D warnings`
Expected: PASS; e2e node list text — stdout-ассерты не задеты (легенда в stderr).

- [ ] **Step 5: Commit**

```bash
git add crates/uefi-engine/src/bin/engine.rs crates/uefi-gateway/src/config.rs crates/uefi-gateway/Cargo.toml crates/uefi-cli/src/output.rs Cargo.lock
git commit -m "fix(engine,gateway,cli): дефолт сокета → uefi_common::default_sock (XDG-state, TODO:1353); node list text печатает format_legend в stderr — спека §9"
```

---

### Task 11: Финальные гейты + TODO-гигиена (спека «Гигиена TODO»)

**Files:**
- Modify: `TODO.md` (закрыть позиции)

- [ ] **Step 1: Полный гейт ветки**

Run:
```bash
cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --all -- --check
```
Expected: всё зелёное. `#[ignore]` real-image — прогнать при наличии образов (Task 7 Step 3).

- [ ] **Step 2: TODO.md — закрыть позиции**

Отметить `[x]` с кратким резюме «Закрыто: цикл varstore-contract (…)»: 490, 496, 3029 (со ссылкой на таблицу §7.1 спеки), 2673, 2676, 2730 (пиннинг; смена дискриминатора — новая позиция при живом дубле), 2681, 1353. В позиции 505-смежных ничего не трогать. Для формулировки high-id конвенции form-export — найти в TODO запись о `max(0x7F00, …)+1`/TODO:490-блокировке (в спеке form-export §Скоуп: «блокируется TODO:490») и снять блок-пометку.

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs(todo): закрыть 490/496/3029/2673/2676/2730/2681/1353 — цикл varstore-contract (таблица ids 21–30 §7.1, пиннинг 2730, form-export §3.6 разблокирован)"
```

- [ ] **Step 4: Итог ветки**

```bash
git log --oneline master..varstore-contract
```
Ожидаемо ~11 кодовых коммитов. Дальше — ревью владельцем и merge по протоколу finishing-a-development-branch.

---

## Self-Review (выполнен при написании)

- **Покрытие спеки:** §1→Task 1; §2→Task 3; §3→Task 2+4; §4→Task 5; §5→Task 6; §6→без задач (контракт для hii-form-export — по спеке реализация там); §7→Task 7; §8→Task 8+9; §9→Task 10; §10 гейты→Task 5/6/7/11; Гигиена TODO→Task 11.
- **Типы:** `list_varstores(&Image, &str) -> Result<Vec<uefi_proto::VarStoreInfo>, HiiError>` сквозь Tasks 2/4/7; `VarStoreInfo` proto-поля u32/string — единообразно в CLI/TUI.
- **Плейсхолдеры:** помеченные «по фактическим структурам» места (поля QuestionAddSchema, e2e-хелперы, mk_node-сборка) — сознательные делегирования к образцу соседнего кода с указанием точного соседа; не TBD-вакуум.
