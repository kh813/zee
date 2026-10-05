use zee_core::buffer::Editor;
use zee_core::theme::Theme;
use zee_core::config::Config;
use zee_core::file_tree::FileTree;
use zee_core::outline::{self, OutlineNode};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Files,
    Outline,
}

pub struct Workspace {
    pub editors: Vec<Editor>,
    pub active_editor_index: usize,
    pub theme: Theme,
    pub config: Config,
    pub sidebar_visible: bool,
    pub sidebar_tab: SidebarTab,
    pub file_tree: FileTree,
    pub outline_nodes: Vec<OutlineNode>,
    pub plugin_manager: zee_core::plugin::PluginManager,
    pub vi_cmd: Option<String>,
}

impl Workspace {
    pub fn new(config: Config) -> Self {
        Self::new_with_root(config, None)
    }

    pub fn new_with_root(config: Config, root_path: Option<PathBuf>) -> Self {
        let is_custom_root = root_path.is_some();
        let root = root_path.unwrap_or_else(zee_core::file_tree::user_root_dir);
        let file_tree = FileTree::new(&root, false);
        let sidebar_visible = if is_custom_root { true } else { config.sidebar };
        let mut plugin_manager = zee_core::plugin::PluginManager::new();
        // Check local development plugins directory if present
        let dev_plugin_dir = PathBuf::from("plugins/zee-plugin-text");
        if dev_plugin_dir.exists() {
            let _ = plugin_manager.load_plugin_dir(&dev_plugin_dir);
        }

        let mut initial_editor = Editor::new();
        initial_editor.vi_mode = if config.vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };

        Self {
            editors: vec![initial_editor],
            active_editor_index: 0,
            theme: Theme::default(),
            config,
            sidebar_visible,
            sidebar_tab: SidebarTab::Files,
            file_tree,
            outline_nodes: Vec::new(),
            plugin_manager,
            vi_cmd: None,
        }
    }

    pub fn set_root_path(&mut self, path: PathBuf) {
        self.file_tree.set_root(path);
        self.sidebar_visible = true;
        self.sidebar_tab = SidebarTab::Files;
    }

    pub fn reload_config(&mut self) {
        let config = Config::load();
        if let Some(t) = Theme::find_by_name(&config.theme) {
            self.theme = t;
        }
        self.config = config;
    }

    pub fn reload_plugins(&mut self) {
        let mut plugin_manager = zee_core::plugin::PluginManager::new();
        let dev_plugin_dir = PathBuf::from("plugins/zee-plugin-text");
        if dev_plugin_dir.exists() {
            let _ = plugin_manager.load_plugin_dir(&dev_plugin_dir);
        }
        self.plugin_manager = plugin_manager;
    }

    pub fn update_outline(&mut self) {
        if let Some((ext, text)) = self.active_editor().map(|e| {
            let ext = e.path.as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .unwrap_or("md")
                .to_string();
            let text = e.rope.to_string();
            (ext, text)
        }) {
            self.outline_nodes = outline::extract_outline(Some(&mut self.plugin_manager), &ext, &text);
        } else {
            self.outline_nodes.clear();
        }
    }

    pub fn apply_plugin_transform(&mut self, command: &str) {
        let (has_selection, range, text_to_transform) = if let Some(editor) = self.active_editor() {
            if let Some(range) = editor.selection.clone() {
                if range.start < range.end {
                    (true, range.clone(), editor.rope.slice(range).to_string())
                } else {
                    (false, 0..0, editor.rope.to_string())
                }
            } else {
                (false, 0..0, editor.rope.to_string())
            }
        } else {
            return;
        };

        let transformed = self.plugin_manager
            .transform_text(command, &text_to_transform)
            .unwrap_or(text_to_transform);

        if let Some(editor) = self.active_editor_mut() {
            if has_selection {
                editor.delete(range.clone());
                editor.insert(range.start, &transformed);
                editor.cursor = range.start + transformed.chars().count();
                editor.selection = Some(range.start..editor.cursor);
            } else {
                editor.delete(0..editor.rope.len_chars());
                editor.insert(0, &transformed);
                editor.cursor = editor.cursor.min(editor.rope.len_chars());
            }
        }
        self.update_outline();
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        self.config.sidebar = self.sidebar_visible;
        let _ = Config::write_key("sidebar", &self.sidebar_visible.to_string());
    }

    pub fn jump_to_line(&mut self, line: usize) {
        self.jump_to_line_col(line, 0);
    }

    pub fn jump_to_line_col(&mut self, line: usize, col: usize) {
        if let Some(editor) = self.active_editor_mut() {
            let line_count = editor.line_count();
            let target_line = line.min(line_count.saturating_sub(1));
            let line_start_char = editor.rope.line_to_char(target_line);
            let line_slice = editor.rope.line(target_line);
            let line_len = line_slice.len_chars().saturating_sub(if line_slice.to_string().ends_with('\n') { 1 } else { 0 });
            editor.cursor = line_start_char + col.min(line_len);
            editor.selection = None;
            editor.selection_anchor = None;
            editor.scroll_row = target_line.saturating_sub(5);
        }
    }

    pub fn reload_active_editor(&mut self) -> anyhow::Result<()> {
        if let Some(editor) = self.active_editor_mut() {
            editor.reload_from_disk()?;
        }
        self.update_outline();
        Ok(())
    }

    pub fn active_editor(&self) -> Option<&Editor> {
        self.editors.get(self.active_editor_index)
    }

    pub fn active_editor_mut(&mut self) -> Option<&mut Editor> {
        self.editors.get_mut(self.active_editor_index)
    }

    #[allow(dead_code)]
    pub fn find_editor_by_path(&self, path: &std::path::Path) -> Option<usize> {
        self.editors.iter().position(|e| e.path.as_ref() == Some(&path.to_path_buf()))
    }

    pub fn new_tab(&mut self) {
        let mut editor = Editor::new();
        editor.vi_mode = if self.config.vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
        self.editors.push(editor);
        self.active_editor_index = self.editors.len() - 1;
    }

    pub fn add_editor(&mut self, mut editor: Editor) {
        editor.vi_mode = if self.config.vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
        if let Some(path) = &editor.path {
            let mut recent = zee_core::recent::RecentFiles::load();
            recent.add(path);

            if let Some(idx) = self.editors.iter().position(|e| e.path.as_ref() == Some(path)) {
                self.active_editor_index = idx;
                return;
            }

            // If there is only 1 tab and it is an untitled, unmodified empty buffer, replace it with the opened file
            if self.editors.len() == 1 {
                let first = &self.editors[0];
                if !first.is_modified() && first.path.is_none() && first.rope.len_chars() == 0 {
                    self.editors[0] = editor;
                    self.active_editor_index = 0;
                    return;
                }
            }

            if self.active_editor_index < self.editors.len() {
                let active = &self.editors[self.active_editor_index];
                if !active.is_modified() && active.path.is_none() && active.rope.len_chars() == 0 {
                    self.editors[self.active_editor_index] = editor;
                    return;
                }
            }
        }

        self.editors.push(editor);
        self.active_editor_index = self.editors.len() - 1;
    }

    pub fn close_editor(&mut self, index: usize) {
        if index < self.editors.len() {
            self.editors.remove(index);
            if self.editors.is_empty() {
                self.active_editor_index = 0;
            } else if self.active_editor_index >= self.editors.len() {
                self.active_editor_index = self.editors.len() - 1;
            } else if self.active_editor_index > index {
                self.active_editor_index -= 1;
            }
        }
    }

    pub fn close_active_editor(&mut self) {
        if !self.editors.is_empty() {
            self.close_editor(self.active_editor_index);
        }
    }

    pub fn next_tab(&mut self) {
        if !self.editors.is_empty() {
            self.active_editor_index = (self.active_editor_index + 1) % self.editors.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.editors.is_empty() {
            self.active_editor_index = (self.active_editor_index + self.editors.len() - 1) % self.editors.len();
        }
    }

    pub fn has_modified_buffers(&self) -> bool {
        self.editors.iter().any(|e| e.is_modified())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tab_always_adds_new_tab() {
        let mut workspace = Workspace::new(Config::default());
        assert_eq!(workspace.editors.len(), 1);
        assert_eq!(workspace.active_editor_index, 0);

        workspace.new_tab();
        assert_eq!(workspace.editors.len(), 2);
        assert_eq!(workspace.active_editor_index, 1);

        workspace.new_tab();
        assert_eq!(workspace.editors.len(), 3);
        assert_eq!(workspace.active_editor_index, 2);
    }

    #[test]
    fn test_add_editor_untitled_always_adds_tab() {
        let mut workspace = Workspace::new(Config::default());
        assert_eq!(workspace.editors.len(), 1);

        workspace.add_editor(Editor::new());
        assert_eq!(workspace.editors.len(), 2);
        assert_eq!(workspace.active_editor_index, 1);
    }

    #[test]
    fn test_close_editor_until_empty() {
        let mut workspace = Workspace::new(Config::default());
        assert_eq!(workspace.editors.len(), 1);
        workspace.close_editor(0);
        assert_eq!(workspace.editors.len(), 0);
        assert!(workspace.active_editor().is_none());
    }

    #[test]
    fn test_add_editor_replaces_clean_initial_buffer() {
        let mut workspace = Workspace::new(Config::default());
        assert_eq!(workspace.editors.len(), 1);
        assert!(workspace.editors[0].path.is_none());

        let mut new_editor = Editor::new();
        new_editor.path = Some(std::path::PathBuf::from("/tmp/test.rs"));
        workspace.add_editor(new_editor);
        assert_eq!(workspace.editors.len(), 1);
        assert_eq!(workspace.active_editor().unwrap().path.as_ref().unwrap().to_str().unwrap(), "/tmp/test.rs");
    }

    #[test]
    fn test_add_editor_keeps_modified_buffer() {
        let mut workspace = Workspace::new(Config::default());
        workspace.active_editor_mut().unwrap().insert(0, "modified text");
        assert!(workspace.has_modified_buffers());

        let mut new_editor = Editor::new();
        new_editor.path = Some(std::path::PathBuf::from("/tmp/second.rs"));
        workspace.add_editor(new_editor);
        assert_eq!(workspace.editors.len(), 2);
        assert_eq!(workspace.active_editor_index, 1);
    }

    #[test]
    fn test_close_active_editor_until_empty() {
        let mut workspace = Workspace::new(Config::default());
        workspace.new_tab();
        assert_eq!(workspace.editors.len(), 2);

        workspace.close_active_editor();
        assert_eq!(workspace.editors.len(), 1);

        workspace.close_active_editor();
        assert_eq!(workspace.editors.len(), 0);
        assert!(workspace.active_editor().is_none());

        // Calling close_active_editor on empty workspace should be a safe no-op
        workspace.close_active_editor();
        assert_eq!(workspace.editors.len(), 0);
    }

    #[test]
    fn test_workspace_vi_mode_defaults_off() {
        let config = Config::default();
        assert!(!config.vi_mode);
        let mut workspace = Workspace::new(config);
        assert_eq!(workspace.editors[0].vi_mode, zee_core::ViMode::Insert);

        workspace.new_tab();
        assert_eq!(workspace.editors[1].vi_mode, zee_core::ViMode::Insert);

        workspace.add_editor(Editor::new());
        assert_eq!(workspace.editors[2].vi_mode, zee_core::ViMode::Insert);
    }

    #[test]
    fn test_open_folder_reveals_sidebar_files_tab() {
        let temp_dir = std::env::temp_dir().join("zee_test_open_folder");
        let _ = std::fs::create_dir_all(&temp_dir);

        let config = Config {
            sidebar: false,
            ..Default::default()
        };

        // Opening with root_path specified
        let workspace = Workspace::new_with_root(config.clone(), Some(temp_dir.clone()));
        assert!(workspace.sidebar_visible);
        assert_eq!(workspace.sidebar_tab, SidebarTab::Files);
        assert_eq!(workspace.file_tree.root_path, temp_dir);

        // Opening without root path (uses user root and respects config.sidebar)
        let mut default_workspace = Workspace::new(config);
        assert!(!default_workspace.sidebar_visible);

        // Calling set_root_path reveals sidebar and switches tab
        default_workspace.set_root_path(temp_dir.clone());
        assert!(default_workspace.sidebar_visible);
        assert_eq!(default_workspace.sidebar_tab, SidebarTab::Files);
        assert_eq!(default_workspace.file_tree.root_path, temp_dir);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_workspace_session_restoration() {
        let temp_dir = std::env::temp_dir().join("zee_test_session_restore");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("subdir")).unwrap();
        let file1 = temp_dir.join("file1.rs");
        let file2 = temp_dir.join("file2.md");
        std::fs::write(&file1, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
        std::fs::write(&file2, "# Title\n\nContent here\n").unwrap();

        let session = zee_core::session::UpdateSession {
            files: vec![
                zee_core::session::SessionEditorState {
                    path: Some(file1.clone()),
                    cursor: 12,
                    scroll_row: 1,
                    is_modified: false,
                    unsaved_content: None,
                },
                zee_core::session::SessionEditorState {
                    path: Some(file2.clone()),
                    cursor: 4,
                    scroll_row: 0,
                    is_modified: true,
                    unsaved_content: Some("# Title\n\nModified draft\n".to_string()),
                },
                zee_core::session::SessionEditorState {
                    path: None,
                    cursor: 3,
                    scroll_row: 0,
                    is_modified: true,
                    unsaved_content: Some("untitled scratch".to_string()),
                },
            ],
            active_index: 1,
            root_folder: Some(temp_dir.clone()),
            expanded_folders: vec![temp_dir.join("subdir")],
            sidebar_visible: true,
            sidebar_tab: "outline".to_string(),
        };

        // Simulate restore logic
        let mut w = Workspace::new_with_root(Config::default(), session.root_folder.clone());
        w.sidebar_visible = session.sidebar_visible;
        w.sidebar_tab = match session.sidebar_tab.as_str() {
            "outline" => SidebarTab::Outline,
            _ => SidebarTab::Files,
        };
        if !session.expanded_folders.is_empty() {
            w.file_tree.restore_expanded_paths(&session.expanded_folders);
        }

        for file_state in session.files {
            let editor = if let Some(path) = &file_state.path {
                if let Ok(mut ed) = zee_core::buffer::Editor::from_file(path) {
                    if let Some(unsaved) = &file_state.unsaved_content {
                        ed.delete(0..ed.rope.len_chars());
                        ed.insert(0, unsaved);
                        ed.modified_since_save = file_state.is_modified;
                    }
                    Some(ed)
                } else {
                    None
                }
            } else if let Some(unsaved) = &file_state.unsaved_content {
                let mut ed = Editor::new();
                ed.insert(0, unsaved);
                ed.modified_since_save = file_state.is_modified;
                Some(ed)
            } else {
                None
            };

            if let Some(mut ed) = editor {
                ed.cursor = file_state.cursor.min(ed.rope.len_chars());
                ed.scroll_row = file_state.scroll_row;
                w.add_editor(ed);
            }
        }

        if session.active_index < w.editors.len() {
            w.active_editor_index = session.active_index;
        }

        w.update_outline();

        // Verify restoration
        assert_eq!(w.editors.len(), 3);
        assert_eq!(w.active_editor_index, 1);
        assert!(w.sidebar_visible);
        assert_eq!(w.sidebar_tab, SidebarTab::Outline);
        assert_eq!(w.file_tree.root_path, temp_dir);
        assert!(w.file_tree.expanded_paths().contains(&temp_dir.join("subdir")));

        // Tab 0
        assert_eq!(w.editors[0].path, Some(file1));
        assert_eq!(w.editors[0].cursor, 12);
        assert_eq!(w.editors[0].scroll_row, 1);
        assert!(!w.editors[0].is_modified());

        // Tab 1 (active, modified, outline updated)
        assert_eq!(w.editors[1].path, Some(file2));
        assert_eq!(w.editors[1].cursor, 4);
        assert!(w.editors[1].is_modified());
        assert_eq!(w.editors[1].rope.to_string(), "# Title\n\nModified draft\n");
        assert!(!w.outline_nodes.is_empty());
        assert_eq!(w.outline_nodes[0].title, "Title");

        // Tab 2 (untitled scratch)
        assert_eq!(w.editors[2].path, None);
        assert_eq!(w.editors[2].cursor, 3);
        assert!(w.editors[2].is_modified());
        assert_eq!(w.editors[2].rope.to_string(), "untitled scratch");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

