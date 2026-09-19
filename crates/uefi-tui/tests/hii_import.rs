mod mock_server;

use mock_server::SchemaCall;
use tempfile::TempDir;

const TARGET: &str = "11111111-2222-3333-4444-555555555555:0x19:0";

const FULL_PACKAGE_JSON: &str = r#"{
    "meta": {"source": {"formset_guid": "11111111-2222-3333-4444-555555555555"}},
    "formset": {
        "formset_guid": "11111111-2222-3333-4444-555555555555",
        "title": "T", "help": "H", "class_guids": [],
        "varstores": [], "default_stores": [],
        "forms": [{"id": 7, "title": "PkgForm", "items": []}]
    },
    "refs": {"parent_form_id": 10001, "entries": []}
}"#;

const FULL_PACKAGE_RPC_SEQUENCE: [&str; 5] = [
    "HiiListForms",
    "HiiListQuestions",
    "HiiListVarstores",
    "HiiFormAdd",
    "HiiQuestionAdd",
];

const REFS_ONLY_RPC_SEQUENCE: [&str; 3] = ["HiiListForms", "HiiListQuestions", "HiiQuestionAdd"];

async fn setup(sock: &std::path::Path) -> (uefi_tui::commands::Client, uefi_tui::app::App) {
    let state = uefi_common::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        active_image_id: None,
        sock_path: Some(sock.display().to_string()),
    };
    let mut client = uefi_tui::commands::connect(None, state).await.unwrap();
    let mut app = uefi_tui::app::App::new();
    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    (client, app)
}

async fn rpc_names(
    calls: &std::sync::Arc<tokio::sync::Mutex<Vec<SchemaCall>>>,
) -> Vec<&'static str> {
    calls.lock().await.iter().map(|c| c.rpc).collect()
}

fn assert_import_sequence(journal: &[&'static str], expected: &[&'static str]) {
    assert!(
        journal.starts_with(expected),
        "журнал импорта {journal:?} обязан начинаться с pinned-последовательности {expected:?} (спека hii-form-export §3/§6)"
    );
    let tail = &journal[expected.len()..];
    assert!(
        tail.iter()
            .all(|r| matches!(*r, "HiiListForms" | "HiiListQuestions")),
        "после фаз импорта допустимы только read-only refresh-вызовы, получено {tail:?}"
    );
}

fn schema_file(td: &TempDir, name: &str, body: &str) -> String {
    let p = td.path().join(name);
    std::fs::write(&p, body).unwrap();
    p.display().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_import_full_package_journal_and_form_id_wiring() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
    let (mut client, mut app) = setup(&sock).await;
    let file = schema_file(&td, "full-pkg.json", FULL_PACKAGE_JSON);

    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii import {TARGET} --file {file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, "10101", "возврат — form id из inserted_form_ids");

    let names = rpc_names(&calls).await;
    assert_import_sequence(&names, &FULL_PACKAGE_RPC_SEQUENCE);

    let locked = calls.lock().await;
    let q_add = locked
        .iter()
        .find(|c| c.rpc == "HiiQuestionAdd")
        .expect("HiiQuestionAdd в журнале");
    assert_eq!(
        q_add.target,
        format!("{TARGET}#10001"),
        "refs-фаза идёт в item_id <target>#<parent_form_id>"
    );
    assert!(
        q_add.schema_json.contains(r#""form_id":10101"#),
        "schema_json question add несёт form_id из ответа form add: {}",
        q_add.schema_json
    );
    drop(locked);
    assert!(
        app.status_msg.contains("refs built under 10001"),
        "статус: {}",
        app.status_msg
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_import_refs_only_journal_skips_form_add() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
    let (mut client, mut app) = setup(&sock).await;
    let pkg = r#"{
        "refs": {
            "parent_form_id": 10001,
            "entries": [
                {"form_id": 902, "formset_guid": "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE", "prompt": "Platform", "help": "platform setup"}
            ]
        }
    }"#;
    let file = schema_file(&td, "refs-only.json", pkg);

    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii import {TARGET} --file {file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, TARGET, "без вставок возврат — сам target");

    let names = rpc_names(&calls).await;
    assert_import_sequence(&names, &REFS_ONLY_RPC_SEQUENCE);
    assert!(
        !names.contains(&"HiiFormAdd"),
        "refs-only пакет не делает form add: {names:?}"
    );
    assert!(
        !names.contains(&"HiiListVarstores"),
        "refs-only пакет без тела не читает varstores: {names:?}"
    );

    let locked = calls.lock().await;
    let q_add = locked
        .iter()
        .find(|c| c.rpc == "HiiQuestionAdd")
        .expect("HiiQuestionAdd в журнале");
    assert!(
        q_add.schema_json.contains(r#""form_id":902"#),
        "явный form_id entry проходит как есть: {}",
        q_add.schema_json
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_form_add_routes_package_envelope_to_import() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
    let (mut client, mut app) = setup(&sock).await;
    let pkg_file = schema_file(&td, "envelope.json", FULL_PACKAGE_JSON);

    let err = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii form add {TARGET} {pkg_file}"),
        &mut client,
    )
    .await
    .unwrap_err();
    assert!(
        err.contains("package file: use hii import"),
        "конверт отвергается с подсказкой маршрутизации: {err}"
    );
    let names = rpc_names(&calls).await;
    assert!(
        !names.contains(&"HiiFormAdd"),
        "конверт в form add не делает мутаций: {names:?}"
    );

    let bare = r#"{"formset_guid":"11111111-2222-3333-4444-555555555555","title":"T","help":"H","class_guids":[],"varstores":[],"default_stores":[],"forms":[{"id":42,"title":"BareForm","items":[]}]}"#;
    let bare_file = schema_file(&td, "bare.json", bare);
    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii form add {TARGET} {bare_file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, "10101");
    let locked = calls.lock().await;
    let form_add = locked
        .iter()
        .find(|c| c.rpc == "HiiFormAdd")
        .expect("bare-файл доходит до HiiFormAdd");
    assert_eq!(form_add.schema_json, bare, "bare-файл байт-в-байт");
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_import_bad_parent_fails_fast_zero_mutations() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
    let (mut client, mut app) = setup(&sock).await;
    let pkg = r#"{
        "meta": {},
        "formset": {
            "formset_guid": "11111111-2222-3333-4444-555555555555",
            "title": "T", "help": "H", "class_guids": [],
            "varstores": [], "default_stores": [],
            "forms": [{"id": 7, "title": "PkgForm", "items": []}]
        },
        "refs": {"parent_form_id": 4242, "entries": []}
    }"#;
    let file = schema_file(&td, "bad-parent.json", pkg);

    let err = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii import {TARGET} --file {file}"),
        &mut client,
    )
    .await
    .unwrap_err();
    assert!(
        err.contains("parent form 4242 not found in target"),
        "pre-check родителя: {err}"
    );

    let names = rpc_names(&calls).await;
    assert_eq!(names, vec!["HiiListForms"], "fail-fast до остальных RPC");
}
