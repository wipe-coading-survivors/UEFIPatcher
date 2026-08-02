#![allow(dead_code)]

mod app;
mod commands;
mod input;
mod theme;
mod ui;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use input::AppEvent;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::stdout;

#[derive(Parser)]
#[command(name = "uefi-tui", version, about = "UEFIPatcher TUI")]
struct Cli {
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = app::App::new();
    loop {
        terminal.draw(|f| render(f, &app))?;
        let Some(ev) = input::poll_event(std::time::Duration::from_millis(100)) else {
            continue;
        };
        match ev {
            AppEvent::Key('j') | AppEvent::Down => app.cursor_down(),
            AppEvent::Key('k') | AppEvent::Up => app.cursor_up(),
            AppEvent::Quit => app.quit = true,
            _ => {}
        }
        if app.quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn render(f: &mut ratatui::Frame, app: &app::App) {
    ui::render(f, app);
    ui::help::render(f, app);
}
