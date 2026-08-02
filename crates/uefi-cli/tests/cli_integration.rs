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

    cli(&sock, cwd).args(["image", "dump"]).assert().success();

    cli(&sock, cwd).args(["image", "list"]).assert().success();

    cli(&sock, cwd)
        .args(["image", "find", "0"])
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
        .args(["image", "dump"])
        .assert()
        .failure()
        .code(3);
}
