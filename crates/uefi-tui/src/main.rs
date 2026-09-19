use std::io::stdout;
use std::time::Duration;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use uefi_tui::app::{App, CmdFlow, Focus, FormsFocus, Mode, RegistryRow, View};
use uefi_tui::commands;
use uefi_tui::input::{self, AppEvent};
use uefi_tui::ui;

#[derive(Parser)]
#[command(name = "uefi-tui", version, about = "UEFIPatcher TUI")]
struct Cli {
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let state = uefi_common::read_state().unwrap_or_default();
    let mut app = App::new();
    let mut client = match commands::connect(cli.sock.as_deref(), state.clone()).await {
        Ok(c) => Some(c),
        Err(e) => {
            app.engine_online = false;
            app.status_msg = format!("ENGINE OFFLINE: {e}");
            eprintln!("warning: cannot connect to engine: {e}");
            None
        }
    };
    app.engine_online = client.is_some();
    if let Some(c) = client.as_mut() {
        let _ = commands::restore_session(&mut app, c).await;
    }
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    loop {
        terminal.draw(|f| {
            ui::render(f, &mut app);
            ui::help::render(f, &app);
        })?;
        let Some(ev) = input::poll_event(Duration::from_millis(100)) else {
            continue;
        };
        match app.mode {
            Mode::Normal => handle_normal(&mut app, &ev, &mut client).await,
            Mode::Command | Mode::Insert => handle_command(&mut app, &ev, &mut client).await,
        }
        if app.quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

async fn handle_normal(app: &mut App, ev: &AppEvent, client: &mut Option<commands::Client>) {
    if app.show_help {
        match ev {
            AppEvent::Key('?') => app.toggle_help(),
            AppEvent::Key('q') | AppEvent::Quit => app.quit = true,
            AppEvent::Key('j') | AppEvent::Down => {
                app.help_scroll = app.help_scroll.saturating_add(1);
            }
            AppEvent::Key('k') | AppEvent::Up => {
                app.help_scroll = app.help_scroll.saturating_sub(1);
            }
            AppEvent::PageDown => app.help_scroll = app.help_scroll.saturating_add(10),
            AppEvent::PageUp => app.help_scroll = app.help_scroll.saturating_sub(10),
            _ => {}
        }
        return;
    }
    if matches!(ev, AppEvent::Tab | AppEvent::BackTab) {
        switch_view(app, client).await;
        return;
    }
    if app.view == View::Forms {
        handle_normal_forms(app, ev, client).await;
        return;
    }
    match ev {
        AppEvent::Ctrl('h') | AppEvent::Ctrl('k') => app.focus_prev(),
        AppEvent::Ctrl('l') | AppEvent::Ctrl('j') => app.focus_next(),
        AppEvent::Key('?') => app.toggle_help(),
        AppEvent::Key('q') | AppEvent::Quit => app.quit = true,
        AppEvent::Key(':') => app.enter_command_mode(),
        AppEvent::Key('i') | AppEvent::Key('r') | AppEvent::Key('d')
            if app.focus == Focus::Tree =>
        {
            let (cmd_str, prefill) = {
                let path = app.selected_path().unwrap_or_default();
                let row = app.current_registry_row();
                let artifacts = &app.registry.artifacts;
                match ev {
                    AppEvent::Key('i') => (
                        "insert",
                        commands::mutation_prefill("insert", &path, row.as_ref(), artifacts),
                    ),
                    AppEvent::Key('r') => (
                        "replace",
                        commands::mutation_prefill("replace", &path, row.as_ref(), artifacts),
                    ),
                    _ => ("remove", format!("remove {path}")),
                }
            };
            app.enter_insert_mode(cmd_str, prefill);
        }
        AppEvent::Key('w') if app.focus != Focus::Details => match client.as_mut() {
            Some(c) => {
                if let Err(e) = commands::reopen(app, c, true).await {
                    app.status_msg = format!("error: {e}");
                }
            }
            None => app.status_msg = "no engine connection".into(),
        },
        AppEvent::Key('j') | AppEvent::Down => match app.focus {
            Focus::Registry => app.registry_cursor_down(),
            Focus::Tree => app.cursor_down(),
            Focus::Details => app.details_scroll_by(1),
        },
        AppEvent::Key('k') | AppEvent::Up => match app.focus {
            Focus::Registry => app.registry_cursor_up(),
            Focus::Tree => app.cursor_up(),
            Focus::Details => app.details_scroll_by(-1),
        },
        AppEvent::PageDown => match app.focus {
            Focus::Tree => app.cursor_page_down(),
            Focus::Details => app.details_scroll_by(10),
            Focus::Registry => {}
        },
        AppEvent::PageUp => match app.focus {
            Focus::Tree => app.cursor_page_up(),
            Focus::Details => app.details_scroll_by(-10),
            Focus::Registry => {}
        },
        AppEvent::Key('h') => {
            if app.focus == Focus::Tree {
                app.set_expand_selected(false);
            }
        }
        AppEvent::Key('l') => {
            if app.focus == Focus::Tree {
                app.set_expand_selected(true);
            }
        }
        AppEvent::Enter if app.focus == Focus::Registry => {
            handle_registry_enter(app, client).await;
        }
        AppEvent::Key('/') => {
            app.enter_insert_mode("goto", "goto ".into());
        }
        _ => {}
    }
}

async fn handle_command(app: &mut App, ev: &AppEvent, client: &mut Option<commands::Client>) {
    match app.cmd_key(ev) {
        CmdFlow::Execute => {
            let cmd = app.cmdline.as_str().to_string();
            if let Some(c) = client {
                match commands::execute_command(app, &cmd, c).await {
                    Ok(_) => {}
                    Err(e) => app.status_msg = format!("error: {e}"),
                }
            } else {
                app.status_msg = "no engine connection".into();
            }
            app.history.submit(&cmd);
            app.history.save();
            app.exit_to_normal();
        }
        CmdFlow::Exit => app.exit_to_normal(),
        CmdFlow::None => {}
    }
}

async fn handle_registry_enter(app: &mut App, client: &mut Option<commands::Client>) {
    let Some(c) = client else { return };
    match app.current_registry_row() {
        Some(RegistryRow::Image(i)) => {
            if let Some(im) = app.registry.images.get(i).cloned() {
                let cmd = format!("switch {}", im.image_id);
                if let Err(e) = commands::execute_command(app, &cmd, c).await {
                    app.status_msg = format!("error: {e}");
                }
                app.focus = Focus::Tree;
            }
        }
        Some(row @ RegistryRow::Artifact(i)) if app.registry.artifacts.get(i).is_some() => {
            let path = app.selected_path().unwrap_or_default();
            let prefill =
                commands::mutation_prefill("insert", &path, Some(&row), &app.registry.artifacts);
            app.enter_insert_mode("insert", prefill);
        }
        _ => {}
    }
}

async fn switch_view(app: &mut App, client: &mut Option<commands::Client>) {
    match app.view {
        View::Image => {
            let has_image = app.active_image_id.is_some()
                || client
                    .as_ref()
                    .and_then(|c| c.state.active_image_id.clone())
                    .is_some();
            if !has_image {
                app.status_msg = "no active image — :open PATH first".into();
                return;
            }
            app.view = View::Forms;
            if let Some(c) = client.as_mut()
                && let Err(e) = commands::refresh_forms(app, c).await
            {
                app.status_msg = format!("error: {e}");
            }
        }
        View::Forms => {
            app.view = View::Image;
        }
    }
}

async fn handle_normal_forms(app: &mut App, ev: &AppEvent, client: &mut Option<commands::Client>) {
    match ev {
        AppEvent::Key('?') => app.toggle_help(),
        AppEvent::Key('q') | AppEvent::Quit => app.quit = true,
        AppEvent::Key(':') => app.enter_command_mode(),
        AppEvent::Ctrl('h') | AppEvent::Ctrl('k') => app.forms.focus = app.forms.focus.prev(),
        AppEvent::Ctrl('l') | AppEvent::Ctrl('j') => app.forms.focus = app.forms.focus.next(),
        AppEvent::Key('j') | AppEvent::Down
            if app.forms.focus == FormsFocus::List
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            app.forms_cursor_down();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_form_details_if_needed(app, c).await;
            }
        }
        AppEvent::Key('k') | AppEvent::Up
            if app.forms.focus == FormsFocus::List
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            app.forms_cursor_up();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_form_details_if_needed(app, c).await;
            }
        }
        AppEvent::Key('j') | AppEvent::Down
            if app.forms.focus == FormsFocus::Details
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            app.forms_question_cursor_down();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_question_info_if_needed(app, c).await;
            }
        }
        AppEvent::Key('k') | AppEvent::Up
            if app.forms.focus == FormsFocus::Details
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            app.forms_question_cursor_up();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_question_info_if_needed(app, c).await;
            }
        }
        AppEvent::PageDown
            if app.forms.focus == FormsFocus::Details
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            app.forms_question_page_down();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_question_info_if_needed(app, c).await;
            }
        }
        AppEvent::PageUp
            if app.forms.focus == FormsFocus::Details
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            app.forms_question_page_up();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_question_info_if_needed(app, c).await;
            }
        }
        AppEvent::Enter
            if app.forms.focus == FormsFocus::Details
                && !app.forms.show_strings
                && !app.forms.show_varstores =>
        {
            if let Some(pre) = commands::set_value_prefill(app) {
                app.enter_insert_mode("hii", pre);
            }
        }
        AppEvent::Key('j') | AppEvent::Down
            if app.forms.show_strings && !app.forms.show_varstores =>
        {
            app.strings_cursor_down()
        }
        AppEvent::Key('k') | AppEvent::Up
            if app.forms.show_strings && !app.forms.show_varstores =>
        {
            app.strings_cursor_up()
        }
        AppEvent::Key('j') | AppEvent::Down if app.forms.show_varstores => {
            app.varstores_cursor_down()
        }
        AppEvent::Key('k') | AppEvent::Up if app.forms.show_varstores => app.varstores_cursor_up(),
        AppEvent::Key('h')
            if !app.forms.show_strings
                && !app.forms.show_varstores
                && app.forms.focus == FormsFocus::List =>
        {
            app.forms_set_expanded(false);
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_form_details_if_needed(app, c).await;
            }
        }
        AppEvent::Key('l')
            if !app.forms.show_strings
                && !app.forms.show_varstores
                && app.forms.focus == FormsFocus::List =>
        {
            app.forms_set_expanded(true);
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_form_details_if_needed(app, c).await;
            }
        }
        AppEvent::Key('S') => {
            let need_fetch = app.forms.strings.is_empty();
            app.forms.show_strings = !app.forms.show_strings;
            if app.forms.show_strings
                && need_fetch
                && let Some(c) = client.as_mut()
                && let Err(e) = commands::refresh_strings(app, c).await
            {
                app.status_msg = format!("error: {e}");
            }
        }
        AppEvent::Key('V') => {
            let need_fetch = match (
                &app.forms.varstores_target,
                commands::selected_form_target(app),
            ) {
                (Some(cached), Some(cur)) if cached == &cur => false,
                (_, Some(_)) => true,
                _ => false,
            };
            app.forms.show_varstores = !app.forms.show_varstores;
            if app.forms.show_varstores
                && need_fetch
                && let Some(target) = commands::selected_form_target(app)
                && let Some(c) = client.as_mut()
                && let Err(e) = commands::refresh_varstores(app, c, &target).await
            {
                app.status_msg = format!("error: {e}");
            }
        }
        AppEvent::Key('v') if !app.forms.show_strings && !app.forms.show_varstores => {
            if let (Some(item), Some(vis)) = (
                commands::selected_form_item_id(app),
                app.selected_form_visible(),
            ) {
                match commands::form_visibility_command(&item, vis) {
                    Some(cmd) => {
                        if let Some(c) = client.as_mut()
                            && let Err(e) = commands::execute_command(app, &cmd, c).await
                        {
                            app.status_msg = format!("error: {e}");
                        }
                    }
                    None => {
                        app.status_msg = format!(
                            "already visible — hiding not supported (engine implements unsuppress only): {item}"
                        );
                    }
                }
            }
        }
        AppEvent::Key('u') if !app.forms.show_strings && !app.forms.show_varstores => {
            if let Some(item) = commands::selected_form_item_id(app) {
                let cmd = format!("hii unlock {item}");
                if let Some(c) = client.as_mut()
                    && let Err(e) = commands::execute_command(app, &cmd, c).await
                {
                    app.status_msg = format!("error: {e}");
                }
            }
        }
        AppEvent::Key('a') if !app.forms.show_strings && !app.forms.show_varstores => {
            if let Some(pre) = commands::add_prefill(app) {
                app.enter_insert_mode("hii", pre);
            }
        }
        AppEvent::Key('e') if !app.forms.show_strings && !app.forms.show_varstores => {
            if let Some(pre) = commands::export_prefill(app) {
                app.enter_insert_mode("hii", pre);
            }
        }
        AppEvent::Key('I') if !app.forms.show_strings && !app.forms.show_varstores => {
            app.enter_insert_mode("hii", commands::import_prefill(app));
        }
        AppEvent::Key('A') if !app.forms.show_strings && !app.forms.show_varstores => {
            if let Some(pre) = commands::question_add_prefill(app) {
                app.enter_insert_mode("hii", pre);
            }
        }
        AppEvent::Key('R') if !app.forms.show_strings && !app.forms.show_varstores => {
            if let Some(key) = app.selected_form_key() {
                app.status_msg = commands::ref_import_hint(&key);
                app.enter_insert_mode("hii", commands::ref_import_prefill(&key));
            }
        }
        AppEvent::Key('T') if !app.forms.show_strings && !app.forms.show_varstores => {
            app.forms.flat_mode = !app.forms.flat_mode;
            app.forms_sanitize_cursor();
            if let Some(c) = client.as_mut() {
                let _ = commands::refresh_form_details_if_needed(app, c).await;
            }
        }
        AppEvent::Key('/') => {
            let need_fetch = app.forms.strings.is_empty();
            app.forms.show_strings = true;
            if need_fetch
                && let Some(c) = client.as_mut()
                && let Err(e) = commands::refresh_strings(app, c).await
            {
                app.status_msg = format!("error: {e}");
                return;
            }
            app.enter_insert_mode("filter", "filter ".into());
        }
        AppEvent::Esc if app.forms.show_strings => app.forms.show_strings = false,
        AppEvent::Esc if app.forms.show_varstores => app.forms.show_varstores = false,
        _ => {}
    }
}
