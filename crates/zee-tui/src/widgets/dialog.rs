use crate::renderer::{Renderer, Cell};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};
use crossterm::style::Color;
use std::path::PathBuf;
use std::fs;
use std::time::SystemTime;
use chrono::{DateTime, Local};
use unicode_width::UnicodeWidthChar;

pub enum DialogResult<T> {
    Ok(T),
    Cancel,
    Pending,
}

pub trait Dialog {
    fn title(&self) -> &str;
    fn dimensions(&self) -> (u16, u16); // width, height
    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16);
    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action>;
    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action>;
    fn set_error(&mut self, _msg: String) {}
    fn cursor_pos(&self) -> Option<(u16, u16)> { None }
    fn selected_encoding(&self) -> Option<zee_core::Encoding> { None }
}

#[derive(Debug, Clone)]
pub enum Action {
    ConfirmPath(PathBuf),
    ConfirmLine(usize),
    Confirm,
    Save,
    DontSave,
    Discard,
    Cancel,
    FileContextMenuAction { action: &'static str, path: PathBuf, #[allow(dead_code)] is_dir: bool },
    InputName(String),
    SaveSettings(Box<zee_core::Config>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub struct FileBrowser {
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected_idx: usize,
    pub show_hidden: bool,
    pub detect_encoding: bool,
    pub is_save_mode: bool,
    pub default_ext: &'static str,
    pub encoding: zee_core::Encoding,
    pub sort_by: SortBy,
    pub sort_order: SortOrder,
    pub input_text: String,
    pub input_focused: bool,

    // i18n labels
    pub i18n_hidden: String,
    pub i18n_encoding: String,
    pub i18n_name: String,
    pub i18n_size: String,
    pub i18n_modified: String,
    pub i18n_filename: String,
}

impl FileBrowser {
    pub fn new(path: PathBuf) -> Self {
        let mut browser = Self {
            current_dir: path,
            entries: Vec::new(),
            selected_idx: 0,
            show_hidden: false,
            detect_encoding: true,
            is_save_mode: false,
            default_ext: ".txt",
            encoding: zee_core::Encoding::Utf8,
            sort_by: SortBy::Name,
            sort_order: SortOrder::Ascending,
            input_text: String::new(),
            input_focused: false,

            i18n_hidden: "Show Hidden (Alt+H)".to_string(),
            i18n_encoding: "Detect Encoding (Alt+E)".to_string(),
            i18n_name: "Name".to_string(),
            i18n_size: "Size".to_string(),
            i18n_modified: "Modified".to_string(),
            i18n_filename: "File name: ".to_string(),
        };
        browser.refresh();
        browser
    }

    pub fn toggle_ext(&mut self) {
        if self.default_ext == ".txt" {
            self.default_ext = ".md";
            if self.input_text.ends_with(".txt") {
                self.input_text.truncate(self.input_text.len() - 4);
                self.input_text.push_str(".md");
            }
        } else {
            self.default_ext = ".txt";
            if self.input_text.ends_with(".md") {
                self.input_text.truncate(self.input_text.len() - 3);
                self.input_text.push_str(".txt");
            }
        }
    }

    pub fn localize(&mut self, i18n: &zee_core::I18n) {
        self.i18n_hidden = format!("{} (Alt+H)", i18n.get("dialog.show_hidden"));
        self.i18n_encoding = format!("{} (Alt+E)", i18n.get("dialog.detect_encoding"));
        self.i18n_name = i18n.get("dialog.file_browser.name").to_string();
        if self.i18n_name == "dialog.file_browser.name" { self.i18n_name = "Name".to_string(); }
        self.i18n_size = i18n.get("dialog.file_browser.size").to_string();
        if self.i18n_size == "dialog.file_browser.size" { self.i18n_size = "Size".to_string(); }
        self.i18n_modified = i18n.get("dialog.file_browser.modified").to_string();
        if self.i18n_modified == "dialog.file_browser.modified" { self.i18n_modified = "Modified".to_string(); }
        self.i18n_filename = format!("{}: ", i18n.get("dialog.file_browser.filename"));
        if self.i18n_filename == "dialog.file_browser.filename: " { self.i18n_filename = "File name: ".to_string(); }
    }

    pub fn refresh(&mut self) {
        self.entries.clear();
        
        // Add parent dir if not at root
        if self.current_dir.parent().is_some() {
            self.entries.push(FileEntry {
                name: "..".to_string(),
                is_dir: true,
                size: 0,
                modified: None,
            });
        }

        if let Ok(read_dir) = fs::read_dir(&self.current_dir) {
            for entry in read_dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }
                let metadata = entry.metadata().ok();
                self.entries.push(FileEntry {
                    name,
                    is_dir: entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
                    size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    modified: metadata.and_then(|m| m.modified().ok()),
                });
            }
        }

        self.sort();
        self.selected_idx = self.selected_idx.min(self.entries.len().saturating_sub(1));
    }

    pub fn sort(&mut self) {
        self.entries.sort_by(|a, b| {
            // Dirs always before files
            if a.name == ".." { return std::cmp::Ordering::Less; }
            if b.name == ".." { return std::cmp::Ordering::Greater; }
            if a.is_dir != b.is_dir {
                return b.is_dir.cmp(&a.is_dir);
            }

            let res = match self.sort_by {
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Size => a.size.cmp(&b.size),
                SortBy::Modified => a.modified.cmp(&b.modified),
            };

            if self.sort_order == SortOrder::Descending {
                res.reverse()
            } else {
                res
            }
        });
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PathBuf> {
        if key.modifiers == KeyModifiers::ALT {
            match key.code {
                KeyCode::Char('h') => {
                    self.show_hidden = !self.show_hidden;
                    self.refresh();
                    return None;
                }
                KeyCode::Char('e') => {
                    if self.is_save_mode {
                        self.encoding = self.encoding.next();
                    } else {
                        self.detect_encoding = !self.detect_encoding;
                    }
                    return None;
                }
                KeyCode::Char('x') => {
                    if self.is_save_mode {
                        self.toggle_ext();
                    }
                    return None;
                }
                _ => {}
            }
        }

        if self.input_focused {
            match key.code {
                KeyCode::Char(c) => {
                    self.input_text.push(c);
                    return None;
                }
                KeyCode::Backspace => {
                    self.input_text.pop();
                    return None;
                }
                KeyCode::Enter => {
                    let mut text = self.input_text.trim().to_string();
                    if !text.is_empty() {
                        if self.is_save_mode && !text.contains('.') {
                            text.push_str(self.default_ext);
                        }
                        return Some(self.current_dir.join(&text));
                    }
                }
                KeyCode::Tab => {
                    self.input_focused = false;
                    return None;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    if let Some(entry) = self.entries.get(self.selected_idx) {
                        if !entry.is_dir {
                            self.input_text = entry.name.clone();
                        }
                    }
                }
                None
            }
            KeyCode::Down => {
                if self.selected_idx + 1 < self.entries.len() {
                    self.selected_idx += 1;
                    if let Some(entry) = self.entries.get(self.selected_idx) {
                        if !entry.is_dir {
                            self.input_text = entry.name.clone();
                        }
                    }
                }
                None
            }
            KeyCode::Enter => {
                if let Some(entry) = self.entries.get(self.selected_idx) {
                    let path = self.current_dir.join(&entry.name);
                    if entry.is_dir {
                        if entry.name == ".." {
                            if let Some(parent) = self.current_dir.parent() {
                                self.current_dir = parent.to_path_buf();
                            }
                        } else {
                            self.current_dir = path;
                        }
                        self.refresh();
                        self.selected_idx = 0;
                        None
                    } else {
                        let mut name = entry.name.clone();
                        if self.is_save_mode && !name.contains('.') {
                            name.push_str(self.default_ext);
                        }
                        Some(self.current_dir.join(&name))
                    }
                } else {
                    None
                }
            }
            KeyCode::Backspace => {
                if let Some(parent) = self.current_dir.parent() {
                    self.current_dir = parent.to_path_buf();
                    self.refresh();
                    self.selected_idx = 0;
                }
                None
            }
            KeyCode::Tab => {
                self.input_focused = true;
                None
            }
            KeyCode::Char(c) if c.is_alphanumeric() => {
                // Typeahead
                let search = c.to_lowercase().to_string();
                if let Some(idx) = self.entries.iter().position(|e| e.name.to_lowercase().starts_with(&search)) {
                    self.selected_idx = idx;
                    if !self.entries[idx].is_dir {
                        self.input_text = self.entries[idx].name.clone();
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> Option<PathBuf> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }

        let (mx, my) = (mouse.column, mouse.row);

        // Options bar (Show Hidden / Detect Encoding or Encoding/Ext)
        if my == y + 2 {
            let hidden_x = x + 2;
            let hidden_w = 24;
            if mx >= hidden_x && mx < hidden_x + hidden_w {
                self.show_hidden = !self.show_hidden;
                self.refresh();
                return None;
            }
            if self.is_save_mode {
                let enc_x = x + 26;
                let enc_w = 26;
                if mx >= enc_x && mx < enc_x + enc_w {
                    self.encoding = self.encoding.next();
                    return None;
                }
                let ext_x = x + 53;
                let ext_w = 20;
                if mx >= ext_x && mx < ext_x + ext_w {
                    self.toggle_ext();
                    return None;
                }
            } else {
                let enc_x = x + 28;
                let enc_w = 26;
                if mx >= enc_x && mx < enc_x + enc_w {
                    self.detect_encoding = !self.detect_encoding;
                    return None;
                }
            }
        }

        // Quick-nav bar
        if my == y + 3 {
            let navs = ["..", "/", "~", "Docs", "Downloads"];
            let mut nx = x + 2;
            for nav in navs {
                let nav_len = nav.len() as u16 + 2;
                if mx >= nx && mx < nx + nav_len {
                    match nav {
                        ".." => {
                            if let Some(parent) = self.current_dir.parent() {
                                self.current_dir = parent.to_path_buf();
                            }
                        }
                        "/" => self.current_dir = PathBuf::from("/"),
                        "~" => {
                            if let Some(home) = std::env::var_os("HOME") {
                                self.current_dir = PathBuf::from(home);
                            }
                        }
                        "Docs" => {
                            if let Some(home) = std::env::var_os("HOME") {
                                self.current_dir = PathBuf::from(home).join("Documents");
                            }
                        }
                        "Downloads" => {
                            if let Some(home) = std::env::var_os("HOME") {
                                self.current_dir = PathBuf::from(home).join("Downloads");
                            }
                        }
                        _ => {}
                    }
                    self.refresh();
                    self.selected_idx = 0;
                    return None;
                }
                nx += nav_len + 1;
            }
        }

        // Header click sorting
        if my == y + 4 {
            let name_x = x + 2;
            let size_x = x + w - 25;
            let mod_x = x + w - 14;
            
            if mx >= name_x && mx < size_x {
                if self.sort_by == SortBy::Name {
                    self.sort_order = if self.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                } else {
                    self.sort_by = SortBy::Name;
                    self.sort_order = SortOrder::Ascending;
                }
                self.sort();
            } else if mx >= size_x && mx < mod_x {
                if self.sort_by == SortBy::Size {
                    self.sort_order = if self.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                } else {
                    self.sort_by = SortBy::Size;
                    self.sort_order = SortOrder::Ascending;
                }
                self.sort();
            } else if mx >= mod_x && mx < x + w - 2 {
                if self.sort_by == SortBy::Modified {
                    self.sort_order = if self.sort_order == SortOrder::Ascending { SortOrder::Descending } else { SortOrder::Ascending };
                } else {
                    self.sort_by = SortBy::Modified;
                    self.sort_order = SortOrder::Ascending;
                }
                self.sort();
            }
            return None;
        }

        // Entries
        let visible_count = h.saturating_sub(9) as usize;
        if my >= y + 5 && my < y + 5 + visible_count as u16 {
            let click_idx = (my - (y + 5)) as usize;
            let start_idx = if self.selected_idx >= visible_count {
                self.selected_idx - visible_count + 1
            } else {
                0
            };
            let idx = start_idx + click_idx;
            if idx < self.entries.len() {
                self.selected_idx = idx;
                self.input_focused = false;
                if let Some(entry) = self.entries.get(idx) {
                    if !entry.is_dir {
                        self.input_text = entry.name.clone();
                    }
                }
            }
        }

        // Input field
        if my == y + h - 2 {
            self.input_focused = true;
        }

        None
    }

    pub fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);
        let active_bg = to_ct_color(theme.ui.button_active_bg, theme);
        let active_fg = to_ct_color(theme.ui.button_active_fg, theme);

        // Render current dir
        let dir_str = self.current_dir.to_string_lossy();
        let mut cur_dir_x = x + 2;
        for c in dir_str.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_dir_x + cw < x + w - 2 {
                renderer.set_cell(cur_dir_x, y + 1, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                cur_dir_x += cw;
            } else {
                break;
            }
        }

        // Options bar
        let hidden_text = format!("[{}] {}", if self.show_hidden { "x" } else { " " }, self.i18n_hidden);
        let mut cur_opt_x = x + 2;
        for c in hidden_text.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_opt_x + cw < x + 26 {
                renderer.set_cell(cur_opt_x, y + 2, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                cur_opt_x += cw;
            }
        }

        if self.is_save_mode {
            let enc_text = format!("Enc: [{}] (Alt+E)", self.encoding.name());
            let mut cur_enc_x = x + 26;
            for c in enc_text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_enc_x + cw < x + 52 {
                    renderer.set_cell(cur_enc_x, y + 2, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                    cur_enc_x += cw;
                }
            }

            let ext_text = format!("Ext: [{}] (Alt+X)", self.default_ext);
            let mut cur_ext_x = x + 53;
            for c in ext_text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_ext_x + cw < x + w - 2 {
                    renderer.set_cell(cur_ext_x, y + 2, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                    cur_ext_x += cw;
                }
            }
        } else {
            let enc_text = format!("[{}] {}", if self.detect_encoding { "x" } else { " " }, self.i18n_encoding);
            let mut cur_enc_x = x + 28;
            for c in enc_text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_enc_x + cw < x + w - 2 {
                    renderer.set_cell(cur_enc_x, y + 2, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                    cur_enc_x += cw;
                }
            }
        }

        // Quick-nav bar
        let navs = ["..", "/", "~", "Docs", "Downloads"];
        let mut nx = x + 2;
        let ny = y + 3;
        for nav in navs {
            let nav_str = format!(" {} ", nav);
            for c in nav_str.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                renderer.set_cell(nx, ny, Cell { ch: c, bg: to_ct_color(theme.ui.status_bar_bg, theme), fg: to_ct_color(theme.ui.status_bar_fg, theme), width: cw as u8, ..Default::default() });
                nx += cw;
            }
            nx += 1;
        }

        // Header
        let name_indicator = if self.sort_by == SortBy::Name { if self.sort_order == SortOrder::Ascending { " ▲" } else { " ▼" } } else { "" };
        let size_indicator = if self.sort_by == SortBy::Size { if self.sort_order == SortOrder::Ascending { " ▲" } else { " ▼" } } else { "" };
        let mod_indicator = if self.sort_by == SortBy::Modified { if self.sort_order == SortOrder::Ascending { " ▲" } else { " ▼" } } else { "" };

        let name_head = format!("{}{}", self.i18n_name, name_indicator);
        let size_head = format!("{}{}", self.i18n_size, size_indicator);
        let mod_head = format!("{}{}", self.i18n_modified, mod_indicator);

        let mut cur_h_x = x + 2;
        for c in name_head.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_h_x + cw < x + w - 25 {
                renderer.set_cell(cur_h_x, y + 4, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, bold: true, width: cw as u8, ..Default::default() });
                cur_h_x += cw;
            }
        }
        let mut cur_s_x = x + w - 25;
        for c in size_head.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_s_x + cw < x + w - 14 {
                renderer.set_cell(cur_s_x, y + 4, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, bold: true, width: cw as u8, ..Default::default() });
                cur_s_x += cw;
            }
        }
        let mut cur_m_x = x + w - 14;
        for c in mod_head.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_m_x + cw < x + w - 2 {
                renderer.set_cell(cur_m_x, y + 4, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, bold: true, width: cw as u8, ..Default::default() });
                cur_m_x += cw;
            }
        }

        // Entries
        let visible_count = h.saturating_sub(9) as usize;
        let start_idx = if self.selected_idx >= visible_count {
            self.selected_idx - visible_count + 1
        } else {
            0
        };

        for i in 0..visible_count {
            let idx = start_idx + i;
            if let Some(entry) = self.entries.get(idx) {
                let iy = y + 5 + i as u16;
                let is_selected = idx == self.selected_idx;
                let bg = if is_selected { 
                    if self.input_focused {
                        // Inactive but selected
                        to_ct_color(theme.ui.tab_inactive_bg, theme)
                    } else {
                        active_bg 
                    }
                } else { 
                    dialog_bg 
                };
                let fg = if is_selected { 
                    if self.input_focused {
                        to_ct_color(theme.ui.tab_inactive_fg, theme)
                    } else {
                        active_fg
                    }
                } else { 
                    dialog_fg 
                };

                for dx in 1..w - 1 {
                    renderer.set_cell(x + dx, iy, Cell { ch: ' ', bg, ..Default::default() });
                }

                let name = if entry.is_dir && entry.name != ".." {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };

                let mut cur_name_x = x + 2;
                for c in name.chars() {
                    let cw = c.width().unwrap_or(0) as u16;
                    if (cur_name_x + cw) < x + w - 25 {
                        renderer.set_cell(cur_name_x, iy, Cell { ch: c, bg, fg, width: cw as u8, ..Default::default() });
                        cur_name_x += cw;
                    } else {
                        break;
                    }
                }

                if !entry.is_dir {
                    let size_str = if entry.size < 1024 {
                        format!("{} B", entry.size)
                    } else if entry.size < 1024 * 1024 {
                        format!("{:.1} KB", entry.size as f64 / 1024.0)
                    } else {
                        format!("{:.1} MB", entry.size as f64 / (1024.0 * 1024.0))
                    };
                    for (j, c) in size_str.chars().enumerate() {
                        if (j as u16) < 10 {
                            renderer.set_cell(x + w - 25 + j as u16, iy, Cell { ch: c, bg, fg, ..Default::default() });
                        }
                    }

                    if let Some(m) = entry.modified {
                        let now = SystemTime::now();
                        let duration = now.duration_since(m).unwrap_or(std::time::Duration::from_secs(0));
                        let mod_str = if duration.as_secs() < 24 * 3600 {
                            if duration.as_secs() < 60 {
                                "just now".to_string()
                            } else if duration.as_secs() < 3600 {
                                format!("{}m ago", duration.as_secs() / 60)
                            } else {
                                format!("{}h ago", duration.as_secs() / 3600)
                            }
                        } else {
                            let datetime: DateTime<Local> = m.into();
                            datetime.format("%Y-%m-%d").to_string()
                        };
                        for (j, c) in mod_str.chars().enumerate() {
                            if (j as u16) < 12 {
                                renderer.set_cell(x + w - 14 + j as u16, iy, Cell { ch: c, bg, fg, ..Default::default() });
                            }
                        }
                    }
                }
            }
        }

        // Input field
        let iy = y + h - 2;
        let input_bg = if self.input_focused { active_bg } else { to_ct_color(theme.ui.panel_bg, theme) };
        let input_fg = if self.input_focused { active_fg } else { to_ct_color(theme.ui.panel_fg, theme) };
        
        let mut cur_label_x = x + 2;
        for c in self.i18n_filename.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_label_x + cw < x + w - 2 {
                renderer.set_cell(cur_label_x, iy, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                cur_label_x += cw;
            }
        }
        
        let input_x = cur_label_x;
        let input_w = (x + w - 2).saturating_sub(input_x);
        for dx in 0..input_w {
            renderer.set_cell(input_x + dx, iy, Cell { ch: ' ', bg: input_bg, ..Default::default() });
        }
        let mut cur_text_x = input_x;
        for c in self.input_text.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_text_x + cw <= input_x + input_w {
                renderer.set_cell(cur_text_x, iy, Cell { ch: c, bg: input_bg, fg: input_fg, width: cw as u8, ..Default::default() });
                cur_text_x += cw;
            } else {
                break;
            }
        }
    }
}

fn to_ct_color(c: zee_core::theme::Color, theme: &zee_core::theme::Theme) -> Color {
    if theme.meta.name == "Terminal Default" {
        match c {
            zee_core::theme::Color::Rgb(0, 0, 0) | zee_core::theme::Color::Rgb(255, 255, 255) => {
                return Color::Reset;
            }
            zee_core::theme::Color::Ansi(i) => {
                return Color::AnsiValue(i);
            }
            _ => {}
        }
    }
    match c {
        zee_core::theme::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
        zee_core::theme::Color::Ansi(i) => Color::AnsiValue(i),
    }
}

pub fn render_base_dialog(renderer: &mut Renderer, theme: &zee_core::theme::Theme, title: &str, x: u16, y: u16, w: u16, h: u16) {
    let bg = to_ct_color(theme.ui.dialog_bg, theme);
    let border_fg = to_ct_color(theme.ui.dialog_border, theme);

    // Fill background
    for dy in 0..h {
        for dx in 0..w {
            renderer.set_cell(x + dx, y + dy, Cell {
                ch: ' ',
                bg,
                ..Default::default()
            });
        }
    }

    // Draw borders (Unicode box-drawing)
    renderer.set_cell(x, y, Cell { ch: '┌', fg: border_fg, bg, ..Default::default() });
    renderer.set_cell(x + w - 1, y, Cell { ch: '┐', fg: border_fg, bg, ..Default::default() });
    renderer.set_cell(x, y + h - 1, Cell { ch: '└', fg: border_fg, bg, ..Default::default() });
    renderer.set_cell(x + w - 1, y + h - 1, Cell { ch: '┘', fg: border_fg, bg, ..Default::default() });

    for dx in 1..w - 1 {
        renderer.set_cell(x + dx, y, Cell { ch: '─', fg: border_fg, bg, ..Default::default() });
        renderer.set_cell(x + dx, y + h - 1, Cell { ch: '─', fg: border_fg, bg, ..Default::default() });
    }

    for dy in 1..h - 1 {
        renderer.set_cell(x, y + dy, Cell { ch: '│', fg: border_fg, bg, ..Default::default() });
        renderer.set_cell(x + w - 1, y + dy, Cell { ch: '│', fg: border_fg, bg, ..Default::default() });
    }

    let title_str = format!(" {} ", title);
    let mut cur_tx = x + 2;
    for c in title_str.chars() {
        let cw = c.width().unwrap_or(0) as u16;
        if cur_tx + cw < x + w - 2 {
            renderer.set_cell(cur_tx, y, Cell { ch: c, fg: border_fg, bg, width: cw as u8, ..Default::default() });
            cur_tx += cw;
        } else {
            break;
        }
    }
}

pub struct OpenDialog {
    pub browser: FileBrowser,
    pub error_message: Option<String>,
    pub i18n_title: String,
}

impl OpenDialog {
    pub fn new(i18n: &zee_core::I18n) -> Self {
        let mut browser = FileBrowser::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        browser.localize(i18n);
        Self {
            browser,
            error_message: None,
            i18n_title: i18n.get("dialog.open_file").to_string(),
        }
    }
}

impl Dialog for OpenDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        (80, 22)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);
        self.browser.render(renderer, theme, x, y, w, h);

        if let Some(ref msg) = self.error_message {
            let mut cur_msg_x = x + 2;
            let msg_y = y + h - 3;
            for c in msg.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_msg_x + cw < x + w - 2 {
                    renderer.set_cell(cur_msg_x, msg_y, Cell { ch: c, bg: to_ct_color(theme.ui.dialog_bg, theme), fg: Color::Red, width: cw as u8, ..Default::default() });
                    cur_msg_x += cw;
                } else {
                    break;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        if let Some(path) = self.browser.handle_key(key) {
            return DialogResult::Ok(Action::ConfirmPath(path));
        }
        match key.code {
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        if let Some(path) = self.browser.handle_mouse(mouse, x, y, w, h) {
            return DialogResult::Ok(Action::ConfirmPath(path));
        }
        DialogResult::Pending
    }

    fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
    }

    fn cursor_pos(&self) -> Option<(u16, u16)> {
        if self.browser.input_focused {
            let label_len: u16 = self.browser.i18n_filename.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            let text_len: u16 = self.browser.input_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            Some((2 + label_len + text_len, 22 - 2))
        } else {
            None
        }
    }
}

pub struct SaveAsDialog {
    pub browser: FileBrowser,
    pub error_message: Option<String>,
    pub i18n_title: String,
}

impl SaveAsDialog {
    pub fn new(current_path: Option<&PathBuf>, default_ext: Option<&str>, encoding: zee_core::Encoding, i18n: &zee_core::I18n) -> Self {
        let mut browser = FileBrowser::new(
            current_path.and_then(|p| p.parent()).map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))));
        
        browser.is_save_mode = true;
        browser.encoding = encoding;
        let ext = default_ext.unwrap_or(".txt");
        browser.default_ext = if ext == ".md" { ".md" } else { ".txt" };

        if let Some(path) = current_path {
            if let Some(name) = path.file_name() {
                browser.input_text = name.to_string_lossy().to_string();
            }
        } else {
            browser.input_text = format!("untitled{}", browser.default_ext);
        }
        
        browser.input_focused = true;
        browser.localize(i18n);
        
        Self { 
            browser,
            error_message: None,
            i18n_title: i18n.get("dialog.save_as").to_string(),
        }
    }
}

impl Dialog for SaveAsDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        (80, 22)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);
        self.browser.render(renderer, theme, x, y, w, h);

        if let Some(ref msg) = self.error_message {
            let mut cur_msg_x = x + 2;
            let msg_y = y + h - 3;
            for c in msg.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_msg_x + cw < x + w - 2 {
                    renderer.set_cell(cur_msg_x, msg_y, Cell { ch: c, bg: to_ct_color(theme.ui.dialog_bg, theme), fg: Color::Red, width: cw as u8, ..Default::default() });
                    cur_msg_x += cw;
                } else {
                    break;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        if let Some(path) = self.browser.handle_key(key) {
            return DialogResult::Ok(Action::ConfirmPath(path));
        }
        match key.code {
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        if let Some(path) = self.browser.handle_mouse(mouse, x, y, w, h) {
            return DialogResult::Ok(Action::ConfirmPath(path));
        }
        DialogResult::Pending
    }

    fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
    }

    fn cursor_pos(&self) -> Option<(u16, u16)> {
        if self.browser.input_focused {
            let label_len: u16 = self.browser.i18n_filename.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            let text_len: u16 = self.browser.input_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            Some((2 + label_len + text_len, 22 - 2))
        } else {
            None
        }
    }

    fn selected_encoding(&self) -> Option<zee_core::Encoding> {
        Some(self.browser.encoding)
    }
}

pub struct MessageDialog {
    pub title: String,
    pub message: String,
    pub buttons: Vec<(String, Action)>,
    pub selected_btn: usize,
}

impl MessageDialog {
    pub fn new(title: String, message: String, buttons: Vec<(String, Action)>) -> Self {
        Self {
            title,
            message,
            buttons,
            selected_btn: 0,
        }
    }
}

impl Dialog for MessageDialog {
    fn title(&self) -> &str {
        &self.title
    }

    fn dimensions(&self) -> (u16, u16) {
        let title_w: u16 = self.title.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let max_line_w: u16 = self.message.lines().map(|line| {
            line.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>()
        }).max().unwrap_or(0);
        let spacer = 2u16;
        let btns_w: u16 = self.buttons.iter().map(|(s, _)| {
            s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4
        }).sum::<u16>() + (self.buttons.len().saturating_sub(1) as u16 * spacer);

        let content_w = title_w.max(max_line_w).max(btns_w);
        // Ensure generous dialog width with padding, minimum 58
        let w = ((content_w + 12).max(58)).min(74);
        let line_count = self.message.lines().count().max(1) as u16;
        let h = (line_count + 6).max(8);
        (w, h)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);

        let lines: Vec<&str> = self.message.lines().collect();
        let start_y = y + 2;
        for (i, line) in lines.iter().enumerate() {
            let line_w: u16 = line.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            let lx = x + (w.saturating_sub(line_w)) / 2;
            let ly = start_y + i as u16;
            let mut cur_lx = lx;
            for c in line.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_lx + cw < x + w - 1 {
                    renderer.set_cell(cur_lx, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                }
                cur_lx += cw;
            }
        }

        // Buttons
        let spacer = 2u16;
        let total_btns_width: u16 = self.buttons.iter().map(|(s, _)| s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4).sum::<u16>() + (self.buttons.len().saturating_sub(1) as u16 * spacer);
        let mut bx = x + (w.saturating_sub(total_btns_width)) / 2;
        let by = y + h - 2;

        for (i, (s, _)) in self.buttons.iter().enumerate() {
            let btn_text = format!("[ {} ]", s);
            let is_selected = i == self.selected_btn;
            let bg = if is_selected { to_ct_color(theme.ui.button_active_bg, theme) } else { to_ct_color(theme.ui.panel_bg, theme) };
            let fg = if is_selected { to_ct_color(theme.ui.button_active_fg, theme) } else { to_ct_color(theme.ui.panel_fg, theme) };

            for c in btn_text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                renderer.set_cell(bx, by, Cell { ch: c, bg, fg, width: cw as u8, ..Default::default() });
                bx += cw;
            }
            bx += spacer;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        match key.code {
            KeyCode::Left | KeyCode::BackTab | KeyCode::Up => {
                if self.selected_btn > 0 {
                    self.selected_btn -= 1;
                } else {
                    self.selected_btn = self.buttons.len().saturating_sub(1);
                }
                DialogResult::Pending
            }
            KeyCode::Right | KeyCode::Tab | KeyCode::Down => {
                if self.selected_btn + 1 < self.buttons.len() {
                    self.selected_btn += 1;
                } else {
                    self.selected_btn = 0;
                }
                DialogResult::Pending
            }
            KeyCode::Enter => DialogResult::Ok(self.buttons[self.selected_btn].1.clone()),
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return DialogResult::Pending;
        }

        let (mx, my) = (mouse.column, mouse.row);
        if my == y + h - 2 {
            let spacer = 2u16;
            let total_btns_width: u16 = self.buttons.iter().map(|(s, _)| s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4).sum::<u16>() + (self.buttons.len().saturating_sub(1) as u16 * spacer);
            let mut bx = x + (w.saturating_sub(total_btns_width)) / 2;
            for (i, (s, _)) in self.buttons.iter().enumerate() {
                let btn_len = s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4;
                if mx >= bx && mx < bx + btn_len {
                    return DialogResult::Ok(self.buttons[i].1.clone());
                }
                bx += btn_len + spacer;
            }
        }
        DialogResult::Pending
    }
}

pub struct AboutDialog {
    pub i18n_about: String,
    pub i18n_ok: String,
    pub i18n_version: String,
    pub i18n_license: String,
}

impl AboutDialog {
    pub fn new(i18n: &zee_core::I18n) -> Self {
        Self {
            i18n_about: i18n.get("dialog.about").to_string(),
            i18n_ok: i18n.get("dialog.ok").to_string(),
            i18n_version: i18n.get("about.version").to_string(),
            i18n_license: i18n.get("about.license").to_string(),
        }
    }
}

impl Dialog for AboutDialog {
    fn title(&self) -> &str {
        &self.i18n_about
    }

    fn dimensions(&self) -> (u16, u16) {
        (50, 10)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);

        let content = [
            format!("zee v{}", env!("CARGO_PKG_VERSION")),
            format!("{}: v{}", self.i18n_version, env!("CARGO_PKG_VERSION")),
            "A lightweight, modern TUI editor.".to_string(),
            "".to_string(),
            format!("{}: MIT", self.i18n_license),
        ];

        for (i, line) in content.iter().enumerate() {
            let line_w: u16 = line.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            let lx = x + (w.saturating_sub(line_w)) / 2;
            let ly = y + 2 + i as u16;
            let mut cur_lx = lx;
            for c in line.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_lx + cw < x + w {
                    renderer.set_cell(cur_lx, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                }
                cur_lx += cw;
            }
        }

        let btn_text = format!("[ {} ]", self.i18n_ok);
        let btn_w: u16 = btn_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let mut bx = x + (w.saturating_sub(btn_w)) / 2;
        let by = y + h - 2;
        for c in btn_text.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            renderer.set_cell(bx, by, Cell {
                ch: c,
                bg: to_ct_color(theme.ui.button_active_bg, theme),
                fg: to_ct_color(theme.ui.button_active_fg, theme),
                width: cw as u8,
                ..Default::default()
            });
            bx += cw;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => DialogResult::Ok(Action::Confirm),
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return DialogResult::Pending;
        }
        let (mx, my) = (mouse.column, mouse.row);
        let btn_text = format!("[ {} ]", self.i18n_ok);
        let btn_w: u16 = btn_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let bx = x + (w.saturating_sub(btn_w)) / 2;
        let by = y + h - 2;
        if my == by && mx >= bx && mx < bx + btn_w {
            return DialogResult::Ok(Action::Confirm);
        }
        DialogResult::Pending
    }
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    Checking,
    UpToDate { version: String },
    Available { latest_version: String, asset_url: Option<String>, html_url: String },
    Downloading,
    Success,
    Failed { error: String, html_url: Option<String> },
}

enum UpdateMsg {
    CheckDone(Result<zee_core::selfupdate::ReleaseInfo, String>),
    ApplyDone(Result<(), String>),
}

pub struct UpdateDialog {
    pub state: std::sync::Mutex<UpdateState>,
    pub selected_btn: std::sync::Mutex<usize>,
    rx: std::sync::mpsc::Receiver<UpdateMsg>,
    tx: std::sync::mpsc::Sender<UpdateMsg>,
    pub i18n_title: String,
    pub i18n_ok: String,
    pub i18n_cancel: String,
    pub i18n_update_now: String,
    pub i18n_open_url: String,
    pub i18n_checking: String,
    pub i18n_up_to_date: String,
    pub i18n_available: String,
    pub i18n_downloading: String,
    pub i18n_success: String,
    pub i18n_failed: String,
}

impl UpdateDialog {
    pub fn new(i18n: &zee_core::I18n) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx_check = tx.clone();
        
        std::thread::spawn(move || {
            let res = zee_core::selfupdate::check_latest(zee_core::selfupdate::AppType::Cli)
                .map_err(|e| e.to_string());
            let _ = tx_check.send(UpdateMsg::CheckDone(res));
        });

        Self {
            state: std::sync::Mutex::new(UpdateState::Checking),
            selected_btn: std::sync::Mutex::new(0),
            rx,
            tx,
            i18n_title: i18n.get("dialog.update.title").to_string(),
            i18n_ok: i18n.get("dialog.ok").to_string(),
            i18n_cancel: i18n.get("dialog.cancel").to_string(),
            i18n_update_now: i18n.get("dialog.update.btn_update").to_string(),
            i18n_open_url: i18n.get("dialog.update.btn_open_url").to_string(),
            i18n_checking: i18n.get("dialog.update.checking").to_string(),
            i18n_up_to_date: i18n.get("dialog.update.up_to_date").to_string(),
            i18n_available: i18n.get("dialog.update.available").to_string(),
            i18n_downloading: i18n.get("dialog.update.downloading").to_string(),
            i18n_success: i18n.get("dialog.update.success").to_string(),
            i18n_failed: i18n.get("dialog.update.failed").to_string(),
        }
    }

    fn poll_messages(&self) {
        while let Ok(msg) = self.rx.try_recv() {
            let mut state = self.state.lock().unwrap();
            let mut selected = self.selected_btn.lock().unwrap();
            match msg {
                UpdateMsg::CheckDone(res) => {
                    match res {
                        Ok(info) => {
                            if zee_core::selfupdate::is_newer(zee_core::selfupdate::CURRENT_VERSION, &info.version) {
                                *state = UpdateState::Available {
                                    latest_version: info.version,
                                    asset_url: info.asset_url,
                                    html_url: info.html_url,
                                };
                                *selected = 0;
                            } else {
                                *state = UpdateState::UpToDate {
                                    version: info.version,
                                };
                                *selected = 0;
                            }
                        }
                        Err(err) => {
                            *state = UpdateState::Failed {
                                error: err,
                                html_url: Some(format!("https://github.com/{}/releases", zee_core::selfupdate::GITHUB_REPO)),
                            };
                            *selected = 0;
                        }
                    }
                }
                UpdateMsg::ApplyDone(res) => {
                    match res {
                        Ok(()) => {
                            *state = UpdateState::Success;
                            *selected = 0;
                        }
                        Err(err) => {
                            *state = UpdateState::Failed {
                                error: err,
                                html_url: Some(format!("https://github.com/{}/releases", zee_core::selfupdate::GITHUB_REPO)),
                            };
                            *selected = 0;
                        }
                    }
                }
            }
        }
    }

    fn trigger_update(&self, asset_url: Option<String>, html_url: String) {
        if let Some(url) = asset_url {
            let mut state = self.state.lock().unwrap();
            *state = UpdateState::Downloading;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let res = zee_core::selfupdate::apply_update(&url, zee_core::selfupdate::AppType::Cli)
                    .map_err(|e| e.to_string());
                let _ = tx.send(UpdateMsg::ApplyDone(res));
            });
        } else {
            let _ = zee_core::selfupdate::open_url(&html_url);
        }
    }
}

impl Dialog for UpdateDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        (56, 11)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        self.poll_messages();

        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);

        let mut lines = Vec::new();

        let state = self.state.lock().unwrap().clone();
        match &state {
            UpdateState::Checking => {
                lines.push(self.i18n_checking.clone());
            }
            UpdateState::UpToDate { version } => {
                lines.push(self.i18n_up_to_date.replace("{version}", version));
            }
            UpdateState::Available { latest_version, .. } => {
                lines.push(self.i18n_available.replace("{version}", latest_version));
            }
            UpdateState::Downloading => {
                lines.push(self.i18n_downloading.clone());
            }
            UpdateState::Success => {
                lines.push(self.i18n_success.clone());
            }
            UpdateState::Failed { error, .. } => {
                lines.push(self.i18n_failed.replace("{error}", error));
            }
        }

        for (i, line) in lines.iter().enumerate() {
            let line_w: u16 = line.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
            let lx = x + (w.saturating_sub(line_w)) / 2;
            let ly = y + 3 + i as u16;
            let mut cur_lx = lx;
            for c in line.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_lx + cw < x + w {
                    renderer.set_cell(cur_lx, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
                }
                cur_lx += cw;
            }
        }

        // Render buttons
        let buttons: Vec<&str> = match &state {
            UpdateState::Checking | UpdateState::Downloading => vec![],
            UpdateState::UpToDate { .. } | UpdateState::Success => vec![&self.i18n_ok],
            UpdateState::Available { .. } => vec![&self.i18n_update_now, &self.i18n_cancel, &self.i18n_open_url],
            UpdateState::Failed { .. } => vec![&self.i18n_ok, &self.i18n_open_url],
        };

        if !buttons.is_empty() {
            let total_btn_len: usize = buttons.iter().map(|b| b.len() + 4).sum::<usize>() + (buttons.len().saturating_sub(1) * 2);
            let mut bx = x + (w.saturating_sub(total_btn_len as u16)) / 2;
            let by = y + h - 2;

            let selected_idx = *self.selected_btn.lock().unwrap();

            for (idx, btn) in buttons.iter().enumerate() {
                let is_sel = idx == selected_idx;
                let (btn_bg, btn_fg) = if is_sel {
                    (to_ct_color(theme.ui.button_active_bg, theme), to_ct_color(theme.ui.button_active_fg, theme))
                } else {
                    (to_ct_color(theme.ui.status_bar_bg, theme), to_ct_color(theme.ui.status_bar_fg, theme))
                };

                let text = format!("[ {} ]", btn);
                for c in text.chars() {
                    let cw = c.width().unwrap_or(0) as u16;
                    renderer.set_cell(bx, by, Cell {
                        ch: c,
                        bg: btn_bg,
                        fg: btn_fg,
                        width: cw as u8,
                        ..Default::default()
                    });
                    bx += cw;
                }
                bx += 2;
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        self.poll_messages();

        let state = self.state.lock().unwrap().clone();
        let num_buttons = match &state {
            UpdateState::Checking | UpdateState::Downloading => 0,
            UpdateState::UpToDate { .. } | UpdateState::Success => 1,
            UpdateState::Available { .. } => 3,
            UpdateState::Failed { .. } => 2,
        };

        match key.code {
            KeyCode::Esc => DialogResult::Cancel,
            KeyCode::Left | KeyCode::BackTab => {
                if num_buttons > 0 {
                    let mut sel = self.selected_btn.lock().unwrap();
                    *sel = (*sel + num_buttons - 1) % num_buttons;
                }
                DialogResult::Pending
            }
            KeyCode::Right | KeyCode::Tab => {
                if num_buttons > 0 {
                    let mut sel = self.selected_btn.lock().unwrap();
                    *sel = (*sel + 1) % num_buttons;
                }
                DialogResult::Pending
            }
            KeyCode::Enter => {
                let sel = *self.selected_btn.lock().unwrap();
                match &state {
                    UpdateState::Checking | UpdateState::Downloading => DialogResult::Pending,
                    UpdateState::UpToDate { .. } => DialogResult::Ok(Action::Confirm),
                    UpdateState::Success => DialogResult::Ok(Action::Confirm),
                    UpdateState::Available { asset_url, html_url, .. } => {
                        let asset_url = asset_url.clone();
                        let html_url = html_url.clone();
                        match sel {
                            0 => {
                                self.trigger_update(asset_url, html_url);
                                DialogResult::Pending
                            }
                            1 => DialogResult::Cancel,
                            2 => {
                                let _ = zee_core::selfupdate::open_url(&html_url);
                                DialogResult::Pending
                            }
                            _ => DialogResult::Pending,
                        }
                    }
                    UpdateState::Failed { html_url, .. } => {
                        let html_url = html_url.clone();
                        match sel {
                            0 => DialogResult::Ok(Action::Confirm),
                            1 => {
                                if let Some(url) = html_url {
                                    let _ = zee_core::selfupdate::open_url(&url);
                                }
                                DialogResult::Pending
                            }
                            _ => DialogResult::Pending,
                        }
                    }
                }
            }
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        self.poll_messages();
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return DialogResult::Pending;
        }

        let (mx, my) = (mouse.column, mouse.row);
        let by = y + h - 2;
        if my != by {
            return DialogResult::Pending;
        }

        let state = self.state.lock().unwrap().clone();
        let buttons: Vec<&str> = match &state {
            UpdateState::Checking | UpdateState::Downloading => vec![],
            UpdateState::UpToDate { .. } | UpdateState::Success => vec![&self.i18n_ok],
            UpdateState::Available { .. } => vec![&self.i18n_update_now, &self.i18n_cancel, &self.i18n_open_url],
            UpdateState::Failed { .. } => vec![&self.i18n_ok, &self.i18n_open_url],
        };

        if buttons.is_empty() {
            return DialogResult::Pending;
        }

        let total_btn_len: usize = buttons.iter().map(|b| b.len() + 4).sum::<usize>() + (buttons.len().saturating_sub(1) * 2);
        let mut bx = x + (w.saturating_sub(total_btn_len as u16)) / 2;

        for (idx, btn) in buttons.iter().enumerate() {
            let btn_w = (btn.len() + 4) as u16;
            if mx >= bx && mx < bx + btn_w {
                *self.selected_btn.lock().unwrap() = idx;
                return self.handle_key(KeyEvent::from(KeyCode::Enter));
            }
            bx += btn_w + 2;
        }

        DialogResult::Pending
    }
}

pub struct ReopenConfirmationDialog {
    pub i18n_title: String,
    pub i18n_message: String,
    pub i18n_discard: String,
    pub i18n_cancel: String,
    pub selected_btn: usize,
}


impl ReopenConfirmationDialog {
    pub fn new(i18n: &zee_core::I18n) -> Self {
        Self {
            i18n_title: i18n.get("dialog.reopen_file").to_string(),
            i18n_message: i18n.get("dialog.discard_reopen_prompt").to_string(),
            i18n_discard: i18n.get("dialog.discard_reopen").to_string(),
            i18n_cancel: i18n.get("dialog.cancel").to_string(),
            selected_btn: 0,
        }
    }
}

impl Dialog for ReopenConfirmationDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        let title_w: u16 = self.i18n_title.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let msg_w: u16 = self.i18n_message.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let buttons = [&self.i18n_discard, &self.i18n_cancel];
        let spacer = 2u16;
        let btns_w: u16 = buttons.iter().map(|s| s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4).sum::<u16>() + spacer;
        let content_w = title_w.max(msg_w).max(btns_w);
        let w = ((content_w + 12).max(58)).min(74);
        (w, 8)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);

        let msg_w: u16 = self.i18n_message.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let lx = x + (w.saturating_sub(msg_w)) / 2;
        let ly = y + 2;
        let mut cur_lx = lx;
        for c in self.i18n_message.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_lx + cw < x + w - 1 {
                renderer.set_cell(cur_lx, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
            }
            cur_lx += cw;
        }

        let buttons = [&self.i18n_discard, &self.i18n_cancel];
        let spacer = 2u16;
        let total_btns_width: u16 = buttons.iter().map(|s| s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4).sum::<u16>() + spacer;
        let mut bx = x + (w.saturating_sub(total_btns_width)) / 2;
        let by = y + h - 2;

        for (i, s) in buttons.iter().enumerate() {
            let btn_text = format!("[ {} ]", s);
            let is_selected = i == self.selected_btn;
            let bg = if is_selected { to_ct_color(theme.ui.button_active_bg, theme) } else { to_ct_color(theme.ui.panel_bg, theme) };
            let fg = if is_selected { to_ct_color(theme.ui.button_active_fg, theme) } else { to_ct_color(theme.ui.panel_fg, theme) };

            for c in btn_text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                renderer.set_cell(bx, by, Cell { ch: c, bg, fg, width: cw as u8, ..Default::default() });
                bx += cw;
            }
            bx += spacer;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        match key.code {
            KeyCode::Left | KeyCode::BackTab | KeyCode::Right | KeyCode::Tab => {
                self.selected_btn = 1 - self.selected_btn;
                DialogResult::Pending
            }
            KeyCode::Enter => {
                if self.selected_btn == 0 {
                    DialogResult::Ok(Action::Discard)
                } else {
                    DialogResult::Cancel
                }
            }
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return DialogResult::Pending;
        }
        let (mx, my) = (mouse.column, mouse.row);
        if my == y + h - 2 {
            let buttons = [&self.i18n_discard, &self.i18n_cancel];
            let spacer = 2u16;
            let total_btns_width: u16 = buttons.iter().map(|s| s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4).sum::<u16>() + spacer;
            let mut bx = x + (w.saturating_sub(total_btns_width)) / 2;
            for (i, s) in buttons.iter().enumerate() {
                let btn_len = s.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>() + 4;
                if mx >= bx && mx < bx + btn_len {
                    if i == 0 { return DialogResult::Ok(Action::Discard); }
                    else { return DialogResult::Cancel; }
                }
                bx += btn_len + spacer;
            }
        }
        DialogResult::Pending
    }
}

pub struct GoToLineDialog {
    pub input_text: String,
    pub i18n_title: String,
    pub i18n_goto: String,
}

impl GoToLineDialog {
    pub fn new(i18n: &zee_core::I18n) -> Self {
        Self {
            input_text: String::new(),
            i18n_title: i18n.get("dialog.go_to_line").to_string(),
            i18n_goto: i18n.get("dialog.go_to_line").to_string(),
        }
    }
}

impl Dialog for GoToLineDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        (30, 6)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);
        let input_bg = to_ct_color(theme.ui.panel_bg, theme);
        let input_fg = to_ct_color(theme.ui.panel_fg, theme);

        let label = format!("{}: ", self.i18n_goto);
        let mut cur_lx = x + 2;
        let ly = y + 2;
        for c in label.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            renderer.set_cell(cur_lx, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
            cur_lx += cw;
        }

        let input_x = cur_lx;
        let input_w = (x + w - 2).saturating_sub(input_x);
        for dx in 0..input_w {
            renderer.set_cell(input_x + dx, ly, Cell { ch: ' ', bg: input_bg, ..Default::default() });
        }
        let mut cur_tx = input_x;
        for c in self.input_text.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_tx + cw <= input_x + input_w {
                renderer.set_cell(cur_tx, ly, Cell { ch: c, bg: input_bg, fg: input_fg, width: cw as u8, ..Default::default() });
                cur_tx += cw;
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        match key.code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.input_text.push(c);
                DialogResult::Pending
            }
            KeyCode::Backspace => {
                self.input_text.pop();
                DialogResult::Pending
            }
            KeyCode::Enter => {
                if let Ok(line) = self.input_text.parse::<usize>() {
                    DialogResult::Ok(Action::ConfirmLine(line))
                } else {
                    DialogResult::Cancel
                }
            }
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _x: u16, _y: u16, _w: u16, _h: u16) -> DialogResult<Action> {
        DialogResult::Pending
    }

    fn cursor_pos(&self) -> Option<(u16, u16)> {
        let label_len: u16 = format!("{}: ", self.i18n_goto).chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let text_len: u16 = self.input_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        Some((2 + label_len + text_len, 2))
    }
}

pub struct FileContextMenuDialog {
    pub target_path: PathBuf,
    pub is_dir: bool,
    pub title: String,
    pub options: Vec<(&'static str, String)>,
    pub selected_idx: usize,
}

impl FileContextMenuDialog {
    pub fn new(path: PathBuf, is_dir: bool, show_hidden: bool, i18n: &zee_core::I18n) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Workspace".to_string());
        let mut options = Vec::new();
        if is_dir {
            options.push(("new_file", format!("📄 {}", i18n.get("sidebar.new_file"))));
            options.push(("new_folder", format!("📁 {}", i18n.get("sidebar.new_folder"))));
        }
        options.push(("rename", format!("✏️ {}", i18n.get("sidebar.rename"))));
        options.push(("delete", format!("🗑️ {}", i18n.get("sidebar.delete"))));
        if show_hidden {
            options.push(("toggle_hidden", format!(".* {}", i18n.get("sidebar.hide_hidden"))));
        } else {
            options.push(("toggle_hidden", format!(".* {}", i18n.get("sidebar.show_hidden"))));
        }

        Self {
            target_path: path,
            is_dir,
            title: name,
            options,
            selected_idx: 0,
        }
    }
}

impl Dialog for FileContextMenuDialog {
    fn title(&self) -> &str {
        &self.title
    }

    fn dimensions(&self) -> (u16, u16) {
        let max_len = self.options.iter().map(|(_, l)| l.chars().map(|c| c.width().unwrap_or(0) as u16).sum::<u16>()).max().unwrap_or(16);
        let w = (max_len + 8).max(24);
        let h = self.options.len() as u16 + 4;
        (w, h)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let normal_fg = to_ct_color(theme.ui.panel_fg, theme);
        let sel_bg = to_ct_color(theme.ui.button_active_bg, theme);
        let sel_fg = to_ct_color(theme.ui.button_active_fg, theme);

        for (i, (_, label)) in self.options.iter().enumerate() {
            let row_y = y + 2 + i as u16;
            let is_selected = i == self.selected_idx;
            let bg = if is_selected { sel_bg } else { dialog_bg };
            let fg = if is_selected { sel_fg } else { normal_fg };

            // Fill row bg
            for col in 1..w - 1 {
                renderer.set_cell(x + col, row_y, Cell { ch: ' ', bg, fg, ..Default::default() });
            }

            let mut cur_x = x + 3;
            for c in label.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_x + cw < x + w - 1 {
                    renderer.set_cell(cur_x, row_y, Cell { ch: c, bg, fg, width: cw as u8, ..Default::default() });
                }
                cur_x += cw;
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        match key.code {
            KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                } else {
                    self.selected_idx = self.options.len().saturating_sub(1);
                }
                DialogResult::Pending
            }
            KeyCode::Down => {
                if self.selected_idx + 1 < self.options.len() {
                    self.selected_idx += 1;
                } else {
                    self.selected_idx = 0;
                }
                DialogResult::Pending
            }
            KeyCode::Enter => {
                if let Some((action_key, _)) = self.options.get(self.selected_idx) {
                    DialogResult::Ok(Action::FileContextMenuAction {
                        action: action_key,
                        path: self.target_path.clone(),
                        is_dir: self.is_dir,
                    })
                } else {
                    DialogResult::Cancel
                }
            }
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, _h: u16) -> DialogResult<Action> {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let (mx, my) = (mouse.column, mouse.row);
            if mx > x && mx < x + w - 1 {
                for i in 0..self.options.len() {
                    let row_y = y + 2 + i as u16;
                    if my == row_y {
                        self.selected_idx = i;
                        let (action_key, _) = &self.options[i];
                        return DialogResult::Ok(Action::FileContextMenuAction {
                            action: action_key,
                            path: self.target_path.clone(),
                            is_dir: self.is_dir,
                        });
                    }
                }
            }
        }
        DialogResult::Pending
    }
}

pub struct InputDialog {
    pub title: String,
    pub prompt: String,
    pub input_text: String,
}

impl InputDialog {
    pub fn new(title: String, prompt: String, initial_value: String) -> Self {
        Self {
            title,
            prompt,
            input_text: initial_value,
        }
    }
}

impl Dialog for InputDialog {
    fn title(&self) -> &str {
        &self.title
    }

    fn dimensions(&self) -> (u16, u16) {
        let title_w: u16 = self.title.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let prompt_w: u16 = self.prompt.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let input_w: u16 = self.input_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let w = ((prompt_w + input_w + 14).max(title_w + 12).max(52)).min(74);
        (w, 6)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);
        let input_bg = to_ct_color(theme.ui.panel_bg, theme);
        let input_fg = to_ct_color(theme.ui.panel_fg, theme);

        let label = format!("{}: ", self.prompt);
        let mut cur_lx = x + 2;
        let ly = y + 2;
        for c in label.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            renderer.set_cell(cur_lx, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, width: cw as u8, ..Default::default() });
            cur_lx += cw;
        }

        let input_x = cur_lx;
        let input_w = (x + w - 2).saturating_sub(input_x);
        for dx in 0..input_w {
            renderer.set_cell(input_x + dx, ly, Cell { ch: ' ', bg: input_bg, ..Default::default() });
        }
        let mut cur_tx = input_x;
        for c in self.input_text.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_tx + cw <= input_x + input_w {
                renderer.set_cell(cur_tx, ly, Cell { ch: c, bg: input_bg, fg: input_fg, width: cw as u8, ..Default::default() });
                cur_tx += cw;
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        match key.code {
            KeyCode::Char(c) => {
                self.input_text.push(c);
                DialogResult::Pending
            }
            KeyCode::Backspace => {
                self.input_text.pop();
                DialogResult::Pending
            }
            KeyCode::Enter => {
                let trimmed = self.input_text.trim().to_string();
                if !trimmed.is_empty() {
                    DialogResult::Ok(Action::InputName(trimmed))
                } else {
                    DialogResult::Cancel
                }
            }
            KeyCode::Esc => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _x: u16, _y: u16, _w: u16, _h: u16) -> DialogResult<Action> {
        DialogResult::Pending
    }

    fn cursor_pos(&self) -> Option<(u16, u16)> {
        let label_len: u16 = format!("{}: ", self.prompt).chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let text_len: u16 = self.input_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        Some((2 + label_len + text_len, 2))
    }
}

#[derive(Debug, Clone)]
pub struct DropdownState {
    pub row: usize,
    pub items: Vec<(String, String)>, // (id, display_label)
    pub selected_idx: usize,
    pub scroll_offset: usize,
}

pub struct SettingsDialog {
    pub config: zee_core::Config,
    pub themes: Vec<(String, String)>,
    pub languages: Vec<(String, String)>,
    pub selected_row: usize,
    pub selected_btn: usize,
    pub active_dropdown: Option<DropdownState>,
    pub i18n_title: String,
    pub i18n_theme: String,
    pub i18n_lang: String,
    pub i18n_sidebar_pos: String,
    pub i18n_left: String,
    pub i18n_right: String,
    pub i18n_tab_size: String,
    pub i18n_expand_tab: String,
    pub i18n_line_numbers: String,
    pub i18n_word_wrap: String,
    pub i18n_vi_mode: String,
    pub i18n_show_hidden: String,
    pub i18n_save: String,
    pub i18n_cancel: String,
}

impl SettingsDialog {
    pub fn new(config: zee_core::Config, themes: &[zee_core::theme::Theme], i18n: &zee_core::I18n) -> Self {
        let theme_list: Vec<(String, String)> = themes.iter().map(|t| {
            let id = t.meta.name.to_lowercase().replace(" ", "-");
            (id, t.meta.name.clone())
        }).collect();

        let lang_list: Vec<(String, String)> = zee_core::i18n::AVAILABLE_LANGUAGES.iter().map(|l| {
            let name = if l.id == "auto" {
                i18n.get("dialog.settings.language_auto").to_string()
            } else {
                l.name.to_string()
            };
            (l.id.to_string(), name)
        }).collect();

        Self {
            config,
            themes: theme_list,
            languages: lang_list,
            selected_row: 0,
            selected_btn: 0,
            active_dropdown: None,
            i18n_title: i18n.get("dialog.settings.title").to_string(),
            i18n_theme: i18n.get("dialog.settings.theme").to_string(),
            i18n_lang: i18n.get("dialog.settings.language").to_string(),
            i18n_sidebar_pos: i18n.get("dialog.settings.sidebar_position").to_string(),
            i18n_left: i18n.get("dialog.settings.sidebar_left").to_string(),
            i18n_right: i18n.get("dialog.settings.sidebar_right").to_string(),
            i18n_tab_size: i18n.get("dialog.settings.tab_size").to_string(),
            i18n_expand_tab: "Expand Tab (Spaces)".to_string(),
            i18n_line_numbers: i18n.get("menu.view.line_numbers").to_string(),
            i18n_word_wrap: i18n.get("menu.view.word_wrap").to_string(),
            i18n_vi_mode: i18n.get("menu.view.vi_mode").to_string(),
            i18n_show_hidden: i18n.get("sidebar.show_hidden").to_string(),
            i18n_save: i18n.get("dialog.save").to_string(),
            i18n_cancel: i18n.get("dialog.cancel").to_string(),
        }
    }

    pub fn open_dropdown(&mut self, row: usize) {
        match row {
            0 => {
                let cur_idx = self.themes.iter().position(|(id, _)| id == &self.config.theme).unwrap_or(0);
                let scroll_offset = if cur_idx >= 6 { cur_idx - 5 } else { 0 };
                self.active_dropdown = Some(DropdownState {
                    row: 0,
                    items: self.themes.clone(),
                    selected_idx: cur_idx,
                    scroll_offset,
                });
            }
            1 => {
                let cur_idx = self.languages.iter().position(|(id, _)| id == &self.config.language).unwrap_or(0);
                let scroll_offset = if cur_idx >= 6 { cur_idx - 5 } else { 0 };
                self.active_dropdown = Some(DropdownState {
                    row: 1,
                    items: self.languages.clone(),
                    selected_idx: cur_idx,
                    scroll_offset,
                });
            }
            2 => {
                let cur_idx = if self.config.sidebar_position == "right" { 1 } else { 0 };
                self.active_dropdown = Some(DropdownState {
                    row: 2,
                    items: vec![
                        ("left".to_string(), self.i18n_left.clone()),
                        ("right".to_string(), self.i18n_right.clone()),
                    ],
                    selected_idx: cur_idx,
                    scroll_offset: 0,
                });
            }
            3 => {
                let cur_idx = match self.config.tab_size {
                    2 => 0,
                    8 => 2,
                    _ => 1,
                };
                self.active_dropdown = Some(DropdownState {
                    row: 3,
                    items: vec![
                        ("2".to_string(), "2 spaces".to_string()),
                        ("4".to_string(), "4 spaces".to_string()),
                        ("8".to_string(), "8 spaces".to_string()),
                    ],
                    selected_idx: cur_idx,
                    scroll_offset: 0,
                });
            }
            _ => {}
        }
    }

    pub fn apply_dropdown_selection(&mut self) {
        if let Some(drop) = self.active_dropdown.take() {
            if let Some((id, _)) = drop.items.get(drop.selected_idx) {
                match drop.row {
                    0 => self.config.theme = id.clone(),
                    1 => self.config.language = id.clone(),
                    2 => self.config.sidebar_position = id.clone(),
                    3 => {
                        if let Ok(sz) = id.parse::<usize>() {
                            self.config.tab_size = sz;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn dropdown_bounds(&self, x: u16, y: u16, w: u16, h: u16) -> Option<(u16, u16, u16, u16, usize)> {
        let drop = self.active_dropdown.as_ref()?;
        let row_y = y + 2 + drop.row as u16;
        let visible_count = 6.min(drop.items.len());
        let drop_h = (visible_count as u16) + 2;
        let drop_w = 28.min(w.saturating_sub(4));
        let drop_x = (x + 25).min(x + w - drop_w - 1);
        let mut drop_y = row_y + 1;
        if drop_y + drop_h > y + h - 1 {
            drop_y = (y + h - 1).saturating_sub(drop_h);
        }
        Some((drop_x, drop_y, drop_w, drop_h, visible_count))
    }

    fn cycle_theme(&mut self, next: bool) {
        if self.themes.is_empty() { return; }
        let cur_idx = self.themes.iter().position(|(id, _)| id == &self.config.theme).unwrap_or(0);
        let next_idx = if next {
            (cur_idx + 1) % self.themes.len()
        } else {
            (cur_idx + self.themes.len() - 1) % self.themes.len()
        };
        self.config.theme = self.themes[next_idx].0.clone();
    }

    fn cycle_lang(&mut self, next: bool) {
        if self.languages.is_empty() { return; }
        let cur_idx = self.languages.iter().position(|(id, _)| id == &self.config.language).unwrap_or(0);
        let next_idx = if next {
            (cur_idx + 1) % self.languages.len()
        } else {
            (cur_idx + self.languages.len() - 1) % self.languages.len()
        };
        self.config.language = self.languages[next_idx].0.clone();
    }

    fn cycle_sidebar_pos(&mut self) {
        if self.config.sidebar_position == "left" {
            self.config.sidebar_position = "right".to_string();
        } else {
            self.config.sidebar_position = "left".to_string();
        }
    }

    fn cycle_tab_size(&mut self, next: bool) {
        let sizes = [2, 4, 8];
        let cur_idx = sizes.iter().position(|&s| s == self.config.tab_size).unwrap_or(1);
        let next_idx = if next {
            (cur_idx + 1) % sizes.len()
        } else {
            (cur_idx + sizes.len() - 1) % sizes.len()
        };
        self.config.tab_size = sizes[next_idx];
    }

    fn cycle_row(&mut self, row: usize, next: bool) {
        match row {
            0 => self.cycle_theme(next),
            1 => self.cycle_lang(next),
            2 => self.cycle_sidebar_pos(),
            3 => self.cycle_tab_size(next),
            4 => self.config.expand_tab = !self.config.expand_tab,
            5 => self.config.line_numbers = !self.config.line_numbers,
            6 => self.config.word_wrap = !self.config.word_wrap,
            7 => self.config.vi_mode = !self.config.vi_mode,
            8 => self.config.show_hidden = !self.config.show_hidden,
            _ => {}
        }
    }
}

impl Dialog for SettingsDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        (56, 14)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);
        let accent = to_ct_color(theme.ui.tab_active_fg, theme);
        let sel_bg = to_ct_color(theme.editor.selection, theme);
        let border_fg = to_ct_color(theme.ui.dialog_border, theme);

        let cur_theme_name = self.themes.iter().find(|(id, _)| id == &self.config.theme).map(|(_, n)| n.as_str()).unwrap_or(&self.config.theme);
        let cur_lang_name = self.languages.iter().find(|(id, _)| id == &self.config.language).map(|(_, n)| n.as_str()).unwrap_or(&self.config.language);
        let cur_sidebar_pos = if self.config.sidebar_position == "left" { &self.i18n_left } else { &self.i18n_right };

        let format_dropdown_box = |val: &str, max_inner_w: usize| -> String {
            let mut truncated = String::new();
            let mut cur_w = 0;
            for c in val.chars() {
                let cw = c.width().unwrap_or(1);
                if cur_w + cw > max_inner_w {
                    break;
                }
                truncated.push(c);
                cur_w += cw;
            }
            let pad = max_inner_w.saturating_sub(cur_w);
            format!("[ {}{} ▼ ]", truncated, " ".repeat(pad))
        };

        let rows: [(&str, String); 9] = [
            (&self.i18n_theme, format_dropdown_box(cur_theme_name, 16)),
            (&self.i18n_lang, format_dropdown_box(cur_lang_name, 16)),
            (&self.i18n_sidebar_pos, format_dropdown_box(cur_sidebar_pos, 16)),
            (&self.i18n_tab_size, format_dropdown_box(&format!("{} spaces", self.config.tab_size), 16)),
            (&self.i18n_expand_tab, if self.config.expand_tab { "[ ON ]".to_string() } else { "[ OFF ]".to_string() }),
            (&self.i18n_line_numbers, if self.config.line_numbers { "[ ON ]".to_string() } else { "[ OFF ]".to_string() }),
            (&self.i18n_word_wrap, if self.config.word_wrap { "[ ON ]".to_string() } else { "[ OFF ]".to_string() }),
            (&self.i18n_vi_mode, if self.config.vi_mode { "[ ON ]".to_string() } else { "[ OFF ]".to_string() }),
            (&self.i18n_show_hidden, if self.config.show_hidden { "[ ON ]".to_string() } else { "[ OFF ]".to_string() }),
        ];

        for (i, (label, val)) in rows.iter().enumerate() {
            let row_y = y + 2 + i as u16;
            let is_sel = self.selected_row == i;
            let row_bg = if is_sel { sel_bg } else { dialog_bg };
            let row_fg = if is_sel { accent } else { dialog_fg };

            // Fill row background
            for dx in 1..w - 1 {
                renderer.set_cell(x + dx, row_y, Cell { ch: ' ', bg: row_bg, fg: row_fg, ..Default::default() });
            }

            // Draw label
            let mut cur_x = x + 3;
            for c in label.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                renderer.set_cell(cur_x, row_y, Cell { ch: c, bg: row_bg, fg: row_fg, bold: is_sel, width: cw as u8, ..Default::default() });
                cur_x += cw;
            }

            // Draw value
            let mut val_x = x + 26;
            for c in val.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                renderer.set_cell(val_x, row_y, Cell { ch: c, bg: row_bg, fg: row_fg, bold: is_sel, width: cw as u8, ..Default::default() });
                val_x += cw;
            }
        }

        // Row 9: Buttons
        let btn_y = y + 2 + 9 as u16;
        let is_btn_row = self.selected_row == 9;

        // Button 1: Save
        let save_str = format!(" [ {} ] ", self.i18n_save);
        let save_x = x + 8;
        let save_sel = is_btn_row && self.selected_btn == 0;
        let mut bx = save_x;
        for c in save_str.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            renderer.set_cell(bx, btn_y, Cell {
                ch: c,
                bg: if save_sel { sel_bg } else { dialog_bg },
                fg: if save_sel { accent } else { border_fg },
                bold: save_sel,
                width: cw as u8,
                ..Default::default()
            });
            bx += cw;
        }

        // Button 2: Cancel
        let cancel_str = format!(" [ {} ] ", self.i18n_cancel);
        let cancel_x = x + 32;
        let cancel_sel = is_btn_row && self.selected_btn == 1;
        let mut cx_btn = cancel_x;
        for c in cancel_str.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            renderer.set_cell(cx_btn, btn_y, Cell {
                ch: c,
                bg: if cancel_sel { sel_bg } else { dialog_bg },
                fg: if cancel_sel { accent } else { border_fg },
                bold: cancel_sel,
                width: cw as u8,
                ..Default::default()
            });
            cx_btn += cw;
        }

        // Render pseudo pull-down popup overlay if open
        if let Some((drop_x, drop_y, drop_w, drop_h, visible_count)) = self.dropdown_bounds(x, y, w, h) {
            if let Some(ref drop) = self.active_dropdown {
                let popup_bg = Color::AnsiValue(236);
                let popup_border_fg = Color::AnsiValue(248);
                let active_row_bg = Color::AnsiValue(24);
                let active_row_fg = Color::White;
                let normal_item_fg = Color::AnsiValue(252);
                let checkmark_fg = Color::AnsiValue(114);

                // Clear popup background
                for dy in 0..drop_h {
                    for dx in 0..drop_w {
                        renderer.set_cell(drop_x + dx, drop_y + dy, Cell {
                            ch: ' ',
                            bg: popup_bg,
                            fg: normal_item_fg,
                            ..Default::default()
                        });
                    }
                }

                // Top border: ┌──────────── ▲ ────────────┐
                renderer.set_cell(drop_x, drop_y, Cell { ch: '┌', fg: popup_border_fg, bg: popup_bg, ..Default::default() });
                for dx in 1..drop_w - 1 {
                    let ch = if drop.scroll_offset > 0 && dx == drop_w / 2 { '▲' } else { '─' };
                    renderer.set_cell(drop_x + dx, drop_y, Cell { ch, fg: popup_border_fg, bg: popup_bg, ..Default::default() });
                }
                renderer.set_cell(drop_x + drop_w - 1, drop_y, Cell { ch: '┐', fg: popup_border_fg, bg: popup_bg, ..Default::default() });

                // Items
                let cur_config_val = match drop.row {
                    0 => self.config.theme.as_str(),
                    1 => self.config.language.as_str(),
                    2 => self.config.sidebar_position.as_str(),
                    _ => "",
                };

                for vi in 0..visible_count {
                    let item_idx = drop.scroll_offset + vi;
                    if item_idx >= drop.items.len() { break; }
                    let (id, label) = &drop.items[item_idx];
                    let item_y = drop_y + 1 + vi as u16;
                    let is_sel = item_idx == drop.selected_idx;
                    let is_current = if drop.row == 3 {
                        id.parse::<usize>().ok() == Some(self.config.tab_size)
                    } else {
                        id == cur_config_val
                    };

                    let row_bg = if is_sel { active_row_bg } else { popup_bg };
                    let row_fg = if is_sel { active_row_fg } else { normal_item_fg };

                    for dx in 1..drop_w - 1 {
                        renderer.set_cell(drop_x + dx, item_y, Cell {
                            ch: ' ',
                            bg: row_bg,
                            fg: row_fg,
                            ..Default::default()
                        });
                    }

                    // Selection cursor prefix
                    let prefix = if is_sel { "▶ " } else { "  " };
                    let mut cur_ix = drop_x + 1;
                    for c in prefix.chars() {
                        let cw = c.width().unwrap_or(1) as u16;
                        renderer.set_cell(cur_ix, item_y, Cell { ch: c, bg: row_bg, fg: row_fg, bold: is_sel, width: cw as u8, ..Default::default() });
                        cur_ix += cw;
                    }

                    // Label
                    let max_label_w = drop_w.saturating_sub(6);
                    let mut label_w = 0;
                    for c in label.chars() {
                        let cw = c.width().unwrap_or(1) as u16;
                        if label_w + cw > max_label_w { break; }
                        renderer.set_cell(cur_ix, item_y, Cell { ch: c, bg: row_bg, fg: row_fg, bold: is_sel, width: cw as u8, ..Default::default() });
                        cur_ix += cw;
                        label_w += cw;
                    }

                    // Checkmark if current config value
                    if is_current {
                        renderer.set_cell(drop_x + drop_w - 3, item_y, Cell {
                            ch: '✓',
                            bg: row_bg,
                            fg: if is_sel { active_row_fg } else { checkmark_fg },
                            bold: true,
                            width: 1,
                            ..Default::default()
                        });
                    }

                    // Side borders
                    renderer.set_cell(drop_x, item_y, Cell { ch: '│', fg: popup_border_fg, bg: popup_bg, ..Default::default() });
                    renderer.set_cell(drop_x + drop_w - 1, item_y, Cell { ch: '│', fg: popup_border_fg, bg: popup_bg, ..Default::default() });
                }

                // Bottom border: └──────────── ▼ ────────────┘
                let bot_y = drop_y + drop_h - 1;
                let has_more_below = drop.scroll_offset + visible_count < drop.items.len();
                renderer.set_cell(drop_x, bot_y, Cell { ch: '└', fg: popup_border_fg, bg: popup_bg, ..Default::default() });
                for dx in 1..drop_w - 1 {
                    let ch = if has_more_below && dx == drop_w / 2 { '▼' } else { '─' };
                    renderer.set_cell(drop_x + dx, bot_y, Cell { ch, fg: popup_border_fg, bg: popup_bg, ..Default::default() });
                }
                renderer.set_cell(drop_x + drop_w - 1, bot_y, Cell { ch: '┘', fg: popup_border_fg, bg: popup_bg, ..Default::default() });
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        // If dropdown is open, process dropdown keyboard interaction
        if let Some(ref mut drop) = self.active_dropdown {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                    if drop.selected_idx > 0 {
                        drop.selected_idx -= 1;
                        if drop.selected_idx < drop.scroll_offset {
                            drop.scroll_offset = drop.selected_idx;
                        }
                    }
                    return DialogResult::Pending;
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                    if drop.selected_idx + 1 < drop.items.len() {
                        drop.selected_idx += 1;
                        let visible_count = 6.min(drop.items.len());
                        if drop.selected_idx >= drop.scroll_offset + visible_count {
                            drop.scroll_offset = drop.selected_idx + 1 - visible_count;
                        }
                    }
                    return DialogResult::Pending;
                }
                KeyCode::Enter => {
                    self.apply_dropdown_selection();
                    return DialogResult::Pending;
                }
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.active_dropdown = None;
                    return DialogResult::Pending;
                }
                _ => return DialogResult::Pending,
            }
        }

        // When dropdown is closed: normal settings dialog navigation
        match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.selected_row > 0 {
                    self.selected_row -= 1;
                } else {
                    self.selected_row = 9;
                }
                DialogResult::Pending
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if self.selected_row < 9 {
                    self.selected_row += 1;
                } else {
                    self.selected_row = 0;
                }
                DialogResult::Pending
            }
            KeyCode::Tab => {
                if self.selected_row < 9 {
                    self.selected_row += 1;
                } else {
                    self.selected_row = 0;
                }
                DialogResult::Pending
            }
            KeyCode::BackTab => {
                if self.selected_row > 0 {
                    self.selected_row -= 1;
                } else {
                    self.selected_row = 9;
                }
                DialogResult::Pending
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                if self.selected_row == 9 {
                    self.selected_btn = 0;
                } else {
                    self.cycle_row(self.selected_row, false);
                }
                DialogResult::Pending
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                if self.selected_row == 9 {
                    self.selected_btn = 1;
                } else {
                    self.cycle_row(self.selected_row, true);
                }
                DialogResult::Pending
            }
            KeyCode::Enter => {
                if self.selected_row < 4 {
                    // Open pseudo pull-down menu for multi-choice settings
                    self.open_dropdown(self.selected_row);
                    DialogResult::Pending
                } else if self.selected_row < 9 {
                    // Toggle boolean settings
                    self.cycle_row(self.selected_row, true);
                    DialogResult::Pending
                } else {
                    // Button row
                    if self.selected_btn == 0 {
                        DialogResult::Ok(Action::SaveSettings(Box::new(self.config.clone())))
                    } else {
                        DialogResult::Cancel
                    }
                }
            }
            KeyCode::Char(' ') => {
                if self.selected_row < 4 {
                    self.open_dropdown(self.selected_row);
                    DialogResult::Pending
                } else if self.selected_row < 9 {
                    self.cycle_row(self.selected_row, true);
                    DialogResult::Pending
                } else {
                    if self.selected_btn == 0 {
                        DialogResult::Ok(Action::SaveSettings(Box::new(self.config.clone())))
                    } else {
                        DialogResult::Cancel
                    }
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                DialogResult::Ok(Action::SaveSettings(Box::new(self.config.clone())))
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => DialogResult::Cancel,
            _ => DialogResult::Pending,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, x: u16, y: u16, w: u16, h: u16) -> DialogResult<Action> {
        let (mx, my) = (mouse.column, mouse.row);

        // If dropdown is open, process dropdown mouse events
        if let Some((drop_x, drop_y, drop_w, drop_h, visible_count)) = self.dropdown_bounds(x, y, w, h) {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if mx >= drop_x && mx < drop_x + drop_w && my >= drop_y && my < drop_y + drop_h {
                        if my > drop_y && my < drop_y + drop_h - 1 {
                            let clicked_visible = (my - (drop_y + 1)) as usize;
                            if let Some(ref mut drop) = self.active_dropdown {
                                let clicked_idx = drop.scroll_offset + clicked_visible;
                                if clicked_idx < drop.items.len() {
                                    drop.selected_idx = clicked_idx;
                                    self.apply_dropdown_selection();
                                    return DialogResult::Pending;
                                }
                            }
                        } else if my == drop_y {
                            if let Some(ref mut drop) = self.active_dropdown {
                                if drop.scroll_offset > 0 {
                                    drop.scroll_offset -= 1;
                                    drop.selected_idx = drop.selected_idx.min(drop.scroll_offset + visible_count - 1);
                                }
                            }
                            return DialogResult::Pending;
                        } else if my == drop_y + drop_h - 1 {
                            if let Some(ref mut drop) = self.active_dropdown {
                                if drop.scroll_offset + visible_count < drop.items.len() {
                                    drop.scroll_offset += 1;
                                    drop.selected_idx = drop.selected_idx.max(drop.scroll_offset);
                                }
                            }
                            return DialogResult::Pending;
                        }
                    } else {
                        // Click outside closes dropdown
                        self.active_dropdown = None;
                        return DialogResult::Pending;
                    }
                }
                MouseEventKind::ScrollUp => {
                    if let Some(ref mut drop) = self.active_dropdown {
                        if drop.selected_idx > 0 {
                            drop.selected_idx -= 1;
                            if drop.selected_idx < drop.scroll_offset {
                                drop.scroll_offset = drop.selected_idx;
                            }
                        }
                    }
                    return DialogResult::Pending;
                }
                MouseEventKind::ScrollDown => {
                    if let Some(ref mut drop) = self.active_dropdown {
                        if drop.selected_idx + 1 < drop.items.len() {
                            drop.selected_idx += 1;
                            if drop.selected_idx >= drop.scroll_offset + visible_count {
                                drop.scroll_offset = drop.selected_idx + 1 - visible_count;
                            }
                        }
                    }
                    return DialogResult::Pending;
                }
                _ => return DialogResult::Pending,
            }
        }

        // When dropdown is closed: normal settings dialog mouse interaction
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if mx >= x && mx < x + w && my >= y && my < y + h {
                    let rel_y = my.saturating_sub(y);
                    let rel_x = mx.saturating_sub(x);
                    if (2..=10).contains(&rel_y) {
                        let row = (rel_y - 2) as usize;
                        self.selected_row = row;
                        if row < 2 {
                            // Theme or Language: open dropdown
                            self.open_dropdown(row);
                        } else if row < 4 {
                            let mid_x = w / 2;
                            if rel_x < mid_x {
                                self.cycle_row(row, false);
                            } else {
                                self.cycle_row(row, true);
                            }
                        } else {
                            self.cycle_row(row, true);
                        }
                        return DialogResult::Pending;
                    } else if rel_y == 11 {
                        self.selected_row = 9;
                        let mid_x = w / 2;
                        if rel_x < mid_x {
                            self.selected_btn = 0;
                            return DialogResult::Ok(Action::SaveSettings(Box::new(self.config.clone())));
                        } else {
                            self.selected_btn = 1;
                            return DialogResult::Cancel;
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if self.selected_row > 0 {
                    self.selected_row -= 1;
                } else {
                    self.selected_row = 9;
                }
                return DialogResult::Pending;
            }
            MouseEventKind::ScrollDown => {
                if self.selected_row < 9 {
                    self.selected_row += 1;
                } else {
                    self.selected_row = 0;
                }
                return DialogResult::Pending;
            }
            _ => {}
        }
        DialogResult::Pending
    }
}

pub enum GDriveMsg {
    ListDone(Result<Vec<zee_core::gdrive::GDriveItem>, String>),
    AuthDone(Result<(), String>),
    DownloadDone(Result<PathBuf, String>),
}

pub struct GoogleDriveDialog {
    pub items: Vec<zee_core::gdrive::GDriveItem>,
    pub selected_idx: usize,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub current_folder_id: Option<String>,
    pub folder_stack: Vec<(Option<String>, String)>,
    pub search_query: String,
    pub selected_btn: usize, // 0: Open, 1: Cancel, 2: Sign Out
    pub rx: std::sync::mpsc::Receiver<GDriveMsg>,
    pub tx: std::sync::mpsc::Sender<GDriveMsg>,

    // Setup state
    pub show_setup: bool,
    pub client_id_input: String,
    pub client_secret_input: String,
    pub setup_active_field: usize, // 0 for client_id, 1 for client_secret

    // i18n
    pub i18n_title: String,
    pub i18n_connect: String,
    pub i18n_connecting: String,
    pub i18n_sign_out: String,
    pub i18n_open: String,
    pub i18n_cancel: String,
    pub i18n_search_placeholder: String,
}

impl GoogleDriveDialog {
    pub fn new(i18n: &zee_core::I18n) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut dialog = Self {
            items: Vec::new(),
            selected_idx: 0,
            is_loading: false,
            error_message: None,
            current_folder_id: None,
            folder_stack: vec![(None, "My Drive".to_string())],
            search_query: String::new(),
            selected_btn: 0,
            rx,
            tx,
            show_setup: !zee_core::gdrive::GDriveManager::has_configured_credentials(),
            client_id_input: zee_core::gdrive::GDriveManager::get_client_id().unwrap_or_default(),
            client_secret_input: zee_core::gdrive::GDriveManager::get_client_secret().unwrap_or_default(),
            setup_active_field: 0,
            i18n_title: i18n.get("gdrive.title").to_string(),
            i18n_connect: i18n.get("gdrive.connect").to_string(),
            i18n_connecting: i18n.get("gdrive.connecting").to_string(),
            i18n_sign_out: i18n.get("gdrive.sign_out").to_string(),
            i18n_open: i18n.get("gdrive.open").to_string(),
            i18n_cancel: i18n.get("gdrive.cancel").to_string(),
            i18n_search_placeholder: i18n.get("gdrive.search_placeholder").to_string(),
        };

        if zee_core::gdrive::GDriveManager::is_authenticated() {
            dialog.fetch_files();
        }
        dialog
    }

    pub fn poll_messages(&mut self) -> Option<PathBuf> {
        let mut downloaded_path = None;
        while let Ok(msg) = self.rx.try_recv() {
            self.is_loading = false;
            match msg {
                GDriveMsg::ListDone(res) => match res {
                    Ok(items) => {
                        self.items = items;
                        self.selected_idx = 0;
                        self.error_message = None;
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                    }
                },
                GDriveMsg::AuthDone(res) => match res {
                    Ok(()) => {
                        self.current_folder_id = None;
                        self.folder_stack = vec![(None, "My Drive".to_string())];
                        self.search_query.clear();
                        self.fetch_files();
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                    }
                },
                GDriveMsg::DownloadDone(res) => match res {
                    Ok(path) => {
                        downloaded_path = Some(path);
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                    }
                },
            }
        }
        downloaded_path
    }

    pub fn fetch_files(&mut self) {
        if self.is_loading {
            return;
        }
        self.is_loading = true;
        self.error_message = None;

        let tx = self.tx.clone();
        let parent_id = self.current_folder_id.clone();
        let search = if self.search_query.trim().is_empty() {
            None
        } else {
            Some(self.search_query.trim().to_string())
        };

        std::thread::spawn(move || {
            let res = zee_core::gdrive::GDriveManager::list_files(parent_id.as_deref(), search.as_deref())
                .map_err(|e| e.to_string());
            let _ = tx.send(GDriveMsg::ListDone(res));
        });
    }

    pub fn start_auth(&mut self) {
        if self.is_loading {
            return;
        }
        self.is_loading = true;
        self.error_message = None;

        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = zee_core::gdrive::GDriveManager::start_oauth_flow()
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = tx.send(GDriveMsg::AuthDone(res));
        });
    }

    pub fn sign_out(&mut self) {
        let _ = zee_core::gdrive::GDriveManager::sign_out();
        self.items.clear();
        self.selected_idx = 0;
        self.error_message = None;
    }

    pub fn open_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let item = match self.items.get(self.selected_idx) {
            Some(it) => it.clone(),
            None => return,
        };

        if item.is_folder {
            self.current_folder_id = Some(item.id.clone());
            self.folder_stack.push((Some(item.id), item.name));
            self.search_query.clear();
            self.fetch_files();
        } else {
            self.is_loading = true;
            self.error_message = None;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let res = zee_core::gdrive::GDriveManager::download_and_cache(&item)
                    .map_err(|e| e.to_string());
                let _ = tx.send(GDriveMsg::DownloadDone(res));
            });
        }
    }
}

impl Dialog for GoogleDriveDialog {
    fn title(&self) -> &str {
        &self.i18n_title
    }

    fn dimensions(&self) -> (u16, u16) {
        (76, 22)
    }

    fn render(&self, renderer: &mut Renderer, theme: &zee_core::theme::Theme, x: u16, y: u16, w: u16, h: u16) {
        render_base_dialog(renderer, theme, self.title(), x, y, w, h);

        let dialog_bg = to_ct_color(theme.ui.dialog_bg, theme);
        let dialog_fg = to_ct_color(theme.ui.panel_fg, theme);
        let active_bg = to_ct_color(theme.ui.button_active_bg, theme);
        let active_fg = to_ct_color(theme.ui.button_active_fg, theme);

        let is_auth = zee_core::gdrive::GDriveManager::is_authenticated();

        if self.show_setup || !zee_core::gdrive::GDriveManager::has_configured_credentials() {
            // Setup & Step-by-Step Guide View in TUI
            let title = "Google Drive API Setup (Required)";
            let tx = x + (w.saturating_sub(title.len() as u16)) / 2;
            for (i, c) in title.chars().enumerate() {
                renderer.set_cell(tx + i as u16, y + 2, Cell { ch: c, bg: dialog_bg, fg: active_fg, ..Default::default() });
            }

            let s1 = "1. Enable 'Google Drive API' in Google Cloud Console";
            let s2 = "2. Create Credentials -> OAuth client ID -> Desktop App";
            let s3 = "3. Enter Client ID and Secret below (Tab to switch fields)";
            for (i, c) in s1.chars().enumerate() {
                renderer.set_cell(x + 3 + i as u16, y + 4, Cell { ch: c, bg: dialog_bg, fg: to_ct_color(theme.syntax.comment.unwrap_or(theme.editor.line_number), theme), ..Default::default() });
            }
            for (i, c) in s2.chars().enumerate() {
                renderer.set_cell(x + 3 + i as u16, y + 5, Cell { ch: c, bg: dialog_bg, fg: to_ct_color(theme.syntax.comment.unwrap_or(theme.editor.line_number), theme), ..Default::default() });
            }
            for (i, c) in s3.chars().enumerate() {
                renderer.set_cell(x + 3 + i as u16, y + 6, Cell { ch: c, bg: dialog_bg, fg: to_ct_color(theme.syntax.comment.unwrap_or(theme.editor.line_number), theme), ..Default::default() });
            }

            // Client ID Field
            let id_lbl = "Client ID: ";
            for (i, c) in id_lbl.chars().enumerate() {
                renderer.set_cell(x + 3 + i as u16, y + 8, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
            }
            let id_box_x = x + 3 + id_lbl.len() as u16;
            let id_box_w = (w.saturating_sub(id_lbl.len() as u16 + 6)).max(10);
            let id_bg = if self.setup_active_field == 0 { to_ct_color(theme.editor.selection, theme) } else { to_ct_color(theme.ui.panel_bg, theme) };
            for dx in 0..id_box_w {
                renderer.set_cell(id_box_x + dx, y + 8, Cell { ch: ' ', bg: id_bg, ..Default::default() });
            }
            let display_id = if self.client_id_input.is_empty() { "(paste client id here)" } else { &self.client_id_input };
            for (i, c) in display_id.chars().take(id_box_w as usize).enumerate() {
                renderer.set_cell(id_box_x + i as u16, y + 8, Cell { ch: c, bg: id_bg, fg: dialog_fg, ..Default::default() });
            }

            // Client Secret Field
            let sec_lbl = "Client Secret: ";
            for (i, c) in sec_lbl.chars().enumerate() {
                renderer.set_cell(x + 3 + i as u16, y + 10, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
            }
            let sec_box_x = x + 3 + sec_lbl.len() as u16;
            let sec_box_w = (w.saturating_sub(sec_lbl.len() as u16 + 6)).max(10);
            let sec_bg = if self.setup_active_field == 1 { to_ct_color(theme.editor.selection, theme) } else { to_ct_color(theme.ui.panel_bg, theme) };
            for dx in 0..sec_box_w {
                renderer.set_cell(sec_box_x + dx, y + 10, Cell { ch: ' ', bg: sec_bg, ..Default::default() });
            }
            let masked_sec = "•".repeat(self.client_secret_input.len().min(sec_box_w as usize));
            let display_sec = if self.client_secret_input.is_empty() { "(paste client secret here)" } else { &masked_sec };
            for (i, c) in display_sec.chars().take(sec_box_w as usize).enumerate() {
                renderer.set_cell(sec_box_x + i as u16, y + 10, Cell { ch: c, bg: sec_bg, fg: dialog_fg, ..Default::default() });
            }

            // Save and Connect button
            let btn_str = "[ Save & Connect (Enter) ]";
            let bx = x + (w.saturating_sub(btn_str.len() as u16)) / 2;
            let by = y + 13;
            for (i, c) in btn_str.chars().enumerate() {
                renderer.set_cell(bx + i as u16, by, Cell { ch: c, bg: active_bg, fg: active_fg, ..Default::default() });
            }

            if let Some(ref err) = self.error_message {
                let err_text = format!("Error: {}", err);
                let ex = x + 3;
                let ey = y + 16;
                for (i, c) in err_text.chars().take((w - 6) as usize).enumerate() {
                    renderer.set_cell(ex + i as u16, ey, Cell { ch: c, bg: dialog_bg, fg: Color::Red, ..Default::default() });
                }
            }
            return;
        }

        if !is_auth {
            // Not connected view
            let msg = "Connect Google Drive to edit cloud files";
            let mx = x + (w.saturating_sub(msg.len() as u16)) / 2;
            let my = y + 6;
            for (i, c) in msg.chars().enumerate() {
                renderer.set_cell(mx + i as u16, my, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
            }

            let btn_label = if self.is_loading { &self.i18n_connecting } else { &self.i18n_connect };
            let btn_text = format!("[ {} ]", btn_label);
            let bx = x + (w.saturating_sub(btn_text.len() as u16)) / 2;
            let by = y + 10;
            let btn_bg = if self.selected_btn == 0 { active_bg } else { to_ct_color(theme.ui.status_bar_bg, theme) };
            let btn_fg = if self.selected_btn == 0 { active_fg } else { to_ct_color(theme.ui.status_bar_fg, theme) };

            for (i, c) in btn_text.chars().enumerate() {
                renderer.set_cell(bx + i as u16, by, Cell { ch: c, bg: btn_bg, fg: btn_fg, ..Default::default() });
            }

            let cfg_str = "[ Configure API Keys (c) ]";
            let cx_pos = x + (w.saturating_sub(cfg_str.len() as u16)) / 2;
            let cy_pos = y + 13;
            for (i, c) in cfg_str.chars().enumerate() {
                renderer.set_cell(cx_pos + i as u16, cy_pos, Cell { ch: c, bg: dialog_bg, fg: to_ct_color(theme.syntax.comment.unwrap_or(theme.editor.line_number), theme), ..Default::default() });
            }

            if let Some(ref err) = self.error_message {
                let err_text = format!("Error: {}", err);
                let ex = x + 3;
                let ey = y + 15;
                for (i, c) in err_text.chars().take((w - 6) as usize).enumerate() {
                    renderer.set_cell(ex + i as u16, ey, Cell { ch: c, bg: dialog_bg, fg: Color::Red, ..Default::default() });
                }
            }
            return;
        }

        // Top info: Breadcrumb on left, Account/Sign Out on right
        let breadcrumb = self.folder_stack.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(" / ");
        let mut cur_bx = x + 2;
        for c in breadcrumb.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_bx + cw < x + w / 2 {
                renderer.set_cell(cur_bx, y + 2, Cell { ch: c, bg: dialog_bg, fg: to_ct_color(theme.syntax.keyword.unwrap_or(theme.editor.cursor), theme), width: cw as u8, ..Default::default() });
                cur_bx += cw;
            } else {
                break;
            }
        }

        if let Some(email) = zee_core::gdrive::GDriveManager::connected_account_email() {
            let email_str = format!("({})", email);
            let ex = x + w - 2 - email_str.len() as u16;
            for (i, c) in email_str.chars().enumerate() {
                renderer.set_cell(ex + i as u16, y + 2, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
            }
        }

        // Search query row
        let search_y = y + 3;
        let search_label = "Search: ";
        for (i, c) in search_label.chars().enumerate() {
            renderer.set_cell(x + 2 + i as u16, search_y, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
        }
        let search_box_x = x + 2 + search_label.len() as u16;
        let search_box_w = (w.saturating_sub(search_label.len() as u16 + 4)).min(40);
        for dx in 0..search_box_w {
            renderer.set_cell(search_box_x + dx, search_y, Cell { ch: ' ', bg: to_ct_color(theme.ui.panel_bg, theme), ..Default::default() });
        }
        let display_search = if self.search_query.is_empty() { &self.i18n_search_placeholder } else { &self.search_query };
        let search_fg = if self.search_query.is_empty() { to_ct_color(theme.editor.line_number, theme) } else { to_ct_color(theme.ui.panel_fg, theme) };
        for (i, c) in display_search.chars().take(search_box_w as usize).enumerate() {
            renderer.set_cell(search_box_x + i as u16, search_y, Cell { ch: c, bg: to_ct_color(theme.ui.panel_bg, theme), fg: search_fg, ..Default::default() });
        }

        // File list area: rows from y+5 to y+h-4
        let list_top = y + 5;
        let list_height = (h.saturating_sub(9)).max(1);

        if self.is_loading {
            let loading_str = "Loading Google Drive...";
            let lx = x + (w.saturating_sub(loading_str.len() as u16)) / 2;
            let ly = list_top + list_height / 2;
            for (i, c) in loading_str.chars().enumerate() {
                renderer.set_cell(lx + i as u16, ly, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
            }
        } else if self.items.is_empty() {
            let empty_str = "No files found in folder";
            let ex = x + (w.saturating_sub(empty_str.len() as u16)) / 2;
            let ey = list_top + list_height / 2;
            for (i, c) in empty_str.chars().enumerate() {
                renderer.set_cell(ex + i as u16, ey, Cell { ch: c, bg: dialog_bg, fg: dialog_fg, ..Default::default() });
            }
        } else {
            let start_idx = if self.selected_idx >= list_height as usize {
                self.selected_idx - list_height as usize + 1
            } else {
                0
            };

            for row in 0..list_height {
                let item_idx = start_idx + row as usize;
                let iy = list_top + row;
                if let Some(item) = self.items.get(item_idx) {
                    let is_sel = item_idx == self.selected_idx;
                    let sel_bg = to_ct_color(theme.editor.selection, theme);
                    let (bg, fg) = if is_sel { (sel_bg, dialog_fg) } else { (dialog_bg, dialog_fg) };

                    // Clear line
                    for dx in 2..w - 2 {
                        renderer.set_cell(x + dx, iy, Cell { ch: ' ', bg, ..Default::default() });
                    }

                    // Icon + Name
                    let icon = if item.is_folder { "📁 " } else { "📄 " };
                    let mut cur_x = x + 3;
                    for c in icon.chars() {
                        renderer.set_cell(cur_x, iy, Cell { ch: c, bg, fg, ..Default::default() });
                        cur_x += 1;
                    }

                    for c in item.name.chars() {
                        let cw = c.width().unwrap_or(0) as u16;
                        if cur_x + cw < x + w - 16 {
                            renderer.set_cell(cur_x, iy, Cell { ch: c, bg, fg, width: cw as u8, ..Default::default() });
                            cur_x += cw;
                        } else {
                            break;
                        }
                    }

                    // Size
                    if let Some(sz) = item.size {
                        let sz_str = if sz < 1024 {
                            format!("{} B", sz)
                        } else if sz < 1024 * 1024 {
                            format!("{:.1} KB", sz as f64 / 1024.0)
                        } else {
                            format!("{:.1} MB", sz as f64 / (1024.0 * 1024.0))
                        };
                        let sx = x + w - 14;
                        for (i, c) in sz_str.chars().enumerate() {
                            renderer.set_cell(sx + i as u16, iy, Cell { ch: c, bg, fg, ..Default::default() });
                        }
                    }
                }
            }
        }

        // Bottom error / status
        if let Some(ref err) = self.error_message {
            let ey = y + h - 3;
            for (i, c) in err.chars().take((w - 6) as usize).enumerate() {
                renderer.set_cell(x + 3 + i as u16, ey, Cell { ch: c, bg: dialog_bg, fg: Color::Red, ..Default::default() });
            }
        }

        // Bottom buttons: Open, Cancel, Sign Out
        let buttons = [&self.i18n_open, &self.i18n_cancel, &self.i18n_sign_out];
        let by = y + h - 2;
        let mut bx = x + w - 35;
        for (idx, btn) in buttons.iter().enumerate() {
            let is_sel = idx == self.selected_btn;
            let (bbg, bfg) = if is_sel {
                (active_bg, active_fg)
            } else {
                (to_ct_color(theme.ui.status_bar_bg, theme), to_ct_color(theme.ui.status_bar_fg, theme))
            };
            let text = format!("[ {} ]", btn);
            for c in text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                renderer.set_cell(bx, by, Cell { ch: c, bg: bbg, fg: bfg, width: cw as u8, ..Default::default() });
                bx += cw;
            }
            bx += 2;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> DialogResult<Action> {
        if let Some(path) = self.poll_messages() {
            return DialogResult::Ok(Action::ConfirmPath(path));
        }

        if self.show_setup || !zee_core::gdrive::GDriveManager::has_configured_credentials() {
            match key.code {
                KeyCode::Esc => {
                    if zee_core::gdrive::GDriveManager::has_configured_credentials() {
                        self.show_setup = false;
                        DialogResult::Pending
                    } else {
                        DialogResult::Cancel
                    }
                }
                KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
                    self.setup_active_field = (self.setup_active_field + 1) % 2;
                    DialogResult::Pending
                }
                KeyCode::Backspace => {
                    if self.setup_active_field == 0 {
                        self.client_id_input.pop();
                    } else {
                        self.client_secret_input.pop();
                    }
                    DialogResult::Pending
                }
                KeyCode::Enter => {
                    let id = self.client_id_input.trim();
                    let sec = self.client_secret_input.trim();
                    if id.is_empty() {
                        self.error_message = Some("Client ID is required".to_string());
                    } else {
                        let _ = zee_core::config::Config::save_gdrive_credentials(id, sec);
                        self.show_setup = false;
                        self.start_auth();
                    }
                    DialogResult::Pending
                }
                KeyCode::Char(c) if key.modifiers.is_empty() => {
                    if self.setup_active_field == 0 {
                        self.client_id_input.push(c);
                    } else {
                        self.client_secret_input.push(c);
                    }
                    DialogResult::Pending
                }
                _ => DialogResult::Pending,
            }
        } else if !zee_core::gdrive::GDriveManager::is_authenticated() {
            match key.code {
                KeyCode::Esc => DialogResult::Cancel,
                KeyCode::Enter => {
                    self.start_auth();
                    DialogResult::Pending
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    self.show_setup = true;
                    DialogResult::Pending
                }
                _ => DialogResult::Pending,
            }
        } else {
            match key.code {
                KeyCode::Esc => DialogResult::Cancel,
                KeyCode::Up => {
                    self.selected_idx = self.selected_idx.saturating_sub(1);
                    DialogResult::Pending
                }
                KeyCode::Down => {
                    if !self.items.is_empty() {
                        self.selected_idx = (self.selected_idx + 1).min(self.items.len() - 1);
                    }
                    DialogResult::Pending
                }
                KeyCode::Left | KeyCode::BackTab => {
                    self.selected_btn = (self.selected_btn + 2) % 3;
                    DialogResult::Pending
                }
                KeyCode::Right | KeyCode::Tab => {
                    self.selected_btn = (self.selected_btn + 1) % 3;
                    DialogResult::Pending
                }
                KeyCode::Backspace => {
                    if !self.search_query.is_empty() {
                        self.search_query.pop();
                        self.fetch_files();
                    } else if self.folder_stack.len() > 1 {
                        self.folder_stack.pop();
                        self.current_folder_id = self.folder_stack.last().and_then(|(id, _)| id.clone());
                        self.fetch_files();
                    }
                    DialogResult::Pending
                }
                KeyCode::Enter => {
                    if self.selected_btn == 0 {
                        self.open_selected();
                    } else if self.selected_btn == 1 {
                        return DialogResult::Cancel;
                    } else if self.selected_btn == 2 {
                        self.sign_out();
                    }
                    DialogResult::Pending
                }
                KeyCode::Char(c) if key.modifiers.is_empty() => {
                    self.search_query.push(c);
                    self.fetch_files();
                    DialogResult::Pending
                }
                _ => DialogResult::Pending,
            }
        }
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent, _x: u16, _y: u16, _w: u16, _h: u16) -> DialogResult<Action> {
        if let Some(path) = self.poll_messages() {
            return DialogResult::Ok(Action::ConfirmPath(path));
        }
        DialogResult::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_message_dialog_navigation() {
        let buttons = vec![
            ("Yes".to_string(), Action::Save),
            ("No".to_string(), Action::DontSave),
            ("Cancel".to_string(), Action::Cancel),
        ];
        let mut dialog = MessageDialog::new("Test".to_string(), "Message".to_string(), buttons);

        assert_eq!(dialog.selected_btn, 0);

        // Test Right / Down / Tab
        dialog.handle_key(make_key(KeyCode::Right));
        assert_eq!(dialog.selected_btn, 1);
        dialog.handle_key(make_key(KeyCode::Down));
        assert_eq!(dialog.selected_btn, 2);
        dialog.handle_key(make_key(KeyCode::Tab));
        assert_eq!(dialog.selected_btn, 0); // Wrap around

        // Test Left / Up / BackTab
        dialog.handle_key(make_key(KeyCode::Left));
        assert_eq!(dialog.selected_btn, 2);
        dialog.handle_key(make_key(KeyCode::Up));
        assert_eq!(dialog.selected_btn, 1);
        dialog.handle_key(make_key(KeyCode::BackTab));
        assert_eq!(dialog.selected_btn, 0);

        // Test Enter confirms current button action
        dialog.handle_key(make_key(KeyCode::Right)); // selected: 1 ("No")
        match dialog.handle_key(make_key(KeyCode::Enter)) {
            DialogResult::Ok(Action::DontSave) => {}
            _ => panic!("Expected Action::DontSave on Enter"),
        }
    }

    #[test]
    fn test_unsaved_changes_dialog_dimensions_and_rendering_japanese() {
        let i18n = zee_core::I18n::load("ja");
        let title = i18n.get("dialog.unsaved_changes_title").to_string(); // "保存されていない変更"
        let filename = i18n.get("status.no_name").to_string(); // "[無題]"
        let message = i18n.get("dialog.unsaved_changes").replace("{filename}", &filename); // "\"[無題]\" は変更されています。保存しますか？"
        let buttons = vec![
            (i18n.get("dialog.save").to_string(), Action::Save),            // "保存"
            (i18n.get("dialog.dont_save").to_string(), Action::DontSave),    // "保存しない"
            (i18n.get("dialog.cancel").to_string(), Action::Cancel),        // "キャンセル"
        ];

        let mut dialog = MessageDialog::new(title, message.clone(), buttons);
        let (dw, dh) = dialog.dimensions();

        // Must be wide enough to comfortably contain the 42-cell message and 40-cell buttons with generous margins
        assert!(dw >= 56, "Expected dialog width >= 56 for Japanese UI, got {}", dw);
        assert!(dh >= 8, "Expected dialog height >= 8, got {}", dh);

        // Test rendering with Renderer
        let theme = zee_core::theme::Theme::load_all().into_iter().next().unwrap();
        let mut renderer = Renderer::new(dw, dh);
        dialog.render(&mut renderer, &theme, 0, 0, dw, dh);

        // Test mouse click coordinates on each button at bottom row (dh - 2)
        let click_y = dh - 2;
        // 1. Click Save button on left
        let click_save = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: dw / 2 - 14,
            row: click_y,
            modifiers: KeyModifiers::NONE,
        };
        match dialog.handle_mouse(click_save, 0, 0, dw, dh) {
            DialogResult::Ok(Action::Save) => {}
            _ => panic!("Expected Action::Save when clicking Save button"),
        }

        // 2. Click Don't Save button in center
        let click_dont_save = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: dw / 2,
            row: click_y,
            modifiers: KeyModifiers::NONE,
        };
        match dialog.handle_mouse(click_dont_save, 0, 0, dw, dh) {
            DialogResult::Ok(Action::DontSave) => {}
            _ => panic!("Expected Action::DontSave when clicking Don't Save button"),
        }

        // 3. Click Cancel button on right
        let click_cancel = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: dw / 2 + 14,
            row: click_y,
            modifiers: KeyModifiers::NONE,
        };
        match dialog.handle_mouse(click_cancel, 0, 0, dw, dh) {
            DialogResult::Ok(Action::Cancel) => {}
            _ => panic!("Expected Action::Cancel when clicking Cancel button"),
        }
    }

    #[test]
    fn test_goto_line_dialog() {
        let i18n = zee_core::I18n::load("en");
        let mut dialog = GoToLineDialog::new(&i18n);

        dialog.handle_key(make_key(KeyCode::Char('4')));
        dialog.handle_key(make_key(KeyCode::Char('2')));
        assert_eq!(dialog.input_text, "42");

        match dialog.handle_key(make_key(KeyCode::Enter)) {
            DialogResult::Ok(Action::ConfirmLine(line)) => assert_eq!(line, 42),
            _ => panic!("Expected Action::ConfirmLine(42)"),
        }
    }

    #[test]
    fn test_save_as_dialog_extension_and_encoding() {
        let i18n = zee_core::I18n::load("en");
        let mut dialog = SaveAsDialog::new(None, Some(".txt"), zee_core::Encoding::Utf8, &i18n);

        assert_eq!(dialog.browser.input_text, "untitled.txt");
        assert_eq!(dialog.selected_encoding(), Some(zee_core::Encoding::Utf8));

        // Toggle extension with Alt+X
        let mut alt_x = make_key(KeyCode::Char('x'));
        alt_x.modifiers = KeyModifiers::ALT;
        dialog.handle_key(alt_x);
        assert_eq!(dialog.browser.input_text, "untitled.md");

        // Cycle encoding with Alt+E
        let mut alt_e = make_key(KeyCode::Char('e'));
        alt_e.modifiers = KeyModifiers::ALT;
        dialog.handle_key(alt_e);
        assert_eq!(dialog.selected_encoding(), Some(zee_core::Encoding::Utf8Bom));

        // Type filename without extension, Enter should auto-append default extension
        dialog.browser.input_text = "test_note".to_string();
        match dialog.handle_key(make_key(KeyCode::Enter)) {
            DialogResult::Ok(Action::ConfirmPath(path)) => {
                assert_eq!(path.file_name().unwrap(), "test_note.md");
            }
            _ => panic!("Expected Action::ConfirmPath"),
        }
    }

    #[test]
    fn test_settings_dialog_workflow() {
        let i18n = zee_core::I18n::load("en");
        let themes = zee_core::theme::Theme::load_all();
        let mut config = zee_core::Config::default();
        config.sidebar_position = "left".to_string();
        config.tab_size = 4;

        let mut dialog = SettingsDialog::new(config, &themes, &i18n);
        assert_eq!(dialog.selected_row, 0);

        // Navigate Down to Sidebar Position (row 2)
        dialog.handle_key(make_key(KeyCode::Down));
        assert_eq!(dialog.selected_row, 1);
        dialog.handle_key(make_key(KeyCode::Down));
        assert_eq!(dialog.selected_row, 2);

        // Toggle Sidebar Position from left to right
        assert_eq!(dialog.config.sidebar_position, "left");
        dialog.handle_key(make_key(KeyCode::Right));
        assert_eq!(dialog.config.sidebar_position, "right");

        // Navigate Down to Tab Size (row 3) and cycle tab size
        dialog.handle_key(make_key(KeyCode::Down));
        assert_eq!(dialog.selected_row, 3);
        dialog.handle_key(make_key(KeyCode::Right));
        assert_eq!(dialog.config.tab_size, 8);

        // Navigate to Buttons row (row 9)
        dialog.selected_row = 9;
        dialog.selected_btn = 0; // Save & Apply

        match dialog.handle_key(make_key(KeyCode::Enter)) {
            DialogResult::Ok(Action::SaveSettings(new_cfg)) => {
                assert_eq!(new_cfg.sidebar_position, "right");
                assert_eq!(new_cfg.tab_size, 8);
            }
            _ => panic!("Expected Action::SaveSettings"),
        }
    }

    #[test]
    fn test_settings_dialog_dropdown_keyboard_navigation_and_selection() {
        let i18n = zee_core::I18n::load("en");
        let themes = zee_core::theme::Theme::load_all();
        let mut config = zee_core::Config::default();
        config.language = "auto".to_string();

        let mut dialog = SettingsDialog::new(config, &themes, &i18n);
        assert!(dialog.active_dropdown.is_none());

        // Move Down to Language row (row 1)
        dialog.handle_key(make_key(KeyCode::Down));
        assert_eq!(dialog.selected_row, 1);

        // Press Enter to open dropdown menu
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_some());
        {
            let drop = dialog.active_dropdown.as_ref().unwrap();
            assert_eq!(drop.row, 1);
            // Current selection should be initial language index
            assert_eq!(drop.selected_idx, 0); // "auto" is at index 0
        }

        // Navigate Down inside dropdown menu
        dialog.handle_key(make_key(KeyCode::Down));
        let selected_id = {
            let drop = dialog.active_dropdown.as_ref().unwrap();
            assert_eq!(drop.selected_idx, 1);
            drop.items[1].0.clone()
        };

        // Press Enter to confirm and select
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_none());
        assert_eq!(dialog.config.language, selected_id);

        // Re-open dropdown with Enter
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_some());

        // Navigate Down then press Esc to cancel without change
        dialog.handle_key(make_key(KeyCode::Down));
        dialog.handle_key(make_key(KeyCode::Esc));
        assert!(dialog.active_dropdown.is_none());
        assert_eq!(dialog.config.language, selected_id);
    }

    #[test]
    fn test_settings_dialog_dropdown_mouse_click_and_scroll() {
        let i18n = zee_core::I18n::load("en");
        let themes = zee_core::theme::Theme::load_all();
        let mut config = zee_core::Config::default();
        config.language = "auto".to_string();

        let mut dialog = SettingsDialog::new(config, &themes, &i18n);
        let (x, y, w, h) = (10, 5, 56, 14);

        // Mouse click on Language row (y + 2 + 1 = 8) to open dropdown
        let click_row = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x + 15,
            row: y + 3,
            modifiers: KeyModifiers::NONE,
        };
        dialog.handle_mouse(click_row, x, y, w, h);
        assert!(dialog.active_dropdown.is_some());

        let (drop_x, drop_y, _, _, _) = dialog.dropdown_bounds(x, y, w, h).unwrap();

        // Mouse click on item at index 1 (drop_y + 1 + 1 = drop_y + 2)
        let click_item = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: drop_x + 5,
            row: drop_y + 2,
            modifiers: KeyModifiers::NONE,
        };
        dialog.handle_mouse(click_item, x, y, w, h);
        assert!(dialog.active_dropdown.is_none());
        assert_eq!(dialog.config.language, dialog.languages[1].0);
    }

    #[test]
    fn test_settings_dialog_dropdown_theme_and_tab_size() {
        let i18n = zee_core::I18n::load("en");
        let themes = zee_core::theme::Theme::load_all();
        let mut config = zee_core::Config::default();
        config.theme = themes[0].meta.name.to_lowercase().replace(" ", "-");
        config.tab_size = 2;

        let mut dialog = SettingsDialog::new(config, &themes, &i18n);

        // 1. Theme dropdown on row 0
        assert_eq!(dialog.selected_row, 0);
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_some());
        assert_eq!(dialog.active_dropdown.as_ref().unwrap().row, 0);

        // Move Down to select second theme
        dialog.handle_key(make_key(KeyCode::Down));
        let expected_theme = dialog.active_dropdown.as_ref().unwrap().items[1].0.clone();
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_none());
        assert_eq!(dialog.config.theme, expected_theme);

        // 2. Tab Size dropdown on row 3
        dialog.selected_row = 3;
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_some());
        assert_eq!(dialog.active_dropdown.as_ref().unwrap().row, 3);
        assert_eq!(dialog.active_dropdown.as_ref().unwrap().selected_idx, 0); // 2 spaces

        // Move Down to 4 spaces (index 1)
        dialog.handle_key(make_key(KeyCode::Down));
        dialog.handle_key(make_key(KeyCode::Enter));
        assert!(dialog.active_dropdown.is_none());
        assert_eq!(dialog.config.tab_size, 4);

        // Open Tab Size again and move Down to 8 spaces (index 2)
        dialog.handle_key(make_key(KeyCode::Enter));
        dialog.handle_key(make_key(KeyCode::Down));
        dialog.handle_key(make_key(KeyCode::Enter));
        assert_eq!(dialog.config.tab_size, 8);
    }
}


