mod mock_server;

use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

fn cli(sock: &str, cwd: &Path) -> Command {
    let mut cmd = Command::cargo_bin("uefi-cli").unwrap();
    cmd.current_dir(cwd).args(["--sock", sock]);
    cmd
}

#[tokio::test(flavor = "multi_thread")]
async fn edit_flow() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("e2e.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args([
            "node",
            "insert",
            "0",
            "--file",
            "/dev/null",
            "--mode",
            "before",
        ])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["node", "remove", "0"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["hii", "form", "set-visibility", "0", "--visible"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["image", "save", "/dev/null"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_list_output_content() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("e2e-hii.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["hii", "form", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Main"));
    cli(&sock, cwd)
        .args(["hii", "string", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Hello"));
    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_question_info_and_set_value_output_content() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("e2e-hii-question.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

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
        .stdout(predicates::str::contains("value = 1"));

    cli(&sock, cwd)
        .args(["hii", "question", "set-value", "0#10029:0x3B", "1"])
        .assert()
        .success()
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
        .args(["--format", "tsv", "hii", "question", "info", "0#42:0x1"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_gates_and_unlock_output_content() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("e2e-hii-unlock.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let cwd = td.path();
    let sock = sock.display().to_string();

    cli(&sock, cwd).args(["session", "init"]).assert().success();
    cli(&sock, cwd)
        .args(["image", "open", "/dev/null", "--mode", "write"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["hii", "form", "gates", "0#10029"])
        .assert()
        .success()
        .stdout(predicates::str::contains("suppress"))
        .stdout(predicates::str::contains("1 == 1"));

    cli(&sock, cwd)
        .args(["hii", "question", "unlock", "0#10029:0x3B"])
        .assert()
        .success()
        .stdout(predicates::str::contains("grayout"))
        .stdout(predicates::str::contains("applied pkg+0xdd1"));

    cli(&sock, cwd)
        .args(["--format", "tsv", "hii", "form", "gates", "0#42"])
        .assert()
        .success();

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
