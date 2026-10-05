use std::path::{Path, PathBuf};
use crossterm::style::Color;
use unicode_width::UnicodeWidthChar;
use zee_core::file_tree::{FileTree, FlatFileItem};
use zee_core::outline::{self, FlatOutlineItem, OutlineNode};
use zee_core::theme::{self, Theme};
use crate::renderer::{Cell, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Files,
    Outline,
}

pub enum SidebarAction {
    None,
    OpenFile(PathBuf),
    JumpToLine(usize),
}

pub struct Sidebar {
    pub visible: bool,
    pub active_tab: SidebarTab,
    #[allow(dead_code)]
    pub width: u16,

    // Plugin Manager
    pub plugin_manager: zee_core::plugin::PluginManager,

    // File Tree State
    pub file_tree: FileTree,
    pub selected_file_idx: usize,
    pub file_scroll_top: usize,

    // Outline State
    pub outline_nodes: Vec<OutlineNode>,
    pub selected_outline_idx: usize,
    pub outline_scroll_top: usize,
}

impl Sidebar {
    pub fn new(root_path: PathBuf, visible: bool) -> Self {
        let file_tree = FileTree::new(root_path, false);
        let mut plugin_manager = zee_core::plugin::PluginManager::new();
        let dev_plugin_dir = PathBuf::from("plugins/zee-plugin-text");
        if dev_plugin_dir.exists() {
            let _ = plugin_manager.load_plugin_dir(&dev_plugin_dir);
        }
        Self {
            visible,
            active_tab: SidebarTab::Files,
            width: 26,
            plugin_manager,
            file_tree,
            selected_file_idx: 0,
            file_scroll_top: 0,
            outline_nodes: Vec::new(),
            selected_outline_idx: 0,
            outline_scroll_top: 0,
        }
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    #[allow(dead_code)]
    pub fn set_root(&mut self, root_path: PathBuf) {
        self.file_tree = FileTree::new(root_path, self.file_tree.show_hidden);
        self.selected_file_idx = 0;
        self.file_scroll_top = 0;
    }

    #[allow(dead_code)]
    pub fn refresh_files(&mut self) {
        self.file_tree.refresh();
    }

    pub fn update_outline(&mut self, text: &str, lang_or_ext: &str) {
        let nodes = outline::extract_outline(Some(&mut self.plugin_manager), lang_or_ext, text);
        self.outline_nodes = nodes;
        let flat = self.flatten_outline();
        if !flat.is_empty() && self.selected_outline_idx >= flat.len() {
            self.selected_outline_idx = flat.len() - 1;
        }
    }

    pub fn flatten_files(&self) -> Vec<FlatFileItem> {
        self.file_tree.flatten()
    }

    pub fn flatten_outline(&self) -> Vec<FlatOutlineItem> {
        let mut flat = Vec::new();
        outline::flatten_outline(&self.outline_nodes, 0, &mut flat);
        flat
    }

    pub fn select_prev(&mut self, _viewport_height: usize) {
        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                if !items.is_empty() && self.selected_file_idx > 0 {
                    self.selected_file_idx -= 1;
                    if self.selected_file_idx < self.file_scroll_top {
                        self.file_scroll_top = self.selected_file_idx;
                    }
                }
            }
            SidebarTab::Outline => {
                let items = self.flatten_outline();
                if !items.is_empty() && self.selected_outline_idx > 0 {
                    self.selected_outline_idx -= 1;
                    if self.selected_outline_idx < self.outline_scroll_top {
                        self.outline_scroll_top = self.selected_outline_idx;
                    }
                }
            }
        }
    }

    pub fn select_next(&mut self, viewport_height: usize) {
        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                if !items.is_empty() && self.selected_file_idx + 1 < items.len() {
                    self.selected_file_idx += 1;
                    if viewport_height > 0 && self.selected_file_idx >= self.file_scroll_top + viewport_height {
                        self.file_scroll_top = self.selected_file_idx - viewport_height + 1;
                    }
                }
            }
            SidebarTab::Outline => {
                let items = self.flatten_outline();
                if !items.is_empty() && self.selected_outline_idx + 1 < items.len() {
                    self.selected_outline_idx += 1;
                    if viewport_height > 0 && self.selected_outline_idx >= self.outline_scroll_top + viewport_height {
                        self.outline_scroll_top = self.selected_outline_idx - viewport_height + 1;
                    }
                }
            }
        }
    }

    pub fn handle_left(&mut self) {
        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                if let Some(item) = items.get(self.selected_file_idx) {
                    if item.is_dir && item.is_expanded {
                        let path = item.path.clone();
                        self.file_tree.toggle_expand(&path);
                    } else if item.depth > 0 {
                        // Jump to parent directory
                        let cur_depth = item.depth;
                        for i in (0..self.selected_file_idx).rev() {
                            if items[i].depth < cur_depth {
                                self.selected_file_idx = i;
                                if self.selected_file_idx < self.file_scroll_top {
                                    self.file_scroll_top = self.selected_file_idx;
                                }
                                break;
                            }
                        }
                    }
                }
            }
            SidebarTab::Outline => {
                let flat = self.flatten_outline();
                if let Some(item) = flat.get(self.selected_outline_idx) {
                    let target_line = item.line;
                    Self::toggle_outline_node(&mut self.outline_nodes, target_line);
                }
            }
        }
    }

    pub fn handle_right(&mut self) {
        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                if let Some(item) = items.get(self.selected_file_idx) {
                    if item.is_dir && !item.is_expanded {
                        let path = item.path.clone();
                        self.file_tree.toggle_expand(&path);
                    }
                }
            }
            SidebarTab::Outline => {
                let flat = self.flatten_outline();
                if let Some(item) = flat.get(self.selected_outline_idx) {
                    let target_line = item.line;
                    Self::toggle_outline_node(&mut self.outline_nodes, target_line);
                }
            }
        }
    }

    fn toggle_outline_node(nodes: &mut [OutlineNode], line: usize) -> bool {
        for node in nodes {
            if node.line == line {
                node.is_expanded = !node.is_expanded;
                return true;
            }
            if Self::toggle_outline_node(&mut node.children, line) {
                return true;
            }
        }
        false
    }

    pub fn activate_current(&mut self) -> SidebarAction {
        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                if let Some(item) = items.get(self.selected_file_idx) {
                    if item.is_dir {
                        let path = item.path.clone();
                        self.file_tree.toggle_expand(&path);
                        SidebarAction::None
                    } else {
                        SidebarAction::OpenFile(item.path.clone())
                    }
                } else {
                    SidebarAction::None
                }
            }
            SidebarTab::Outline => {
                let items = self.flatten_outline();
                if let Some(item) = items.get(self.selected_outline_idx) {
                    SidebarAction::JumpToLine(item.line)
                } else {
                    SidebarAction::None
                }
            }
        }
    }

    pub fn handle_click(&mut self, rel_x: u16, rel_y: u16, _viewport_height: usize) -> SidebarAction {
        if rel_y == 0 {
            // Tab header row
            if rel_x < 10 {
                self.active_tab = SidebarTab::Files;
            } else if rel_x < 22 {
                self.active_tab = SidebarTab::Outline;
            }
            return SidebarAction::None;
        }

        let item_idx = (rel_y.saturating_sub(1) as usize) + match self.active_tab {
            SidebarTab::Files => self.file_scroll_top,
            SidebarTab::Outline => self.outline_scroll_top,
        };

        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                if item_idx < items.len() {
                    self.selected_file_idx = item_idx;
                    return self.activate_current();
                }
            }
            SidebarTab::Outline => {
                let items = self.flatten_outline();
                if item_idx < items.len() {
                    self.selected_outline_idx = item_idx;
                    return self.activate_current();
                }
            }
        }
        SidebarAction::None
    }

    fn to_crossterm_color(c: theme::Color, is_terminal_default: bool) -> Color {
        if is_terminal_default {
            match c {
                theme::Color::Rgb(0, 0, 0) | theme::Color::Rgb(255, 255, 255) => {
                    return Color::Reset;
                }
                theme::Color::Ansi(i) => {
                    return Color::AnsiValue(i);
                }
                _ => {}
            }
        }
        match c {
            theme::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
            theme::Color::Ansi(i) => Color::AnsiValue(i),
        }
    }

    pub fn render(
        &self,
        renderer: &mut Renderer,
        bounds: (u16, u16, u16, u16),
        is_focused: bool,
        theme: &Theme,
        active_buffer_path: Option<&Path>,
    ) {
        let (bx, by, bw, bh) = bounds;
        if bw == 0 || bh == 0 {
            return;
        }

        let is_td = theme.meta.name == "Terminal Default";
        let bg = Self::to_crossterm_color(theme.ui.status_bar_bg, is_td);
        let fg = Self::to_crossterm_color(theme.ui.status_bar_fg, is_td);
        let accent = Self::to_crossterm_color(theme.ui.tab_active_fg, is_td);
        let border_color = Self::to_crossterm_color(theme.ui.dialog_border, is_td);
        let sel_bg = if is_focused {
            Self::to_crossterm_color(theme.editor.selection, is_td)
        } else {
            Self::to_crossterm_color(theme.ui.panel_bg, is_td)
        };

        // 1. Fill sidebar background
        for y in 0..bh {
            for x in 0..bw {
                renderer.set_cell(
                    bx + x,
                    by + y,
                    Cell {
                        ch: ' ',
                        fg,
                        bg,
                        bold: false,
                        underline: false,
                        width: 1,
                    },
                );
            }
            // Draw right vertical divider between sidebar and main editor
            if bw > 1 {
                renderer.set_cell(
                    bx + bw - 1,
                    by + y,
                    Cell {
                        ch: '│',
                        fg: border_color,
                        bg,
                        bold: false,
                        underline: false,
                        width: 1,
                    },
                );
            }
        }

        // 2. Render Header tabs (Files / Outline)
        let files_tab_label = " [Files] ";
        let outline_tab_label = " [Outline] ";

        let files_active = self.active_tab == SidebarTab::Files;
        let outline_active = self.active_tab == SidebarTab::Outline;

        let mut header_x = bx;
        for c in files_tab_label.chars() {
            if header_x >= bx + bw - 1 {
                break;
            }
            renderer.set_cell(
                header_x,
                by,
                Cell {
                    ch: c,
                    fg: if files_active { accent } else { fg },
                    bg,
                    bold: files_active,
                    underline: false,
                    width: 1,
                },
            );
            header_x += 1;
        }

        for c in outline_tab_label.chars() {
            if header_x >= bx + bw - 1 {
                break;
            }
            renderer.set_cell(
                header_x,
                by,
                Cell {
                    ch: c,
                    fg: if outline_active { accent } else { fg },
                    bg,
                    bold: outline_active,
                    underline: false,
                    width: 1,
                },
            );
            header_x += 1;
        }

        // 3. Render content
        let content_h = (bh as usize).saturating_sub(1);
        let content_w = (bw as usize).saturating_sub(1); // excluding right border

        match self.active_tab {
            SidebarTab::Files => {
                let items = self.flatten_files();
                for row_idx in 0..content_h {
                    let item_idx = self.file_scroll_top + row_idx;
                    if item_idx >= items.len() {
                        break;
                    }
                    let item = &items[item_idx];
                    let is_selected = item_idx == self.selected_file_idx;
                    let is_current_file = active_buffer_path.is_some_and(|p| p == item.path);

                    let item_bg = if is_selected { sel_bg } else { bg };
                    let item_fg = if is_current_file {
                        accent
                    } else if item.is_dir {
                        Color::Cyan
                    } else {
                        fg
                    };

                    let indent = "  ".repeat(item.depth);
                    let prefix = if item.is_dir {
                        if item.is_expanded { "▼ " } else { "► " }
                    } else {
                        "  "
                    };
                    let full_line = format!("{}{}{}", indent, prefix, item.name);

                    let y = by + 1 + row_idx as u16;
                    let mut cur_x = bx;
                    let mut col_used = 0;

                    for ch in full_line.chars() {
                        let w = ch.width().unwrap_or(1);
                        if col_used + w > content_w {
                            break;
                        }
                        renderer.set_cell(
                            cur_x,
                            y,
                            Cell {
                                ch,
                                fg: item_fg,
                                bg: item_bg,
                                bold: is_selected || is_current_file || item.is_dir,
                                underline: false,
                                width: w as u8,
                            },
                        );
                        cur_x += w as u16;
                        col_used += w;
                    }

                    // Fill remainder of row with selection background if selected
                    while col_used < content_w {
                        renderer.set_cell(
                            cur_x,
                            y,
                            Cell {
                                ch: ' ',
                                fg: item_fg,
                                bg: item_bg,
                                bold: false,
                                underline: false,
                                width: 1,
                            },
                        );
                        cur_x += 1;
                        col_used += 1;
                    }
                }
            }
            SidebarTab::Outline => {
                let items = self.flatten_outline();
                if items.is_empty() {
                    let empty_msg = " (No headings)";
                    let y = by + 1;
                    for (cur_x, ch) in (bx..bx + content_w as u16).zip(empty_msg.chars()) {
                        renderer.set_cell(
                            cur_x,
                            y,
                            Cell {
                                ch,
                                fg: border_color,
                                bg,
                                bold: false,
                                underline: false,
                                width: 1,
                            },
                        );
                    }
                } else {
                    for row_idx in 0..content_h {
                        let item_idx = self.outline_scroll_top + row_idx;
                        if item_idx >= items.len() {
                            break;
                        }
                        let item = &items[item_idx];
                        let is_selected = item_idx == self.selected_outline_idx;
                        let item_bg = if is_selected { sel_bg } else { bg };
                        let item_fg = if is_selected { accent } else { fg };

                        let indent = "  ".repeat(item.depth);
                        let prefix = if item.has_children {
                            if item.is_expanded { "▼ " } else { "► " }
                        } else {
                            "• "
                        };
                        let h_tag = format!("H{}: ", item.level);
                        let full_line = format!("{}{}{}{}", indent, prefix, h_tag, item.title);

                        let y = by + 1 + row_idx as u16;
                        let mut cur_x = bx;
                        let mut col_used = 0;

                        for ch in full_line.chars() {
                            let w = ch.width().unwrap_or(1);
                            if col_used + w > content_w {
                                break;
                            }
                            renderer.set_cell(
                                cur_x,
                                y,
                                Cell {
                                    ch,
                                    fg: item_fg,
                                    bg: item_bg,
                                    bold: is_selected || item.level == 1,
                                    underline: false,
                                    width: w as u8,
                                },
                            );
                            cur_x += w as u16;
                            col_used += w;
                        }

                        while col_used < content_w {
                            renderer.set_cell(
                                cur_x,
                                y,
                                Cell {
                                    ch: ' ',
                                    fg: item_fg,
                                    bg: item_bg,
                                    bold: false,
                                    underline: false,
                                    width: 1,
                                },
                            );
                            cur_x += 1;
                            col_used += 1;
                        }
                    }
                }
            }
        }
    }
}
