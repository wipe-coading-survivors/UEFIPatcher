use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;

pub fn render(f: &mut Frame, app: &App) {
    if !app.show_help {
        return;
    }
    let area = centered(60, 20, f.area());
    f.render_widget(Clear, area);
    let help_text = "UEFI TUI Help\n\
        \nNormal mode:\n\
        j/k      — move cursor\n\
        Space    — expand/collapse\n\
        Enter    — select node\n\
        :        — enter command mode\n\
        ?        — toggle this help\n\
        q        — quit\n\
        \nCommands:\n\
        :open PATH [--mode read|write]\n\
        :dump [--format text|tsv]\n\
        :find TARGET\n\
        :insert TARGET FFS [--mode into|before|after]\n\
        :remove TARGET\n\
        :replace TARGET DATA [--body-only]\n\
        :rebuild TARGET\n\
        :set-visibility ITEM [--visible|--hidden]\n\
        :save OUTPUT\n\
        :extract TARGET [--body-only]\n\
        :export ARTIFACT_ID [PATH]\n\
        :import FILE\n\
        :artifacts\n\
        :session init|destroy|list\n\
        :help / :h\n\
        :quit / :q";
    let p = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(p, area);
}

fn centered(percent_w: u16, percent_h: u16, area: Rect) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_h) / 2),
            Constraint::Percentage(percent_h),
            Constraint::Percentage((100 - percent_h) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_w) / 2),
            Constraint::Percentage(percent_w),
            Constraint::Percentage((100 - percent_w) / 2),
        ])
        .split(popup[1])[1]
}
