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
async fn varstores_cache_invalidated_on_refresh_and_reload() {
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

    app.forms.varstores = vec![uefi_proto::VarStoreInfo {
        id: 2,
        guid: String::new(),
        size: 0x94,
        name: "Setup".into(),
    }];
    app.forms.varstores_target = Some("t:0x19:0".into());
    app.forms.varstores_cursor = 1;
    app.forms.show_varstores = true;
    uefi_tui::commands::refresh_forms(&mut app, &mut client)
        .await
        .unwrap();
    assert!(
        app.forms.varstores.is_empty(),
        "refresh_forms: varstores-кэш очищен (как strings)"
    );
    assert!(app.forms.varstores_target.is_none());
    assert_eq!(app.forms.varstores_cursor, 0);
    assert!(
        !app.forms.show_varstores,
        "refresh_forms: панель закрыта (как strings)"
    );

    app.forms.varstores_target = Some("t:0x19:0".into());
    uefi_tui::commands::reload_forms(&mut app, &mut client)
        .await
        .unwrap();
    assert!(
        app.forms.varstores_target.is_none(),
        "reload_forms: мутация форсит re-fetch по следующему 'V'"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn question_info_cache_invalidated_on_refresh_forms() {
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
    let key = app.selected_form_key().unwrap();
    app.forms.question_info_key = Some((key, 0x211));
    app.forms.question_info = Some(uefi_proto::QuestionInfo {
        question_id: 0x211,
        ..Default::default()
    });
    uefi_tui::commands::refresh_forms(&mut app, &mut client)
        .await
        .unwrap();
    assert!(
        app.forms.question_info.is_none(),
        "refresh_forms: question_info-кэш очищен (как current_value)"
    );
    assert!(app.forms.question_info_key.is_none());
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

    uefi_tui::commands::refresh_current_value_if_needed(&mut app, &mut client)
        .await
        .unwrap();
    let cv = app.forms.current_value.as_ref().unwrap();
    assert_eq!(cv.value, Some(1));
    assert_eq!(cv.option.as_deref(), Some("Enabled"));
    assert_eq!(cv.store_path.as_deref(), Some("0/0/0"));
    assert!(
        app.forms
            .current_value_key
            .as_ref()
            .is_some_and(|(_, qid)| *qid != 0 || true),
        "ключ кэша установлен"
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
        app.status_msg.contains("form-level unlock"),
        "form-таргет: подсказка про построчные гейты"
    );

    let qitem = "11111111-2222-3333-4444-555555555555:0x19:0#10001:0x210";
    uefi_tui::commands::execute_command(&mut app, &format!("hii unlock {qitem}"), &mut client)
        .await
        .unwrap();
    assert!(
        !app.status_msg.contains("form-level unlock"),
        "question-таргет: note не печатается"
    );

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
async fn switch_top_level_image_bare_moved_hint() {
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

    let r = uefi_tui::commands::execute_command(&mut app, "switch img-existing", &mut client).await;
    assert_eq!(r.unwrap(), "img-existing");
    assert_eq!(app.active_image_id.as_deref(), Some("img-existing"));
    assert!(app.image_loaded);
    assert!(!app.tree.is_empty());

    uefi_tui::commands::execute_command(&mut app, "forms", &mut client)
        .await
        .unwrap();
    assert!(matches!(app.view, uefi_tui::app::View::Forms));
    let r = uefi_tui::commands::execute_command(&mut app, "image", &mut client).await;
    assert_eq!(r.unwrap(), "image");
    assert!(matches!(app.view, uefi_tui::app::View::Image));

    let err =
        uefi_tui::commands::execute_command(&mut app, "image switch img-existing", &mut client)
            .await
            .unwrap_err();
    assert!(
        err.contains("moved"),
        "ошибка переносится с подсказкой: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reopen_read_to_write_and_guards() {
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
    uefi_tui::commands::execute_command(&mut app, "refresh", &mut client)
        .await
        .unwrap();
    app.focus = uefi_tui::app::Focus::Registry;

    let r = uefi_tui::commands::reopen(&mut app, &mut client, true).await;
    assert!(r.is_ok());
    assert_ne!(app.active_image_id.as_deref(), Some("mock-img-1"));
    assert!(
        app.status_msg.contains("reopened"),
        "статус: {}",
        app.status_msg
    );

    app.registry.images[0].mode = 1;
    uefi_tui::commands::reopen(&mut app, &mut client, true)
        .await
        .unwrap();
    assert!(app.status_msg.contains("already in write mode"));

    let err = uefi_tui::commands::reopen(&mut app, &mut client, false)
        .await
        .unwrap_err();
    assert!(err.contains(":save first"), "WRITE→read отказ: {err}");
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
async fn nvar_list_summary_and_var_filter() {
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

    let err = uefi_tui::commands::execute_command(&mut app, "nvar list", &mut client).await;
    assert!(err.is_err(), "без активного образа — отказ");

    uefi_tui::commands::execute_command(&mut app, "open /dev/null", &mut client)
        .await
        .unwrap();
    uefi_tui::commands::execute_command(&mut app, "nvar list", &mut client)
        .await
        .unwrap();
    assert_eq!(
        app.status_msg, "nvar: 1 stores · 2 vars (1/0/0)",
        "сводный статус по всем сторам"
    );
    assert!(
        app.nvar.stores.is_empty(),
        "сводный режим панель не трогает"
    );

    uefi_tui::commands::execute_command(&mut app, "nvar list --var Timeout", &mut client)
        .await
        .unwrap();
    assert!(
        app.status_msg.contains("Timeout 0x0050057a 2B @1/0/0"),
        "строки переменной с офетом и стором: {}",
        app.status_msg
    );

    uefi_tui::commands::execute_command(&mut app, "nvar list --var Nope", &mut client)
        .await
        .unwrap();
    assert_eq!(app.status_msg, "nvar: no vars named Nope");

    let err =
        uefi_tui::commands::execute_command(&mut app, "nvar list 1/0/0 --var Timeout", &mut client)
            .await
            .unwrap_err();
    assert!(err.contains("--var"), "PATH + --var — usage-отказ: {err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn nvar_list_targeted_loads_pane_and_moves_cursor() {
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
    app.cursor = 0;

    uefi_tui::commands::execute_command(&mut app, "nvar list 1/0", &mut client)
        .await
        .unwrap();
    assert_eq!(app.nvar.stores.len(), 1, "панель загружена по PATH");
    assert_eq!(app.nvar.stores[0].path, "1/0");
    assert_eq!(app.nvar.cursor, 0, "курсор панель сбрасывает");
    assert!(
        app.nvar.key.as_deref().is_some_and(|k| k.ends_with(":1/0")),
        "ключ кэша панель: {:?}",
        app.nvar.key
    );
    assert_eq!(
        app.selected_path().as_deref(),
        Some("1/0"),
        "курсор дерева переведён"
    );
    assert!(
        app.status_msg.starts_with("nvar 1/0: 2 vars · records 3"),
        "статус целевого режима: {}",
        app.status_msg
    );

    let err = uefi_tui::commands::execute_command(&mut app, "nvar list 9/9", &mut client)
        .await
        .unwrap_err();
    assert!(
        err.contains("no node at path"),
        "goto несуществующего пути: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn nvar_set_sends_rpc_invalidates_pane_and_validates() {
    let td = TempDir::new().unwrap();
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
    app.nvar.key = Some("stale".into());
    app.nvar.stores = vec![uefi_proto::NvarStoreInfo::default()];

    uefi_tui::commands::execute_command(
        &mut app,
        "nvar set Timeout --offset 0 --value 5 --width 2",
        &mut client,
    )
    .await
    .unwrap();
    let calls = calls.lock().await;
    let sets: Vec<_> = calls.iter().filter(|c| c.rpc == "NvarSet").collect();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].target, "Timeout");
    assert_eq!(sets[0].schema_json, "0x0");
    assert_eq!(sets[0].extra, "|0x5|2", "guid пуст, value/width на месте");
    drop(calls);
    assert_eq!(app.nvar.key, None, "кэш панель сброшен после set");
    assert!(
        app.status_msg.contains("nvar set Timeout+0x0=0x5 (w2)"),
        "статус set: {}",
        app.status_msg
    );
    assert!(
        app.status_msg
            .contains("applied 0/2 AMI NVAR store+0x0: 00 -> 05"),
        "applied-строка движка в статусе: {}",
        app.status_msg
    );

    let err = uefi_tui::commands::execute_command(
        &mut app,
        "nvar set Timeout --offset 0 --value 5 --width 3",
        &mut client,
    )
    .await
    .unwrap_err();
    assert_eq!(err, "width must be one of 1, 2, 4, 8");

    let err =
        uefi_tui::commands::execute_command(&mut app, "nvar set Timeout --value 5", &mut client)
            .await
            .unwrap_err();
    assert!(
        err.contains("usage: :nvar set"),
        "--offset обязателен: {err}"
    );

    let err = uefi_tui::commands::execute_command(&mut app, "nvar bogus", &mut client)
        .await
        .unwrap_err();
    assert!(err.contains("usage: :nvar"), "неизвестный подглагол: {err}");
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
    let formset_adds: Vec<_> = calls.iter().filter(|c| c.rpc == "HiiFormSetAdd").collect();
    assert_eq!(formset_adds.len(), 1);
    assert_eq!(formset_adds[0].schema_json, body);
    assert_eq!(
        formset_adds[0].target, "",
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
async fn hii_formset_add_flag_aware_grammar() {
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
        &format!("hii formset add --ffs ABC-GUID {file}"),
        &mut client,
    )
    .await
    .unwrap();
    {
        let calls = calls.lock().await;
        assert_eq!(calls[0].rpc, "HiiFormSetAdd");
        assert_eq!(
            calls[0].target, "ABC-GUID",
            "флаг-первый порядок эквивалентен"
        );
    }

    let e = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii formset add {file} --ffs"),
        &mut client,
    )
    .await
    .unwrap_err();
    assert!(e.contains("usage"), "dangling --ffs — usage-ошибка: {e}");

    let e = uefi_tui::commands::execute_command(
        &mut app,
        &format!("hii formset add {file} extra.json"),
        &mut client,
    )
    .await
    .unwrap_err();
    assert!(e.contains("usage"), "лишний позиционный токен — usage: {e}");
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
    let hijacks: Vec<_> = calls.iter().filter(|c| c.rpc == "HiiFormHijack").collect();
    assert_eq!(hijacks.len(), 2);
    assert_eq!(hijacks[1].extra, "SETUP-GUID");
    drop(calls);
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

#[tokio::test(flavor = "multi_thread")]
async fn smoke_open_collapse_switch_registry_pick() {
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
    uefi_tui::commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
        .await
        .unwrap();
    assert!(app.tree[0].expanded);
    assert!(!app.tree[1].expanded);
    app.toggle_expand_selected();
    assert!(!app.tree[app.selected_tree_idx().unwrap()].expanded);
    uefi_tui::commands::execute_command(&mut app, "switch mock-img-1", &mut client)
        .await
        .unwrap();
    assert_eq!(app.active_image_id.as_deref(), Some("mock-img-1"));
    app.focus_next();
    app.focus_next();
    assert_eq!(app.focus, uefi_tui::app::Focus::Registry);
    assert!(matches!(
        app.current_registry_row(),
        Some(uefi_tui::app::RegistryRow::Artifact(0)) | Some(uefi_tui::app::RegistryRow::Image(_))
    ));
}

use hyper_util::rt::TokioIo;
use tonic::transport::Endpoint;
use tower::service_fn;
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;
use uefi_tui::app::App;
use uefi_tui::commands;

use mock_server::{SchemaCall, start_mock};
#[tokio::test]
async fn mock_roundtrip() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let sock_str = sock.to_string_lossy().to_string();
    let channel = Endpoint::try_from("http://localhost")
        .unwrap()
        .connect_with_connector(service_fn(move |_: http::Uri| {
            let s = sock_str.clone();
            async move {
                Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
            }
        }))
        .await
        .unwrap();
    let mut client = EngineServiceClient::new(channel);
    let resp = client
        .session_create(SessionCreateRequest {
            name: "test".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.session_id.is_empty());
    assert!(!resp.token.is_empty());
}

#[tokio::test]
async fn schema_rpcs_record_calls_and_fixture_responses() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let (_h, calls) = start_mock(&sock).await;
    let sock_str = sock.to_string_lossy().to_string();
    let channel = Endpoint::try_from("http://localhost")
        .unwrap()
        .connect_with_connector(service_fn(move |_: http::Uri| {
            let s = sock_str.clone();
            async move {
                Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
            }
        }))
        .await
        .unwrap();
    let mut client = EngineServiceClient::new(channel);
    let r = client
        .hii_form_set_add(HiiFormSetAddRequest {
            image_id: "img-1".into(),
            schema_json: "{\"forms\":[]}".into(),
            target_ffs_guid: "FFS-GUID".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(r.new_ffs_id, "mock-ffs-1");
    assert_eq!(r.inserted_form_ids, vec![10101]);
    let calls = calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        SchemaCall {
            rpc: "HiiFormSetAdd",
            image_id: "img-1".into(),
            target: "FFS-GUID".into(),
            schema_json: "{\"forms\":[]}".into(),
            extra: String::new(),
        }
    );
}

#[tokio::test]
async fn open_sets_active_image_and_builds_tree() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    let res = commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client).await;
    assert!(res.is_ok(), "open failed: {:?}", res.err());
    assert_eq!(app.active_image_id, client.state.active_image_id);
    assert!(app.active_image_id.is_some());
    assert!(app.tree.len() >= 5);
    assert!(app.tree[0].has_children);
    assert!(app.registry.images.len() == 1);
    assert!(app.registry.artifacts.len() == 1);
}

#[tokio::test]
async fn upload_sets_active_image_and_tree() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    let out = std::env::temp_dir().join("tui-upload-test.bin");
    std::fs::write(&out, b"mock").unwrap();
    let r = commands::execute_command(&mut app, &format!("upload {}", out.display()), &mut client)
        .await
        .unwrap();
    assert_eq!(app.active_image_id.as_deref(), Some(r.as_str()));
    assert_eq!(client.state.active_image_id.as_deref(), Some(r.as_str()));
    assert!(app.image_loaded);
    assert!(!app.tree.is_empty());
    let _ = std::fs::remove_file(&out);
}

#[tokio::test]
async fn snapshot_returns_id() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
        .await
        .unwrap();
    let r = commands::execute_command(&mut app, "snapshot before", &mut client)
        .await
        .unwrap();
    assert_eq!(r, "snap-1");
    assert_eq!(app.status_msg, "snapshot snap-1 (before)");
}

#[tokio::test]
async fn snapshots_puts_list_into_status_msg() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
        .await
        .unwrap();
    let r = commands::execute_command(&mut app, "snapshots", &mut client)
        .await
        .unwrap();
    assert_eq!(app.status_msg, "snap-1  before  16B");
    assert_eq!(r, "snap-1");
}

#[tokio::test]
async fn restore_refreshes_tree_and_registry() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
        .await
        .unwrap();
    app.tree.clear();
    app.registry.images.clear();
    let r = commands::execute_command(&mut app, "restore snap-1", &mut client)
        .await
        .unwrap();
    assert_eq!(r, "snap-1");
    assert!(!app.tree.is_empty(), ":restore should refresh tree");
    assert_eq!(
        app.registry.images.len(),
        1,
        ":restore should refresh registry"
    );
    assert_eq!(app.status_msg, "restored snap-1");
}

#[tokio::test]
async fn refresh_populates_registry() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::execute_command(&mut app, "refresh", &mut client)
        .await
        .unwrap();
    assert_eq!(app.registry.images.len(), 1);
    assert_eq!(app.registry.artifacts.len(), 1);
}

#[tokio::test]
async fn extract_refreshes_registry() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
        .await
        .unwrap();
    app.registry.artifacts.clear();
    commands::execute_command(&mut app, "extract 1/0", &mut client)
        .await
        .unwrap();
    assert_eq!(
        app.registry.artifacts.len(),
        1,
        ":extract should refresh registry"
    );
}

#[tokio::test]
async fn import_refreshes_registry() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    app.registry.images.clear();
    app.registry.artifacts.clear();
    let f = std::fs::File::create("/tmp/uefi_tui_import_dummy.bin").unwrap();
    drop(f);
    commands::execute_command(
        &mut app,
        "import /tmp/uefi_tui_import_dummy.bin",
        &mut client,
    )
    .await
    .unwrap();
    assert!(
        !app.registry.artifacts.is_empty(),
        ":import should refresh registry"
    );
    let _ = std::fs::remove_file("/tmp/uefi_tui_import_dummy.bin");
}

#[tokio::test]
async fn image_close_clears_state() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::execute_command(&mut app, "open /tmp/mock.bin", &mut client)
        .await
        .unwrap();
    assert!(app.image_loaded);
    assert!(app.active_image_id.is_some());
    let active = app.active_image_id.clone().unwrap();
    commands::execute_command(&mut app, &format!("close {active}"), &mut client)
        .await
        .unwrap();
    assert!(app.tree.is_empty(), "tree should be cleared after close");
    assert!(
        app.active_image_id.is_none(),
        "active_image_id should be None after close"
    );
    assert!(
        !app.image_loaded,
        "image_loaded should be false after close"
    );
    assert_eq!(app.cursor, 0);
}

#[tokio::test]
async fn restore_session_populates_tree_and_registry() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        active_image_id: Some("mock-img-1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::restore_session(&mut app, &mut client)
        .await
        .unwrap();
    assert_eq!(app.active_image_id.as_deref(), Some("mock-img-1"));
    assert!(app.image_loaded, "image_loaded should be set on restore");
    assert!(
        app.tree.len() >= 5,
        "tree should be rebuilt from nodes list"
    );
    assert_eq!(app.registry.images.len(), 1);
    assert_eq!(app.registry.artifacts.len(), 1);
}

#[tokio::test]
async fn restore_session_without_active_image_only_registry() {
    let td = TempDir::new().unwrap();
    let sock = td.path().join("mock.sock");
    let _handle = start_mock(&sock).await;
    let state = uefi_common::state::State {
        session_id: Some("s1".into()),
        token: Some("t1".into()),
        ..Default::default()
    };
    let mut client = commands::connect(Some(sock.to_str().unwrap()), state)
        .await
        .unwrap();
    let mut app = App::new();
    commands::restore_session(&mut app, &mut client)
        .await
        .unwrap();
    assert!(app.active_image_id.is_none());
    assert!(app.tree.is_empty());
    assert_eq!(app.registry.images.len(), 1);
    assert_eq!(app.registry.artifacts.len(), 1);
}
