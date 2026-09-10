mod mock_server;

use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

async fn setup_env() -> (TempDir, String) {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let _handle = mock_server::start_mock(&sock).await;
    (td, sock.display().to_string())
}

fn cli(sock: &str, cwd: &Path) -> Command {
    let mut cmd = Command::cargo_bin("uefi-cli").unwrap();
    cmd.current_dir(cwd).args(["--sock", sock]);
    cmd
}

#[tokio::test(flavor = "multi_thread")]
async fn full_flow() {
    let (td, sock) = setup_env().await;
    let cwd = td.path();

    cli(&sock, cwd)
        .args(["session", "init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\t"));
    assert!(cwd.join(".uefipatcher").exists());

    cli(&sock, cwd)
        .args(["session", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "session_id\tcreated_at\tlast_activity",
        ));

    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "read"])
        .assert()
        .success()
        .stdout(predicates::str::contains("mock.bin"));

    cli(&sock, cwd)
        .args(["node", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("0"));

    cli(&sock, cwd)
        .args(["image", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "image_id\tname\tmode\tsize\tlast_activity",
        ))
        .stdout(predicates::str::contains("mock-image-1"));

    cli(&sock, cwd)
        .args(["image", "status"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "image_id\tname\tpath\tmode\tsize\tcreated\tlast_activity",
        ));

    cli(&sock, cwd)
        .args(["node", "search", "0"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["image", "close"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ok"));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
    assert!(!cwd.join(".uefipatcher").exists());
}

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

#[tokio::test(flavor = "multi_thread")]
async fn no_state_errors() {
    let td = TempDir::new().unwrap();
    let cwd = td.path();
    cli("/tmp/x", cwd)
        .args(["node", "list"])
        .assert()
        .failure()
        .code(3);
}

#[tokio::test(flavor = "multi_thread")]
async fn formset_add_flow() {
    let (td, sock) = setup_env().await;
    let cwd = td.path();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();

    let schema = r#"{
        "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
        "title": "T", "help": "H", "class_guids": [],
        "varstores": [], "default_stores": [],
        "forms": [{"id": 1, "title": "Main", "items": []}]
    }"#;
    let schema_path = cwd.join("schema.json");
    std::fs::write(&schema_path, schema).unwrap();

    cli(&sock, cwd)
        .args([
            "hii",
            "formset",
            "add",
            "--file",
            schema_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("mock"));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_question_info_and_set_value_flow() {
    let (td, sock) = setup_env().await;
    let cwd = td.path();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["hii", "question", "info", "0#10029:0x3B"])
        .assert()
        .success()
        .stdout(predicates::str::contains("one_of"))
        .stdout(predicates::str::contains("Setup"))
        .stdout(predicates::str::contains("0x3a"))
        .stdout(predicates::str::contains("58"))
        .stdout(predicates::str::contains("value = 1"))
        .stdout(predicates::str::contains("default = 1 (id 0, type 0)"));

    cli(&sock, cwd)
        .args([
            "--format",
            "json",
            "hii",
            "question",
            "info",
            "0#10029:0x3B",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"varstore\""))
        .stdout(predicates::str::contains("\"kind\": \"one_of\""));

    cli(&sock, cwd)
        .args(["--format", "tsv", "hii", "question", "info", "0#10029:0x3B"])
        .assert()
        .success()
        .stdout(predicates::str::contains("form_id\tquestion_id\tkind"))
        .stdout(predicates::str::contains("option\t3\t1\t0"));

    cli(&sock, cwd)
        .args(["hii", "question", "set-value", "0#10029:0x3B", "1"])
        .assert()
        .success()
        .stdout(predicates::str::contains("one_of"))
        .stdout(predicates::str::contains(
            "applied file …raw body store+0x62: 00 -> 01",
        ))
        .stdout(predicates::str::contains("stores: 1"));

    cli(&sock, cwd)
        .args(["hii", "question", "set-value", "0#10029:0x3B", "0x1"])
        .assert()
        .success()
        .stdout(predicates::str::contains("00 -> 01"));

    cli(&sock, cwd)
        .args([
            "--format",
            "json",
            "hii",
            "question",
            "set-value",
            "0#10029:0x3B",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"applied\""))
        .stdout(predicates::str::contains("\"stores\""));

    cli(&sock, cwd)
        .args(["hii", "question", "set-value", "0#10029:0x3B", "zz"])
        .assert()
        .failure();

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn form_add_flow() {
    let (td, sock) = setup_env().await;
    let cwd = td.path();

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
    let schema_path = cwd.join("form-schema.json");
    std::fs::write(&schema_path, schema).unwrap();

    cli(&sock, cwd)
        .args([
            "hii",
            "form",
            "add",
            "--target",
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0",
            "--file",
            schema_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("42"))
        .stdout(predicates::str::contains("mock"));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
