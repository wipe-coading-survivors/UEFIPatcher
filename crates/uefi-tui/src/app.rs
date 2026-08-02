#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Insert,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub path: String,
    pub depth: usize,
    pub node_type: u8,
    pub subtype: u8,
    pub guid: Option<String>,
    pub name: String,
    pub action: u8,
    pub expanded: bool,
    pub has_children: bool,
}

pub struct App {
    pub mode: Mode,
    pub tree: Vec<TreeNode>,
    pub cursor: usize,
    pub selected: Option<String>,
    pub cmdline: String,
    pub status_msg: String,
    pub image_loaded: bool,
    pub quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            tree: vec![],
            cursor: 0,
            selected: None,
            cmdline: String::new(),
            status_msg: "Welcome. Press : for commands, ? for help".into(),
            image_loaded: false,
            quit: false,
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.tree.len() {
            self.cursor += 1;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.cmdline.clear();
    }

    pub fn exit_to_normal(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_new_starts_normal() {
        let app = App::new();
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.quit);
    }

    #[test]
    fn cursor_down_increments() {
        let mut app = App::new();
        app.tree = vec![
            TreeNode {
                path: "0".into(),
                depth: 0,
                node_type: 62,
                subtype: 0,
                guid: None,
                name: "Image".into(),
                action: 50,
                expanded: true,
                has_children: true,
            },
            TreeNode {
                path: "0/0".into(),
                depth: 1,
                node_type: 65,
                subtype: 0,
                guid: None,
                name: "Volume".into(),
                action: 50,
                expanded: false,
                has_children: true,
            },
        ];
        app.cursor_down();
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn cursor_down_clamps() {
        let mut app = App::new();
        app.tree = vec![TreeNode {
            path: "0".into(),
            depth: 0,
            node_type: 62,
            subtype: 0,
            guid: None,
            name: "X".into(),
            action: 50,
            expanded: false,
            has_children: false,
        }];
        app.cursor_down();
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn mode_transitions() {
        let mut app = App::new();
        app.enter_command_mode();
        assert_eq!(app.mode, Mode::Command);
        app.exit_to_normal();
        assert_eq!(app.mode, Mode::Normal);
    }
}
