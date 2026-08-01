use assert_cmd::Command;

#[test]
fn cli_help() {
    let mut cmd = Command::cargo_bin("uefi-cli").unwrap();
    cmd.arg("--help").assert().success();
}
