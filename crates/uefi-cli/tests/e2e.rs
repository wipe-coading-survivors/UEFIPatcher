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
        .args(["edit", "insert", "0", "/dev/null", "--mode", "before"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["edit", "remove", "0"])
        .assert()
        .success();
    cli(&sock, cwd)
        .args(["setup", "set-visibility", "0", "--visible"])
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
