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

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    assert!(cwd.join(".uefipatcher").exists());

    cli(&sock, cwd).args(["session", "list"]).assert().success();

    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "read"])
        .assert()
        .success();

    cli(&sock, cwd).args(["node", "list"]).assert().success();

    cli(&sock, cwd).args(["image", "list"]).assert().success();

    cli(&sock, cwd).args(["image", "status"]).assert().success();

    cli(&sock, cwd)
        .args(["node", "search", "0"])
        .assert()
        .success();

    cli(&sock, cwd).args(["image", "close"]).assert().success();

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
    assert!(!cwd.join(".uefipatcher").exists());
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
