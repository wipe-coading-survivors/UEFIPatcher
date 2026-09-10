# cli-polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Закрыть 11 CLI-находок TODO: help-строки, валидация `image switch`, NotFound-сообщения HII с целью, ArgGroup-тесты, content-ассерты stdout, дедуп мока, покрытие defaults-ветки принтера.

**Architecture:** Крошечная engine-правка (обогащение NotFound на RPC-границе, где известен target запроса) + правки крейта uefi-cli (main.rs about-строки, commands/image.rs валидация, mock_server дедуп, тесты cli_integration/e2e). Протокол, топология команд, exit-коды и форматы вывода не меняются.

**Tech Stack:** Rust workspace (edition 2024), clap derive, tonic, assert_cmd + predicates (интеграционные тесты против мок-сервера на unix-сокете).

**Spec:** `docs/superpowers/specs/2026-09-10-cli-polish-design.md` (включая правки `bd2a9cf`).

## Global Constraints

- Правило 8 AGENTS.md: никаких `//`-комментариев; rustdoc `///` только на pub/pub(crate)-функциях.
- Правило 11 AGENTS.md: дефект плана — отдельный docs-коммит ДО реализации.
- Протокол/выход не меняются, кроме: обогащение NotFound-сообщений (`"not found"` → `"{ctx} not found"`), добавление help-строк, `images_list` мока теперь возвращает непустой список.
- Exit-коды: RpcInvalidArgument → 2, state-ошибки → 3, clap-ошибки парсинга → failure (код не пинимаем).
- Ворота после каждой задачи: `cargo test -p <затронутый крейт>`, `cargo clippy -p <крейт> -- -D warnings`, `cargo fmt --all -- --check`.
- Все тесты — на мок-сервере/фикстурах; реальный BIOS-образ не нужен.
- Ветка: `fix/cli-polish` от master.

---

### Task 1: engine — NotFound несёт цель запроса (спека §3.4)

**Files:**
- Modify: `crates/uefi-engine/src/rpc/server.rs` (helper рядом с `hii_error_status` @ :42; call-sites :676, :784, :833, :878, :893, :911, :931, :960, :963, :966, :979, :1011)

**Interfaces:**
- Produces: `fn hii_error_status_ctx(e: crate::hii::HiiError, ctx: &str) -> Status` (private, rpc/server.rs). Для `HiiError::NotFound` → `Status::not_found(format!("{ctx} not found"))`, остальные варианты делегируются `hii_error_status`.

- [ ] **Step 1: Write the failing test**

В существующий `mod tests` в конце `rpc/server.rs` (рядом с `hii_error_status_maps_*`, :1137+) добавить:

```rust
    #[test]
    fn hii_error_status_ctx_enriches_only_not_found() {
        let st = hii_error_status_ctx(crate::hii::HiiError::NotFound, "0#99");
        assert_eq!(st.code(), tonic::Code::NotFound);
        assert_eq!(st.message(), "0#99 not found");

        let st = hii_error_status_ctx(crate::hii::HiiError::NotWritable, "0#99");
        assert_eq!(st.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            st.message(),
            crate::hii::HiiError::NotWritable.to_string()
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p uefi-engine hii_error_status_ctx`
Expected: FAIL — «cannot find function `hii_error_status_ctx`» (compile error).

- [ ] **Step 3: Implement the helper**

Сразу после `fn hii_error_status` (server.rs:42-62):

```rust
fn hii_error_status_ctx(e: crate::hii::HiiError, ctx: &str) -> Status {
    match e {
        crate::hii::HiiError::NotFound => Status::not_found(format!("{ctx} not found")),
        _ => hii_error_status(e),
    }
}
```

- [ ] **Step 4: Rewire the target-bearing handlers**

Заменить `.map_err(hii_error_status)?` на `.map_err(|e| hii_error_status_ctx(e, &r.item_id))?` (или `&r.target)`) в:

| Обработчик | Строка | ctx |
|---|---|---|
| `hii_set_form_visibility` | :676 | `&r.item_id` |
| `hii_form_add` | :784 | `&r.target` |
| `hii_form_hijack` | :833 (arm `_ =>`) | `&r.target` |
| `hii_gates_list` | :878 | `&r.item_id` |
| `hii_unlock` | :893 | `&r.item_id` |
| `hii_question_info` | :911 | `&r.item_id` |
| `hii_set_value` | :931 | `&r.item_id` |
| `hii_question_add` | :960, :963, :966, :979 (4 сайта) | `&r.target` |
| `hii_page_add` | :1011 | `&r.target` |

Для `hii_form_hijack` arm остаётся кастомным:

```rust
            .map_err(|e| match e {
                crate::hii::HiiError::AmiFilesNotFound => Status::not_found(e.to_string()),
                _ => hii_error_status_ctx(e, &r.target),
            })?
```

Строки даны по master на момент планирования; ориентируйся на содержимое (`crate::hii::...map_err(hii_error_status)`), а не только на номер.

- [ ] **Step 5: Run tests**

Run: `cargo test -p uefi-engine rpc`
Expected: PASS (включая новый `hii_error_status_ctx_enriches_only_not_found` и все существующие `hii_error_status_maps_*`).

- [ ] **Step 6: Lint + commit**

```bash
cargo clippy -p uefi-engine -- -D warnings
git add crates/uefi-engine/src/rpc/server.rs
git commit -m "feat(uefi-engine): hii_error_status_ctx — NotFound несёт цель из запроса (0#99 not found вместо голого not found)"
```

---

### Task 2: about-строки на все подкоманды + help на несамодостаточные аргументы (спека §3.1, пункты 1+10)

**Files:**
- Modify: `crates/uefi-cli/src/main.rs` (enum-дерево :10-280)

**Interfaces:**
- Никаких сигнатур не меняет; только `#[command(about = ...)]` / `#[arg(help = ...)]` атрибуты.

- [ ] **Step 1: Проставить about (полный список — 39 новых + 1 переформулировка)**

Стиль: английский, императив/краткое описание, нижний регистр, без точки. Существующие три (`question info` / `set-value` / `add`) НЕ трогаем.

```text
Cmd::Session            "manage engine sessions"
Cmd::Image              "open and manage BIOS images on the engine"
Cmd::Node               "inspect and edit firmware tree nodes"
Cmd::Artifact           "manage stored artifacts"
Cmd::Hii                "read and edit HII forms, strings and questions"

SessionCmd::Init        "create a session and write client state"
SessionCmd::List        "list active sessions"
SessionCmd::Destroy     "destroy the session and remove client state"

ImageCmd::Open          "open a BIOS image from the server filesystem"
ImageCmd::Switch        "set the active image for subsequent commands"
ImageCmd::Close         "close an opened image"
ImageCmd::Save          "write the active image to a file on the server"
ImageCmd::List          "list images opened in the session"
ImageCmd::Status        "show details of the active image"

NodeCmd::List           "list nodes of the active image"
NodeCmd::Search         "search nodes by name or data"
NodeCmd::Insert         "insert a file or artifact as a new node"
NodeCmd::Remove         "remove a node"
NodeCmd::Replace        "replace a node body from a file or artifact"
NodeCmd::Rebuild        "rebuild a node from its children"
NodeCmd::Extract        "extract a node body into a new artifact"

ArtifactCmd::List       "list stored artifacts"
ArtifactCmd::Import     "import a local file as an artifact"
ArtifactCmd::Export     "export an artifact to the server filesystem"

HiiCmd::Form            "form-level HII operations"
HiiCmd::FormSet         "formset-level HII operations"   # на вариант с #[command(name = "formset")] — атрибут объединить: #[command(name = "formset", about = "…")]
HiiCmd::Question        "question-level HII operations"
HiiCmd::Page            "$SPF page-table operations"
HiiCmd::String          "HII string operations"

HiiFormCmd::List        "list HII forms"
HiiFormCmd::SetVisibility "show or hide a form via its suppress scope"
HiiFormCmd::Gates       "list gates (suppress/grayout) guarding a form"
HiiFormCmd::Unlock      "flip gates to unlock a form"
HiiFormCmd::Add         "insert a form from a schema file into a live formset"
HiiFormCmd::Hijack      "replace a form via AMI SetupData hijack"

HiiFormSetCmd::Add      "append a new formset from a schema file"

HiiQuestionCmd::Gates   "list gates (suppress/grayout) guarding a question"
HiiQuestionCmd::Unlock  "flip gates to unlock a question"

HiiPageCmd::Add         "add a form to the $SPF page table (no IFR validation; create the form first)"   # переформулировка пункта 10

HiiStringCmd::List      "list HII strings"
```

- [ ] **Step 2: Проставить arg-help только на несамодостаточные аргументы**

Ровно перечень спеки §3.1 (`--mode`, `--body-only`, source-группа):

```text
NodeCmd::Insert.mode     help = "placement relative to the target"
NodeCmd::Insert.file     help = "read the new node body from a server-side file"
NodeCmd::Insert.artifact help = "reuse a stored artifact as the new node body"
NodeCmd::Replace.file    help = "read the new node body from a server-side file"
NodeCmd::Replace.artifact help = "reuse a stored artifact as the new node body"
NodeCmd::Replace.body_only help = "replace the body only, keep the node header"
NodeCmd::Extract.body_only help = "extract the body without the section header"
```

- [ ] **Step 3: Проверить сборку и существующие тесты**

Run: `cargo test -p uefi-cli`
Expected: PASS — unit-тесты парсинга (`parse_hii_*` в main.rs) зелёные, nothing broken.

- [ ] **Step 4: Ручная проверка help (все уровни)**

```bash
cargo run -p uefi-cli -- --help
cargo run -p uefi-cli -- session --help
cargo run -p uefi-cli -- image --help
cargo run -p uefi-cli -- node --help
cargo run -p uefi-cli -- artifact --help
cargo run -p uefi-cli -- hii --help
cargo run -p uefi-cli -- hii form --help
cargo run -p uefi-cli -- hii formset --help
cargo run -p uefi-cli -- hii question --help
cargo run -p uefi-cli -- hii page --help
cargo run -p uefi-cli -- hii string --help
```
Expected: у каждого варианта видна about-строка; у page add — новая формулировка.

- [ ] **Step 5: Lint + commit**

```bash
cargo clippy -p uefi-cli -- -D warnings
git add crates/uefi-cli/src/main.rs
git commit -m "feat(uefi-cli): about на все подкоманды + help на несамодостаточные аргументы; page add переформулирован"
```

---

### Task 3: image switch валидирует образ на сервере (спека §3.2, пункт 2)

**Files:**
- Modify: `crates/uefi-cli/src/commands/image.rs:28-33` (switch)
- Modify: `crates/uefi-cli/src/main.rs:314` (dispatch arm)
- Modify: `crates/uefi-cli/tests/mock_server.rs:72-77` (images_list — непустой список)
- Test: `crates/uefi-cli/tests/cli_integration.rs` (новый тест)

**Interfaces:**
- Изменяет: `commands::image::switch(image_id: &str, cli_sock: Option<&str>, _format: OutputFormat)` — новый параметр `cli_sock` (диспетчеру передавать `sock`).
- Мок `MockEngine::images_list` теперь возвращает `vec![ImageInfo { image_id: "mock-image-1", name: "mock.bin", path: "/dev/null", ..Default::default() }]`.

- [ ] **Step 1: Write the failing test**

В `cli_integration.rs` добавить:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn image_switch_validates_against_server() {
    let (td, sock) = setup_env().await;
    let cwd = td.path();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "read"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["image", "switch", "mock-image-1"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["image", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("mock-image-1"));

    cli(&sock, cwd)
        .args(["image", "switch", "no-such-image"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains(
            "image no-such-image not found on server; see 'image list'",
        ));

    cli(&sock, cwd)
        .args(["image", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains("mock-image-1"));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
```

Смысл проверок: happy-path переключает и `status` показывает новый id; опечатка даёт exit 2 и НЕ перезаписывает state (повторный `status` всё ещё `mock-image-1`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p uefi-cli --test cli_integration image_switch_validates`
Expected: FAIL — `image switch no-such-image` сейчас выходит success (молчаливая запись state), тест ждёт failure.

- [ ] **Step 3: Мок возвращает непустой список**

`mock_server.rs` `images_list` (:72-77):

```rust
    async fn images_list(
        &self,
        _req: Request<ImagesListRequest>,
    ) -> Result<Response<ImagesListResponse>, Status> {
        Ok(Response::new(ImagesListResponse {
            images: vec![ImageInfo {
                image_id: "mock-image-1".into(),
                name: "mock.bin".into(),
                path: "/dev/null".into(),
                ..Default::default()
            }],
        }))
    }
```

- [ ] **Step 4: Реализовать валидацию в switch**

`commands/image.rs`:

```rust
pub async fn switch(
    image_id: &str,
    cli_sock: Option<&str>,
    _format: OutputFormat,
) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st.clone()).await?;
    let images = client.images_list().await?;
    if !images.iter().any(|i| i.image_id == image_id) {
        return Err(AppError::new(
            uefi_common::error::ErrKind::RpcNotFound,
            format!("image {image_id} not found on server; see 'image list'"),
        ));
    }
    st.active_image_id = Some(image_id.into());
    state::write_state(&st)?;
    Ok(())
}
```

Диспетчер `main.rs:314`:

```rust
            ImageCmd::Switch { image_id } => {
                commands::image::switch(image_id, sock, format).await
            }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p uefi-cli --test cli_integration`
Expected: PASS — новый тест зелёный, `full_flow` не сломан (мок-изменение расширяет список, остальные consume без точных ассертов на пустоту).

- [ ] **Step 6: Lint + commit**

```bash
cargo clippy -p uefi-cli -- -D warnings
git add crates/uefi-cli/src/commands/image.rs crates/uefi-cli/src/main.rs crates/uefi-cli/tests/mock_server.rs crates/uefi-cli/tests/cli_integration.rs
git commit -m "feat(uefi-cli): image switch валидирует образ через images_list перед записью state"
```

---

### Task 4: mock_question() дедуп + defaults-ветка принтера (спека §3.5, пункты 8+9)

**Files:**
- Modify: `crates/uefi-cli/tests/mock_server.rs` (два литерала QuestionInfo :290-319 и :327-356 → `mock_question()`)
- Modify: `crates/uefi-cli/src/output.rs` (выделить text-ветку `print_question_info` в приватный хелпер; tests-mod :519+)
- Test: `crates/uefi-cli/src/output.rs` (unit) + `crates/uefi-cli/tests/cli_integration.rs` (один stdout-ассерт)

**Interfaces:**
- Produces: `fn mock_question() -> QuestionInfo` (private, tests/mock_server.rs) — с непустыми `defaults: vec![DefaultEntry { default_id: 0, r#type: 0, value: 1 }]`.
- Produces: `fn question_info_text(q: &QuestionInfo) -> String` (private, output.rs) — text-ветка принтера, `print_question_info` печатает её через `print!`.

- [ ] **Step 1: Write the failing unit test**

В tests-mod `output.rs` добавить (импорты `VarStoreInfo`, `OptionEntry`, `DefaultEntry` из `uefi_proto` — уже есть `use uefi_proto::{...}` в модуле, дополни):

```rust
    #[test]
    fn question_info_text_prints_defaults_and_varstore() {
        let q = QuestionInfo {
            form_id: 10029,
            question_id: 0x3B,
            kind: "one_of".into(),
            var_store_id: 1,
            varstore: Some(VarStoreInfo {
                id: 1,
                guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 0x72,
                name: "Setup".into(),
            }),
            var_offset: 0x3A,
            width: 1,
            min: 0,
            max: 0,
            step: 0,
            options: vec![OptionEntry {
                string_id: 3,
                value: 1,
                flags: 0x00,
            }],
            defaults: vec![DefaultEntry {
                default_id: 0,
                r#type: 0,
                value: 1,
            }],
        };
        let text = question_info_text(&q);
        assert!(text.contains("varstore Setup (EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9) id 1 size 0x72"));
        assert!(text.contains("default = 1 (id 0, type 0)"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p uefi-cli question_info_text`
Expected: FAIL — «cannot find function `question_info_text`» (compile error).

- [ ] **Step 3: Выделить text-ветку в хелпер**

`output.rs`, `print_question_info` Text-arm заменить на вызов:

```rust
        OutputFormat::Text => print!("{}", question_info_text(q)),
```

и добавить приватную функцию (перенос тела Text-arm без изменений формата):

```rust
fn question_info_text(q: &QuestionInfo) -> String {
    let mut s = format!("question {} #{}:{:#x}\n", q.kind, q.form_id, q.question_id);
    match &q.varstore {
        Some(vs) => {
            s.push_str(&format!(
                "varstore {} ({}) id {} size {:#x}\n",
                vs.name, vs.guid, vs.id, vs.size
            ));
        }
        None => s.push_str(&format!("varstore id {} (undeclared)\n", q.var_store_id)),
    }
    s.push_str(&format!(
        "width {}, offset {:#x} ({})\n",
        q.width, q.var_offset, q.var_offset
    ));
    if q.options.is_empty() {
        s.push_str("no options\n");
    }
    for o in &q.options {
        s.push_str(&format!(
            "value = {} (string {}, flags {:#x})\n",
            o.value, o.string_id, o.flags
        ));
    }
    for d in &q.defaults {
        s.push_str(&format!(
            "default = {} (id {}, type {})\n",
            d.value, d.default_id, d.r#type
        ));
    }
    s
}
```

Вывод байт-в-байт тот же, что был у println!-версии (каждая строка с `\n`).

- [ ] **Step 4: Run unit test**

Run: `cargo test -p uefi-cli question_info_text`
Expected: PASS.

- [ ] **Step 5: mock_question() в mock_server.rs**

Приватная функция в `tests/mock_server.rs` (вне impl):

```rust
fn mock_question() -> QuestionInfo {
    QuestionInfo {
        form_id: 10029,
        question_id: 0x3B,
        kind: "one_of".into(),
        var_store_id: 1,
        varstore: Some(VarStoreInfo {
            id: 1,
            guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
            size: 0x72,
            name: "Setup".into(),
        }),
        var_offset: 0x3A,
        width: 1,
        min: 0,
        max: 0,
        step: 0,
        options: vec![
            OptionEntry {
                string_id: 4,
                value: 0,
                flags: 0x30,
            },
            OptionEntry {
                string_id: 3,
                value: 1,
                flags: 0x00,
            },
        ],
        defaults: vec![DefaultEntry {
            default_id: 0,
            r#type: 0,
            value: 1,
        }],
    }
}
```

`hii_question_info` (:285) и `hii_set_value` (:322): `question: Some(mock_question())` вместо литералов.

- [ ] **Step 6: Content-ассерт defaults в cli_integration**

В `hii_question_info_and_set_value_flow` к первому `hii question info`-вызову (text) добавить:

```rust
        .stdout(predicates::str::contains("default = 1 (id 0, type 0)"))
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p uefi-cli`
Expected: PASS — включая e2e (`hii_question_info_and_set_value_output_content` не конфликтует с новой defaults-строкой).

- [ ] **Step 8: Lint + commit**

```bash
cargo clippy -p uefi-cli -- -D warnings
git add crates/uefi-cli/tests/mock_server.rs crates/uefi-cli/src/output.rs crates/uefi-cli/tests/cli_integration.rs
git commit -m "refactor(uefi-cli): mock_question() дедуп + DefaultEntry; question_info_text хелпер — defaults-ветка покрыта"
```

---

### Task 5: ArgGroup exclusivity + content-ассерты stdout (спека §3.3, §3.6, пункты 4-7)

**Files:**
- Test: `crates/uefi-cli/tests/cli_integration.rs` (ArgGroup runtime-кейс; усиление `full_flow`)
- Test: `crates/uefi-cli/tests/e2e.rs` (ArgGroup clap-кейс; TSV-строки; edit_flow-ассерты; новый флоу для hijack/question add/page add/extract)

**Interfaces:**
- Потребляет: `mock-image-1` из мока (Task 3), `mock_question()` с defaults (Task 4). Кода вне тестов этот таск не меняет.

Точные ожидаемые строки (из текущих принтеров, значения моков):

- `print_gates` TSV заголовок: `gate_kind\twraps\tform_id\thost_form_id\tquestion_id\texpression\tflippable\tflip\tscope_offset`
- gates data-row (mock gates_list): `suppress\tref\t10029\t10002\t0\t1 == 1\ttrue\tpkg+0x67a: 01 -> 02\t1644`
- `print_question_info` TSV заголовок: `form_id\tquestion_id\tkind\tvar_store_id\tvarstore\tvar_offset\twidth\tmin\tmax\tstep`
- question data-row (mock_question): `10029\t59\tone_of\t1\tSetup\t58\t1\t0\t0\t0`
- `print_form_hijack` text: `unlock\tpkg+0x67a: 01 -> 02`, `help_control\tqid=0x3B\tstr@0x40\t2A→3`, `help_record\tqid=0x3B\trec@0x1F4\t1A4→3`, `ifr [0x5A..0x8C)`
- `print_question_add_result` text: `question_id\tspf_record_offset\tstring_id\tname`, `0x200\t0x13C\t2\tSerial Console`
- `print_page_add` text: `form_id\tslot\tpage_offset\ttitle_string_id`, `10021\t1\t376\t423`
- `print_image_info` text header: `image_id\tname\tpath\tmode\tsize\tcreated\tlast_activity`
- `print_images_list` text header: `image_id\tname\tmode\tsize\tlast_activity`
- `print_sessions` header: `session_id\tcreated_at\tlast_activity`
- `image open` (mock): содержит `mock.bin`

- [ ] **Step 1: Ассерты в cli_integration full_flow**

К существующим вызовам `full_flow` добавить `.stdout(...)` (вызовы не менять, только ассерты):

```rust
    cli(&sock, cwd).args(["session", "init"]).assert().success()
        .stdout(predicates::str::contains("\t"));

    cli(&sock, cwd).args(["session", "list"]).assert().success()
        .stdout(predicates::str::contains("session_id\tcreated_at\tlast_activity"));

    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "read"])
        .assert()
        .success()
        .stdout(predicates::str::contains("mock.bin"));

    cli(&sock, cwd).args(["node", "list"]).assert().success()
        .stdout(predicates::str::contains("0"));

    cli(&sock, cwd).args(["image", "list"]).assert().success()
        .stdout(predicates::str::contains("image_id\tname\tmode\tsize\tlast_activity"))
        .stdout(predicates::str::contains("mock-image-1"));

    cli(&sock, cwd).args(["image", "status"]).assert().success()
        .stdout(predicates::str::contains("image_id\tname\tpath\tmode\tsize\tcreated\tlast_activity"));

    cli(&sock, cwd).args(["image", "close"]).assert().success()
        .stdout(predicates::str::contains("ok"));
```

`node search` остаётся без content-ассерта (мок возвращает пустой список).

- [ ] **Step 2: ArgGroup-тесты**

В `cli_integration.rs` (runtime-кейс, требует state + активный образ):

```rust
#[tokio::test(flavor = "multi_thread")]
async fn node_source_args_required_exactly_one() {
    let (td, sock) = setup_env().await;
    let cwd = td.path();
    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["node", "insert", "0"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains(
            "exactly one of --file or --artifact required",
        ));

    cli(&sock, cwd)
        .args(["node", "replace", "0", "--body-only"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains(
            "exactly one of --file or --artifact required",
        ));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
```

В `e2e.rs` (clap-кейс, state не нужен — clap падает до dispatch):

```rust
#[test]
fn node_source_args_are_mutually_exclusive() {
    let td = tempfile::TempDir::new().unwrap();
    let cwd = td.path();
    let mut cmd = assert_cmd::Command::cargo_bin("uefi-cli").unwrap();
    cmd.current_dir(cwd)
        .args(["node", "insert", "0", "--file", "/dev/null", "--artifact", "a1"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));

    let mut cmd = assert_cmd::Command::cargo_bin("uefi-cli").unwrap();
    cmd.current_dir(cwd)
        .args(["node", "replace", "0", "--file", "/dev/null", "--artifact", "a1"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));
}
```

- [ ] **Step 3: TSV-строки в e2e**

В `hii_gates_and_unlock_output_content` заменить хвостовой tsv-вызов:

```rust
    cli(&sock, cwd)
        .args(["--format", "tsv", "hii", "form", "gates", "0#42"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "gate_kind\twraps\tform_id\thost_form_id\tquestion_id\texpression\tflippable\tflip\tscope_offset",
        ))
        .stdout(predicates::str::contains(
            "suppress\tref\t10029\t10002\t0\t1 == 1\ttrue\tpkg+0x67a: 01 -> 02\t1644",
        ));
```

В `hii_question_info_and_set_value_output_content` заменить tsv-вызов:

```rust
    cli(&sock, cwd)
        .args(["--format", "tsv", "hii", "question", "info", "0#42:0x1"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "form_id\tquestion_id\tkind\tvar_store_id\tvarstore\tvar_offset\twidth\tmin\tmax\tstep",
        ))
        .stdout(predicates::str::contains("10029\t59\tone_of\t1\tSetup\t58\t1\t0\t0\t0"));
```

- [ ] **Step 4: edit_flow-ассерты**

В `e2e.rs` `edit_flow` добавить к существующим вызовам:

```rust
    // node insert … --mode before → print_node_id("mock")
        .stdout(predicates::str::contains("mock"));
    // node remove → print_ok
        .stdout(predicates::str::contains("ok"));
    // hii form set-visibility → print_ok
        .stdout(predicates::str::contains("ok"));
    // image save → print_ok
        .stdout(predicates::str::contains("ok"));
    // node extract 0 (добавить новый вызов в конец флоу, до destroy) → artifact_id (uuid) печатается
    cli(&sock, cwd)
        .args(["node", "extract", "0"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty().not());
```

Для `.not()` в начале файла: `use predicates::boolean::PredicateBooleanExt;` — если в e2e.rs нет прямого `use predicates`, добавить `use predicates::prelude::*;` и использовать `.not()` оттуда.

- [ ] **Step 5: Новый флоу для непокрытых принтеров (hijack/question add/page add)**

В `e2e.rs` новый тест (фикстура-схема та же, что в cli_integration `form_add_flow`):

```rust
#[tokio::test(flavor = "multi_thread")]
async fn hii_add_hijack_page_output_content() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("e2e-hii-add.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();

    let schema = r#"{
        "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
        "title": "T", "help": "H", "class_guids": [],
        "varstores": [], "default_stores": [],
        "forms": [{"id": 42, "title": "NewForm", "items": []}]
    }"#;
    let schema_path = cwd.join("schema.json");
    std::fs::write(&schema_path, schema).unwrap();
    let target = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0";

    cli(&sock, cwd)
        .args(["hii", "question", "add", "0#10029:0x3B", "--file", schema_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "question_id\tspf_record_offset\tstring_id\tname",
        ))
        .stdout(predicates::str::contains("0x200\t0x13C\t2\tSerial Console"));

    cli(&sock, cwd)
        .args(["hii", "page", "add", target, "--file", schema_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("form_id\tslot\tpage_offset\ttitle_string_id"))
        .stdout(predicates::str::contains("10021\t1\t376\t423"));

    cli(&sock, cwd)
        .args(["hii", "form", "hijack", "--target", target, "--file", schema_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicates::str::contains("unlock\tpkg+0x67a: 01 -> 02"))
        .stdout(predicates::str::contains("help_control\tqid=0x3B\tstr@0x40\t2A→3"))
        .stdout(predicates::str::contains("help_record\tqid=0x3B\trec@0x1F4\t1A4→3"))
        .stdout(predicates::str::contains("ifr [0x5A..0x8C)"));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p uefi-cli`
Expected: PASS — все интеграционные/e2e/unit зелёные.

- [ ] **Step 7: Lint + commit**

```bash
cargo clippy -p uefi-cli -- -D warnings
git add crates/uefi-cli/tests/cli_integration.rs crates/uefi-cli/tests/e2e.rs
git commit -m "test(uefi-cli): ArgGroup exclusivity + content-ассерты stdout (TSV-строки, hijack/question-add/page-add/extract, full_flow)"
```

---

### Task 6: закрыть пункты TODO (спека §8)

**Files:**
- Modify: `TODO.md` (11 пунктов, поиск по заголовкам)

**Interfaces:**
- Чистая docs-правка; после Task 1-5 все утверждения истинны.

- [ ] **Step 1: Перевести пункты в закрытые**

Каждый пункт `* [ ] …` → `* [x] …` + строка «Закрыто:» (в стиле существующих закрытий). Заголовки (искать по тексту):

1. «`image switch <image_id>` не валидирует образ на сервере» → Закрыто: cli-polish — валидация через `images_list` до записи state.
2. «Все подкоманды без `about:`» → Закрыто: cli-polish — about на все подкоманды + help на несамодостаточные аргументы.
3. «`client.rs image_status` делает `.info.unwrap()`» → Закрыто: ещё `4d1d473` (Plan A final review M1); отмечено циклом cli-polish.
4. «Нет теста на ArgGroup exclusivity» → Закрыто: cli-polish — clap-конфликт и runtime «exactly one» покрыты (e2e/cli_integration).
5. «Интеграционные тесты CLI проверяют только exit-code, не stdout» → Закрыто: cli-polish — content-ассерты на print-fn (full_flow/edit_flow/новые флоу).
6. «uefi-cli e2e: TSV-инвокация `hii form gates` ассертит только exit success» → Закрыто: cli-polish — точный заголовок + data-строка.
7. «CLI: несуществующий item_id в gates/unlock отдаёт `RPC_NOT_FOUND`» → Закрыто: cli-polish — `hii_error_status_ctx`: «{target} not found», код RPC оставлен.
8. «CLI: defaults-ветка принтера question info не покрыта тестами» → Закрыто: cli-polish — `question_info_text` unit + интеграционный ассерт.
9. «CLI: e2e tsv-подкейс без content-ассертов» → Закрыто: cli-polish — точный заголовок + data-строка question info.
10. «uefi-cli мок: дублирование ~40-строчного QuestionInfo-литерала» → Закрыто: cli-polish — `mock_question()`.
11. «uefi-cli: about строки page add» → Закрыто: cli-polish — переформулировано.

- [ ] **Step 2: Финальные ворота**

```bash
cargo test --all
cargo clippy --all -- -D warnings
cargo fmt --all -- --check
```
Expected: всё зелёное.

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs(todo): закрыть 11 пунктов cli-polish (ревизия CLI + u1-u5 + setup-new-page final review)"
```

---

## Чек-таблица покрытия print-fn (для ревью Task 5)

| print-fn | покрытие после Task 5 |
|---|---|
| print_nodes | full_flow `node list` (contains "0"); search — мок пуст |
| print_session_created | full_flow init (contains "\t") |
| print_sessions | full_flow list (header) |
| print_image_info | full_flow open (mock.bin) + status (header) |
| print_images_list | full_flow image list (header + mock-image-1) |
| print_image_status | full_flow status (header) — делегирует print_image_info |
| print_forms | e2e hii_list_output_content (Main) — уже было |
| print_strings | e2e hii_list_output_content (Hello) — уже было |
| print_gates | e2e gates (suppress/expr) + TSV header+row |
| print_unlock | e2e question unlock (grayout/applied) — уже было |
| print_question_info | cli_integration info (text+json+tsv+default) + e2e TSV row |
| print_set_value | cli_integration set-value (text+json) — уже было |
| print_node_id | e2e edit_flow insert (mock) |
| print_formset_add | cli_integration formset_add (mock) — уже было |
| print_form_add | cli_integration form_add (42/mock) — уже было |
| print_form_hijack | e2e hii_add_hijack_page (unlock/help_control/help_record/ifr) |
| print_question_add_result | e2e hii_add_hijack_page (header + row) |
| print_page_add | e2e hii_add_hijack_page (header + row) |
| print_text | dead code (`#[allow(dead_code)]`) — сознательно без теста |
| print_ok | edit_flow remove/save/set-visibility (ok) |
