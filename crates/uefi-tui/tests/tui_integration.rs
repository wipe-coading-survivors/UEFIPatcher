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
    uefi_tui::commands::refresh_form_details_if_needed(&mut app, &mut client)
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
    uefi_tui::commands::refresh_form_details_if_needed(&mut app, &mut client)
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

#[tokio::test(flavor = "multi_thread")]
async fn forms_tree_mode_nested_path() {
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

    let rows = app.forms_rows();
    assert_eq!(
        rows.len(),
        5,
        "SET1 + Main + Serial(вложенно) + SET2 + Platform"
    );
    assert!(
        matches!(
            &rows[2],
            uefi_tui::forms::FormsRow::Form { key, depth: 2, path, .. }
                if key.form_id_ifr == 10019
                    && path == "Main → Serial Port 1 Configuration"
        ),
        "Serial вложена в Main, путь в details"
    );

    app.forms.flat_mode = true;
    let flat = app.forms_rows();
    assert!(
        matches!(&flat[2], uefi_tui::forms::FormsRow::Form { depth: 1, path, .. } if path.is_empty()),
        "плоский режим: без вложенности и пути"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn forms_details_question_info_gates_and_prefill() {
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

    app.forms_cursor_down();
    assert_eq!(
        uefi_tui::commands::selected_form_item_id(&app).unwrap(),
        "11111111-2222-3333-4444-555555555555:0x19:0#10001"
    );
    uefi_tui::commands::refresh_form_details_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    assert_eq!(app.forms.questions.len(), 2);
    assert_eq!(app.forms.gates.len(), 1, "gates пришли вместе с вопросами");

    app.forms.focus = uefi_tui::app::FormsFocus::Details;
    app.forms_question_cursor_down();
    uefi_tui::commands::refresh_question_info_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    let qi = app.forms.question_info.as_ref().unwrap();
    assert_eq!(qi.question_id, 0x211, "мок эхом возвращает qid из item_id");
    assert_eq!(app.selected_question_id(), Some(0x211));

    assert_eq!(
        uefi_tui::commands::set_value_prefill(&app).unwrap(),
        "hii set-value 11111111-2222-3333-4444-555555555555:0x19:0#10001:0x211 "
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_verbs_visibility_setvalue_unlock() {
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
    let item = "11111111-2222-3333-4444-555555555555:0x19:0#10001";

    uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii visibility {item} off"),
        &mut client,
    )
    .await
    .unwrap();
    assert!(app.status_msg.contains("visibility"));
    assert_eq!(app.forms.forms.len(), 3, "forms reloaded after mutation");

    uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii set-value {item}:0x210 1"),
        &mut client,
    )
    .await
    .unwrap();
    assert!(app.status_msg.contains("flips pkg+0x3e: 01 -> 00"));

    uefi_tui::commands::execute_command(&mut app, &format!("hii unlock {item}"), &mut client)
        .await
        .unwrap();
    assert!(app.status_msg.contains("unlock"));

    assert!(
        uefi_tui::commands::execute_command(
            &mut app,
            &format!("hii set-value {item} abc"),
            &mut client
        )
        .await
        .is_err(),
        "нечисловой value — ошибка"
    );
    assert!(
        uefi_tui::commands::execute_command(&mut app, "hii bogus x", &mut client)
            .await
            .is_err(),
        "неизвестный глагол — ошибка"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn image_switch_sets_active_state() {
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
    assert!(!app.image_loaded);

    let r = uefi_tui::commands::execute_command(&mut app, "image switch img-existing", &mut client)
        .await;

    assert_eq!(r.unwrap(), "img-existing");
    assert_eq!(app.active_image_id.as_deref(), Some("img-existing"));
    assert!(
        app.image_loaded,
        "switch помечает образ загруженным (статус-бар)"
    );
    assert!(!app.tree.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_set_value_keeps_question_cursor() {
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
    uefi_tui::commands::execute_command(&mut app, "forms", &mut client)
        .await
        .unwrap();
    app.forms_cursor_down();
    uefi_tui::commands::refresh_form_details_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    app.forms.question_cursor = 1;
    assert_eq!(app.forms.questions[1].question_id, 0x211);

    uefi_tui::commands::execute_command(
        &mut app,
        "hii set-value 11111111-2222-3333-4444-555555555555:0x19:0#10001:0x211 1",
        &mut client,
    )
    .await
    .unwrap();

    assert_eq!(app.forms.questions.len(), 2, "детали перезагружены");
    assert_eq!(
        app.forms.question_cursor, 1,
        "курсор вопроса остаётся на редактируемом вопросе после set-value"
    );
}

fn schema_file(td: &tempfile::TempDir, name: &str, body: &str) -> String {
    let p = td.path().join(name);
    std::fs::write(&p, body).unwrap();
    p.display().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_formset_add_sends_schema_and_refreshes() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
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
    let body = r#"{"formset_guid":"NEW-GUID","forms":[]}"#;
    let file = schema_file(&td, "formset.json", body);

    app.forms_cursor_down();
    uefi_tui::commands::refresh_form_details_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    assert!(
        !app.forms.questions.is_empty(),
        "кэш вопросов заполнен до add"
    );
    app.forms.cursor = 0;
    app.tree.clear();

    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii formset add {file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, "mock-ffs-1");
    let calls = calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].rpc, "HiiFormSetAdd");
    assert_eq!(calls[0].schema_json, body);
    assert_eq!(
        calls[0].target, "",
        "без --ffs уходит пустой target_ffs_guid"
    );
    drop(calls);
    assert_eq!(
        app.status_msg,
        "formset added: ffs mock-ffs-1 · forms 10101 · strings title=600"
    );
    assert!(
        app.forms.questions.is_empty(),
        "reload_forms сбросил кэш вопросов (курсор на FormSet — re-fetch не вернул)"
    );
    assert!(
        !app.tree.is_empty(),
        "дерево образа обновлено после вставки FFS"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_formset_add_ffs_flag() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
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
    let file = schema_file(&td, "formset.json", "{}");
    uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii formset add {file} --ffs ABC-GUID"),
        &mut client,
    )
    .await
    .unwrap();
    let calls = calls.lock().await;
    assert_eq!(calls[0].target, "ABC-GUID");
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_form_add_sends_target_and_schema() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
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
    let body = r#"{"forms":[{"id":10101}]}"#;
    let file = schema_file(&td, "form.json", body);

    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii form add 11111111-2222-3333-4444-555555555555:0x19:0 {file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, "10101");
    let calls = calls.lock().await;
    assert_eq!(calls[0].rpc, "HiiFormAdd");
    assert_eq!(
        calls[0].target,
        "11111111-2222-3333-4444-555555555555:0x19:0"
    );
    assert_eq!(calls[0].schema_json, body);
    assert_eq!(
        app.status_msg,
        "form added: forms 10101 · strings title=600"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_add_usage_and_file_errors() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, _calls) = mock_server::start_mock(&sock).await;
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
    let e = uefi_tui::commands::execute_command(&mut app, "hii", &mut client)
        .await
        .unwrap_err();
    assert!(
        e.contains("formset add"),
        "usage перечисляет V3-глаголы: {e}"
    );
    assert!(
        uefi_tui::commands::execute_command(&mut app, "hii formset", &mut client)
            .await
            .is_err()
    );
    assert!(
        uefi_tui::commands::execute_command(
            &mut app,
            "hii form add 11111111-2222-3333-4444-555555555555:0x19:0",
            &mut client
        )
        .await
        .is_err()
    );
    let e = uefi_tui::commands::execute_command(
        &mut app,
        "hii formset add /nonexistent-9f1/schema.json",
        &mut client,
    )
    .await
    .unwrap_err();
    assert!(
        e.contains("/nonexistent-9f1/schema.json"),
        "ошибка несёт путь: {e}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_question_add_item_id_and_status() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
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
    let body = r#"{"questions":[{"form_id":10019,"question_id":512}]}"#;
    let file = schema_file(&td, "questions.json", body);

    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii question add 11111111-2222-3333-4444-555555555555:0x19:0#10019 {file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, "0x258", "возврат — qid вставленных вопросов (hex)");
    let calls = calls.lock().await;
    assert_eq!(calls[0].rpc, "HiiQuestionAdd");
    assert_eq!(
        calls[0].target, "11111111-2222-3333-4444-555555555555:0x19:0#10019",
        "item_id: form_id десятичное (контракт parse_item_id)"
    );
    assert_eq!(calls[0].schema_json, body);
    assert_eq!(
        app.status_msg,
        "question add: questions 0x258 · refs (none) · strings prompt=600"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_page_add_status() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
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
    let file = schema_file(&td, "page.json", r#"{"title":"New"}"#);

    let r = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii page add 11111111-2222-3333-4444-555555555555:0x19:0 {file}"),
        &mut client,
    )
    .await
    .unwrap();
    assert_eq!(r, "10019");
    let calls = calls.lock().await;
    assert_eq!(calls[0].rpc, "HiiPageAdd");
    assert_eq!(
        app.status_msg,
        "page added: form 10019 · slot 1 · offset 42 · title sid 600"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hii_hijack_with_and_without_setupdata_guid() {
    let td = tempfile::TempDir::new().unwrap();
    let sock = td.path().join("test.sock");
    let (_h, calls) = mock_server::start_mock(&sock).await;
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
    let file = schema_file(&td, "hijack.json", r#"{"form":{"id":1}}"#);

    uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii hijack 11111111-2222-3333-4444-555555555555:0x19:0 {file}"),
        &mut client,
    )
    .await
    .unwrap();
    {
        let calls = calls.lock().await;
        assert_eq!(calls[0].rpc, "HiiFormHijack");
        assert_eq!(calls[0].extra, "", "setupdata_guid опционален");
        assert_eq!(
            app.status_msg,
            "hijack: ifr 0x1000..0x1100 · flips pkg+0x1c: 01 00 -> ff ff · strings 1"
        );
    }

    uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii hijack 11111111-2222-3333-4444-555555555555:0x19:0 {file} SETUP-GUID"),
        &mut client,
    )
    .await
    .unwrap();
    let calls = calls.lock().await;
    assert_eq!(calls[1].extra, "SETUP-GUID");
    assert!(
        uefi_tui::commands::execute_command(
            &mut app,
            "hii hijack 11111111-2222-3333-4444-555555555555:0x19:0",
            &mut client
        )
        .await
        .is_err(),
        "без FILE — usage-ошибка"
    );
}
