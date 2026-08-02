use std::io::stdout;
use std::time::Duration;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use uefi_tui::app::{App, Mode};
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
            Mode::Normal => handle_normal(&mut app, &ev),
            Mode::Command => handle_command(&mut app, &ev, &mut client).await,
            Mode::Insert => handle_insert(&mut app, &ev),
        }
        if app.quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_normal(app: &mut App, ev: &AppEvent) {
    match ev {
        AppEvent::Key('j') | AppEvent::Down => app.cursor_down(),
        AppEvent::Key('k') | AppEvent::Up => app.cursor_up(),
        AppEvent::Key(':') => app.enter_command_mode(),
        AppEvent::Key('?') => app.show_help = !app.show_help,
        AppEvent::Key('q') => app.quit = true,
        AppEvent::Quit => app.quit = true,
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

fn handle_insert(app: &mut App, ev: &AppEvent) {
    if ev == &AppEvent::Esc {
        app.exit_to_normal();
    }
}
