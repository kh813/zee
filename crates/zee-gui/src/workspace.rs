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
    pub vi_cmd_preedit: Option<String>,
    pub vi_message: Option<(String, bool)>,
    pub plugins_version: usize,
}

impl Workspace {
    pub fn new(config: Config) -> Self {
        Self::new_with_root(config, None)
    }

    pub fn new_with_root(config: Config, root_path: Option<PathBuf>) -> Self {
        let is_custom_root = root_path.is_some();
        let root = root_path.unwrap_or_else(zee_core::file_tree::user_root_dir);
        let file_tree = FileTree::new(&root, config.show_hidden);
        let sidebar_visible = if is_custom_root { true } else { config.sidebar };
        let mut plugin_manager = zee_core::plugin::PluginManager::new();
        // Check local development plugins directory if present
        let dev_plugin_dir = if PathBuf::from("plugins/zee-plugin-text").exists() {
            Some(PathBuf::from("plugins/zee-plugin-text"))
        } else if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let candidate = PathBuf::from(manifest_dir)
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins/zee-plugin-text"));
            if candidate.as_ref().map(|p| p.exists()).unwrap_or(false) {
                candidate
            } else {
                None
            }
        } else {
            None
        };
        if let Some(dir) = dev_plugin_dir {
            let _ = plugin_manager.load_plugin_dir(&dir);
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
            vi_cmd_preedit: None,
            vi_message: None,
            plugins_version: 0,
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
        let dev_plugin_dir = if PathBuf::from("plugins/zee-plugin-text").exists() {
            Some(PathBuf::from("plugins/zee-plugin-text"))
        } else if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let candidate = PathBuf::from(manifest_dir)
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("plugins/zee-plugin-text"));
            if candidate.as_ref().map(|p| p.exists()).unwrap_or(false) {
                candidate
            } else {
                None
            }
        } else {
            None
        };
        if let Some(dir) = dev_plugin_dir {
            let _ = plugin_manager.load_plugin_dir(&dir);
        }
        self.plugin_manager = plugin_manager;
        self.plugins_version = self.plugins_version.wrapping_add(1);
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
                let start = range.start.min(range.end);
                let end = range.start.max(range.end);
                if start < end {
                    (true, start..end, editor.rope.slice(start..end).to_string())
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

        let is_insertion_cmd = command.starts_with("lorem_") || command == "generate_toc";

        if let Some(editor) = self.active_editor_mut() {
            if has_selection {
                editor.delete(range.clone());
                editor.insert(range.start, &transformed);
                editor.cursor = range.start + transformed.chars().count();
                editor.selection = Some(range.start..editor.cursor);
            } else if is_insertion_cmd {
                let insert_pos = editor.cursor.min(editor.rope.len_chars());
                editor.insert(insert_pos, &transformed);
                editor.cursor = insert_pos + transformed.chars().count();
                editor.selection = None;
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

    pub fn save_active_editor(&mut self) -> anyhow::Result<()> {
        let trim = self.config.trim_trailing_whitespace;
        let ensure_nl = self.config.ensure_final_newline;
        if let Some(editor) = self.active_editor_mut() {
            editor.cleanup_on_save(trim, ensure_nl);
            editor.save()
        } else {
            anyhow::bail!("No active editor")
        }
    }

    pub fn save_as_active_editor<P: AsRef<std::path::Path>>(&mut self, path: P) -> anyhow::Result<()> {
        let trim = self.config.trim_trailing_whitespace;
        let ensure_nl = self.config.ensure_final_newline;
        if let Some(editor) = self.active_editor_mut() {
            editor.cleanup_on_save(trim, ensure_nl);
            editor.save_as(path)
        } else {
            anyhow::bail!("No active editor")
        }
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

    pub fn new_from_template(&mut self, template_id: &str) {
        let templates = zee_core::template::Template::load_all();
        if let Some(tpl) = templates.iter().find(|t| t.id == template_id) {
            let (content, cursor_offset) = tpl.expand(None);
            let mut editor = Editor::new();
            editor.vi_mode = if self.config.vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
            editor.insert(0, &content);
            editor.cursor = cursor_offset.min(editor.rope.len_chars());
            editor.selection = None;
            editor.selection_anchor = None;

            if !tpl.extension.is_empty() {
                let ext_clean = tpl.extension.trim_start_matches('.');
                let syntax_defs = zee_core::syntax::SyntaxDefinition::builtins();
                if let Some(def) = syntax_defs.iter().find(|s| s.meta.extensions.iter().any(|e| e == ext_clean)) {
                    if let Ok(highlighter) = zee_core::syntax::SyntaxHighlighter::new(def.clone()) {
                        editor.update_syntax(Some(highlighter));
                    }
                }
            }

            if self.editors.len() == 1 {
                let first = &self.editors[0];
                if !first.is_modified() && first.path.is_none() && first.rope.len_chars() == 0 {
                    self.editors[0] = editor;
                    self.active_editor_index = 0;
                    return;
                }
            }

            self.add_editor(editor);
        } else {
            self.new_tab();
        }
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
        assert_eq!(workspace.vi_cmd, None);
        assert_eq!(workspace.vi_message, None);
    }

    #[test]
    fn test_workspace_vi_substitute_command_with_uppercase() {
        let mut config = Config::default();
        config.vi_mode = true;
        let mut workspace = Workspace::new(config);
        let editor = workspace.active_editor_mut().unwrap();
        editor.vi_mode = zee_core::ViMode::Normal;
        editor.insert(0, "English and English text\n");

        let ex_cmd = zee_core::parse_ex_command(":%s/English/english/g");
        if let zee_core::ExCommand::Substitute { range: _, pattern, replacement, global, ignore_case } = ex_cmd {
            let res = editor.substitute_range(1, editor.line_count(), &pattern, &replacement, global, ignore_case);
            assert_eq!(res, Ok(2));
            assert_eq!(editor.rope.to_string(), "english and english text\n");
        } else {
            panic!("Expected ExCommand::Substitute");
        }
    }

    #[test]
    fn test_workspace_vi_substitute_japanese_text() {
        let mut config = Config::default();
        config.vi_mode = true;
        let mut workspace = Workspace::new(config);
        let editor = workspace.active_editor_mut().unwrap();
        editor.vi_mode = zee_core::ViMode::Normal;
        editor.insert(0, "日本語と英語と日本語の文章\n第二行の日本語\n");

        // Execute :%s/日本語/Japanese/g
        let ex_cmd = zee_core::parse_ex_command(":%s/日本語/Japanese/g");
        if let zee_core::ExCommand::Substitute { range: _, pattern, replacement, global, ignore_case } = ex_cmd {
            let res = editor.substitute_range(1, editor.line_count(), &pattern, &replacement, global, ignore_case);
            assert_eq!(res, Ok(3));
            assert_eq!(editor.rope.to_string(), "Japaneseと英語とJapaneseの文章\n第二行のJapanese\n");
        } else {
            panic!("Expected ExCommand::Substitute");
        }

        // Full-width IME Japanese syntax tokens
        let ime_cmd = zee_core::parse_ex_command("：％ｓ／Japanese／和文／ｇ");
        if let zee_core::ExCommand::Substitute { range: _, pattern, replacement, global, ignore_case } = ime_cmd {
            let res = editor.substitute_range(1, editor.line_count(), &pattern, &replacement, global, ignore_case);
            assert_eq!(res, Ok(3));
            assert_eq!(editor.rope.to_string(), "和文と英語と和文の文章\n第二行の和文\n");
        } else {
            panic!("Expected ExCommand::Substitute");
        }
    }

    #[test]
    fn test_workspace_vi_cmd_and_preedit_lifecycle() {
        let mut config = Config::default();
        config.vi_mode = true;
        let mut workspace = Workspace::new(config);
        assert_eq!(workspace.vi_cmd, None);
        assert_eq!(workspace.vi_cmd_preedit, None);

        // Enter vi cmd mode
        workspace.vi_cmd = Some(":".to_string());
        assert_eq!(workspace.vi_cmd.as_deref(), Some(":"));

        // Type %s/
        workspace.vi_cmd.as_mut().unwrap().push_str("%s/");
        assert_eq!(workspace.vi_cmd.as_deref(), Some(":%s/"));

        // Preedit arrives (e.g. typing "にほんご")
        workspace.vi_cmd_preedit = Some("にほんご".to_string());
        assert_eq!(workspace.vi_cmd_preedit.as_deref(), Some("にほんご"));

        // IME commit (preedit becomes committed "日本語")
        workspace.vi_cmd.as_mut().unwrap().push_str("日本語");
        workspace.vi_cmd_preedit = None;
        assert_eq!(workspace.vi_cmd.as_deref(), Some(":%s/日本語"));
        assert_eq!(workspace.vi_cmd_preedit, None);

        // Cancel command via Escape
        workspace.vi_cmd = None;
        workspace.vi_cmd_preedit = None;
        assert_eq!(workspace.vi_cmd, None);
        assert_eq!(workspace.vi_cmd_preedit, None);
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

    #[test]
    fn test_workspace_visual_block_selection_and_copy() {
        let mut workspace = Workspace::new(Config::default());
        let mut editor = Editor::new();
        editor.insert(0, "ABCDEF\n123456\nGHIJKL\n");
        workspace.add_editor(editor);

        let editor = workspace.active_editor_mut().unwrap();
        // Simulate Alt+Drag: anchor at (0, 1) 'B', cursor at (2, 3) 'J'
        editor.vi_mode = zee_core::ViMode::VisualBlock;
        let anchor = editor.line_col_to_char(0, 1);
        let cursor = editor.line_col_to_char(2, 3);
        editor.selection_anchor = Some(anchor);
        editor.cursor = cursor;
        editor.selection = Some(anchor..cursor);

        let block_text = editor.get_visual_block_text();
        assert_eq!(block_text, "BCD\n234\nHIJ");

        editor.delete_visual_block();
        assert_eq!(editor.rope.to_string(), "AEF\n156\nAKL\n".replace("AKL", "GKL"));
    }

    #[test]
    fn test_workspace_new_from_template() {
        let mut workspace = Workspace::new(Config::default());
        // Clean initial untitled buffer
        assert_eq!(workspace.editors.len(), 1);

        workspace.new_from_template("rust_bin");
        // Replaces clean initial buffer with rust template
        assert_eq!(workspace.editors.len(), 1);
        let editor = workspace.active_editor().unwrap();
        let content = editor.rope.to_string();
        assert!(content.contains("fn main()"));
        assert!(content.contains("println!(\"Hello, world!\");"));
        assert!(!content.contains("{cursor}"));
        assert_eq!(editor.syntax_highlighter.as_ref().map(|h| h.def.meta.name.as_str()), Some("Rust"));
    }

    #[test]
    fn test_workspace_standard_editing_and_selection() {
        let mut workspace = Workspace::new(Config::default());
        let editor = workspace.active_editor_mut().unwrap();
        assert_eq!(editor.vi_mode, zee_core::ViMode::Insert);

        // Insert and verify content
        editor.insert(0, "First Line\nSecond Line\nThird Line");
        assert_eq!(editor.line_count(), 3);
        assert!(editor.is_modified());

        // Test line/column calculations
        let (l, c) = editor.char_to_line_col(editor.cursor);
        assert_eq!(l, 2);
        assert_eq!(c, 10);

        // Undo & Redo
        editor.undo();
        assert_eq!(editor.rope.to_string(), "");
        editor.redo();
        assert_eq!(editor.rope.to_string(), "First Line\nSecond Line\nThird Line");

        // Selection deletion
        editor.selection = Some(0..5);
        let sel_range = editor.selection.take().unwrap();
        editor.delete(sel_range);
        assert_eq!(editor.rope.to_string(), " Line\nSecond Line\nThird Line");
    }

    #[test]
    fn test_workspace_tab_cycling_and_close() {
        let mut workspace = Workspace::new(Config::default());
        assert_eq!(workspace.editors.len(), 1);
        assert_eq!(workspace.active_editor_index, 0);

        // Add 2 tabs
        workspace.new_tab();
        workspace.new_tab();
        assert_eq!(workspace.editors.len(), 3);
        assert_eq!(workspace.active_editor_index, 2);

        // Next tab wraps around
        workspace.next_tab();
        assert_eq!(workspace.active_editor_index, 0);
        workspace.next_tab();
        assert_eq!(workspace.active_editor_index, 1);

        // Prev tab moves backward
        workspace.prev_tab();
        assert_eq!(workspace.active_editor_index, 0);
        workspace.prev_tab();
        assert_eq!(workspace.active_editor_index, 2);

        // Close middle tab
        workspace.active_editor_index = 1;
        workspace.close_active_editor();
        assert_eq!(workspace.editors.len(), 2);
        assert_eq!(workspace.active_editor_index, 1);
    }

    #[test]
    fn test_workspace_find_and_replace() {
        let mut workspace = Workspace::new(Config::default());
        let editor = workspace.active_editor_mut().unwrap();
        editor.insert(0, "Apple Banana Apple Orange Apple");

        // Search for "Apple"
        let query = zee_core::search::SearchQuery {
            pattern: "Apple".to_string(),
            flags: zee_core::search::SearchFlags {
                match_case: true,
                whole_word: false,
                use_regex: false,
            },
        };
        let matches = editor.search(&query);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].char_range, 0..5);
        assert_eq!(matches[1].char_range, 13..18);
        assert_eq!(matches[2].char_range, 26..31);

        // Replace all in reverse order
        for m in matches.into_iter().rev() {
            editor.delete(m.char_range);
            editor.insert(editor.cursor, "Mango");
        }
        assert_eq!(editor.rope.to_string(), "Mango Banana Mango Orange Mango");
    }

    #[test]
    fn test_native_menus_and_language_localization() {
        #[cfg(target_os = "macos")]
        {
            use zee_core::i18n::I18n;
            let mut config = Config::default();
            config.language = "ja".to_string();
            let i18n_ja = I18n::load("ja");
            let menus_ja = crate::app::build_native_menus(&i18n_ja, &config, None);

            assert_eq!(menus_ja[0].name.as_ref(), "zee");
            assert_eq!(menus_ja[1].name.as_ref(), "ファイル");
            assert_eq!(menus_ja[2].name.as_ref(), "編集");
            assert_eq!(menus_ja[3].name.as_ref(), "表示");
            assert_eq!(menus_ja[4].name.as_ref(), "タブ");

            // English
            config.language = "en".to_string();
            let i18n_en = I18n::load("en");
            let menus_en = crate::app::build_native_menus(&i18n_en, &config, None);
            assert_eq!(menus_en[0].name.as_ref(), "zee");
            assert_eq!(menus_en[1].name.as_ref(), "File");
            assert_eq!(menus_en[2].name.as_ref(), "Edit");
            assert_eq!(menus_en[3].name.as_ref(), "View");
            assert_eq!(menus_en[4].name.as_ref(), "Tabs");
        }
    }

    #[test]
    fn test_plugins_reload_and_menu_updates() {
        let mut workspace = Workspace::new(Config::default());
        assert_eq!(workspace.plugins_version, 0);

        // Native menus initially
        #[cfg(target_os = "macos")]
        {
            use zee_core::i18n::I18n;
            let i18n = I18n::load("ja");
            let menus = crate::app::build_native_menus(&i18n, &workspace.config, Some(&workspace.plugin_manager));
            let plugin_menu = menus.iter().find(|m| m.name.as_ref() == "プラグイン").expect("Plugin menu must exist");
            assert!(!plugin_menu.items.is_empty());

            // If no plugins loaded, the first item must be "インストール済みのプラグインはありません" action
            if workspace.plugin_manager.all_manifests().is_empty() {
                match &plugin_menu.items[0] {
                    gpui::MenuItem::Action { name, .. } => {
                        assert!(name.contains("プラグイン") || name.contains("plugin"));
                    }
                    _ => panic!("Expected Action item for no plugins message"),
                }
            }
        }

        workspace.reload_plugins();
        assert_eq!(workspace.plugins_version, 1);

        workspace.reload_plugins();
        assert_eq!(workspace.plugins_version, 2);
    }

    #[test]
    fn test_dialog_dimensions_and_wide_types() {
        use crate::widgets::dialog::{Dialog, DialogType, UnsavedChangesIntent};

        // Wide dialogs must be at least 600px wide and 660px tall to fit Japanese labels and scroll containers
        let wide_dialogs = vec![
            DialogType::Settings,
            DialogType::PluginManager,
            DialogType::GoogleDrive,
        ];

        for dt in wide_dialogs {
            let (w, h) = Dialog::dialog_dimensions_for_type(&dt);
            assert!(w >= 600.0, "Wide dialog width must be >= 600.0, got {}", w);
            assert!(h >= 660.0, "Wide dialog max height must be >= 660.0, got {}", h);
        }

        // PluginManager must have widened dimensions (720x680) to fit single-line Japanese descriptions and search bar
        let (pm_w, pm_h) = Dialog::dialog_dimensions_for_type(&DialogType::PluginManager);
        assert_eq!((pm_w, pm_h), (720.0, 680.0));

        // Standard dialogs (e.g. UnsavedChanges, About)
        let standard_dialog_type = DialogType::UnsavedChanges {
            filename: "test.txt".to_string(),
            intent: UnsavedChangesIntent::CloseTab,
        };
        let (w, h) = Dialog::dialog_dimensions_for_type(&standard_dialog_type);
        assert_eq!((w, h), (460.0, 580.0));
    }

    #[test]
    fn test_plugin_manager_keyword_search_filtering() {
        use zee_core::plugin::{PluginManifest, PluginType, PluginCapabilities};

        let manifests = vec![
            PluginManifest {
                id: "case-converter".to_string(),
                name: "Case Converter".to_string(),
                version: "1.0.0".to_string(),
                description: Some("大文字と小文字の相互変換を行います".to_string()),
                plugin_type: PluginType::Lua,
                entry: Some("main.lua".to_string()),
                author: None,
                homepage: None,
                languages: Vec::new(),
                capabilities: PluginCapabilities::default(),
            },
            PluginManifest {
                id: "markdown-toc".to_string(),
                name: "Markdown TOC".to_string(),
                version: "0.2.0".to_string(),
                description: Some("見出しから目次を自動生成".to_string()),
                plugin_type: PluginType::Wasm,
                entry: Some("toc.wasm".to_string()),
                author: None,
                homepage: None,
                languages: Vec::new(),
                capabilities: PluginCapabilities::default(),
            },
        ];

        let filter_by = |query: &str| -> Vec<String> {
            manifests.iter().filter(|m| {
                if query.trim().is_empty() {
                    return true;
                }
                let q = query.trim().to_lowercase();
                m.name.to_lowercase().contains(&q)
                    || m.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || m.id.to_lowercase().contains(&q)
            }).map(|m| m.id.clone()).collect()
        };

        // 1. Empty query matches all
        assert_eq!(filter_by("").len(), 2);
        assert_eq!(filter_by("   ").len(), 2);

        // 2. Filter by name (case-insensitive)
        assert_eq!(filter_by("case"), vec!["case-converter"]);
        assert_eq!(filter_by("CASE"), vec!["case-converter"]);
        assert_eq!(filter_by("toc"), vec!["markdown-toc"]);

        // 3. Filter by Japanese description
        assert_eq!(filter_by("目次"), vec!["markdown-toc"]);
        assert_eq!(filter_by("大文字"), vec!["case-converter"]);

        // 4. Filter by ID
        assert_eq!(filter_by("markdown-toc"), vec!["markdown-toc"]);

        // 5. No match returns empty
        assert_eq!(filter_by("nonexistent"), Vec::<String>::new());
    }


    #[test]
    fn test_apply_plugin_transform_selection_and_builtins() {
        let mut workspace = Workspace::new(Config::default());
        let editor = workspace.active_editor_mut().unwrap();
        editor.insert(0, "hello world");
        
        // Test backwards selection normalization (11..6)
        editor.selection = Some(6..11);
        workspace.apply_plugin_transform("to_uppercase");
        assert_eq!(workspace.active_editor().unwrap().rope.to_string(), "hello WORLD");

        // Test forward selection
        let editor = workspace.active_editor_mut().unwrap();
        editor.selection = Some(0..5);
        workspace.apply_plugin_transform("to_uppercase");
        assert_eq!(workspace.active_editor().unwrap().rope.to_string(), "HELLO WORLD");

        // Test whole buffer transform when no selection
        let editor = workspace.active_editor_mut().unwrap();
        editor.selection = None;
        workspace.apply_plugin_transform("to_lowercase");
        assert_eq!(workspace.active_editor().unwrap().rope.to_string(), "hello world");
    }

    #[test]
    fn test_scrollbar_proportions_and_drag_calculations() {
        let mut workspace = Workspace::new(Config::default());
        let editor = workspace.active_editor_mut().unwrap();

        // Populate 100 lines
        let content = (0..100).map(|i| format!("Line {i}")).collect::<Vec<_>>().join("\n");
        editor.insert(0, &content);
        assert_eq!(editor.line_count(), 100);

        let track_height = 800.0f32;
        let line_height = 20.0f32;
        let visible_lines = (track_height / line_height).floor(); // 40.0 lines
        let max_scroll_row = 100 - 40; // 60 rows

        let ratio = (visible_lines / 100.0).clamp(0.04, 0.95); // 0.4
        let thumb_height = (track_height * ratio).max(24.0).min(track_height); // 320.0
        let scrollable_track = track_height - thumb_height; // 480.0

        // At scroll_row = 0
        editor.scroll_row = 0;
        let scroll_ratio = (editor.scroll_row as f32 / max_scroll_row as f32).clamp(0.0, 1.0);
        let thumb_top = scrollable_track * scroll_ratio;
        assert_eq!(thumb_top, 0.0);

        // At scroll_row = 30 (middle)
        editor.scroll_row = 30;
        let scroll_ratio = (editor.scroll_row as f32 / max_scroll_row as f32).clamp(0.0, 1.0);
        let thumb_top = scrollable_track * scroll_ratio;
        assert_eq!(thumb_top, 240.0);

        // At scroll_row = max_scroll_row (60)
        editor.scroll_row = 60;
        let scroll_ratio = (editor.scroll_row as f32 / max_scroll_row as f32).clamp(0.0, 1.0);
        let thumb_top = scrollable_track * scroll_ratio;
        assert_eq!(thumb_top, 480.0);
        assert_eq!(thumb_top + thumb_height, 800.0); // Exactly at track bottom

        // Drag thumb down by 120.0 px
        let drag_target_top = 240.0 + 120.0; // 360.0
        let new_ratio = (drag_target_top / scrollable_track).clamp(0.0, 1.0); // 360 / 480 = 0.75
        let target_scroll_row = (new_ratio * max_scroll_row as f32).round() as usize; // 45
        assert_eq!(target_scroll_row, 45);
        editor.scroll_row = target_scroll_row;
        assert_eq!(editor.scroll_row, 45);
        assert!(editor.selection.is_none());
    }

    #[test]
    fn test_settings_dropdown_state_isolation_and_transitions() {
        use crate::widgets::dialog::SettingsDropdown;

        let mut current_dropdown: Option<SettingsDropdown> = None;

        // 1. Initial state is closed
        assert_eq!(current_dropdown, None);

        // 2. Open Theme dropdown
        current_dropdown = if current_dropdown == Some(SettingsDropdown::Theme) {
            None
        } else {
            Some(SettingsDropdown::Theme)
        };
        assert_eq!(current_dropdown, Some(SettingsDropdown::Theme));

        // 3. Toggling Theme dropdown closes it
        current_dropdown = if current_dropdown == Some(SettingsDropdown::Theme) {
            None
        } else {
            Some(SettingsDropdown::Theme)
        };
        assert_eq!(current_dropdown, None);

        // 4. Open Theme, then switch directly to Language dropdown
        current_dropdown = Some(SettingsDropdown::Theme);
        current_dropdown = if current_dropdown == Some(SettingsDropdown::Language) {
            None
        } else {
            Some(SettingsDropdown::Language)
        };
        assert_eq!(current_dropdown, Some(SettingsDropdown::Language));
        assert_ne!(current_dropdown, Some(SettingsDropdown::Theme));

        // 5. Open Language, then switch directly to Font dropdown
        current_dropdown = if current_dropdown == Some(SettingsDropdown::Font) {
            None
        } else {
            Some(SettingsDropdown::Font)
        };
        assert_eq!(current_dropdown, Some(SettingsDropdown::Font));

        // 6. Dismiss dropdown (backdrop click or Escape)
        current_dropdown = None;
        assert_eq!(current_dropdown, None);
    }

    #[test]
    fn test_settings_dropdown_background_opacity_across_all_themes() {
        use crate::widgets::led_color_to_gpui;
        use zee_core::theme::Theme;

        let themes = Theme::load_all();
        assert!(!themes.is_empty(), "Themes should be available");

        for theme in themes {
            // Dropdown popup background must be fully opaque (alpha == 1.0)
            let panel_bg_gpui = led_color_to_gpui(theme.ui.panel_bg);
            assert_eq!(
                panel_bg_gpui.a, 1.0,
                "Theme '{}' panel_bg must have alpha 1.0 to prevent see-through overlapping",
                theme.meta.name
            );

            let editor_bg_gpui = led_color_to_gpui(theme.editor.background);
            assert_eq!(
                editor_bg_gpui.a, 1.0,
                "Theme '{}' editor background must have alpha 1.0",
                theme.meta.name
            );
        }
    }

    #[test]
    fn test_workspace_file_operations_and_hidden_files() {
        let temp_dir = std::env::temp_dir().join("zee_test_gui_workspace_file_ops");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join(".hidden"), "secret").unwrap();

        let mut workspace = Workspace::new(Config::default());
        workspace.file_tree.set_root(&temp_dir);

        // 1. Initial state without hidden files
        workspace.file_tree.show_hidden = false;
        let items: Vec<String> = workspace.file_tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(!items.contains(&".hidden".to_string()));

        // 2. Toggle show hidden
        let is_shown = workspace.file_tree.toggle_show_hidden();
        assert!(is_shown);
        let items: Vec<String> = workspace.file_tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(items.contains(&".hidden".to_string()));

        // 3. Create file via workspace file tree
        let created = workspace.file_tree.create_file(&temp_dir, "gui_new.rs").unwrap();
        assert!(created.exists());
        let items: Vec<String> = workspace.file_tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(items.contains(&"gui_new.rs".to_string()));

        // 4. Rename file
        let renamed = workspace.file_tree.rename_item(&created, "gui_renamed.rs").unwrap();
        assert!(renamed.exists());
        assert!(!created.exists());
        let items: Vec<String> = workspace.file_tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(items.contains(&"gui_renamed.rs".to_string()));

        // 5. Delete file
        workspace.file_tree.delete_item(&renamed).unwrap();
        assert!(!renamed.exists());
        let items: Vec<String> = workspace.file_tree.flatten().into_iter().map(|i| i.name).collect();
        assert!(!items.contains(&"gui_renamed.rs".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_workspace_toggle_vi_mode_preserves_cursor_position() {
        let mut workspace = Workspace::new(Config::default());
        let editor = workspace.active_editor_mut().unwrap();
        editor.insert(0, "Hello, Vi Mode World!\nSecond Line Test");
        // Place cursor at col 7 ("Vi Mode World!")
        editor.cursor = 7;
        let (line_before, col_before) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_before, 0);
        assert_eq!(col_before, 7);

        // 1. Toggle Vi mode ON (Insert -> Normal)
        workspace.config.vi_mode = true;
        let vi_mode = workspace.config.vi_mode;
        for e in workspace.editors.iter_mut() {
            e.vi_mode = if vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
            e.selection = None;
            e.selection_anchor = None;
            e.ensure_cursor_visible(30, 80, workspace.config.word_wrap);
        }

        let editor = workspace.active_editor().unwrap();
        assert_eq!(editor.vi_mode, zee_core::ViMode::Normal);
        assert_eq!(editor.cursor, 7);
        let (line_vi, col_vi) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_vi, 0);
        assert_eq!(col_vi, 7);

        // 2. Toggle Vi mode OFF (Normal -> Insert)
        workspace.config.vi_mode = false;
        let vi_mode = workspace.config.vi_mode;
        for e in workspace.editors.iter_mut() {
            e.vi_mode = if vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
            e.selection = None;
            e.selection_anchor = None;
            e.ensure_cursor_visible(30, 80, workspace.config.word_wrap);
        }

        let editor = workspace.active_editor().unwrap();
        assert_eq!(editor.vi_mode, zee_core::ViMode::Insert);
        assert_eq!(editor.cursor, 7);
        let (line_after, col_after) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_after, 0);
        assert_eq!(col_after, 7);
    }

    #[test]
    fn test_workspace_page_up_down_home_end_motions() {
        let mut workspace = Workspace::new(Config::default());
        let editor = workspace.active_editor_mut().unwrap();

        // Create 50 lines
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("Line {:02} hello world\n", i));
        }
        editor.insert(0, &content);
        assert_eq!(editor.line_count(), 51);

        // 1. Home and End in standard mode
        editor.cursor = 10;
        editor.move_cursor_home(false);
        assert_eq!(editor.cursor, 0);

        editor.move_cursor_end(false);
        let (line_0, col_end) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_0, 0);
        assert_eq!(col_end, 19); // Length of "Line 00 hello world"

        // 2. PageDown and PageUp in standard mode
        for _ in 0..20 {
            editor.move_cursor_down(false);
        }
        let (line_down, _) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_down, 20);

        for _ in 0..20 {
            editor.move_cursor_up(false);
        }
        let (line_up, _) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_up, 0);

        // 3. Selection with Shift / Visual mode
        editor.cursor = 0;
        editor.selection_anchor = Some(0);
        editor.move_cursor_end(true);
        assert!(editor.selection.is_some());
        assert_eq!(editor.cursor, 19);

        for _ in 0..20 {
            editor.move_cursor_down(true);
        }
        let (line_v_down, _) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_v_down, 20);
        assert!(editor.selection.is_some());

        for _ in 0..20 {
            editor.move_cursor_up(true);
        }
        let (line_v_up, _) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line_v_up, 0);

        // Moving back to home (offset 0) matches anchor 0, so selection becomes empty
        editor.move_cursor_home(true);
        assert_eq!(editor.cursor, 0);

        // 4. Vi mode toggle and preserve navigation
        workspace.config.vi_mode = true;
        let editor = workspace.active_editor_mut().unwrap();
        editor.vi_mode = zee_core::ViMode::Normal;
        editor.selection = None;
        editor.cursor = 15;

        editor.move_cursor_home(false);
        assert_eq!(editor.cursor, 0);

        editor.move_cursor_end(false);
        assert_eq!(editor.cursor, 19);
    }
}

