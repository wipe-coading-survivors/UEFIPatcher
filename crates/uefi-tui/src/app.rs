use std::collections::HashSet;

use ratatui::widgets::ListState;
use uefi_proto::{
    ArtifactInfo, FormEdge, FormInfo, GateInfo, ImageInfo, QuestionInfo, QuestionSummary,
    StringInfo, VarStoreInfo,
};

use crate::input::AppEvent;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Image,
    Forms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormsFocus {
    #[default]
    List,
    Details,
}

impl FormsFocus {
    pub fn next(self) -> Self {
        match self {
            FormsFocus::List => FormsFocus::Details,
            FormsFocus::Details => FormsFocus::List,
        }
    }
    pub fn prev(self) -> Self {
        self.next()
    }
}

#[derive(Debug, Clone, Default)]
pub struct FormsData {
    pub forms: Vec<FormInfo>,
    pub edges: Vec<FormEdge>,
    pub flat_mode: bool,
    pub expanded: HashSet<String>,
    pub cursor: usize,
    pub focus: FormsFocus,
    pub questions: Vec<QuestionSummary>,
    pub questions_key: Option<crate::forms::FormKey>,
    pub gates: Vec<GateInfo>,
    pub question_cursor: usize,
    pub question_info: Option<QuestionInfo>,
    pub question_info_key: Option<(crate::forms::FormKey, u32)>,
    pub questions_viewport: usize,
    pub list_viewport: usize,
    pub show_strings: bool,
    pub strings: Vec<StringInfo>,
    pub strings_filter: String,
    pub strings_cursor: usize,
    pub strings_viewport: usize,
    pub show_varstores: bool,
    pub varstores: Vec<VarStoreInfo>,
    pub varstores_target: Option<String>,
    pub varstores_cursor: usize,
    pub varstores_viewport: usize,
    pub questions_state: ratatui::widgets::ListState,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub path: String,
    pub depth: usize,
    pub node_type: u8,
    pub subtype: u8,
    pub guid: Option<String>,
    pub name: String,
    pub region: String,
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

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub display: String,
    pub apply: String,
}

pub const MENU_ROWS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct MenuState {
    pub open: bool,
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub offset: usize,
}

impl MenuState {
    pub fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.selected = 0;
        self.offset = 0;
    }

    pub fn open_with(&mut self, items: Vec<MenuItem>) {
        self.items = items;
        self.open = true;
        self.selected = 0;
        self.offset = 0;
    }

    pub fn refresh(&mut self, items: Vec<MenuItem>) {
        if items.is_empty() {
            self.close();
        } else {
            self.items = items;
            self.open = true;
            self.selected = 0;
            self.offset = 0;
        }
    }

    pub fn down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
        self.scroll();
    }

    pub fn up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.items.len() - 1
        } else {
            self.selected - 1
        };
        self.scroll();
    }

    fn scroll(&mut self) {
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + MENU_ROWS {
            self.offset = self.selected + 1 - MENU_ROWS;
        }
    }

    pub fn selected_apply(&self) -> Option<&str> {
        self.items.get(self.selected).map(|i| i.apply.as_str())
    }
}

pub struct App {
    pub mode: Mode,
    pub tree: Vec<TreeNode>,
    pub cursor: usize,
    pub focus: Focus,
    pub registry: RegistryData,
    pub view: View,
    pub forms: FormsData,
    pub active_image_id: Option<String>,
    pub selected: Option<String>,
    pub cmdline: crate::line::LineBuffer,
    pub menu: MenuState,
    pub history: crate::history::History,
    pub insert_cmd: &'static str,
    pub status_msg: String,
    pub image_loaded: bool,
    pub engine_online: bool,
    pub quit: bool,
    pub show_help: bool,
    pub help_scroll: u16,
    pub details_scroll: u16,
    pub details_anchor: Option<String>,
    pub tree_state: ListState,
    pub registry_state: ListState,
    pub forms_list_state: ListState,
    pub strings_list_state: ListState,
    pub varstores_list_state: ListState,
    pub tree_viewport_rows: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            tree: vec![],
            cursor: 0,
            focus: Focus::Tree,
            registry: RegistryData::default(),
            view: View::Image,
            forms: FormsData::default(),
            active_image_id: None,
            selected: None,
            cmdline: crate::line::LineBuffer::new(),
            menu: MenuState::default(),
            history: crate::history::History::load(),
            insert_cmd: "",
            status_msg: "Welcome. Press : for commands, ? for help".into(),
            image_loaded: false,
            engine_online: true,
            quit: false,
            show_help: false,
            help_scroll: 0,
            details_scroll: 0,
            details_anchor: None,
            tree_state: ListState::default(),
            registry_state: ListState::default(),
            forms_list_state: ListState::default(),
            strings_list_state: ListState::default(),
            varstores_list_state: ListState::default(),
            tree_viewport_rows: 0,
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

    fn page_size(&self) -> usize {
        if self.tree_viewport_rows == 0 {
            10
        } else {
            self.tree_viewport_rows
        }
    }

    pub fn cursor_page_down(&mut self) {
        let n = self.visible().len();
        if n == 0 {
            return;
        }
        self.cursor = crate::ui::scroll::page_down(self.cursor, n, self.page_size());
    }

    pub fn cursor_page_up(&mut self) {
        self.cursor = crate::ui::scroll::page_up(self.cursor, self.page_size());
    }

    pub fn toggle_expand_selected(&mut self) {
        if let Some(idx) = self.selected_tree_idx()
            && let Some(node) = self.tree.get_mut(idx)
            && node.has_children
        {
            node.expanded = !node.expanded;
        }
    }

    /// Toggle help-оверлея; скролл сбрасывается при каждом переключении. Спека R6.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        self.help_scroll = 0;
    }

    /// Ручной скролл details-панели основного вида. Спека R8.
    pub fn details_scroll_by(&mut self, delta: i32) {
        if delta >= 0 {
            self.details_scroll = self.details_scroll.saturating_add(delta as u16);
        } else {
            self.details_scroll = self.details_scroll.saturating_sub((-delta) as u16);
        }
    }

    pub fn set_expand_selected(&mut self, expand: bool) {
        if let Some(idx) = self.selected_tree_idx()
            && let Some(node) = self.tree.get_mut(idx)
            && node.has_children
        {
            node.expanded = expand;
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

    pub fn goto_path(&mut self, target: &str) -> Result<(), String> {
        let norm = target.trim_start_matches('/');
        let idx = self
            .tree
            .iter()
            .position(|n| n.path == norm)
            .ok_or_else(|| format!("no node at path {target}"))?;
        let want = crate::tree::segments(norm);
        for node in &mut self.tree {
            let segs = crate::tree::segments(&node.path);
            if segs.len() < want.len()
                && segs.iter().zip(want.iter()).all(|(a, b)| a == b)
                && node.has_children
            {
                node.expanded = true;
            }
        }
        let vis = self.visible();
        self.cursor = vis
            .iter()
            .position(|&v| v == idx)
            .ok_or("node hidden after expand")?;
        Ok(())
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

    pub fn forms_rows(&self) -> Vec<crate::forms::FormsRow> {
        if self.forms.flat_mode {
            crate::forms::build_rows(&self.forms.forms, &self.forms.expanded)
        } else {
            crate::forms::build_tree_rows(
                &self.forms.forms,
                &self.forms.edges,
                &self.forms.expanded,
            )
        }
    }

    pub fn selected_form_key(&self) -> Option<crate::forms::FormKey> {
        crate::forms::selected_key(&self.forms_rows(), self.forms.cursor)
    }

    pub fn forms_cursor_down(&mut self) {
        let n = self.forms_rows().len();
        if n > 0 && self.forms.cursor + 1 < n {
            self.forms.cursor += 1;
        }
    }

    pub fn forms_cursor_up(&mut self) {
        if self.forms.cursor > 0 {
            self.forms.cursor -= 1;
        }
    }

    fn forms_page_size(&self) -> usize {
        if self.forms.list_viewport == 0 {
            10
        } else {
            self.forms.list_viewport
        }
    }

    pub fn forms_page_down(&mut self) {
        let n = self.forms_rows().len();
        if n == 0 {
            return;
        }
        self.forms.cursor =
            crate::ui::scroll::page_down(self.forms.cursor, n, self.forms_page_size());
    }

    pub fn forms_page_up(&mut self) {
        self.forms.cursor = crate::ui::scroll::page_up(self.forms.cursor, self.forms_page_size());
    }

    pub fn forms_question_cursor_down(&mut self) {
        let n = self.forms.questions.len();
        if n > 0 && self.forms.question_cursor + 1 < n {
            self.forms.question_cursor += 1;
        }
    }

    pub fn forms_question_cursor_up(&mut self) {
        if self.forms.question_cursor > 0 {
            self.forms.question_cursor -= 1;
        }
    }

    pub fn forms_question_page_size(&self) -> usize {
        if self.forms.questions_viewport == 0 {
            10
        } else {
            self.forms.questions_viewport
        }
    }

    pub fn forms_question_page_down(&mut self) {
        let n = self.forms.questions.len();
        if n == 0 {
            return;
        }
        self.forms.question_cursor = crate::ui::scroll::page_down(
            self.forms.question_cursor,
            n,
            self.forms_question_page_size(),
        );
    }

    pub fn forms_question_page_up(&mut self) {
        self.forms.question_cursor =
            crate::ui::scroll::page_up(self.forms.question_cursor, self.forms_question_page_size());
    }

    /// qid вопроса под `question_cursor` — только когда кэш вопросов
    /// (`questions_key`) принадлежит выделенной строке-форме (гейт как в
    /// `form_panel`); иначе None — кросс-форменный prefill исключён.
    pub fn selected_question_id(&self) -> Option<u32> {
        let key = self.selected_form_key()?;
        if self.forms.questions_key.as_ref() != Some(&key) {
            return None;
        }
        self.forms
            .questions
            .get(self.forms.question_cursor)
            .map(|q| q.question_id)
    }

    pub fn selected_form_visible(&self) -> Option<bool> {
        match self.forms_rows().get(self.forms.cursor) {
            Some(crate::forms::FormsRow::Form { visible, .. }) => Some(*visible),
            _ => None,
        }
    }

    /// h/l-семантика как в Image-view: на FormSet-строке — сам формсет,
    /// на Form-строке — её родительский формсет (в REF-дереве при
    /// наличии детей — сама форма). После сворачивания курсор clamps
    /// к видимым строкам.
    pub fn forms_set_expanded(&mut self, expand: bool) {
        let rows = self.forms_rows();
        let (exp_key, sel) = match rows.get(self.forms.cursor) {
            Some(crate::forms::FormsRow::FormSet { guid, .. }) => {
                (Some(guid.clone()), Some((guid.clone(), None)))
            }
            Some(crate::forms::FormsRow::Form {
                key, has_children, ..
            }) => {
                let exp = if !self.forms.flat_mode && *has_children {
                    Some(format!("{}#{}", key.formset_guid, key.form_id_ifr))
                } else if !self.forms.flat_mode {
                    self.forms
                        .edges
                        .iter()
                        .find(|e| {
                            e.formset_guid == key.formset_guid && e.form_id == key.form_id_ifr
                        })
                        .map(|e| format!("{}#{}", key.formset_guid, e.parent_form_id))
                        .or_else(|| Some(key.formset_guid.clone()))
                } else {
                    Some(key.formset_guid.clone())
                };
                (exp, Some((key.formset_guid.clone(), Some(key.form_id_ifr))))
            }
            _ => (None, None),
        };
        if let Some(key) = &exp_key {
            if expand {
                self.forms.expanded.insert(key.clone());
            } else {
                self.forms.expanded.remove(key);
            }
        }
        let rows = self.forms_rows();
        let target = sel
            .and_then(|(guid, id)| {
                rows.iter().position(|r| match (id, r) {
                    (Some(fid), crate::forms::FormsRow::Form { key, .. }) => {
                        key.formset_guid == guid && key.form_id_ifr == fid
                    }
                    (None, crate::forms::FormsRow::FormSet { guid: g, .. }) => *g == guid,
                    _ => false,
                })
            })
            .or_else(|| {
                let flat = self.forms.flat_mode;
                exp_key.as_ref().and_then(|k| {
                    rows.iter().position(|r| match r {
                        crate::forms::FormsRow::FormSet { guid, .. } => guid == k,
                        crate::forms::FormsRow::Form { key, .. } => {
                            !flat && k == &format!("{}#{}", key.formset_guid, key.form_id_ifr)
                        }
                        _ => false,
                    })
                })
            });
        if let Some(i) = target {
            self.forms.cursor = i;
        }
        let n = rows.len();
        if self.forms.cursor >= n {
            self.forms.cursor = n.saturating_sub(1);
        }
    }

    pub fn forms_sanitize_cursor(&mut self) {
        let n = self.forms_rows().len();
        if n == 0 {
            self.forms.cursor = 0;
        } else if self.forms.cursor >= n {
            self.forms.cursor = n - 1;
        }
    }

    pub fn strings_visible(&self) -> Vec<usize> {
        let needle = self.forms.strings_filter.to_lowercase();
        self.forms
            .strings
            .iter()
            .enumerate()
            .filter(|(_, s)| needle.is_empty() || s.text.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// `strings_cursor` — индекс в `forms.strings` (НЕ позиция в видимом
    /// списке): движение — к следующему/предыдущему видимому индексу.
    pub fn strings_cursor_down(&mut self) {
        if let Some(&next) = self
            .strings_visible()
            .iter()
            .find(|&&i| i > self.forms.strings_cursor)
        {
            self.forms.strings_cursor = next;
        }
    }

    pub fn strings_cursor_up(&mut self) {
        if let Some(&prev) = self
            .strings_visible()
            .iter()
            .rev()
            .find(|&&i| i < self.forms.strings_cursor)
        {
            self.forms.strings_cursor = prev;
        }
    }

    fn strings_page_size(&self) -> usize {
        if self.forms.strings_viewport == 0 {
            10
        } else {
            self.forms.strings_viewport
        }
    }

    pub fn strings_page_down(&mut self) {
        let vis = self.strings_visible();
        let Some(pos) = vis.iter().position(|&i| i == self.forms.strings_cursor) else {
            return;
        };
        let target = crate::ui::scroll::page_down(pos, vis.len(), self.strings_page_size());
        self.forms.strings_cursor = vis[target];
    }

    pub fn strings_page_up(&mut self) {
        let vis = self.strings_visible();
        let Some(pos) = vis.iter().position(|&i| i == self.forms.strings_cursor) else {
            return;
        };
        let target = crate::ui::scroll::page_up(pos, self.strings_page_size());
        self.forms.strings_cursor = vis[target];
    }

    /// Курсор varstores-панели: без фильтра — список мал. Спека
    /// varstore-contract §5.
    pub fn varstores_cursor_down(&mut self) {
        if self.forms.varstores.is_empty() {
            return;
        }
        self.forms.varstores_cursor =
            (self.forms.varstores_cursor + 1).min(self.forms.varstores.len() - 1);
    }

    pub fn varstores_cursor_up(&mut self) {
        self.forms.varstores_cursor = self.forms.varstores_cursor.saturating_sub(1);
    }

    fn varstores_page_size(&self) -> usize {
        if self.forms.varstores_viewport == 0 {
            10
        } else {
            self.forms.varstores_viewport
        }
    }

    pub fn varstores_page_down(&mut self) {
        let n = self.forms.varstores.len();
        if n == 0 {
            return;
        }
        self.forms.varstores_cursor = crate::ui::scroll::page_down(
            self.forms.varstores_cursor,
            n,
            self.varstores_page_size(),
        );
    }

    pub fn varstores_page_up(&mut self) {
        self.forms.varstores_cursor =
            crate::ui::scroll::page_up(self.forms.varstores_cursor, self.varstores_page_size());
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.cmdline.clear();
        self.insert_cmd = "";
        self.menu.close();
        self.history.reset();
    }

    pub fn enter_insert_mode(&mut self, cmd: &'static str, prefill: String) {
        self.mode = Mode::Insert;
        self.insert_cmd = cmd;
        self.cmdline.set_str(&prefill);
        self.menu.close();
        self.history.reset();
    }

    pub fn exit_to_normal(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
        self.insert_cmd = "";
        self.menu.close();
    }

    pub fn node_label(&self, node: &TreeNode) -> String {
        if !node.name.is_empty() {
            return node.name.clone();
        }
        match node.node_type {
            crate::theme::TYPE_IMAGE => match &self.active_image_id {
                Some(id) => self
                    .registry
                    .images
                    .iter()
                    .find(|i| &i.image_id == id)
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| id.clone()),
                None => "Image".into(),
            },
            crate::theme::TYPE_VOLUME => "Volume".into(),
            crate::theme::TYPE_PADDING => "Padding".into(),
            crate::theme::TYPE_FREESPACE => "Free space".into(),
            66 => uefi_common::names::file_type_name_or_raw(node.subtype),
            67 => uefi_common::names::section_type_name_or_raw(node.subtype),
            _ => format!("0x{:02X}", node.subtype),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdFlow {
    Execute,
    Exit,
    None,
}

impl App {
    pub fn cmd_key(&mut self, ev: &AppEvent) -> CmdFlow {
        use crate::input::AppEvent as E;
        let mut mutated = false;
        let flow = match ev {
            E::Key('j') if self.menu.open => {
                self.menu.down();
                CmdFlow::None
            }
            E::Key('k') if self.menu.open => {
                self.menu.up();
                CmdFlow::None
            }
            E::Key(c) => {
                self.cmdline.insert(*c);
                mutated = true;
                CmdFlow::None
            }
            E::Ctrl(c) => match c {
                'a' => {
                    self.cmdline.home();
                    CmdFlow::None
                }
                'e' => {
                    self.cmdline.end();
                    CmdFlow::None
                }
                'w' => {
                    self.cmdline.kill_word();
                    mutated = true;
                    CmdFlow::None
                }
                'u' => {
                    self.cmdline.kill_to_start();
                    mutated = true;
                    CmdFlow::None
                }
                'k' => {
                    self.cmdline.kill_to_end();
                    mutated = true;
                    CmdFlow::None
                }
                _ => CmdFlow::None,
            },
            E::Backspace => {
                self.cmdline.backspace();
                mutated = true;
                CmdFlow::None
            }
            E::Delete => {
                self.cmdline.delete();
                mutated = true;
                CmdFlow::None
            }
            E::Left => {
                self.cmdline.left();
                CmdFlow::None
            }
            E::Right => {
                if self.menu.open {
                    self.accept_menu_selection();
                } else {
                    self.cmdline.right();
                }
                CmdFlow::None
            }
            E::WordLeft => {
                self.cmdline.word_left();
                CmdFlow::None
            }
            E::WordRight => {
                self.cmdline.word_right();
                CmdFlow::None
            }
            E::Home => {
                self.cmdline.home();
                CmdFlow::None
            }
            E::End => {
                self.cmdline.end();
                CmdFlow::None
            }
            E::Up => {
                if self.menu.open {
                    self.menu.up();
                } else if let Some(s) = self.history.prev(self.cmdline.as_str()) {
                    self.cmdline.set_str(&s);
                }
                CmdFlow::None
            }
            E::Down => {
                if self.menu.open {
                    self.menu.down();
                } else if let Some(s) = self.history.next(self.cmdline.as_str()) {
                    self.cmdline.set_str(&s);
                }
                CmdFlow::None
            }
            E::Tab => {
                if self.menu.open {
                    self.accept_menu_selection();
                } else {
                    let comp = crate::commands::complete(self, self.cmdline.as_str());
                    if let Some(c) = comp.common {
                        self.cmdline.set_str(&c);
                    }
                    if comp.items.is_empty() {
                        self.menu.close();
                    } else {
                        self.menu.open_with(comp.items);
                    }
                    self.history.reset();
                }
                CmdFlow::None
            }
            E::BackTab => {
                if self.menu.open {
                    self.menu.up();
                }
                CmdFlow::None
            }
            E::Enter => {
                if self.menu.open {
                    self.accept_menu_selection();
                    CmdFlow::None
                } else {
                    CmdFlow::Execute
                }
            }
            E::Esc => {
                if self.menu.open {
                    self.menu.close();
                    CmdFlow::None
                } else {
                    CmdFlow::Exit
                }
            }
            _ => CmdFlow::None,
        };
        if mutated {
            self.history.reset();
            if self.menu.open {
                let comp = crate::commands::complete(self, self.cmdline.as_str());
                self.menu.refresh(comp.items);
            }
        }
        flow
    }

    fn accept_menu_selection(&mut self) {
        if let Some(apply) = self.menu.selected_apply().map(str::to_string) {
            self.cmdline.set_str(&apply);
            self.history.reset();
            let comp = crate::commands::complete(self, self.cmdline.as_str());
            self.menu.refresh(comp.items);
        }
    }
}

/// Полный ID выбранной registry-строки (image/artifact) для hint-бара.
/// Спека R1: short-ID в списке остаются, полный UUID живёт в hint.
pub fn registry_selected_full_id(app: &App) -> Option<String> {
    match app.current_registry_row()? {
        RegistryRow::Image(i) => app.registry.images.get(i).map(|im| im.image_id.clone()),
        RegistryRow::Artifact(i) => app.registry.artifacts.get(i).map(|a| a.artifact_id.clone()),
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
    let region_part = if node.region.is_empty() {
        String::new()
    } else {
        format!("\nRegion:   {} (read-only)", node.region)
    };
    format!(
        "Path:     {}\nType:     {} ({} / 0x{:02X})\nSubtype:  {}\nGUID:     {}\nName:     {}\nAction:   {}\nChildren: {}{}",
        node.path,
        type_name,
        node.node_type,
        node.node_type,
        sub_part,
        node.guid.as_deref().unwrap_or("(none)"),
        node.name,
        node.action,
        node.has_children,
        region_part,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands;
    use crate::input::AppEvent;
    use crate::theme::ACTION_NO;

    fn node(path: &str, depth: usize) -> TreeNode {
        TreeNode {
            path: path.into(),
            depth,
            node_type: 62,
            subtype: 0,
            guid: None,
            name: String::new(),
            region: String::new(),
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
    fn page_down_advances_by_viewport_and_clamps() {
        let mut app = App::new();
        app.tree = (0..30).map(|i| node(&format!("n{i}"), 0)).collect();
        app.tree_viewport_rows = 10;
        app.cursor = 0;
        app.cursor_page_down();
        assert_eq!(app.cursor, 10);
        app.cursor_page_down();
        assert_eq!(app.cursor, 20);
        app.cursor_page_down();
        assert_eq!(app.cursor, 29, "clamp at last visible row");
    }

    #[test]
    fn page_up_decreases_by_viewport_and_clamps() {
        let mut app = App::new();
        app.tree = (0..30).map(|i| node(&format!("n{i}"), 0)).collect();
        app.tree_viewport_rows = 10;
        app.cursor = 25;
        app.cursor_page_up();
        assert_eq!(app.cursor, 15);
        app.cursor_page_up();
        app.cursor_page_up();
        assert_eq!(app.cursor, 0, "clamp at 0");
    }

    #[test]
    fn page_uses_default_when_viewport_unknown() {
        let mut app = App::new();
        app.tree = (0..30).map(|i| node(&format!("n{i}"), 0)).collect();
        app.cursor = 0;
        app.cursor_page_down();
        assert_eq!(app.cursor, 10, "fallback page size 10");
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
            path: "1/0".into(),
            depth: 2,
            node_type: 66,
            subtype: 0x07,
            guid: Some("ABC".into()),
            name: "Setup".into(),
            region: String::new(),
            action: ACTION_NO,
            expanded: false,
            has_children: true,
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
            path: "1".into(),
            depth: 1,
            node_type: 65,
            subtype: 0,
            guid: None,
            name: "DXE".into(),
            action: ACTION_NO,
            expanded: true,
            has_children: true,
            region: String::new(),
        };
        let t = details_text(&v);
        assert!(t.contains("Type:     Volume (65 / 0x41)"));
        assert!(t.contains("Subtype:  0x00"));
        assert!(t.contains("GUID:     (none)"));
    }

    #[test]
    fn details_text_region_read_only_line() {
        let r = TreeNode {
            path: "0".into(),
            depth: 1,
            node_type: crate::theme::TYPE_REGION,
            subtype: 0,
            guid: None,
            name: "ME region".into(),
            action: ACTION_NO,
            expanded: false,
            has_children: true,
            region: "ME".into(),
        };
        let t = details_text(&r);
        assert!(t.contains("Region:   ME (read-only)"));
    }

    #[test]
    fn node_label_image_from_registry_name() {
        let mut app = App::new();
        app.active_image_id = Some("img-9".into());
        app.registry.images = vec![ImageInfo {
            image_id: "img-9".into(),
            name: "HNX99TF.bin".into(),
            ..Default::default()
        }];
        assert_eq!(app.node_label(&node("", 0)), "HNX99TF.bin");
    }

    #[test]
    fn node_label_image_fallback_when_no_registry_match() {
        let mut app = App::new();
        app.active_image_id = Some("img-9".into());
        assert_eq!(app.node_label(&node("", 0)), "img-9");
        app.active_image_id = None;
        assert_eq!(app.node_label(&node("", 0)), "Image");
    }

    #[test]
    fn goto_path_expands_ancestors_and_moves_cursor() {
        let mut app = App::new();
        app.tree = vec![node("", 0), node("0", 1), node("0/0", 2), node("0/0/0", 3)];
        app.cursor = 0;
        app.goto_path("0/0/0").unwrap();
        assert!(app.tree[1].expanded);
        assert!(app.tree[2].expanded);
        assert_eq!(app.selected_path().as_deref(), Some("0/0/0"));
    }

    #[test]
    fn goto_path_accepts_leading_slash_and_errors_on_miss() {
        let mut app = App::new();
        app.tree = vec![node("", 0), node("0", 1)];
        app.goto_path("/0").unwrap();
        assert_eq!(app.selected_path().as_deref(), Some("0"));
        assert!(app.goto_path("9/9").is_err());
    }

    #[test]
    fn node_label_volume_and_subtype_fallback() {
        let app = App::new();
        let mut vol = node("0", 1);
        vol.node_type = 65;
        assert_eq!(app.node_label(&vol), "Volume");
        let mut file = node("1/0", 2);
        file.node_type = 66;
        file.subtype = 0x07;
        assert_eq!(app.node_label(&file), "DXE driver");
        let mut sec = node("1/0/0", 3);
        sec.node_type = 67;
        sec.subtype = 0x77;
        assert_eq!(app.node_label(&sec), "Unknown 77h");
        let mut unk = node("3", 1);
        unk.node_type = 99;
        unk.subtype = 0x42;
        assert_eq!(app.node_label(&unk), "0x42");
        let mut pad = node("4", 1);
        pad.node_type = crate::theme::TYPE_PADDING;
        assert_eq!(app.node_label(&pad), "Padding");
        let mut free = node("5", 1);
        free.node_type = crate::theme::TYPE_FREESPACE;
        assert_eq!(app.node_label(&free), "Free space");
    }

    fn form_info(set: &str, id: u32) -> uefi_proto::FormInfo {
        uefi_proto::FormInfo {
            form_id: "t:0x19:0".into(),
            formset_guid: set.into(),
            form_id_ifr: id,
            title: format!("f{id}"),
            visible: true,
        }
    }

    #[test]
    fn forms_cursor_and_collapse_parent() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1), form_info("S", 2)];
        app.forms.expanded = ["S".into()].into();
        assert_eq!(app.forms_rows().len(), 3);
        app.forms_cursor_down();
        assert_eq!(app.selected_form_key().unwrap().form_id_ifr, 1);
        app.forms.cursor = 2;
        app.forms_set_expanded(false);
        assert_eq!(app.forms_rows().len(), 1, "formset collapsed");
        assert_eq!(app.forms.cursor, 0, "cursor clamped onto formset row");
        app.forms_set_expanded(true);
        assert_eq!(app.forms_rows().len(), 3);
    }

    #[test]
    fn forms_collapse_from_form_lands_cursor_on_formset_row() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1), form_info("T", 9)];
        app.forms.expanded = ["S".into(), "T".into()].into();
        app.forms.cursor = 1;
        app.forms_set_expanded(false);
        assert_eq!(
            app.forms.cursor, 0,
            "свёрнут формсет выделенного листа — курсор на его строке, не на чужом формсете"
        );
        assert!(matches!(
            &app.forms_rows()[app.forms.cursor],
            crate::forms::FormsRow::FormSet { guid, .. } if guid == "S"
        ));
    }

    #[test]
    fn forms_collapse_parent_form_lands_cursor_on_parent_row() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1), form_info("S", 2)];
        app.forms.edges = vec![uefi_proto::FormEdge {
            formset_guid: "S".into(),
            parent_form_id: 1,
            form_id: 2,
            target_formset_guid: String::new(),
        }];
        app.forms.expanded = ["S".into(), "S#1".into()].into();
        app.forms.cursor = 2;
        app.forms_set_expanded(false);
        assert_eq!(app.forms_rows().len(), 2, "родительская форма свёрнута");
        assert_eq!(app.forms.cursor, 1, "курсор на строке свернутого родителя");
        assert!(matches!(
            &app.forms_rows()[app.forms.cursor],
            crate::forms::FormsRow::Form { key, .. } if key.form_id_ifr == 1
        ));
    }

    #[test]
    fn forms_expand_keeps_cursor_on_selected_row() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1)];
        app.forms.cursor = 0;
        app.forms_set_expanded(true);
        assert_eq!(app.forms_rows().len(), 2);
        assert_eq!(
            app.forms.cursor, 0,
            "разворачивание не уводит курсор с формсета"
        );
    }

    #[test]
    fn strings_visible_filters_case_insensitive() {
        let mut app = App::new();
        app.forms.strings = vec![
            uefi_proto::StringInfo {
                language: "en-US".into(),
                string_id: 1,
                text: "Setup".into(),
            },
            uefi_proto::StringInfo {
                language: "en-US".into(),
                string_id: 2,
                text: "serial port".into(),
            },
        ];
        assert_eq!(app.strings_visible().len(), 2);
        app.forms.strings_filter = "SERIAL".into();
        assert_eq!(app.strings_visible(), vec![1]);
        app.strings_cursor_down();
        assert_eq!(app.forms.strings_cursor, 1);
        app.strings_cursor_down();
        assert_eq!(app.forms.strings_cursor, 1, "clamp at last visible");
    }

    #[test]
    fn varstores_panel_toggles_and_clamps_cursor() {
        let mut app = App::default();
        app.forms.varstores = vec![
            VarStoreInfo {
                id: 1,
                guid: "A".into(),
                size: 4,
                name: "One".into(),
            },
            VarStoreInfo {
                id: 2,
                guid: "B".into(),
                size: 8,
                name: "Two".into(),
            },
        ];
        assert!(!app.forms.show_varstores);
        app.forms.show_varstores = true;
        app.varstores_cursor_down();
        assert_eq!(app.forms.varstores_cursor, 1);
        app.varstores_cursor_down();
        assert_eq!(app.forms.varstores_cursor, 1, "clamp at last");
        app.varstores_cursor_up();
        assert_eq!(app.forms.varstores_cursor, 0);
    }

    #[test]
    fn question_cursor_clamps_and_selected_question() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1)];
        app.forms.expanded = ["S".into()].into();
        app.forms.cursor = 1;
        app.forms.questions_key = Some(crate::forms::FormKey {
            target: "t:0x19:0".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "f1".into(),
        });
        app.forms.questions = vec![
            uefi_proto::QuestionSummary {
                question_id: 0x210,
                ..Default::default()
            },
            uefi_proto::QuestionSummary {
                question_id: 0x211,
                ..Default::default()
            },
        ];
        assert_eq!(app.selected_question_id(), Some(0x210));
        app.forms_question_cursor_down();
        assert_eq!(app.selected_question_id(), Some(0x211));
        app.forms_question_cursor_down();
        assert_eq!(
            app.selected_question_id(),
            Some(0x211),
            "clamp на последнем"
        );
        app.forms_question_cursor_up();
        assert_eq!(app.selected_question_id(), Some(0x210));
    }

    #[test]
    fn forms_question_page_moves_clamp() {
        let mut app = App::new();
        app.forms.questions = (0..30)
            .map(|i| uefi_proto::QuestionSummary {
                question_id: 0x210 + i,
                prompt: format!("q{i}"),
                ..Default::default()
            })
            .collect();
        app.forms.questions_viewport = 10;
        app.forms_question_page_down();
        assert_eq!(app.forms.question_cursor, 10);
        app.forms_question_page_down();
        assert_eq!(app.forms.question_cursor, 20);
        app.forms_question_page_down();
        assert_eq!(app.forms.question_cursor, 29, "clamp по последнему вопросу");
        app.forms_question_page_up();
        assert_eq!(app.forms.question_cursor, 19);
        app.forms.question_cursor = 2;
        app.forms_question_page_up();
        assert_eq!(app.forms.question_cursor, 0, "saturating");
    }

    #[test]
    fn forms_question_page_size_defaults_to_10() {
        let app = App::new();
        assert_eq!(app.forms_question_page_size(), 10);
    }

    #[test]
    fn forms_page_down_advances_by_viewport_and_clamps() {
        let mut app = App::new();
        app.forms.forms = (0..30).map(|i| form_info("S", i)).collect();
        app.forms.expanded = ["S".into()].into();
        app.forms.list_viewport = 10;
        app.forms_page_down();
        assert_eq!(app.forms.cursor, 10);
        app.forms_page_down();
        assert_eq!(app.forms.cursor, 20);
        app.forms_page_down();
        assert_eq!(
            app.forms.cursor, 30,
            "clamp по последней строке (formset + 30 форм)"
        );
        app.forms_page_up();
        assert_eq!(app.forms.cursor, 20);
        app.forms.cursor = 3;
        app.forms_page_up();
        assert_eq!(app.forms.cursor, 0, "saturating");
    }

    #[test]
    fn forms_page_uses_default_when_viewport_unknown() {
        let mut app = App::new();
        app.forms.forms = (0..30).map(|i| form_info("S", i)).collect();
        app.forms.expanded = ["S".into()].into();
        app.forms_page_down();
        assert_eq!(app.forms.cursor, 10, "fallback page size 10");
    }

    #[test]
    fn selected_form_target_resolves_formset_rows() {
        let mk = |set: &str, target: &str, id: u32| uefi_proto::FormInfo {
            form_id: target.into(),
            formset_guid: set.into(),
            form_id_ifr: id,
            title: format!("f{id}"),
            visible: true,
        };
        let mut app = App::new();
        app.forms.forms = vec![mk("SETUP", "t-setup:0x19:0", 1), mk("RC", "t-rc:0x19:0", 9)];
        app.forms.expanded = ["SETUP".into(), "RC".into()].into();
        // rows: [FormSet SETUP, Form 1, FormSet RC, Form 9]
        assert_eq!(
            commands::selected_form_target(&app).as_deref(),
            Some("t-setup:0x19:0"),
            "FormSet-строка резолвится таргетом своего формсета"
        );
        app.forms.cursor = 1;
        assert_eq!(
            commands::selected_form_target(&app).as_deref(),
            Some("t-setup:0x19:0"),
            "Form-строка — таргетом формы"
        );
        app.forms.cursor = 2;
        assert_eq!(
            commands::selected_form_target(&app).as_deref(),
            Some("t-rc:0x19:0"),
            "смена FormSet-строки видна V-кэшу (не стылый формсет)"
        );
    }

    #[test]
    fn strings_page_moves_across_visible_only() {
        let mut app = App::new();
        app.forms.strings = (0..40)
            .map(|i| uefi_proto::StringInfo {
                string_id: i,
                language: "en".into(),
                text: if i % 2 == 0 {
                    format!("s{i}")
                } else {
                    format!("x{i}")
                },
            })
            .collect();
        app.forms.strings_filter = "s".into();
        app.forms.strings_viewport = 10;
        app.forms.strings_cursor = 2;
        app.strings_page_down();
        assert_eq!(app.forms.strings_cursor, 22, "+10 видимых (все чётные)");
        app.strings_page_down();
        app.strings_page_down();
        assert_eq!(app.forms.strings_cursor, 38, "clamp по последнему видимому");
        app.strings_page_up();
        assert_eq!(app.forms.strings_cursor, 18);
    }

    #[test]
    fn varstores_page_clamps_and_saturates() {
        let mut app = App::new();
        app.forms.varstores = (0..25)
            .map(|i| uefi_proto::VarStoreInfo {
                id: i,
                ..Default::default()
            })
            .collect();
        app.forms.varstores_viewport = 10;
        app.varstores_page_down();
        assert_eq!(app.forms.varstores_cursor, 10);
        app.varstores_page_down();
        assert_eq!(app.forms.varstores_cursor, 20);
        app.varstores_page_down();
        assert_eq!(app.forms.varstores_cursor, 24, "clamp по последнему");
        app.varstores_page_up();
        assert_eq!(app.forms.varstores_cursor, 14);
        app.forms.varstores_cursor = 3;
        app.varstores_page_up();
        assert_eq!(app.forms.varstores_cursor, 0, "saturating");
    }

    #[test]
    fn selected_question_id_none_when_questions_from_other_form() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1), form_info("S", 2)];
        app.forms.expanded = ["S".into()].into();
        app.forms.questions_key = Some(crate::forms::FormKey {
            target: "t:0x19:0".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "f1".into(),
        });
        app.forms.questions = vec![uefi_proto::QuestionSummary {
            question_id: 0x210,
            ..Default::default()
        }];
        app.forms.cursor = 1;
        assert_eq!(app.selected_question_id(), Some(0x210));
        app.forms.cursor = 2;
        assert_eq!(
            app.selected_question_id(),
            None,
            "кэш вопросов формы 1 не отвечает за курсор на форме 2"
        );
        app.forms.cursor = 0;
        assert_eq!(
            app.selected_question_id(),
            None,
            "FormSet-строка не форма — вопросов нет"
        );
    }

    #[test]
    fn selected_form_visible_reads_row() {
        let mut app = App::new();
        app.forms.forms = vec![form_info("S", 1), form_info("S", 2)];
        app.forms.expanded = ["S".into()].into();
        app.forms.cursor = 2;
        assert_eq!(app.selected_form_visible(), Some(true));
        app.forms.forms[1].visible = false;
        assert_eq!(app.selected_form_visible(), Some(false));
        app.forms.cursor = 0;
        assert_eq!(app.selected_form_visible(), None, "FormSet-строка не форма");
    }

    #[test]
    fn menu_navigation_wraps_and_scrolls() {
        let mut m = MenuState::default();
        let items: Vec<MenuItem> = (0..10)
            .map(|i| MenuItem {
                display: format!("i{i}"),
                apply: format!("a{i}"),
            })
            .collect();
        m.open_with(items);
        assert_eq!(m.selected, 0);
        m.up();
        assert_eq!(m.selected, 9);
        assert_eq!(m.offset, 2);
        m.down();
        assert_eq!(m.selected, 0);
        assert_eq!(m.offset, 0);
        for _ in 0..5 {
            m.down();
        }
        assert_eq!(m.selected, 5);
        assert_eq!(m.offset, 0);
        m.down();
        m.down();
        m.down();
        assert_eq!(m.selected, 8);
        assert_eq!(m.offset, 1);
    }

    #[test]
    fn menu_refresh_resets_and_closes_on_empty() {
        let mut m = MenuState::default();
        m.open_with(vec![MenuItem {
            display: "a".into(),
            apply: "x a".into(),
        }]);
        m.down();
        m.refresh(vec![]);
        assert!(!m.open);
        let items = vec![
            MenuItem {
                display: "b".into(),
                apply: "x b".into(),
            },
            MenuItem {
                display: "c".into(),
                apply: "x c".into(),
            },
        ];
        m.refresh(items);
        assert!(m.open);
        assert_eq!(m.selected, 0);
        assert_eq!(m.selected_apply(), Some("x b"));
    }

    #[test]
    fn cmd_key_editing_and_history() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        for c in "save x".chars() {
            app.cmd_key(&AppEvent::Key(c));
        }
        assert_eq!(app.cmdline.as_str(), "save x");
        app.cmd_key(&AppEvent::Left);
        app.cmd_key(&AppEvent::WordLeft);
        app.cmd_key(&AppEvent::End);
        app.cmd_key(&AppEvent::Ctrl('w'));
        assert_eq!(app.cmdline.as_str(), "save ");
        assert_eq!(app.cmd_key(&AppEvent::Enter), CmdFlow::Execute);
        app.history.submit("save x");

        app.cmdline.set_str("");
        app.cmd_key(&AppEvent::Up);
        assert_eq!(app.cmdline.as_str(), "save x");
        app.cmd_key(&AppEvent::Backspace);
        assert_eq!(app.cmdline.as_str(), "save ");
        app.cmd_key(&AppEvent::Up);
        assert_eq!(app.cmdline.as_str(), "save x");
    }

    #[test]
    fn cmd_key_repeated_up_walks_history() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        for cmd in ["ls", "open a", "open b"] {
            app.history.submit(cmd);
        }
        app.cmd_key(&AppEvent::Up);
        assert_eq!(app.cmdline.as_str(), "open b");
        app.cmd_key(&AppEvent::Up);
        assert_eq!(app.cmdline.as_str(), "open a");
        app.cmd_key(&AppEvent::Up);
        assert_eq!(app.cmdline.as_str(), "ls");
        app.cmd_key(&AppEvent::Down);
        assert_eq!(app.cmdline.as_str(), "open a");
    }

    #[test]
    fn cmd_key_tab_opens_menu_and_right_accepts() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        app.cmdline.set_str("snap");
        assert_eq!(app.cmd_key(&AppEvent::Tab), CmdFlow::None);
        assert!(app.menu.open);
        assert!(app.menu.selected_apply().unwrap().starts_with("snap"));
        app.cmd_key(&AppEvent::Down);
        let expected = app.menu.items[1].apply.clone();
        app.cmd_key(&AppEvent::Right);
        assert_eq!(app.cmdline.as_str(), expected);
        assert!(!app.menu.open);
    }

    #[test]
    fn cmd_key_esc_two_stage_and_enter_flow() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        app.cmdline.set_str("s");
        app.cmd_key(&AppEvent::Tab);
        assert!(app.menu.open);
        assert_eq!(app.cmd_key(&AppEvent::Esc), CmdFlow::None);
        assert!(!app.menu.open);
        assert_eq!(app.cmd_key(&AppEvent::Esc), CmdFlow::Exit);
    }

    #[test]
    fn cmd_key_enter_with_open_menu_accepts_selected() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        app.cmdline.set_str("q");
        app.cmd_key(&AppEvent::Tab);
        assert!(app.menu.open);
        assert_eq!(app.menu.selected_apply(), Some("quit "));
        assert_eq!(app.cmd_key(&AppEvent::Enter), CmdFlow::None);
        assert_eq!(app.cmdline.as_str(), "quit ");
        assert!(!app.menu.open);
        assert_eq!(app.cmd_key(&AppEvent::Enter), CmdFlow::Execute);
        assert_eq!(app.cmdline.as_str(), "quit ");
    }

    #[test]
    fn cmd_key_h_tab_enter_accepts_then_runs_after_esc() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        app.cmdline.set_str("h");
        app.cmd_key(&AppEvent::Tab);
        assert!(app.menu.open);
        assert_eq!(app.cmd_key(&AppEvent::Enter), CmdFlow::None);
        assert_eq!(app.cmdline.as_str(), "hii ");
        assert!(app.menu.open);
        app.cmd_key(&AppEvent::Esc);
        assert!(!app.menu.open);
        assert_eq!(app.cmd_key(&AppEvent::Enter), CmdFlow::Execute);
        assert_eq!(app.cmdline.as_str(), "hii ");
    }

    #[test]
    fn cmd_key_jk_navigate_menu_when_open() {
        let mut app = App::new();
        app.history = crate::history::History::empty();
        app.mode = Mode::Command;
        app.cmdline.set_str("snap");
        app.cmd_key(&AppEvent::Tab);
        assert!(app.menu.open);
        assert_eq!(app.cmdline.as_str(), "snapshot");
        assert_eq!(app.menu.selected, 0);
        assert_eq!(app.cmd_key(&AppEvent::Key('j')), CmdFlow::None);
        assert_eq!(app.menu.selected, 1);
        assert_eq!(app.cmdline.as_str(), "snapshot");
        assert_eq!(app.cmd_key(&AppEvent::Key('k')), CmdFlow::None);
        assert_eq!(app.menu.selected, 0);
        app.cmd_key(&AppEvent::Key('x'));
        assert_eq!(app.cmdline.as_str(), "snapshotx");
        assert!(!app.menu.open);
    }

    #[test]
    fn details_scroll_by_saturates() {
        let mut app = App::new();
        app.details_scroll_by(5);
        assert_eq!(app.details_scroll, 5);
        app.details_scroll_by(-10);
        assert_eq!(app.details_scroll, 0);
    }
}
