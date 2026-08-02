#![allow(dead_code)]

mod app;

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
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
        terminal.draw(|f| render_placeholder(f, &app))?;
        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(k) = event::read()?
        {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match k.code {
                KeyCode::Char('q') => app.quit = true,
                KeyCode::Char('j') => app.cursor_down(),
                KeyCode::Char('k') => app.cursor_up(),
                _ => {}
            }
        }
        if app.quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn render_placeholder(f: &mut ratatui::Frame, app: &app::App) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    let area = f.area();
    let p = Paragraph::new(format!(
        "UEFI TUI — mode={:?} cursor={} tree={}",
        app.mode,
        app.cursor,
        app.tree.len()
    ))
    .block(Block::default().borders(Borders::ALL).title("Placeholder"));
    f.render_widget(p, area);
}
