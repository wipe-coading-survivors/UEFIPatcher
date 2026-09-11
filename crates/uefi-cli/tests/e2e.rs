mod mock_server;

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
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
        .success()
        .stdout(predicates::str::contains("mock"));
    cli(&sock, cwd)
        .args(["node", "remove", "0"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ok"));
    cli(&sock, cwd)
        .args(["hii", "form", "set-visibility", "0", "--visible"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ok"));
    cli(&sock, cwd)
        .args(["image", "save", "/dev/null"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ok"));
    cli(&sock, cwd)
        .args(["node", "extract", "0"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty().not());
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
        .stdout(predicates::str::contains("value = 1"))
        .stdout(predicates::str::contains(
            "value = 1 \"Enabled\" (string 3, flags 0x0)",
        ));

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
        .success()
        .stdout(predicates::str::contains(
            "form_id\tquestion_id\tkind\tvar_store_id\tvarstore\tvar_offset\twidth\tmin\tmax\tstep",
        ))
        .stdout(predicates::str::contains(
            "10029\t59\tone_of\t1\tSetup\t58\t1\t0\t0\t0",
        ));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

#[test]
fn node_source_args_are_mutually_exclusive() {
    let td = tempfile::TempDir::new().unwrap();
    let cwd = td.path();
    let mut cmd = assert_cmd::Command::cargo_bin("uefi-cli").unwrap();
    cmd.current_dir(cwd)
        .args([
            "node",
            "insert",
            "0",
            "--file",
            "/dev/null",
            "--artifact",
            "a1",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));

    let mut cmd = assert_cmd::Command::cargo_bin("uefi-cli").unwrap();
    cmd.current_dir(cwd)
        .args([
            "node",
            "replace",
            "0",
            "--file",
            "/dev/null",
            "--artifact",
            "a1",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_gates_and_unlock_output_content() {
    let td = tempfile::TempDir::new().unwrap();
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
        .success()
        .stdout(predicates::str::contains(
            "gate_kind\twraps\tform_id\thost_form_id\tquestion_id\texpression\tflippable\tflip\tscope_offset",
        ))
        .stdout(predicates::str::contains(
            "suppress\tref\t10029\t10002\t0\t1 == 1\ttrue\tpkg+0x67a: 01 -> 02\t1644",
        ));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}

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
        .args([
            "hii",
            "question",
            "add",
            "0#10029:0x3B",
            "--file",
            schema_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "question_id\tspf_record_offset\tstring_id\tname",
        ))
        .stdout(predicates::str::contains("0x200\t0x13C\t2\tSerial Console"));

    cli(&sock, cwd)
        .args([
            "hii",
            "page",
            "add",
            target,
            "--file",
            schema_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "form_id\tslot\tpage_offset\ttitle_string_id",
        ))
        .stdout(predicates::str::contains("10021\t1\t376\t423"));

    cli(&sock, cwd)
        .args([
            "hii",
            "form",
            "hijack",
            "--target",
            target,
            "--file",
            schema_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("unlock\tpkg+0x67a: 01 -> 02"))
        .stdout(predicates::str::contains(
            "help_control\tqid=0x3B\tstr@0x40\t2A→3",
        ))
        .stdout(predicates::str::contains(
            "help_record\tqid=0x3B\trec@0x1F4\t1A4→3",
        ))
        .stdout(predicates::str::contains("ifr [0x5A..0x8C)"));

    cli(&sock, cwd)
        .args(["session", "destroy"])
        .assert()
        .success();
}
