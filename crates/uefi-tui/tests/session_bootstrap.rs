mod mock_server;

use std::sync::atomic::Ordering;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn session_bootstrap_lazy_create_switch_persist_and_revive() {
    let td = TempDir::new().unwrap();
    std::env::set_current_dir(td.path()).unwrap();
    let sock = td.path().join("bootstrap.sock");
    let (_handle, _calls, enforce, sessions) = mock_server::start_mock_full(&sock).await;

    let state = uefi_common::State {
        session_id: None,
        token: None,
        active_image_id: None,
        sock_path: Some(sock.display().to_string()),
    };
    let mut client = uefi_tui::commands::connect(None, state).await.unwrap();
    let mut app = uefi_tui::app::App::new();

    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    assert!(app.image_loaded);
    assert_eq!(sessions.lock().await.len(), 1, "lazy create ровно один");
    let saved = uefi_common::state::read_state().unwrap();
    let sid = client.state.session_id.clone().unwrap();
    assert_eq!(saved.session_id.as_deref(), Some(sid.as_str()));
    assert!(saved.token.is_some(), "токен персистится");

    uefi_tui::commands::execute_command(&mut app, "switch mock-img-1", &mut client)
        .await
        .unwrap();
    let saved = uefi_common::state::read_state().unwrap();
    assert_eq!(
        saved.active_image_id.as_deref(),
        Some("mock-img-1"),
        ":switch персистит active_image_id"
    );

    enforce.store(true, Ordering::SeqCst);
    sessions.lock().await.remove(&sid);
    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    assert_eq!(
        sessions.lock().await.len(),
        1,
        "мёртвая сессия оживлена один раз"
    );
    let saved = uefi_common::state::read_state().unwrap();
    assert_eq!(
        saved.session_id, client.state.session_id,
        "state-файл перезаписан оживлённой сессией"
    );
}
