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

#[tokio::test(flavor = "multi_thread")]
async fn forms_command_switches_view_and_loads() {
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

    let r = uefi_tui::commands::execute_command(&mut app, "forms", &mut client).await;
    assert!(r.is_err(), "no active image yet");
    assert_eq!(app.view, uefi_tui::app::View::Image);

    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    uefi_tui::commands::execute_command(&mut app, "forms", &mut client)
        .await
        .unwrap();
    assert_eq!(app.view, uefi_tui::app::View::Forms);
    assert_eq!(app.forms.forms.len(), 3, "mock fixture: 3 forms");
    assert_eq!(
        app.forms_rows().len(),
        5,
        "2 formsets + 3 forms, both expanded"
    );

    app.forms_cursor_down();
    uefi_tui::commands::refresh_questions_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    assert_eq!(
        app.forms.questions.len(),
        2,
        "mock: 2 questions for any form"
    );
    assert!(
        app.forms
            .questions_key
            .as_ref()
            .is_some_and(|k| k.form_id_ifr == 10001)
    );

    uefi_tui::commands::execute_command(&mut app, "image", &mut client)
        .await
        .unwrap();
    assert_eq!(app.view, uefi_tui::app::View::Image);
}
