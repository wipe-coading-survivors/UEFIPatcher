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

#[tokio::test(flavor = "multi_thread")]
async fn forms_view_strings_fetch() {
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
    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    app.forms.show_strings = true;
    uefi_tui::commands::refresh_strings(&mut app, &mut client)
        .await
        .unwrap();
    assert_eq!(app.forms.strings.len(), 3, "mock fixture: 3 strings");
}

#[tokio::test(flavor = "multi_thread")]
async fn forms_view_strings_filter() {
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
    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    app.forms.show_strings = true;
    uefi_tui::commands::refresh_strings(&mut app, &mut client)
        .await
        .unwrap();

    uefi_tui::commands::execute_command(&mut app, "filter serial", &mut client)
        .await
        .unwrap();
    assert_eq!(app.strings_visible(), vec![2]);
    assert_eq!(app.forms.strings_cursor, 0);

    uefi_tui::commands::execute_command(&mut app, "filter", &mut client)
        .await
        .unwrap();
    assert_eq!(app.strings_visible().len(), 3, "empty filter clears");

    app.forms.show_strings = false;
    assert!(
        uefi_tui::commands::execute_command(&mut app, "filter x", &mut client)
            .await
            .is_err(),
        "filter requires strings view open"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn forms_load_fetches_edges_and_reload_preserves_state() {
    let td = tempfile::TempDir::new().unwrap();
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
    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    uefi_tui::commands::execute_command(&mut app, "forms", &mut client)
        .await
        .unwrap();

    assert_eq!(app.forms.edges.len(), 1, "mock fixture: Main -> Serial");
    assert_eq!(app.forms.edges[0].parent_form_id, 10001);
    assert_eq!(app.forms.edges[0].form_id, 10019);

    app.forms.cursor = 4;
    app.forms_set_expanded(false);
    assert_eq!(app.forms_rows().len(), 4, "SET-B collapsed hides 902");
    app.forms.cursor = 2;
    assert_eq!(app.selected_form_key().unwrap().form_id_ifr, 10019);
    uefi_tui::commands::refresh_questions_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    assert_eq!(
        app.forms.questions.len(),
        2,
        "кэш вопросов заполнен до reload"
    );

    uefi_tui::commands::reload_forms(&mut app, &mut client)
        .await
        .unwrap();
    assert_eq!(app.forms.forms.len(), 3, "re-fetched");
    assert_eq!(app.forms.edges.len(), 1, "edges re-fetched");
    assert_eq!(app.forms_rows().len(), 4, "SET-B still collapsed");
    assert_eq!(
        app.selected_form_key().unwrap().form_id_ifr,
        10019,
        "cursor restored by form key"
    );
    assert!(app.forms.questions.is_empty(), "per-form cache reset");
}
