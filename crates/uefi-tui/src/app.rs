use uefi_proto::{ArtifactInfo, ImageInfo};

use crate::tree::visible_rows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Details,
    Registry,
}

impl Focus {
    pub fn next(self) -> Focus {
        match self {
            Focus::Tree => Focus::Details,
            Focus::Details => Focus::Registry,
            Focus::Registry => Focus::Tree,
        }
    }
    pub fn prev(self) -> Focus {
        match self {
            Focus::Tree => Focus::Registry,
            Focus::Details => Focus::Tree,
            Focus::Registry => Focus::Details,
        }
    }
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

#[derive(Debug, Clone, Default)]
pub struct RegistryData {
    pub images: Vec<ImageInfo>,
    pub artifacts: Vec<ArtifactInfo>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub enum RegistryRow {
    Image(usize),
    Artifact(usize),
}

pub struct App {
    pub mode: Mode,
    pub tree: Vec<TreeNode>,
    pub cursor: usize,
    pub focus: Focus,
    pub registry: RegistryData,
    pub active_image_id: Option<String>,
    pub selected: Option<String>,
    pub cmdline: String,
    pub insert_cmd: &'static str,
    pub status_msg: String,
    pub image_loaded: bool,
    pub quit: bool,
    pub show_help: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            tree: vec![],
            cursor: 0,
            focus: Focus::Tree,
            registry: RegistryData::default(),
            active_image_id: None,
            selected: None,
            cmdline: String::new(),
            insert_cmd: "",
            status_msg: "Welcome. Press : for commands, ? for help".into(),
            image_loaded: false,
            quit: false,
            show_help: false,
        }
    }

    pub fn visible(&self) -> Vec<usize> {
        visible_rows(&self.tree)
    }

    pub fn selected_tree_idx(&self) -> Option<usize> {
        self.visible().get(self.cursor).copied()
    }

    pub fn selected_path(&self) -> Option<String> {
        self.selected_tree_idx()
            .and_then(|i| self.tree.get(i))
            .map(|n| n.path.clone())
    }

    pub fn cursor_down(&mut self) {
        let n = self.visible().len();
        if n > 0 && self.cursor + 1 < n {
            self.cursor += 1;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn toggle_expand_selected(&mut self) {
        if let Some(idx) = self.selected_tree_idx()
            && let Some(node) = self.tree.get_mut(idx)
            && node.has_children
        {
            node.expanded = !node.expanded;
        }
    }

    pub fn sanitize_cursor(&mut self) {
        let n = self.visible().len();
        if n == 0 {
            self.cursor = 0;
        } else if self.cursor >= n {
            self.cursor = n - 1;
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    pub fn registry_selectable(&self) -> Vec<RegistryRow> {
        let mut rows = Vec::new();
        for i in 0..self.registry.images.len() {
            rows.push(RegistryRow::Image(i));
        }
        for i in 0..self.registry.artifacts.len() {
            rows.push(RegistryRow::Artifact(i));
        }
        rows
    }

    pub fn current_registry_row(&self) -> Option<RegistryRow> {
        self.registry_selectable()
            .get(self.registry.cursor)
            .cloned()
    }

    pub fn registry_cursor_down(&mut self) {
        let n = self.registry_selectable().len();
        if n > 0 && self.registry.cursor + 1 < n {
            self.registry.cursor += 1;
        }
    }

    pub fn registry_cursor_up(&mut self) {
        if self.registry.cursor > 0 {
            self.registry.cursor -= 1;
        }
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.cmdline.clear();
        self.insert_cmd = "";
    }

    pub fn enter_insert_mode(&mut self, cmd: &'static str, prefill: String) {
        self.mode = Mode::Insert;
        self.insert_cmd = cmd;
        self.cmdline = prefill;
    }

    pub fn exit_to_normal(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
        self.insert_cmd = "";
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

pub fn details_text(node: &TreeNode) -> String {
    let type_name = uefi_common::names::node_type_name(node.node_type as u32);
    let sub_name = match node.node_type {
        66 => uefi_common::names::file_type_name_or_raw(node.subtype),
        67 => uefi_common::names::section_type_name_or_raw(node.subtype),
        _ => String::new(),
    };
    let sub_part = if sub_name.is_empty() {
        format!("0x{:02X}", node.subtype)
    } else {
        format!("{} (0x{:02X})", sub_name, node.subtype)
    };
    format!(
        "Path:     {}\nType:     {} ({} / 0x{:02X})\nSubtype:  {}\nGUID:     {}\nName:     {}\nAction:   {}\nChildren: {}",
        node.path,
        type_name,
        node.node_type,
        node.node_type,
        sub_part,
        node.guid.as_deref().unwrap_or("(none)"),
        node.name,
        node.action,
        node.has_children,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ACTION_NO;

    fn node(path: &str, depth: usize) -> TreeNode {
        TreeNode {
            path: path.into(),
            depth,
            node_type: 62,
            subtype: 0,
            guid: None,
            name: String::new(),
            action: ACTION_NO,
            expanded: true,
            has_children: depth == 0,
        }
    }

    #[test]
    fn app_new_starts_normal_tree_focus() {
        let app = App::new();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Tree);
        assert!(app.active_image_id.is_none());
    }

    #[test]
    fn selected_path_indexes_visible() {
        let mut app = App::new();
        app.tree = vec![node("0", 0), node("0/0", 1)];
        assert_eq!(app.selected_path(), Some("0".into()));
        app.cursor_down();
        assert_eq!(app.selected_path(), Some("0/0".into()));
    }

    #[test]
    fn toggle_expand_selected_flips_only_parents() {
        let mut app = App::new();
        app.tree = vec![node("", 0), node("0", 1)];
        app.cursor = 0;
        app.toggle_expand_selected();
        assert!(!app.tree[0].expanded);
        app.toggle_expand_selected();
        assert!(app.tree[0].expanded);
    }

    #[test]
    fn sanitize_cursor_clamps_to_visible() {
        let mut app = App::new();
        app.tree = vec![node("", 0), node("0", 1)];
        app.cursor = 5;
        app.sanitize_cursor();
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn focus_ring_cycles() {
        let mut app = App::new();
        assert_eq!(app.focus, Focus::Tree);
        app.focus_next();
        assert_eq!(app.focus, Focus::Details);
        app.focus_next();
        assert_eq!(app.focus, Focus::Registry);
        app.focus_next();
        assert_eq!(app.focus, Focus::Tree);
        app.focus_prev();
        assert_eq!(app.focus, Focus::Registry);
    }

    #[test]
    fn registry_selectable_and_cursor() {
        let mut app = App::new();
        app.registry.images = vec![ImageInfo::default()];
        app.registry.artifacts = vec![ArtifactInfo::default(), ArtifactInfo::default()];
        let rows = app.registry_selectable();
        assert_eq!(rows.len(), 3);
        app.registry_cursor_down();
        assert_eq!(app.registry.cursor, 1);
        assert!(matches!(
            app.current_registry_row(),
            Some(RegistryRow::Artifact(0))
        ));
    }

    #[test]
    fn details_text_named_file_and_section() {
        let f = TreeNode {
            path: "1/0".into(), depth: 2, node_type: 66, subtype: 0x07,
            guid: Some("ABC".into()), name: "Setup".into(),
            action: ACTION_NO, expanded: false, has_children: true,
        };
        let t = details_text(&f);
        assert!(t.contains("Type:     File (66 / 0x42)"));
        assert!(t.contains("Subtype:  DXE driver (0x07)"));
        assert!(t.contains("GUID:     ABC"));
        assert!(t.contains("Children: true"));
    }

    #[test]
    fn details_text_volume_no_subtype_name() {
        let v = TreeNode {
            path: "1".into(), depth: 1, node_type: 65, subtype: 0,
            guid: None, name: "DXE".into(),
            action: ACTION_NO, expanded: true, has_children: true,
        };
        let t = details_text(&v);
        assert!(t.contains("Type:     Volume (65 / 0x41)"));
        assert!(t.contains("Subtype:  0x00"));
        assert!(t.contains("GUID:     (none)"));
    }
}
