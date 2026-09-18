use ratatui::Frame;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;

const HELP: &str = "\
UEFI TUI — Help
===============

Modes:  Normal (default) · Command (:) · Insert (i/r/d)

NORMAL (Tree focus)
  j / k          move cursor (по видимым строкам)
  PgDn / PgUp    page down / up (по высоте панели)
  h              collapse selected node
  l              expand selected node
  i              insert  -> prefill :insert <path> --file
  r              replace -> prefill :replace <path> --file
  d              remove  -> prefill :remove <path>
  Ctrl-h / Ctrl-k  focus prev panel (cycle: Tree->Registry->Details->Tree)
  Ctrl-l / Ctrl-j  focus next panel
  :              Command mode
  ?              toggle this help
  q              quit

REGISTRY focus
  j / k          move selection
  Enter on image    -> :switch <id>  (return to Tree)
  Enter on artifact -> prefill :insert <path> --artifact-id <id>

FORMS VIEW (Tab / Shift-Tab, :forms / :image)
  j / k          move cursor (формы / строки)
  h / l          collapse / expand формсета (и формы в REF-дереве)
  v              показать скрытую форму (unsuppress; скрытие не поддержано)
  u              unlock выбранной формы (:hii unlock)
  a              add: FormSet → :hii formset add · Form → :hii form add <target>
  T              плоский список <-> REF-дерево (путь в details)
  S              strings-браузер (повторно — закрыть; Esc тоже)
  /              открыть strings + промпт фильтра (:filter TEXT)
  Ctrl-hjkl      focus List <-> Details
  Details focus: j/k выбор вопроса · PgUp/PgDn страница · Enter → :hii set-value <item> <value>
  Tab            обратно в Image-view

COMMAND / INSERT
  TAB compl · ↑↓ hist · Ctrl+←→ word · Ctrl+W/U/K del · Home/End · Enter run · Esc cancel
  [menu]: ↑↓/jk select · TAB/→/Enter accept · BackTab back · Esc close · Enter (меню закрыто) run

EX-COMMANDS
  :open PATH [--mode read|write]
  :save OUTPUT
  :extract TARGET [--body-only]
  :export ARTIFACT_ID [PATH]
  :import FILE
  :insert [TARGET] (--file PATH | --artifact-id ID) [--mode into|before|after]
  :replace [TARGET] (--file PATH | --artifact-id ID) [--body-only]
  :remove [TARGET]
  :rebuild [TARGET]
  :switch ID | :close [ID]
  :forms                      переключить Forms-view (требует активный образ)
  :image                      обратно в Image-view
  :hii set-value ITEM VALUE   (ITEM = target#form[:qid], form — десятичное)
  :hii visibility ITEM on|off
  :hii unlock ITEM
  :hii formset add FILE [--ffs GUID]
  :hii form add TARGET FILE
  :hii question add TARGET#FORM FILE     (form_id — десятичное)
  :hii page add TARGET FILE
  :hii hijack TARGET FILE [SETUPDATA-GUID]
  :filter TEXT                фильтр strings-браузера (пустой — сброс)
  :refresh
  :artifacts
  :help | :h
  :quit | :q

TARGET default = node under cursor. Exactly one of --file / --artifact-id.
";

/// Clamp скролла по числу строк и высоте окна (минус рамка). Спека R6.
pub fn clamp_offset(off: u16, total: usize, visible: u16) -> u16 {
    if visible == 0 {
        return 0;
    }
    off.min((total as u16).saturating_sub(visible))
}

pub fn render(f: &mut Frame, app: &App) {
    if !app.show_help {
        return;
    }
    let area = f.area();
    f.render_widget(Clear, area);
    let visible = area.height.saturating_sub(2);
    let off = clamp_offset(app.help_scroll, HELP.lines().count(), visible);
    let p = Paragraph::new(HELP).scroll((off, 0)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Help (? to close)"),
    );
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::{HELP, clamp_offset};

    #[test]
    fn clamp_offset_bounds() {
        assert_eq!(clamp_offset(0, 70, 20), 0);
        assert_eq!(clamp_offset(100, 70, 20), 50);
        assert_eq!(clamp_offset(100, 10, 20), 0, "текст короче окна — скролл 0");
        assert_eq!(clamp_offset(5, 70, 0), 0, "нулевая высота — 0");
    }

    #[test]
    fn help_documents_top_level_switch_close() {
        assert!(HELP.contains(":switch ID | :close [ID]"));
        assert!(HELP.contains("Enter on image    -> :switch <id>"));
        assert!(HELP.contains(":image                      обратно в Image-view"));
    }

    #[test]
    fn help_documents_forms_details_paging() {
        assert!(HELP.contains(
            "Details focus: j/k выбор вопроса · PgUp/PgDn страница · Enter → :hii set-value <item> <value>"
        ));
    }

    #[test]
    fn help_documents_v3_add_commands_and_a_key() {
        for cmd in [
            ":hii formset add FILE [--ffs GUID]",
            ":hii form add TARGET FILE",
            ":hii question add TARGET#FORM FILE",
            ":hii page add TARGET FILE",
            ":hii hijack TARGET FILE [SETUPDATA-GUID]",
        ] {
            assert!(HELP.contains(cmd), "help должен документировать {cmd}");
        }
        assert!(
            HELP.contains("a              add"),
            "клавиша a в FORMS VIEW-секции"
        );
    }
}
