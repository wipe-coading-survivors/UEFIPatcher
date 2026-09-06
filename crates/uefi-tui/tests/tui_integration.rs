mod mock_server;

use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn app_open_and_quit() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let _handle = mock_server::start_mock(&sock).await;
    let state = uefi_common::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        active_image_id: None,
        sock_path: Some(sock.display().to_string()),
    };
    let mut client = uefi_tui::commands::connect(None, state).await.unwrap();
    let mut app = uefi_tui::app::App::new();
    let result = uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client).await;
    assert!(result.is_ok());
    assert!(app.image_loaded);
    assert!(!app.tree.is_empty());
    app.quit = true;
    assert!(app.quit);
}
