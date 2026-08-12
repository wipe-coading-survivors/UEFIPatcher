use std::io::stdout;
use std::time::Duration;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use uefi_tui::app::{App, Focus, Mode, RegistryRow};
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
    let mut client = match commands::connect(cli.sock.as_deref(), state.clone()).await {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("warning: cannot connect to engine: {e}");
            None
        }
    };
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    loop {
        terminal.draw(|f| {
            ui::render(f, &app);
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
    match ev {
        AppEvent::Ctrl('h') | AppEvent::Ctrl('k') => app.focus_prev(),
        AppEvent::Ctrl('l') | AppEvent::Ctrl('j') => app.focus_next(),
        AppEvent::Key('?') => app.show_help = !app.show_help,
        AppEvent::Key('q') | AppEvent::Quit => app.quit = true,
        AppEvent::Key(':') => app.enter_command_mode(),
        AppEvent::Key('i') | AppEvent::Key('r') | AppEvent::Key('d')
            if app.focus == Focus::Tree =>
        {
            let (cmd_str, prefill) = match ev {
                AppEvent::Key('i') => (
                    "insert",
                    format!("insert {} --file ", app.selected_path().unwrap_or_default()),
                ),
                AppEvent::Key('r') => (
                    "replace",
                    format!(
                        "replace {} --file ",
                        app.selected_path().unwrap_or_default()
                    ),
                ),
                _ => ("remove", "remove ".to_string()),
            };
            app.enter_insert_mode(cmd_str, prefill);
        }
        AppEvent::Key('j') | AppEvent::Down => match app.focus {
            Focus::Registry => app.registry_cursor_down(),
            Focus::Tree => app.cursor_down(),
            Focus::Details => {}
        },
        AppEvent::Key('k') | AppEvent::Up => match app.focus {
            Focus::Registry => app.registry_cursor_up(),
            Focus::Tree => app.cursor_up(),
            Focus::Details => {}
        },
        AppEvent::Key('h') | AppEvent::Key('l') => {
            if app.focus == Focus::Tree {
                app.toggle_expand_selected();
            }
        }
        AppEvent::Enter if app.focus == Focus::Registry => {
            handle_registry_enter(app, client).await;
        }
        _ => {}
    }
}

async fn handle_command(app: &mut App, ev: &AppEvent, client: &mut Option<commands::Client>) {
    match ev {
        AppEvent::Key(c) => app.cmdline.push(*c),
        AppEvent::Enter => {
            let cmd = app.cmdline.clone();
            if let Some(c) = client {
                match commands::execute_command(app, &cmd, c).await {
                    Ok(_) => {}
                    Err(e) => app.status_msg = format!("error: {e}"),
                }
            } else {
                app.status_msg = "no engine connection".into();
            }
            app.exit_to_normal();
        }
        AppEvent::Esc => app.exit_to_normal(),
        AppEvent::Backspace => {
            app.cmdline.pop();
        }
        _ => {}
    }
}

async fn handle_registry_enter(app: &mut App, client: &mut Option<commands::Client>) {
    let Some(c) = client else { return };
    match app.current_registry_row() {
        Some(RegistryRow::Image(i)) => {
            if let Some(im) = app.registry.images.get(i).cloned() {
                let cmd = format!("image switch {}", im.image_id);
                if let Err(e) = commands::execute_command(app, &cmd, c).await {
                    app.status_msg = format!("error: {e}");
                }
                app.focus = Focus::Tree;
            }
        }
        Some(RegistryRow::Artifact(i)) => {
            if let Some(ar) = app.registry.artifacts.get(i).cloned() {
                let path = app.selected_path().unwrap_or_default();
                app.enter_insert_mode(
                    "insert",
                    format!("insert {path} --artifact-id {} ", ar.artifact_id),
                );
            }
        }
        None => {}
    }
}
