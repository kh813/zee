use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    execute,
    style::Color,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use anyhow::Result;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use crate::renderer::{Renderer, Cell};
use crate::layout::Layout;
use crate::clipboard;
use crate::widgets::menu::{Menu, MenuItem};
use crate::widgets::find_panel::PanelField;
use crate::widgets::dialog::{self, Dialog, DialogResult};
use zee_core::{Action, Config, I18n, Encoding, LineEnding, buffer::Editor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Menu,
    Panel,
    Dialog,
    Sidebar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOp {
    None,
    Open,
    SaveAs,
    Exit,
    Close,
    Reload,
    NewFile,
    NewFolder,
    Rename,
    Delete,
    OpenFolder,
    ImportConfig,
    ExportConfig,
    ExportAll,
}

pub struct App {
    pub focus: Focus,
    pub running: bool,
    pub width: u16,
    pub height: u16,
    pub renderer: Renderer,
    pub layout: Layout,
    pub last_click_time: Instant,
    pub last_click_pos: (u16, u16),
    pub click_count: u8,
    
    pub config: Config,
    pub i18n: I18n,
    pub menus: Vec<Menu>,
    pub active_menu: Option<usize>,
    pub selected_item: usize,
    pub submenu_stack: Vec<(usize, usize)>, // (menu_idx, item_idx)
    pub dropdown_rects: Vec<(u16, u16, u16, u16, usize, usize)>, // x, y, w, h, depth, item_idx
    pub current_dialog: Option<Box<dyn Dialog>>,
    pub pending_op: PendingOp,
    pub target_encoding: Option<Encoding>,
    #[allow(dead_code)]
    pub target_line_ending: Option<LineEnding>,
    pub target_path: Option<PathBuf>,
    #[allow(dead_code)]
    pub drag_start: Option<usize>,
    
    pub buffers: Vec<Editor>,
    pub active_buffer: usize,

    pub theme: zee_core::theme::Theme,
    pub themes: Vec<zee_core::theme::Theme>,
    pub syntax_defs: Vec<zee_core::syntax::SyntaxDefinition>,

    pub find_panel: crate::widgets::find_panel::FindPanel,
    pub sidebar: crate::widgets::sidebar::Sidebar,
    pub vi_cmd: String,
    pub is_vi_cmd_mode: bool,
    pub vi_message: Option<(String, bool)>,
    pub pending_g: bool,
    pub pending_d: bool,
    pub pending_y: bool,
    pub pending_c: bool,
    pub pending_r: bool,
    pub pending_f: bool,
    pub pending_capital_f: bool,
    pub pending_t: bool,
    pub pending_capital_t: bool,
    pub pending_m: bool,
    pub pending_single_quote: bool,
    pub pending_backtick: bool,
    pub count: usize,
    pub pending_op_count: usize,
}

impl App {
    #[allow(dead_code)]
    pub fn new(paths: Vec<PathBuf>) -> Result<Self> {
        let targets = paths.into_iter().map(|path| zee_core::cli::FileTarget {
            path,
            line: None,
            col: None,
        }).collect();
        Self::with_targets(targets)
    }

    pub fn with_targets(targets: Vec<zee_core::cli::FileTarget>) -> Result<Self> {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        
        let mut config = Config::load();
        config.vi_mode = false;
        let i18n = I18n::load(&config.language);

        let themes = zee_core::theme::Theme::load_all();
        let syntax_defs = zee_core::syntax::SyntaxDefinition::builtins();
        let theme = zee_core::theme::Theme::find_by_name(&config.theme)
            .unwrap_or_else(|| themes.first().cloned().unwrap_or_default());

        let mut buffers = Vec::new();
        let mut errors = Vec::new();
        let mut root_dir = None;
        let mut recent = zee_core::recent::RecentFiles::load();

        if targets.is_empty() {
            buffers.push(Editor::new());
        } else {
            for target in targets {
                if target.path.is_dir() {
                    if root_dir.is_none() {
                        root_dir = Some(target.path.clone());
                    }
                    continue;
                }
                match Editor::from_file(&target.path) {
                    Ok(mut editor) => {
                        // Detect syntax
                        let ext = target.path.extension().and_then(|e| e.to_str());
                        if let Some(ext) = ext {
                            if let Some(def) = syntax_defs.iter().find(|s| s.meta.extensions.iter().any(|e| e == ext)) {
                                if let Ok(highlighter) = zee_core::syntax::SyntaxHighlighter::new(def.clone()) {
                                    editor.update_syntax(Some(highlighter));
                                }
                            }
                        }
                        if let Some(line) = target.line {
                            let line_idx = line.saturating_sub(1);
                            let col_idx = target.col.unwrap_or(1).saturating_sub(1);
                            if line_idx < editor.line_count() {
                                let line_start = editor.rope.line_to_char(line_idx);
                                let line_slice = editor.rope.line(line_idx);
                                let col = col_idx.min(line_slice.len_chars().saturating_sub(1));
                                editor.cursor = line_start + col;
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                        }
                        recent.add(&target.path);
                        buffers.push(editor);
                    }
                    Err(e) => {
                        errors.push(i18n.get("error.failed_to_open")
                            .replace("{path}", &target.path.display().to_string())
                            .replace("{error}", &e.to_string()));
                    }
                }
            }
        }

        if buffers.is_empty() {
            buffers.push(Editor::new());
        }

        for b in buffers.iter_mut() {
            b.vi_mode = if config.vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
        }

        let active_buffer = 0;
        let menus = Self::build_menus(&i18n, &config, buffers.get(active_buffer), &themes, &syntax_defs);

        let mut layout = Layout::new(width, height);
        layout.recompute(&menus, &buffers, active_buffer, config.line_numbers, config.sidebar, &config.sidebar_position, config.vi_mode);

        let root_dir = root_dir.unwrap_or_else(zee_core::file_tree::user_root_dir);
        let mut sidebar = crate::widgets::sidebar::Sidebar::new(root_dir, config.sidebar);
        sidebar.file_tree.set_show_hidden(config.show_hidden);

        let mut app = App {
            focus: Focus::Editor,
            running: true,
            width,
            height,
            renderer: Renderer::new(width, height),
            layout,
            last_click_time: Instant::now(),
            last_click_pos: (0, 0),
            click_count: 0,
            
            config,
            i18n,
            menus,
            active_menu: None,
            selected_item: 0,
            submenu_stack: Vec::new(),
            dropdown_rects: Vec::new(),
            current_dialog: None,
            pending_op: PendingOp::None,
            target_encoding: None,
            target_line_ending: None,
            target_path: None,
            drag_start: None,
            
            buffers,
            active_buffer,

            theme,
            themes,
            syntax_defs,

            find_panel: crate::widgets::find_panel::FindPanel::new(),
            sidebar,
            vi_cmd: String::new(),
            is_vi_cmd_mode: false,
            vi_message: None,
            pending_g: false,
            pending_d: false,
            pending_y: false,
            pending_c: false,
            pending_r: false,
            pending_f: false,
            pending_capital_f: false,
            pending_t: false,
            pending_capital_t: false,
            pending_m: false,
            pending_single_quote: false,
            pending_backtick: false,
            count: 0,
            pending_op_count: 0,
        };

        app.update_active_outline();

        if !errors.is_empty() {
            let message = errors.join("\n");
            app.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                app.i18n.get("error").to_string(),
                message,
                vec![(app.i18n.get("dialog.ok").to_string(), dialog::Action::Confirm)],
            )));
            app.focus = Focus::Dialog;
        }

        Ok(app)
    }

    fn to_ct_color(&self, c: zee_core::theme::Color) -> crossterm::style::Color {
        if self.theme.meta.name == "Terminal Default" {
            match c {
                zee_core::theme::Color::Rgb(0, 0, 0) | zee_core::theme::Color::Rgb(255, 255, 255) => {
                    return crossterm::style::Color::Reset;
                }
                zee_core::theme::Color::Ansi(i) => {
                    return crossterm::style::Color::AnsiValue(i);
                }
                _ => {}
            }
        }
        match c {
            zee_core::theme::Color::Rgb(r, g, b) => crossterm::style::Color::Rgb { r, g, b },
            zee_core::theme::Color::Ansi(i) => crossterm::style::Color::AnsiValue(i),
        }
    }

    fn get_token_color(&self, token: zee_core::syntax::TokenType) -> Color {
        let theme = &self.theme;
        let color = match token {
            zee_core::syntax::TokenType::Keyword => theme.syntax.keyword,
            zee_core::syntax::TokenType::TypeName => theme.syntax.type_name,
            zee_core::syntax::TokenType::Function => theme.syntax.function,
            zee_core::syntax::TokenType::String => theme.syntax.string,
            zee_core::syntax::TokenType::Number => theme.syntax.number,
            zee_core::syntax::TokenType::Comment => theme.syntax.comment,
            zee_core::syntax::TokenType::Operator => theme.syntax.operator,
            zee_core::syntax::TokenType::Punctuation => theme.syntax.punctuation,
            zee_core::syntax::TokenType::Constant => theme.syntax.constant,
            zee_core::syntax::TokenType::Attribute => theme.syntax.attribute,
            zee_core::syntax::TokenType::Error => theme.syntax.error,
        };
        self.to_ct_color(color.unwrap_or(theme.editor.foreground))
    }

    fn detect_syntax(&self, path: &std::path::Path) -> Option<zee_core::syntax::SyntaxHighlighter> {
        let first_line = if let Ok(mut f) = std::fs::File::open(path) {
            use std::io::Read;
            let mut buf = [0u8; 256];
            let n = f.read(&mut buf).unwrap_or(0);
            std::str::from_utf8(&buf[..n]).ok().and_then(|s| s.lines().next().map(|l| l.to_string()))
        } else {
            None
        };
        Editor::detect_syntax_with_content(path, first_line.as_deref())
    }

    fn build_menus(
        i18n: &I18n,
        config: &Config,
        buffer: Option<&Editor>,
        themes: &[zee_core::theme::Theme],
        syntax_defs: &[zee_core::syntax::SyntaxDefinition],
    ) -> Vec<Menu> {
        let cur_enc = buffer.map(|b| b.encoding).unwrap_or(Encoding::Utf8);
        let cur_le = buffer.map(|b| b.line_ending).unwrap_or(LineEnding::Lf);
        let cur_syntax = buffer.and_then(|b| b.syntax_highlighter.as_ref().map(|h| h.def.meta.name.clone())).unwrap_or_else(|| "Plain Text".to_string());

        let encodings = [(Encoding::Utf8, "UTF-8"),
            (Encoding::Utf8Bom, "UTF-8 with BOM"),
            (Encoding::Utf16Le, "UTF-16 LE"),
            (Encoding::Utf16Be, "UTF-16 BE"),
            (Encoding::ShiftJis, "Shift-JIS"),
            (Encoding::EucJp, "EUC-JP"),
            (Encoding::Iso2022Jp, "ISO-2022-JP"),
            (Encoding::Latin1, "Latin-1 (ISO-8859-1)")];

        let reopen_items = encodings.iter().map(|(enc, label)| {
            MenuItem::Action { label: label.to_string(), action: Action::ReopenWithEncoding(*enc), shortcut: None }
        }).collect();

        let convert_items = encodings.iter().map(|(enc, label)| {
            MenuItem::Toggle { label: label.to_string(), action: Action::ConvertToEncoding(*enc), checked: cur_enc == *enc, is_radio: true }
        }).collect();

        let line_ending_items = vec![
            (LineEnding::Lf, "LF"),
            (LineEnding::Crlf, "CRLF"),
            (LineEnding::Cr, "CR"),
        ].into_iter().map(|(le, label)| {
            MenuItem::Toggle { label: label.to_string(), action: Action::SetLineEnding(le), checked: cur_le == le, is_radio: true }
        }).collect();

        let theme_items = themes.iter().map(|t| {
            let id = t.meta.name.to_lowercase().replace(" ", "-");
            MenuItem::Toggle {
                label: t.meta.name.clone(),
                action: Action::SetTheme(id.clone()),
                checked: config.theme == id,
                is_radio: true,
            }
        }).collect();

        let syntax_items = syntax_defs.iter().map(|s| {
            MenuItem::Toggle {
                label: s.meta.name.clone(),
                action: Action::SetSyntax(s.meta.name.clone()),
                checked: cur_syntax == s.meta.name,
                is_radio: true,
            }
        }).collect();

        let language_items = zee_core::i18n::AVAILABLE_LANGUAGES.iter().map(|lang| {
            let is_current = if lang.id == "auto" {
                config.language == "auto" || config.language.is_empty()
            } else {
                config.language == lang.id
            };
            let label = if lang.id == "auto" {
                i18n.get("dialog.settings.language_auto").to_string()
            } else {
                lang.name.to_string()
            };
            MenuItem::Toggle {
                label,
                action: Action::SetLanguage(lang.id.to_string()),
                checked: is_current,
                is_radio: true,
            }
        }).collect();

        let templates = zee_core::template::Template::load_all();
        let mut template_items = Vec::new();
        for tpl in &templates {
            template_items.push(MenuItem::Action {
                label: tpl.name.clone(),
                action: Action::NewFromTemplate(tpl.id.clone()),
                shortcut: None,
            });
        }
        template_items.push(MenuItem::Separator);
        template_items.push(MenuItem::Action {
            label: i18n.get("menu.file.open_templates_folder").to_string(),
            action: Action::OpenTemplatesFolder,
            shortcut: None,
        });

        let recent = zee_core::recent::RecentFiles::load();
        let mut file_items = vec![
            MenuItem::Action { label: i18n.get("menu.file.new").to_string(), action: Action::New, shortcut: Some("Ctrl+T".to_string()) },
            MenuItem::Submenu {
                label: i18n.get("menu.file.new_from_template").to_string(),
                menu: Menu::new("Templates", template_items),
            },
            MenuItem::Action { label: i18n.get("menu.file.open").to_string(), action: Action::Open, shortcut: Some("Ctrl+O".to_string()) },
            MenuItem::Action { label: i18n.get("menu.file.open_folder").to_string(), action: Action::OpenFolder, shortcut: Some("Ctrl+Shift+O".to_string()) },
            MenuItem::Action { label: i18n.get("menu.file.open_gdrive").to_string(), action: Action::OpenGoogleDrive, shortcut: None },
            MenuItem::Separator,
            MenuItem::Action { label: i18n.get("menu.file.reload").to_string(), action: Action::ReloadFile, shortcut: Some("Ctrl+Shift+R".to_string()) },
            MenuItem::Separator,
        ];

        let mut recent_items = Vec::new();
        if recent.files.is_empty() {
            recent_items.push(MenuItem::Action { label: i18n.get("menu.file.no_recent").to_string(), action: Action::NoOp, shortcut: None });
        } else {
            for p in &recent.files {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned());
                recent_items.push(MenuItem::Action {
                    label: name,
                    action: Action::OpenRecent(p.clone()),
                    shortcut: None,
                });
            }
            recent_items.push(MenuItem::Separator);
            recent_items.push(MenuItem::Action { label: i18n.get("menu.file.clear_recent").to_string(), action: Action::ClearRecent, shortcut: None });
        }
        file_items.push(MenuItem::Submenu {
            label: i18n.get("menu.file.open_recent").to_string(),
            menu: Menu::new("Recent", recent_items),
        });
        file_items.push(MenuItem::Separator);
        file_items.push(MenuItem::Action { label: i18n.get("menu.file.save").to_string(), action: Action::Save, shortcut: Some("Ctrl+S".to_string()) });
        file_items.push(MenuItem::Action { label: i18n.get("menu.file.save_as").to_string(), action: Action::SaveAs, shortcut: Some("Ctrl+Shift+S".to_string()) });
        file_items.push(MenuItem::Separator);
        file_items.push(MenuItem::Toggle {
            label: i18n.get("menu.file.trim_trailing_whitespace").to_string(),
            action: Action::ToggleTrimTrailingWhitespace,
            checked: config.trim_trailing_whitespace,
            is_radio: false,
        });
        file_items.push(MenuItem::Toggle {
            label: i18n.get("menu.file.ensure_final_newline").to_string(),
            action: Action::ToggleEnsureFinalNewline,
            checked: config.ensure_final_newline,
            is_radio: false,
        });
        file_items.push(MenuItem::Separator);
        file_items.push(MenuItem::Action {
            label: i18n.get("menu.file.export_config").to_string(),
            action: Action::ExportConfig,
            shortcut: None,
        });
        file_items.push(MenuItem::Action {
            label: i18n.get("menu.file.import_config").to_string(),
            action: Action::ImportConfig,
            shortcut: None,
        });
        file_items.push(MenuItem::Separator);
        file_items.push(MenuItem::Action { label: i18n.get("menu.app.preferences").to_string(), action: Action::OpenSettings, shortcut: Some("Ctrl+,".to_string()) });
        file_items.push(MenuItem::Separator);
        file_items.push(MenuItem::Action { label: i18n.get("menu.file.close").to_string(), action: Action::Close, shortcut: Some("Ctrl+W".to_string()) });
        file_items.push(MenuItem::Separator);
        file_items.push(MenuItem::Action { label: i18n.get("menu.file.exit").to_string(), action: Action::Exit, shortcut: Some("Ctrl+Q".to_string()) });

        let mut plugin_items = Vec::new();
        let mut pm = zee_core::plugin::PluginManager::new();
        let dev_plugin_dir = std::path::PathBuf::from("plugins/zee-plugin-text");
        if dev_plugin_dir.exists() {
            let _ = pm.load_plugin_dir(&dev_plugin_dir);
        }
        let manifests = pm.all_manifests();
        if manifests.is_empty() {
            plugin_items.push(MenuItem::Action {
                label: i18n.get("menu.plugins.no_plugins").to_string(),
                action: Action::NoOp,
                shortcut: None,
            });
        } else {
            for manifest in manifests {
                if manifest.capabilities.commands.is_empty() {
                    plugin_items.push(MenuItem::Action {
                        label: format!("✓ {}", manifest.name),
                        action: Action::NoOp,
                        shortcut: None,
                    });
                } else {
                    let mut cmd_items = Vec::new();
                    for cmd in &manifest.capabilities.commands {
                        cmd_items.push(MenuItem::Action {
                            label: cmd.clone(),
                            action: Action::PluginCommand(cmd.clone()),
                            shortcut: None,
                        });
                    }
                    plugin_items.push(MenuItem::Submenu {
                        label: manifest.name.clone(),
                        menu: Menu::new(&manifest.name, cmd_items),
                    });
                }
            }
        }
        plugin_items.push(MenuItem::Separator);
        plugin_items.push(MenuItem::Action {
            label: i18n.get("menu.plugins.manage").to_string(),
            action: Action::ManagePlugins,
            shortcut: None,
        });
        plugin_items.push(MenuItem::Action {
            label: i18n.get("menu.plugins.open_folder").to_string(),
            action: Action::OpenPluginsFolder,
            shortcut: None,
        });

        vec![
            Menu::new(i18n.get("menu.file"), file_items),
            Menu::new(i18n.get("menu.edit"), vec![
                MenuItem::Action { label: i18n.get("menu.edit.undo").to_string(), action: Action::Undo, shortcut: Some("Ctrl+Z".to_string()) },
                MenuItem::Action { label: i18n.get("menu.edit.redo").to_string(), action: Action::Redo, shortcut: Some("Ctrl+Y".to_string()) },
                MenuItem::Separator,
                MenuItem::Action { label: i18n.get("menu.edit.cut").to_string(), action: Action::Cut, shortcut: Some("Ctrl+X".to_string()) },
                MenuItem::Action { label: i18n.get("menu.edit.copy").to_string(), action: Action::Copy, shortcut: Some("Ctrl+C".to_string()) },
                MenuItem::Action { label: i18n.get("menu.edit.paste").to_string(), action: Action::Paste, shortcut: Some("Ctrl+V".to_string()) },
                MenuItem::Separator,
                MenuItem::Action { label: i18n.get("menu.edit.find").to_string(), action: Action::Find, shortcut: Some("Ctrl+F".to_string()) },
                MenuItem::Action { label: i18n.get("menu.edit.replace").to_string(), action: Action::Replace, shortcut: Some("Ctrl+R".to_string()) },
                MenuItem::Separator,
                MenuItem::Action { label: i18n.get("menu.edit.select_all").to_string(), action: Action::SelectAll, shortcut: Some("Ctrl+A".to_string()) },
                MenuItem::Separator,
                MenuItem::Action { label: i18n.get("menu.edit.format_document").to_string(), action: Action::FormatDocument, shortcut: None },
                MenuItem::Action { label: i18n.get("menu.edit.sort_lines").to_string(), action: Action::SortLines, shortcut: None },
                MenuItem::Separator,
                MenuItem::Action { label: i18n.get("menu.edit.to_uppercase").to_string(), action: Action::ToUpperCase, shortcut: None },
                MenuItem::Action { label: i18n.get("menu.edit.to_lowercase").to_string(), action: Action::ToLowerCase, shortcut: None },
                MenuItem::Action { label: i18n.get("menu.edit.to_snake_case").to_string(), action: Action::ToSnakeCase, shortcut: None },
                MenuItem::Action { label: i18n.get("menu.edit.to_camel_case").to_string(), action: Action::ToCamelCase, shortcut: None },
            ]),
            Menu::new(i18n.get("menu.view"), vec![
                MenuItem::Action { label: i18n.get("menu.view.go_to_line").to_string(), action: Action::GoToLine, shortcut: Some("Ctrl+G".to_string()) },
                MenuItem::Separator,
                MenuItem::Toggle { label: format!("{} (Ctrl+B)", i18n.get("menu.view.sidebar")), action: Action::ToggleSidebar, checked: config.sidebar, is_radio: false },
                MenuItem::Action { label: i18n.get("menu.view.files").to_string(), action: Action::ToggleFiles, shortcut: Some("Alt+1".to_string()) },
                MenuItem::Action { label: i18n.get("menu.view.outline").to_string(), action: Action::ToggleOutline, shortcut: Some("Alt+2".to_string()) },
                MenuItem::Action { label: i18n.get("menu.view.refresh_files").to_string(), action: Action::RefreshFileTree, shortcut: Some("F5".to_string()) },
                MenuItem::Separator,
                MenuItem::Toggle { label: i18n.get("menu.view.line_numbers").to_string(), action: Action::ToggleLineNumbers, checked: config.line_numbers, is_radio: false },
                MenuItem::Toggle { label: i18n.get("menu.view.word_wrap").to_string(), action: Action::ToggleWordWrap, checked: config.word_wrap, is_radio: false },
                MenuItem::Toggle { label: format!("{} (Ctrl+E)", i18n.get("menu.view.vi_mode")), action: Action::ToggleViMode, checked: config.vi_mode, is_radio: false },
                MenuItem::Separator,
                MenuItem::Submenu { label: i18n.get("menu.view.encoding").to_string(), menu: Menu::new(i18n.get("menu.view.encoding"), vec![
                    MenuItem::Submenu { label: i18n.get("menu.view.reopen_with_encoding").to_string(), menu: Menu::new(i18n.get("menu.view.reopen_with_encoding"), reopen_items)},
                    MenuItem::Submenu { label: i18n.get("menu.view.convert_to_encoding").to_string(), menu: Menu::new(i18n.get("menu.view.convert_to_encoding"), convert_items)},
                ])},
                MenuItem::Submenu { label: i18n.get("menu.view.line_ending").to_string(), menu: Menu::new(i18n.get("menu.view.line_ending"), line_ending_items)},
                MenuItem::Separator,
                MenuItem::Submenu { label: i18n.get("menu.view.theme").to_string(), menu: Menu::new(i18n.get("menu.view.theme"), theme_items)},
                MenuItem::Submenu { label: i18n.get("menu.view.syntax").to_string(), menu: Menu::new(i18n.get("menu.view.syntax"), syntax_items)},
                MenuItem::Submenu { label: i18n.get("menu.view.language").to_string(), menu: Menu::new(i18n.get("menu.view.language"), language_items)},
            ]),
            Menu::new(i18n.get("menu.plugins"), plugin_items),
            Menu::new(i18n.get("menu.help"), vec![
                MenuItem::Action { label: i18n.get("menu.help.about").to_string(), action: Action::About, shortcut: Some("Ctrl+H".to_string()) },
                MenuItem::Action { label: i18n.get("menu.help.check_for_updates").to_string(), action: Action::CheckForUpdates, shortcut: None },
            ]),
        ]
    }

    pub fn recompute_layout(&mut self) {
        self.layout.recompute(
            &self.menus,
            &self.buffers,
            self.active_buffer,
            self.config.line_numbers,
            self.sidebar.visible,
            &self.config.sidebar_position,
            self.config.vi_mode,
        );
    }


    pub fn update_active_outline(&mut self) {
        if let Some(buffer) = self.buffers.get(self.active_buffer) {
            let content = buffer.rope.to_string();
            let lang = buffer.path.as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .unwrap_or("markdown");
            self.sidebar.update_outline(&content, lang);
        }
    }

    pub fn apply_plugin_transform(&mut self, cmd: &str) {
        let (has_selection, range, text_to_transform) = if let Some(buffer) = self.buffers.get(self.active_buffer) {
            if let Some(range) = buffer.selection.clone() {
                let start = range.start.min(range.end);
                let end = range.start.max(range.end);
                if start < end {
                    (true, start..end, buffer.rope.slice(start..end).to_string())
                } else {
                    (false, 0..0, buffer.rope.to_string())
                }
            } else {
                (false, 0..0, buffer.rope.to_string())
            }
        } else {
            return;
        };

        let transformed = self.sidebar.plugin_manager
            .transform_text(cmd, &text_to_transform)
            .unwrap_or(text_to_transform);

        let is_insertion_cmd = cmd.starts_with("lorem_") || cmd == "generate_toc";

        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            if has_selection {
                buffer.delete(range.clone());
                buffer.insert(range.start, &transformed);
                buffer.cursor = range.start + transformed.chars().count();
                buffer.selection = Some(range.start..buffer.cursor);
            } else if is_insertion_cmd {
                let insert_pos = buffer.cursor.min(buffer.rope.len_chars());
                buffer.insert(insert_pos, &transformed);
                buffer.cursor = insert_pos + transformed.chars().count();
                buffer.selection = None;
            } else {
                buffer.delete(0..buffer.rope.len_chars());
                buffer.insert(0, &transformed);
                buffer.cursor = buffer.cursor.min(buffer.rope.len_chars());
            }
        }
        self.ensure_cursor_visible();
        self.update_active_outline();
    }

    fn new_editor(&self) -> Editor {
        let mut editor = Editor::new();
        editor.vi_mode = if self.config.vi_mode {
            zee_core::ViMode::Normal
        } else {
            zee_core::ViMode::Insert
        };
        editor
    }

    fn editor_from_file(&self, path: impl AsRef<Path>) -> Result<Editor> {
        let mut editor = Editor::from_file(path)?;
        editor.vi_mode = if self.config.vi_mode {
            zee_core::ViMode::Normal
        } else {
            zee_core::ViMode::Insert
        };
        Ok(editor)
    }

    pub fn open_or_switch_to_file(&mut self, path: PathBuf) {
        if let Some(idx) = self.buffers.iter().position(|b| b.path.as_ref() == Some(&path)) {
            self.active_buffer = idx;
        } else {
            match self.editor_from_file(&path) {
                Ok(mut editor) => {
                    let ext = path.extension().and_then(|e| e.to_str());
                    if let Some(ext) = ext {
                        if let Some(def) = self.syntax_defs.iter().find(|s| s.meta.extensions.iter().any(|e| e == ext)) {
                            if let Ok(highlighter) = zee_core::syntax::SyntaxHighlighter::new(def.clone()) {
                                editor.update_syntax(Some(highlighter));
                            }
                        }
                    }
                    if self.buffers.len() == 1
                        && self.buffers[0].path.is_none()
                        && !self.buffers[0].is_modified()
                        && self.buffers[0].rope.len_chars() == 0
                    {
                        self.buffers[0] = editor;
                        self.active_buffer = 0;
                    } else {
                        self.buffers.push(editor);
                        self.active_buffer = self.buffers.len() - 1;
                    }
                }
                Err(e) => {
                    self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                        self.i18n.get("error").to_string(),
                        e.to_string(),
                        vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Confirm)],
                    )));
                    self.focus = Focus::Dialog;
                    return;
                }
            }
        }
        self.update_active_outline();
        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
        self.recompute_layout();
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let (_sx, _sy, _sw, sh) = self.layout.sidebar_bounds();
        let viewport_h = sh.saturating_sub(1) as usize;

        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Editor;
            }
            KeyCode::Tab => {
                self.sidebar.active_tab = match self.sidebar.active_tab {
                    crate::widgets::sidebar::SidebarTab::Files => crate::widgets::sidebar::SidebarTab::Outline,
                    crate::widgets::sidebar::SidebarTab::Outline => crate::widgets::sidebar::SidebarTab::Files,
                };
            }
            KeyCode::Char('1') => {
                self.sidebar.active_tab = crate::widgets::sidebar::SidebarTab::Files;
            }
            KeyCode::Char('2') => {
                self.sidebar.active_tab = crate::widgets::sidebar::SidebarTab::Outline;
            }
            KeyCode::Up => {
                self.sidebar.select_prev(viewport_h);
            }
            KeyCode::Down => {
                self.sidebar.select_next(viewport_h);
            }
            KeyCode::Left => {
                self.sidebar.handle_left();
            }
            KeyCode::Right => {
                self.sidebar.handle_right();
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let action = self.sidebar.activate_current();
                match action {
                    crate::widgets::sidebar::SidebarAction::OpenFile(path) => {
                        self.open_or_switch_to_file(path);
                        self.focus = Focus::Editor;
                    }
                    crate::widgets::sidebar::SidebarAction::JumpToLine(line) => {
                        if let Some(buf) = self.buffers.get_mut(self.active_buffer) {
                            let line_idx = line.min(buf.line_count().saturating_sub(1));
                            buf.cursor = buf.rope.line_to_char(line_idx);
                            buf.scroll_row = line_idx.saturating_sub(5);
                            buf.selection = None;
                            buf.selection_anchor = None;
                        }
                        self.ensure_cursor_visible();
                        self.focus = Focus::Editor;
                    }
                    crate::widgets::sidebar::SidebarAction::ToggleHidden => {
                        let new_val = self.sidebar.toggle_show_hidden();
                        self.config.show_hidden = new_val;
                        let _ = zee_core::Config::save_show_hidden(new_val);
                    }
                    crate::widgets::sidebar::SidebarAction::None => {}
                }
            }
            KeyCode::F(5) | KeyCode::Char('r') | KeyCode::Char('R') => {
                self.sidebar.refresh_files();
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                if self.sidebar.active_tab == crate::widgets::sidebar::SidebarTab::Files {
                    let new_val = self.sidebar.toggle_show_hidden();
                    self.config.show_hidden = new_val;
                    let _ = zee_core::Config::save_show_hidden(new_val);
                }
            }
            KeyCode::Char('m') | KeyCode::Char('M') | KeyCode::F(10) => {
                if self.sidebar.active_tab == crate::widgets::sidebar::SidebarTab::Files {
                    let items = self.sidebar.flatten_files();
                    if let Some(item) = items.get(self.sidebar.selected_file_idx) {
                        self.current_dialog = Some(Box::new(dialog::FileContextMenuDialog::new(
                            item.path.clone(),
                            item.is_dir,
                            self.sidebar.file_tree.show_hidden,
                            &self.i18n,
                        )));
                        self.focus = Focus::Dialog;
                    }
                }
            }
            _ => {}
        }
    }

    fn render_sidebar(&mut self) {
        let bounds = self.layout.sidebar_bounds();
        let is_focused = self.focus == Focus::Sidebar;
        let active_path = self.buffers.get(self.active_buffer).and_then(|b| b.path.as_deref());
        self.sidebar.render(
            &mut self.renderer,
            bounds,
            is_focused,
            &self.theme,
            active_path,
            self.layout.is_right_sidebar,
        );
    }

    pub fn run(&mut self) -> Result<()> {
        let mut stdout = io::stdout();
        self.init_terminal(&mut stdout)?;

        while self.running {
            if self.width < 40 || self.height < 24 {
                self.render_too_small(&mut stdout)?;
            } else {
                self.render(&mut stdout)?;
            }

            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key),
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Resize(w, h) => {
                        self.width = w;
                        self.height = h;
                        self.renderer.resize(w, h);
                        self.layout.width = w;
                        self.layout.height = h;
                        self.recompute_layout();
                    }
                    _ => {}
                }
            }
        }

        self.cleanup_terminal(&mut stdout)?;
        Ok(())
    }

    fn init_terminal(&self, stdout: &mut Stdout) -> Result<()> {
        terminal::enable_raw_mode()?;
        execute!(
            stdout,
            EnterAlternateScreen,
            event::EnableMouseCapture,
            event::PushKeyboardEnhancementFlags(
                event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            ),
            cursor::Hide
        )?;
        
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, event::PopKeyboardEnhancementFlags);
            let _ = terminal::disable_raw_mode();
            let _ = execute!(
                stdout,
                LeaveAlternateScreen,
                event::DisableMouseCapture,
                cursor::Show,
                cursor::SetCursorStyle::DefaultUserShape
            );
            original_hook(panic_info);
        }));

        Ok(())
    }

    fn cleanup_terminal(&self, stdout: &mut Stdout) -> Result<()> {
        let _ = execute!(stdout, event::PopKeyboardEnhancementFlags);
        terminal::disable_raw_mode()?;
        execute!(
            stdout,
            LeaveAlternateScreen,
            event::DisableMouseCapture,
            cursor::Show,
            cursor::SetCursorStyle::DefaultUserShape
        )?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Modal dialog ALWAYS captures all keyboard input!
        if self.current_dialog.is_some() {
            self.focus = Focus::Dialog;
            self.handle_dialog_key(key);
            return;
        }

        // F4 toggles Vi mode globally
        if key.code == KeyCode::F(4) {
            self.perform_action(Action::ToggleViMode);
            return;
        }

        // F5 refreshes file tree
        if key.code == KeyCode::F(5) {
            self.perform_action(Action::RefreshFileTree);
            return;
        }

        // Global shortcuts (Ctrl+...) only if not in a dialog
        if self.focus != Focus::Dialog && key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('q') => { self.perform_action(Action::Exit); return; }
                KeyCode::Char('t') | KeyCode::Char('n') => { self.perform_action(Action::New); return; }
                KeyCode::Char('o') => { self.perform_action(Action::Open); return; }
                KeyCode::Char('s') => { self.perform_action(Action::Save); return; }
                KeyCode::Char('w') => { self.perform_action(Action::Close); return; }
                KeyCode::Char('z') => { self.perform_action(Action::Undo); return; }
                KeyCode::Char('y') => {
                    if !self.config.vi_mode {
                        self.perform_action(Action::Redo);
                        return;
                    }
                }
                KeyCode::Char('f') => { self.perform_action(Action::Find); return; }
                KeyCode::Char('r') => { self.perform_action(Action::Replace); return; }
                KeyCode::Char('b') => { self.perform_action(Action::ToggleSidebar); return; }
                KeyCode::Char('h') => { self.perform_action(Action::About); return; }
                KeyCode::Char('e') => { self.perform_action(Action::ToggleViMode); return; }
                KeyCode::Char(',') => { self.perform_action(Action::OpenSettings); return; }
                KeyCode::Tab | KeyCode::PageDown | KeyCode::Char(']') => {
                    self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                    if self.layout.panel_height > 0 {
                        self.run_search();
                    }
                    return;
                }
                KeyCode::PageUp | KeyCode::Char('[') => {
                    self.active_buffer = if self.active_buffer == 0 { self.buffers.len() - 1 } else { self.active_buffer - 1 };
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                    if self.layout.panel_height > 0 {
                        self.run_search();
                    }
                    return;
                }
                _ => {}
            }
        }
        if self.focus != Focus::Dialog && key.modifiers == (KeyModifiers::CONTROL | KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Char('O') | KeyCode::Char('o') => { self.perform_action(Action::OpenFolder); return; }
                KeyCode::Char('F') | KeyCode::Char('f') => { self.perform_action(Action::Replace); return; }
                KeyCode::Char('S') | KeyCode::Char('s') => { self.perform_action(Action::SaveAs); return; }
                KeyCode::Char('R') | KeyCode::Char('r') => { self.perform_action(Action::ReloadFile); return; }
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('[') | KeyCode::Char('{') => {
                    self.active_buffer = if self.active_buffer == 0 { self.buffers.len() - 1 } else { self.active_buffer - 1 };
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                    if self.layout.panel_height > 0 {
                        self.run_search();
                    }
                    return;
                }
                KeyCode::Char(']') | KeyCode::Char('}') => {
                    self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                    if self.layout.panel_height > 0 {
                        self.run_search();
                    }
                    return;
                }
                _ => {}
            }
        }

        // Handle Alt shortcuts for menus, sidebar, and tabs (only if not in a dialog)
        if self.focus != Focus::Dialog && key.modifiers == KeyModifiers::ALT {
            match key.code {
                KeyCode::Char('f') => { self.open_menu(0); return; }
                KeyCode::Char('e') => { self.open_menu(1); return; }
                KeyCode::Char('v') => { self.open_menu(2); return; }
                KeyCode::Char('p') => { self.open_menu(3); return; }
                KeyCode::Char('h') => { self.open_menu(4); return; }
                KeyCode::Left => {
                    self.active_buffer = if self.active_buffer == 0 { self.buffers.len() - 1 } else { self.active_buffer - 1 };
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                    if self.layout.panel_height > 0 {
                        self.run_search();
                    }
                    return;
                }
                KeyCode::Right => {
                    self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                    if self.layout.panel_height > 0 {
                        self.run_search();
                    }
                    return;
                }
                KeyCode::Char('1') => {
                    self.perform_action(Action::ToggleFiles);
                    return;
                }
                KeyCode::Char('2') => {
                    self.perform_action(Action::ToggleOutline);
                    return;
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.perform_action(Action::ToggleViMode);
                    return;
                }
                _ => {}
            }
        }
        
        // Dispatch based on focus
        match self.focus {
            Focus::Menu => self.handle_menu_key(key),
            Focus::Dialog => self.handle_dialog_key(key),
            Focus::Panel => self.handle_panel_key(key),
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::Editor => {
                if self.config.vi_mode {
                    self.handle_vi_key(key);
                } else {
                    self.handle_editor_key(key);
                }
            }
        }
    }

    fn handle_vi_key(&mut self, key: KeyEvent) {
        if self.is_vi_cmd_mode {
            self.handle_vi_cmd_key(key);
            return;
        }

        let buffer = if let Some(b) = self.buffers.get_mut(self.active_buffer) {
            b
        } else {
            return;
        };

        // Handle Ctrl+V / KeyCode::Char('v') with CONTROL modifier for Visual Block mode
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v')
            && buffer.vi_mode != zee_core::ViMode::Insert {
                if buffer.vi_mode == zee_core::ViMode::VisualBlock {
                    buffer.vi_mode = zee_core::ViMode::Normal;
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                } else {
                    buffer.vi_mode = zee_core::ViMode::VisualBlock;
                    buffer.ensure_selection();
                }
                self.ensure_cursor_visible();
                return;
            }

        if key.modifiers.contains(KeyModifiers::CONTROL) && buffer.vi_mode != zee_core::ViMode::Insert {
            match key.code {
                KeyCode::Char('r') => {
                    self.perform_action(Action::Redo);
                    return;
                }
                KeyCode::Char('d') => {
                    let is_visual = buffer.vi_mode != zee_core::ViMode::Normal;
                    for _ in 0..10 {
                        if self.config.word_wrap {
                            self.move_cursor_vdown(is_visual);
                        } else if let Some(b) = self.buffers.get_mut(self.active_buffer) {
                            b.move_cursor_down(is_visual);
                        }
                    }
                    self.ensure_cursor_visible();
                    return;
                }
                KeyCode::Char('u') => {
                    let is_visual = buffer.vi_mode != zee_core::ViMode::Normal;
                    for _ in 0..10 {
                        if self.config.word_wrap {
                            self.move_cursor_vup(is_visual);
                        } else if let Some(b) = self.buffers.get_mut(self.active_buffer) {
                            b.move_cursor_up(is_visual);
                        }
                    }
                    self.ensure_cursor_visible();
                    return;
                }
                KeyCode::Char('f') => {
                    let is_visual = buffer.vi_mode != zee_core::ViMode::Normal;
                    for _ in 0..20 {
                        if self.config.word_wrap {
                            self.move_cursor_vdown(is_visual);
                        } else if let Some(b) = self.buffers.get_mut(self.active_buffer) {
                            b.move_cursor_down(is_visual);
                        }
                    }
                    self.ensure_cursor_visible();
                    return;
                }
                KeyCode::Char('b') => {
                    let is_visual = buffer.vi_mode != zee_core::ViMode::Normal;
                    for _ in 0..20 {
                        if self.config.word_wrap {
                            self.move_cursor_vup(is_visual);
                        } else if let Some(b) = self.buffers.get_mut(self.active_buffer) {
                            b.move_cursor_up(is_visual);
                        }
                    }
                    self.ensure_cursor_visible();
                    return;
                }
                KeyCode::Char('e') => {
                    let (_, _, _, _eh) = self.layout.editor_bounds();
                    let is_visual = buffer.vi_mode != zee_core::ViMode::Normal;
                    let max_scroll = buffer.line_count().saturating_sub(1);
                    if buffer.scroll_row < max_scroll {
                        buffer.scroll_row += 1;
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        if line < buffer.scroll_row {
                            if self.config.word_wrap {
                                self.move_cursor_vdown(is_visual);
                            } else {
                                buffer.move_cursor_down(is_visual);
                            }
                        }
                    }
                    self.ensure_cursor_visible();
                    return;
                }
                KeyCode::Char('y') => {
                    let is_visual = buffer.vi_mode != zee_core::ViMode::Normal;
                    if buffer.scroll_row > 0 {
                        buffer.scroll_row -= 1;
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        let (_, _, _, eh) = self.layout.editor_bounds();
                        if line >= buffer.scroll_row + (eh as usize) {
                            if self.config.word_wrap {
                                self.move_cursor_vup(is_visual);
                            } else {
                                buffer.move_cursor_up(is_visual);
                            }
                        }
                    }
                    self.ensure_cursor_visible();
                    return;
                }
                _ => {}
            }
        }

        match buffer.vi_mode {
            zee_core::ViMode::Normal => self.handle_vi_normal_key(key),
            zee_core::ViMode::Insert => {
                if key.code == KeyCode::Esc {
                    buffer.vi_mode = zee_core::ViMode::Normal;
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                } else {
                    self.handle_editor_key(key);
                }
            }
            zee_core::ViMode::Visual | zee_core::ViMode::VisualLine | zee_core::ViMode::VisualBlock => self.handle_vi_visual_key(key),
        }
    }

    fn handle_vi_normal_key(&mut self, key: KeyEvent) {
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(zee_core::normalize_vi_char(c)),
            other => other,
        };

        if !self.pending_r && !self.pending_f && !self.pending_capital_f && !self.pending_t && !self.pending_capital_t
            && !self.pending_m && !self.pending_single_quote && !self.pending_backtick
            && !self.pending_g
        {
            if let KeyCode::Char(c) = code {
                if (self.count == 0 && matches!(c, '1'..='9')) || (self.count > 0 && matches!(c, '0'..='9')) {
                    if let Some(digit) = c.to_digit(10) {
                        self.count = self.count.saturating_mul(10).saturating_add(digit as usize);
                        return;
                    }
                }
            }
        }

        let has_count = self.count > 0;
        let count_val = if has_count { self.count } else { 1 };
        let repeat = if self.pending_d || self.pending_c || self.pending_y {
            self.pending_op_count.max(1) * count_val
        } else {
            count_val
        };
        self.count = 0;

        if matches!(code, KeyCode::Char('っ') | KeyCode::Char('ッ')) {
            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                let (line, _) = buffer.char_to_line_col(buffer.cursor);
                buffer.select_line(line);
                if let Some(range) = buffer.selection.clone() {
                    let text = buffer.rope.slice(range.clone()).to_string();
                    let _ = clipboard::set_clipboard(&text);
                    buffer.delete(range);
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            self.ensure_cursor_visible();
            return;
        }

        if self.pending_r {
            if let KeyCode::Char(c) = code {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.cursor < buffer.rope.len_chars() {
                        let (line, col) = buffer.char_to_line_col(buffer.cursor);
                        let max_col = buffer.get_line_max_col(line);
                        if col < max_col {
                            buffer.delete(buffer.cursor..buffer.cursor + 1);
                            buffer.insert(buffer.cursor, &c.to_string());
                            buffer.cursor = buffer.cursor.saturating_sub(1);
                        }
                    }
                }
            }
            self.pending_r = false;
            self.ensure_cursor_visible();
            return;
        }

        if self.pending_f || self.pending_capital_f || self.pending_t || self.pending_capital_t {
            let forward = self.pending_f || self.pending_t;
            let till = self.pending_t || self.pending_capital_t;
            self.pending_f = false;
            self.pending_capital_f = false;
            self.pending_t = false;
            self.pending_capital_t = false;

            if let KeyCode::Char(c) = code {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(pos) = buffer.find_and_record_inline_char(c, forward, till) {
                        buffer.cursor = pos;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            self.ensure_cursor_visible();
            return;
        }

        if self.pending_m {
            self.pending_m = false;
            if let KeyCode::Char(c) = code {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.set_mark(c);
                }
            }
            self.ensure_cursor_visible();
            return;
        }

        if self.pending_single_quote || self.pending_backtick {
            let line_only = self.pending_single_quote;
            self.pending_single_quote = false;
            self.pending_backtick = false;
            if let KeyCode::Char(c) = code {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.jump_to_mark(c, line_only, false);
                }
            }
            self.ensure_cursor_visible();
            return;
        }

        if self.pending_d {
            let mut handled = true;
            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                let start_pos = buffer.cursor;
                let target_pos = match code {
                    KeyCode::Char('d') | KeyCode::Char('っ') | KeyCode::Char('ッ') => {
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        let line_start = buffer.rope.line_to_char(line);
                        let target_end_line = (line + repeat).min(buffer.rope.len_lines());
                        let next_line_start = if target_end_line < buffer.rope.len_lines() {
                            buffer.rope.line_to_char(target_end_line)
                        } else {
                            buffer.rope.len_chars()
                        };
                        let range = line_start..next_line_start;
                        if !range.is_empty() {
                            let text = buffer.rope.slice(range.clone()).to_string();
                            let _ = clipboard::set_clipboard(&text);
                            buffer.delete(range);
                            buffer.cursor = line_start.min(buffer.rope.len_chars());
                            let (new_line, _) = buffer.char_to_line_col(buffer.cursor);
                            let max_col = buffer.get_line_max_col(new_line);
                            let col = buffer.cursor - buffer.rope.line_to_char(new_line);
                            if col > max_col {
                                buffer.cursor = buffer.line_col_to_char(new_line, max_col);
                            }
                            buffer.selection = None;
                            buffer.selection_anchor = None;
                        }
                        None
                    }
                    KeyCode::Char('w') => {
                        buffer.move_word_forward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('e') => {
                        buffer.move_word_end(false);
                        Some((buffer.cursor + 1).min(buffer.rope.len_chars()))
                    }
                    KeyCode::Char('b') => {
                        buffer.move_word_backward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('$') => {
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                        Some(line_end)
                    }
                    KeyCode::Char('W') => {
                        buffer.move_bigword_forward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('E') => {
                        buffer.move_bigword_end(false);
                        Some((buffer.cursor + 1).min(buffer.rope.len_chars()))
                    }
                    KeyCode::Char('B') => {
                        buffer.move_bigword_backward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('{') => {
                        for _ in 0..repeat { buffer.move_to_prev_paragraph(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('}') => {
                        for _ in 0..repeat { buffer.move_to_next_paragraph(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('(') => {
                        for _ in 0..repeat { buffer.move_to_prev_sentence(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char(')') => {
                        for _ in 0..repeat { buffer.move_to_next_sentence(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('0') | KeyCode::Char('^') | KeyCode::Char('_') => {
                        let line = buffer.rope.char_to_line(buffer.cursor);
                        let line_start = if matches!(code, KeyCode::Char('0')) {
                            buffer.rope.line_to_char(line)
                        } else {
                            let line_str = buffer.rope.line(line).to_string();
                            let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                            buffer.line_col_to_char(line, indent)
                        };
                        Some(line_start)
                    }
                    KeyCode::Char('h') => Some(buffer.cursor.saturating_sub(1)),
                    KeyCode::Char('l') => Some((buffer.cursor + 1).min(buffer.rope.len_chars())),
                    _ => {
                        handled = false;
                        None
                    }
                };

                if let Some(end_pos) = target_pos {
                    let range = if start_pos <= end_pos {
                        start_pos..end_pos
                    } else {
                        end_pos..start_pos
                    };
                    if !range.is_empty() {
                        let text = buffer.rope.slice(range.clone()).to_string();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete(range.clone());
                        buffer.cursor = range.start;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            self.pending_d = false;
            self.pending_op_count = 0;
            if handled {
                self.ensure_cursor_visible();
                return;
            }
        }

        if self.pending_c {
            let mut handled = true;
            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                let start_pos = buffer.cursor;
                let target_pos = match code {
                    KeyCode::Char('c') => {
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        let line_start = buffer.rope.line_to_char(line);
                        let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                        let range = line_start..line_end;
                        if !range.is_empty() {
                            let text = buffer.rope.slice(range.clone()).to_string();
                            let _ = clipboard::set_clipboard(&text);
                            buffer.delete(range);
                            buffer.cursor = line_start;
                        }
                        buffer.vi_mode = zee_core::ViMode::Insert;
                        None
                    }
                    KeyCode::Char('w') => {
                        buffer.move_word_forward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('e') => {
                        buffer.move_word_end(false);
                        Some((buffer.cursor + 1).min(buffer.rope.len_chars()))
                    }
                    KeyCode::Char('b') => {
                        buffer.move_word_backward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('$') => {
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                        Some(line_end)
                    }
                    KeyCode::Char('W') => {
                        buffer.move_bigword_forward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('E') => {
                        buffer.move_bigword_end(false);
                        Some((buffer.cursor + 1).min(buffer.rope.len_chars()))
                    }
                    KeyCode::Char('B') => {
                        buffer.move_bigword_backward(false);
                        Some(buffer.cursor)
                    }
                    KeyCode::Char('{') => {
                        for _ in 0..repeat { buffer.move_to_prev_paragraph(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('}') => {
                        for _ in 0..repeat { buffer.move_to_next_paragraph(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('(') => {
                        for _ in 0..repeat { buffer.move_to_prev_sentence(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char(')') => {
                        for _ in 0..repeat { buffer.move_to_next_sentence(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('0') | KeyCode::Char('^') | KeyCode::Char('_') => {
                        let line = buffer.rope.char_to_line(buffer.cursor);
                        let line_start = if matches!(code, KeyCode::Char('0')) {
                            buffer.rope.line_to_char(line)
                        } else {
                            let line_str = buffer.rope.line(line).to_string();
                            let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                            buffer.line_col_to_char(line, indent)
                        };
                        Some(line_start)
                    }
                    _ => {
                        handled = false;
                        None
                    }
                };

                if let Some(end_pos) = target_pos {
                    let range = if start_pos <= end_pos {
                        start_pos..end_pos
                    } else {
                        end_pos..start_pos
                    };
                    if !range.is_empty() {
                        let text = buffer.rope.slice(range.clone()).to_string();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete(range.clone());
                        buffer.cursor = range.start;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            self.pending_c = false;
            self.pending_op_count = 0;
            if handled {
                self.ensure_cursor_visible();
                return;
            }
        }

        if self.pending_y {
            let mut handled = true;
            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                let start_pos = buffer.cursor;
                let target_pos = match code {
                    KeyCode::Char('y') => {
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        buffer.select_line(line);
                        if let Some(range) = buffer.selection.clone() {
                            let text = buffer.rope.slice(range).to_string();
                            let _ = clipboard::set_clipboard(&text);
                            buffer.selection = None;
                            buffer.selection_anchor = None;
                        }
                        None
                    }
                    KeyCode::Char('w') => {
                        buffer.move_word_forward(false);
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('e') => {
                        buffer.move_word_end(false);
                        let pos = (buffer.cursor + 1).min(buffer.rope.len_chars());
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('b') => {
                        buffer.move_word_backward(false);
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('$') => {
                        let (line, _) = buffer.char_to_line_col(buffer.cursor);
                        let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                        Some(line_end)
                    }
                    KeyCode::Char('W') => {
                        buffer.move_bigword_forward(false);
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('E') => {
                        buffer.move_bigword_end(false);
                        let pos = (buffer.cursor + 1).min(buffer.rope.len_chars());
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('B') => {
                        buffer.move_bigword_backward(false);
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('{') => {
                        for _ in 0..repeat { buffer.move_to_prev_paragraph(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('}') => {
                        for _ in 0..repeat { buffer.move_to_next_paragraph(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('(') => {
                        for _ in 0..repeat { buffer.move_to_prev_sentence(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char(')') => {
                        for _ in 0..repeat { buffer.move_to_next_sentence(false); }
                        let pos = buffer.cursor;
                        buffer.cursor = start_pos;
                        Some(pos)
                    }
                    KeyCode::Char('0') | KeyCode::Char('^') | KeyCode::Char('_') => {
                        let line = buffer.rope.char_to_line(buffer.cursor);
                        let line_start = if matches!(code, KeyCode::Char('0')) {
                            buffer.rope.line_to_char(line)
                        } else {
                            let line_str = buffer.rope.line(line).to_string();
                            let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                            buffer.line_col_to_char(line, indent)
                        };
                        Some(line_start)
                    }
                    _ => {
                        handled = false;
                        None
                    }
                };

                if let Some(end_pos) = target_pos {
                    let range = if start_pos <= end_pos {
                        start_pos..end_pos
                    } else {
                        end_pos..start_pos
                    };
                    if !range.is_empty() {
                        let text = buffer.rope.slice(range).to_string();
                        let _ = clipboard::set_clipboard(&text);
                    }
                }
            }
            self.pending_y = false;
            self.pending_op_count = 0;
            if handled {
                self.ensure_cursor_visible();
                return;
            }
        }
        
        match code {
            KeyCode::Char('i') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('I') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let line = buffer.rope.char_to_line(buffer.cursor);
                    let line_str = buffer.rope.line(line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(line, indent);
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('a') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_right(false);
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('A') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    buffer.cursor = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('o') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_end(false);
                    buffer.insert(buffer.cursor, "\n");
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('O') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_home(false);
                    buffer.insert(buffer.cursor, "\n");
                    buffer.move_cursor_up(false);
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('v') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.vi_mode = zee_core::ViMode::Visual;
                    buffer.ensure_selection();
                }
            }
            KeyCode::Char('V') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.vi_mode = zee_core::ViMode::VisualLine;
                    buffer.ensure_selection();
                    buffer.update_selection();
                }
            }
            KeyCode::Char('{') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_to_prev_paragraph(false);
                    }
                }
            }
            KeyCode::Char('}') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_to_next_paragraph(false);
                    }
                }
            }
            KeyCode::Char('(') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_to_prev_sentence(false);
                    }
                }
            }
            KeyCode::Char(')') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_to_next_sentence(false);
                    }
                }
            }
            KeyCode::Char('H') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let offset = repeat.saturating_sub(1);
                    let target_line = (buffer.scroll_row + offset).min(buffer.line_count().saturating_sub(1));
                    let line_str = buffer.rope.line(target_line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(target_line, indent);
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            KeyCode::Char('M') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (_, _, _, eh) = self.layout.editor_bounds();
                    let half = (eh as usize) / 2;
                    let target_line = (buffer.scroll_row + half).min(buffer.line_count().saturating_sub(1));
                    let line_str = buffer.rope.line(target_line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(target_line, indent);
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            KeyCode::Char('L') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (_, _, _, eh) = self.layout.editor_bounds();
                    let offset = repeat.saturating_sub(1);
                    let visible_bottom = buffer.scroll_row + (eh as usize).saturating_sub(1);
                    let target_line = visible_bottom.saturating_sub(offset).min(buffer.line_count().saturating_sub(1));
                    let line_str = buffer.rope.line(target_line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(target_line, indent);
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            KeyCode::Char('h') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_cursor_left(false);
                    }
                }
            }
            KeyCode::Char('j') => {
                for _ in 0..repeat {
                    if self.config.word_wrap {
                        self.move_cursor_vdown(false);
                    } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.move_cursor_down(false);
                    }
                }
            }
            KeyCode::Char('k') => {
                for _ in 0..repeat {
                    if self.config.word_wrap {
                        self.move_cursor_vup(false);
                    } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.move_cursor_up(false);
                    }
                }
            }
            KeyCode::Char('l') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_cursor_right(false);
                    }
                }
            }
            KeyCode::Char('w') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_word_forward(false);
                    }
                }
            }
            KeyCode::Char('b') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_word_backward(false);
                    }
                }
            }
            KeyCode::Char('e') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_word_end(false);
                    }
                }
            }
            KeyCode::Char('0') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let line = buffer.rope.char_to_line(buffer.cursor);
                    buffer.cursor = buffer.rope.line_to_char(line);
                }
            }
            KeyCode::Char('^') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let line = buffer.rope.char_to_line(buffer.cursor);
                    let line_str = buffer.rope.line(line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(line, indent);
                }
            }
            KeyCode::Char('$') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if repeat > 1 {
                        for _ in 1..repeat {
                            buffer.move_cursor_down(false);
                        }
                    }
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    buffer.cursor = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                }
            }
            KeyCode::Char('W') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_bigword_forward(false);
                    }
                }
            }
            KeyCode::Char('B') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_bigword_backward(false);
                    }
                }
            }
            KeyCode::Char('E') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_bigword_end(false);
                    }
                }
            }
            KeyCode::Char('_') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if repeat > 1 {
                        for _ in 1..repeat {
                            buffer.move_cursor_down(false);
                        }
                    }
                    buffer.move_to_first_non_blank(false);
                }
            }
            KeyCode::Char('+') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_to_next_line_non_blank(false);
                    }
                }
            }
            KeyCode::Char('-') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.move_to_prev_line_non_blank(false);
                    }
                }
            }
            KeyCode::Char('f') => {
                self.pending_f = true;
                return;
            }
            KeyCode::Char('F') => {
                self.pending_capital_f = true;
                return;
            }
            KeyCode::Char('t') => {
                self.pending_t = true;
                return;
            }
            KeyCode::Char('T') => {
                self.pending_capital_t = true;
                return;
            }
            KeyCode::Char(';') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        if let Some(pos) = buffer.repeat_inline_find(false) {
                            buffer.cursor = pos;
                            buffer.selection = None;
                            buffer.selection_anchor = None;
                        }
                    }
                }
            }
            KeyCode::Char(',') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        if let Some(pos) = buffer.repeat_inline_find(true) {
                            buffer.cursor = pos;
                            buffer.selection = None;
                            buffer.selection_anchor = None;
                        }
                    }
                }
            }
            KeyCode::Char('m') => {
                self.pending_m = true;
                return;
            }
            KeyCode::Char('\'') => {
                self.pending_single_quote = true;
                return;
            }
            KeyCode::Char('`') => {
                self.pending_backtick = true;
                return;
            }
            KeyCode::Char('u') => {
                for _ in 0..repeat {
                    self.perform_action(Action::Undo);
                }
            }
            KeyCode::Char('x') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        if buffer.cursor < buffer.rope.len_chars() {
                            buffer.delete(buffer.cursor..buffer.cursor+1);
                        }
                    }
                }
            }
            KeyCode::Char('r') => {
                self.pending_r = true;
                return;
            }
            KeyCode::Char('s') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.cursor < buffer.rope.len_chars() {
                        buffer.delete(buffer.cursor..buffer.cursor + 1);
                    }
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('S') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    let line_start = buffer.rope.line_to_char(line);
                    let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                    let range = line_start..line_end;
                    if !range.is_empty() {
                        let text = buffer.rope.slice(range.clone()).to_string();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete(range);
                        buffer.cursor = line_start;
                    }
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('C') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                    let range = buffer.cursor..line_end;
                    if !range.is_empty() {
                        let text = buffer.rope.slice(range.clone()).to_string();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete(range);
                    }
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('D') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                    let range = buffer.cursor..line_end;
                    if !range.is_empty() {
                        let text = buffer.rope.slice(range.clone()).to_string();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete(range);
                    }
                }
            }
            KeyCode::Char('Y') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    buffer.select_line(line);
                    self.perform_action(Action::Copy);
                    if let Some(b) = self.buffers.get_mut(self.active_buffer) {
                        b.selection = None;
                        b.selection_anchor = None;
                    }
                }
            }
            KeyCode::Char('J') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (line, _) = buffer.char_to_line_col(buffer.cursor);
                    if line + 1 < buffer.line_count() {
                        let line_end = buffer.line_col_to_char(line, buffer.get_line_max_col(line));
                        let next_line_start = buffer.rope.line_to_char(line + 1);
                        let next_line_str = buffer.rope.line(line + 1).to_string();
                        let next_indent = next_line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        let next_text_start = next_line_start + next_indent;
                        buffer.delete(line_end..next_text_start);
                        buffer.insert(line_end, " ");
                        buffer.cursor = line_end;
                    }
                }
            }
            KeyCode::Char('d') => {
                self.pending_d = true;
                self.pending_op_count = repeat;
                return;
            }
            KeyCode::Char('c') => {
                self.pending_c = true;
                self.pending_op_count = repeat;
                return;
            }
            KeyCode::Char('y') => {
                self.pending_y = true;
                self.pending_op_count = repeat;
                return;
            }
            KeyCode::Char('p') => {
                if let Some(text) = clipboard::get_clipboard() {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        if let Some(range) = buffer.selection.clone() {
                            buffer.delete(range);
                        }
                        if text.ends_with('\n') {
                            let (line, _) = buffer.char_to_line_col(buffer.cursor);
                            let next_line_start = if line + 1 < buffer.line_count() {
                                buffer.rope.line_to_char(line + 1)
                            } else {
                                buffer.rope.len_chars()
                            };
                            buffer.insert(next_line_start, &text);
                            buffer.cursor = next_line_start;
                        } else {
                            buffer.move_cursor_right(false);
                            buffer.insert(buffer.cursor, &text);
                        }
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            KeyCode::Char('P') => {
                if let Some(text) = clipboard::get_clipboard() {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        if let Some(range) = buffer.selection.clone() {
                            buffer.delete(range);
                        }
                        if text.ends_with('\n') {
                            let (line, _) = buffer.char_to_line_col(buffer.cursor);
                            let line_start = buffer.rope.line_to_char(line);
                            buffer.insert(line_start, &text);
                            buffer.cursor = line_start;
                        } else {
                            buffer.insert(buffer.cursor, &text);
                        }
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            KeyCode::Char('/') | KeyCode::Char('?') => self.perform_action(Action::Find),
            KeyCode::Char('n') => self.find_next(),
            KeyCode::Char('N') => self.find_prev(),
            KeyCode::Char('*') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.search_word_at_cursor(true);
                }
            }
            KeyCode::Char('#') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.search_word_at_cursor(false);
                }
            }
            KeyCode::Char('%') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(pos) = buffer.find_matching_bracket(buffer.cursor) {
                        buffer.cursor = pos;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            KeyCode::Char('~') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        buffer.toggle_case_at_cursor();
                    }
                }
            }
            KeyCode::Char('X') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    for _ in 0..repeat {
                        if buffer.cursor > 0 {
                            let (_line, col) = buffer.char_to_line_col(buffer.cursor);
                            if col > 0 {
                                buffer.delete(buffer.cursor - 1..buffer.cursor);
                            }
                        }
                    }
                }
            }
            KeyCode::Char(':') | KeyCode::Char('：') => {
                self.is_vi_cmd_mode = true;
                self.vi_cmd = ":".to_string();
                self.vi_message = None;
            }

            KeyCode::Char('g') => {
                if self.pending_g {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.cursor = 0;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
                return;
            }
            KeyCode::Char('G') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let line = if has_count {
                        (repeat.saturating_sub(1)).min(buffer.line_count().saturating_sub(1))
                    } else {
                        buffer.line_count().saturating_sub(1)
                    };
                    let col = buffer.get_line_max_col(line);
                    buffer.cursor = buffer.line_col_to_char(line, col);
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            KeyCode::Esc => {
                let had_pending = self.pending_d || self.pending_y || self.pending_c || self.pending_g
                    || self.pending_r || self.pending_f || self.pending_capital_f || self.pending_t
                    || self.pending_capital_t || self.pending_m || self.pending_single_quote
                    || self.pending_backtick || self.count > 0 || self.pending_op_count > 0;
                let had_selection = self.buffers.get(self.active_buffer).map_or(false, |b| b.selection.is_some());

                self.count = 0;
                self.pending_op_count = 0;
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
                self.pending_d = false;
                self.pending_y = false;
                self.pending_c = false;
                self.pending_g = false;
                self.pending_r = false;
                self.pending_f = false;
                self.pending_capital_f = false;
                self.pending_t = false;
                self.pending_capital_t = false;
                self.pending_m = false;
                self.pending_single_quote = false;
                self.pending_backtick = false;

                if !had_pending && !had_selection && self.layout.panel_height > 0 {
                    self.layout.panel_height = 0;
                    self.recompute_layout();
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.search_status = None;
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let line = buffer.rope.char_to_line(buffer.cursor);
                    if line + 1 < buffer.line_count() {
                        let next_line = line + 1;
                        let line_str = buffer.rope.line(next_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        buffer.cursor = buffer.line_col_to_char(next_line, indent);
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            _ => {
                self.pending_d = false;
                self.pending_y = false;
                self.pending_c = false;
                self.pending_op_count = 0;
                self.pending_g = false;
                self.pending_r = false;
                self.pending_f = false;
                self.pending_capital_f = false;
                self.pending_t = false;
                self.pending_capital_t = false;
                self.pending_m = false;
                self.pending_single_quote = false;
                self.pending_backtick = false;
                // Allow arrows and some other keys even in normal mode
                match code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down |
                    KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown => {
                        self.handle_editor_key(key);
                    }
                    _ => {}
                }
            }
        }
        self.ensure_cursor_visible();
    }

    fn handle_vi_visual_key(&mut self, key: KeyEvent) {
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(zee_core::normalize_vi_char(c)),
            other => other,
        };

        if self.pending_single_quote || self.pending_backtick {
            let line_only = self.pending_single_quote;
            self.pending_single_quote = false;
            self.pending_backtick = false;
            if let KeyCode::Char(c) = code {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.jump_to_mark(c, line_only, true);
                }
            }
            self.ensure_cursor_visible();
            return;
        }

        let is_block = self.buffers.get(self.active_buffer).map(|b| b.vi_mode == zee_core::ViMode::VisualBlock).unwrap_or(false);
        
        match code {
            KeyCode::Esc => {
                self.pending_single_quote = false;
                self.pending_backtick = false;
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.vi_mode = zee_core::ViMode::Normal;
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            KeyCode::Char('v') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.vi_mode == zee_core::ViMode::Visual {
                        buffer.vi_mode = zee_core::ViMode::Normal;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    } else {
                        buffer.vi_mode = zee_core::ViMode::Visual;
                        buffer.update_selection();
                    }
                }
            }
            KeyCode::Char('V') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.vi_mode == zee_core::ViMode::VisualLine {
                        buffer.vi_mode = zee_core::ViMode::Normal;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    } else {
                        buffer.vi_mode = zee_core::ViMode::VisualLine;
                        buffer.update_selection();
                    }
                }
            }
            KeyCode::Char('{') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_prev_paragraph(true);
                }
            }
            KeyCode::Char('}') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_next_paragraph(true);
                }
            }
            KeyCode::Char('(') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_prev_sentence(true);
                }
            }
            KeyCode::Char(')') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_next_sentence(true);
                }
            }
            KeyCode::Char('H') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let target_line = buffer.scroll_row.min(buffer.line_count().saturating_sub(1));
                    let line_str = buffer.rope.line(target_line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(target_line, indent);
                    buffer.update_selection();
                }
            }
            KeyCode::Char('M') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (_, _, _, eh) = self.layout.editor_bounds();
                    let half = (eh as usize) / 2;
                    let target_line = (buffer.scroll_row + half).min(buffer.line_count().saturating_sub(1));
                    let line_str = buffer.rope.line(target_line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(target_line, indent);
                    buffer.update_selection();
                }
            }
            KeyCode::Char('L') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    let (_, _, _, eh) = self.layout.editor_bounds();
                    let visible_bottom = buffer.scroll_row + (eh as usize).saturating_sub(1);
                    let target_line = visible_bottom.min(buffer.line_count().saturating_sub(1));
                    let line_str = buffer.rope.line(target_line).to_string();
                    let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                    buffer.cursor = buffer.line_col_to_char(target_line, indent);
                    buffer.update_selection();
                }
            }
            KeyCode::Char('h') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_left(true);
                }
            }
            KeyCode::Char('j') => {
                if self.config.word_wrap {
                    self.move_cursor_vdown(true);
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_down(true);
                }
            }
            KeyCode::Char('k') => {
                if self.config.word_wrap {
                    self.move_cursor_vup(true);
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_up(true);
                }
            }
            KeyCode::Char('l') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_right(true);
                }
            }
            KeyCode::Char('w') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_word_forward(true);
                }
            }
            KeyCode::Char('b') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_word_backward(true);
                }
            }
            KeyCode::Char('e') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_word_end(true);
                }
            }
            KeyCode::Char('0') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_home(true);
                }
            }
            KeyCode::Char('$') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_end(true);
                }
            }
            KeyCode::Char('W') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_bigword_forward(true);
                }
            }
            KeyCode::Char('B') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_bigword_backward(true);
                }
            }
            KeyCode::Char('E') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_bigword_end(true);
                }
            }
            KeyCode::Char('_') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_first_non_blank(true);
                }
            }
            KeyCode::Char('+') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_next_line_non_blank(true);
                }
            }
            KeyCode::Char('-') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_to_prev_line_non_blank(true);
                }
            }
            KeyCode::Char(';') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(pos) = buffer.repeat_inline_find(false) {
                        buffer.cursor = pos;
                        buffer.update_selection();
                    }
                }
            }
            KeyCode::Char(',') => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(pos) = buffer.repeat_inline_find(true) {
                        buffer.cursor = pos;
                        buffer.update_selection();
                    }
                }
            }
            KeyCode::Char('\'') => {
                self.pending_single_quote = true;
                return;
            }
            KeyCode::Char('`') => {
                self.pending_backtick = true;
                return;
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                if is_block {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        let text = buffer.get_visual_block_text();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete_visual_block();
                    }
                } else {
                    self.perform_action(Action::Cut);
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.vi_mode = zee_core::ViMode::Normal;
                    }
                }
            }
            KeyCode::Char('c') | KeyCode::Char('s') => {
                if is_block {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        let text = buffer.get_visual_block_text();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete_visual_block();
                        buffer.vi_mode = zee_core::ViMode::Insert;
                    }
                } else {
                    self.perform_action(Action::Cut);
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.vi_mode = zee_core::ViMode::Insert;
                    }
                }
            }
            KeyCode::Char('y') => {
                if is_block {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        let text = buffer.get_visual_block_text();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                        buffer.vi_mode = zee_core::ViMode::Normal;
                    }
                } else {
                    self.perform_action(Action::Copy);
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.vi_mode = zee_core::ViMode::Normal;
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
            }
            KeyCode::Char('p') => {
                if let Some(text) = clipboard::get_clipboard() {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        if buffer.vi_mode == zee_core::ViMode::VisualBlock {
                            buffer.delete_visual_block();
                        } else if let Some(range) = buffer.selection.clone() {
                            buffer.delete(range);
                        }
                        buffer.insert(buffer.cursor, &text);
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                        buffer.vi_mode = zee_core::ViMode::Normal;
                    }
                }
            }
            KeyCode::Char('I') if is_block => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(anchor) = buffer.selection_anchor {
                        let (anchor_line, anchor_col) = buffer.char_to_line_col(anchor);
                        let (cursor_line, cursor_col) = buffer.char_to_line_col(buffer.cursor);
                        let target_line = anchor_line.min(cursor_line);
                        let target_col = anchor_col.min(cursor_col);
                        buffer.cursor = buffer.line_col_to_char(target_line, target_col);
                    }
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char('A') if is_block => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(anchor) = buffer.selection_anchor {
                        let (anchor_line, anchor_col) = buffer.char_to_line_col(anchor);
                        let (cursor_line, cursor_col) = buffer.char_to_line_col(buffer.cursor);
                        let target_line = anchor_line.min(cursor_line);
                        let target_col = anchor_col.max(cursor_col) + 1;
                        buffer.cursor = buffer.line_col_to_char(target_line, target_col);
                    }
                    buffer.vi_mode = zee_core::ViMode::Insert;
                }
            }
            KeyCode::Char(':') | KeyCode::Char('：') => {
                self.is_vi_cmd_mode = true;
                self.vi_cmd = ":".to_string();
                self.vi_message = None;
            }
            _ => {

                match code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down |
                    KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown => {
                        let mut v_key = key;
                        v_key.modifiers.insert(KeyModifiers::SHIFT);
                        self.handle_editor_key(v_key);
                    }
                    _ => {}
                }
            }
        }
        self.ensure_cursor_visible();
    }

    fn handle_vi_cmd_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.is_vi_cmd_mode = false;
                self.vi_cmd.clear();
            }
            KeyCode::Enter => {
                let cmd_raw = self.vi_cmd.clone();
                self.is_vi_cmd_mode = false;
                self.vi_cmd.clear();

                let ex_cmd = zee_core::parse_ex_command(&cmd_raw);
                match ex_cmd {
                    zee_core::ExCommand::Write { path, force: _ } => {
                        if let Some(ref p) = path {
                            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                let trim = self.config.trim_trailing_whitespace;
                                let ensure_nl = self.config.ensure_final_newline;
                                buffer.cleanup_on_save(trim, ensure_nl);
                                match buffer.save_as(p) {
                                    Ok(()) => {
                                        self.vi_message = Some((format!("\"{}\" [New] written", p), false));
                                    }
                                    Err(e) => {
                                        self.vi_message = Some((format!("E212: Can't open file for writing: {}", e), true));
                                    }
                                }
                            }
                        } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            if buffer.path.is_none() {
                                self.vi_message = Some(("E32: No file name".into(), true));
                            } else {
                                let trim = self.config.trim_trailing_whitespace;
                                let ensure_nl = self.config.ensure_final_newline;
                                buffer.cleanup_on_save(trim, ensure_nl);
                                match buffer.save() {
                                    Ok(()) => {
                                        let name = buffer.path.as_ref()
                                            .and_then(|p| p.file_name())
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_else(|| "file".to_string());
                                        self.vi_message = Some((format!("\"{}\" written", name), false));
                                    }
                                    Err(e) => {
                                        self.vi_message = Some((format!("E212: Can't open file for writing: {}", e), true));
                                    }
                                }
                            }
                        }
                    }
                    zee_core::ExCommand::Quit { force } => {
                        let is_modified = self.buffers.get(self.active_buffer).map(|b| b.is_modified()).unwrap_or(false);
                        if is_modified && !force {
                            self.vi_message = Some(("E37: No write since last change (add ! to override)".into(), true));
                        } else if self.buffers.len() <= 1 {
                            self.running = false;
                        } else {
                            self.perform_action(Action::Close);
                        }
                    }
                    zee_core::ExCommand::WriteQuit { path, force: _ } => {
                        let mut write_ok = false;
                        if let Some(ref p) = path {
                            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                let trim = self.config.trim_trailing_whitespace;
                                let ensure_nl = self.config.ensure_final_newline;
                                buffer.cleanup_on_save(trim, ensure_nl);
                                if buffer.save_as(p).is_ok() {
                                    write_ok = true;
                                } else {
                                    self.vi_message = Some((format!("E212: Can't open file for writing: {}", p), true));
                                }
                            }
                        } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            if buffer.path.is_none() {
                                self.vi_message = Some(("E32: No file name".into(), true));
                            } else {
                                let trim = self.config.trim_trailing_whitespace;
                                let ensure_nl = self.config.ensure_final_newline;
                                buffer.cleanup_on_save(trim, ensure_nl);
                                if buffer.save().is_ok() {
                                    write_ok = true;
                                } else {
                                    self.vi_message = Some(("Error saving file".into(), true));
                                }
                            }
                        }
                        if write_ok {
                            if self.buffers.len() <= 1 {
                                self.running = false;
                            } else {
                                self.perform_action(Action::Close);
                            }
                        }
                    }

                    zee_core::ExCommand::QuitAll { force } => {
                        let any_modified = self.buffers.iter().any(|b| b.is_modified());
                        if any_modified && !force {
                            self.vi_message = Some(("E37: No write since last change (add ! to override)".into(), true));
                        } else {
                            self.running = false;
                        }
                    }
                    zee_core::ExCommand::WriteQuitAll { force: _ } => {
                        let mut all_ok = true;
                        for b in &mut self.buffers {
                            if b.is_modified() {
                                let trim = self.config.trim_trailing_whitespace;
                                let ensure_nl = self.config.ensure_final_newline;
                                b.cleanup_on_save(trim, ensure_nl);
                                if b.save().is_err() {
                                    self.vi_message = Some(("Error saving buffer".into(), true));
                                    all_ok = false;
                                    break;
                                }
                            }
                        }
                        if all_ok {
                            self.running = false;
                        }
                    }
                    zee_core::ExCommand::Edit { path, force } => {
                        if let Some(p) = path {
                            let path_buf = std::path::PathBuf::from(&p);
                            if let Ok(editor) = zee_core::buffer::Editor::from_file(&path_buf) {
                                self.buffers.push(editor);
                                self.active_buffer = self.buffers.len() - 1;
                                self.recompute_layout();
                                self.update_active_outline();
                                self.vi_message = Some((format!("\"{}\" opened", p), false));
                            } else {
                                self.vi_message = Some((format!("E484: Can't open file: {}", p), true));
                            }
                        } else if force {
                            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                match buffer.reload_from_disk() {
                                    Ok(()) => {
                                        self.vi_message = Some(("Reloaded from disk".into(), false));
                                    }
                                    Err(e) => {
                                        self.vi_message = Some((format!("Failed to reload: {}", e), true));
                                    }
                                }
                            }
                        }
                    }
                    zee_core::ExCommand::GoToLine(line_num) => {
                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            let max_line = buffer.line_count().saturating_sub(1);
                            let target_line = line_num.saturating_sub(1).min(max_line);
                            buffer.cursor = buffer.line_col_to_char(target_line, 0);
                            buffer.selection = None;
                            buffer.selection_anchor = None;
                        }
                        self.vi_message = None;
                    }
                    zee_core::ExCommand::NoHighlight => {
                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            buffer.find_results.clear();
                            buffer.current_match_idx = None;
                            buffer.search_status = None;
                        }
                        self.vi_message = None;
                    }
                    zee_core::ExCommand::BufferNext => {
                        if !self.buffers.is_empty() {
                            self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                            self.recompute_layout();
                            self.update_active_outline();
                        }
                        self.vi_message = None;
                    }
                    zee_core::ExCommand::BufferPrev => {
                        if !self.buffers.is_empty() {
                            self.active_buffer = if self.active_buffer == 0 { self.buffers.len() - 1 } else { self.active_buffer - 1 };
                            self.recompute_layout();
                            self.update_active_outline();
                        }
                        self.vi_message = None;
                    }

                    zee_core::ExCommand::Set { option, value: _ } => {
                        match option.as_str() {
                            "nu" | "number" => {
                                self.config.line_numbers = true;
                                self.recompute_layout();
                                self.vi_message = Some(("number enabled".into(), false));
                            }
                            "nonu" | "nonumber" => {
                                self.config.line_numbers = false;
                                self.recompute_layout();
                                self.vi_message = Some(("number disabled".into(), false));
                            }
                            "wrap" => {
                                self.config.word_wrap = true;
                                self.vi_message = Some(("wrap enabled".into(), false));
                            }
                            "nowrap" => {
                                self.config.word_wrap = false;
                                self.vi_message = Some(("wrap disabled".into(), false));
                            }
                            _ => {
                                self.vi_message = Some((format!("Unknown option: {}", option), true));
                            }
                        }
                    }
                    zee_core::ExCommand::Substitute { range, pattern, replacement, global, ignore_case } => {
                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            let (start_l, end_l) = match range {
                                zee_core::ExRange::CurrentLine => {
                                    let (cur_l, _) = buffer.char_to_line_col(buffer.cursor);
                                    (cur_l + 1, cur_l + 1)
                                }
                                zee_core::ExRange::EntireBuffer => {
                                    (1, buffer.line_count())
                                }
                                zee_core::ExRange::LineRange(s, e) => (s, e),
                            };
                            match buffer.substitute_range(start_l, end_l, &pattern, &replacement, global, ignore_case) {
                                Ok(count) => {
                                    self.vi_message = Some((format!("{} substitution(s) made", count), false));
                                }
                                Err(e) => {
                                    self.vi_message = Some((format!("E486: {}", e), true));
                                }
                            }
                        }
                    }
                    zee_core::ExCommand::Plugin { subcmd, arg, extra } => {
                        match subcmd.as_str() {
                            "list" => {
                                let mut names = Vec::new();
                                for p in &self.sidebar.plugin_manager.plugins {
                                    names.push(format!("{} (wasm)", p.manifest.name));
                                }
                                for p in &self.sidebar.plugin_manager.lua_plugins {
                                    names.push(format!("{} (lua)", p.manifest.name));
                                }
                                if names.is_empty() {
                                    self.vi_message = Some(("No plugins installed".to_string(), false));
                                } else {
                                    self.vi_message = Some((format!("Installed plugins: {}", names.join(", ")), false));
                                }
                            }
                            "repo" => {
                                let action = arg.as_deref().unwrap_or("list");
                                match action {
                                    "list" => {
                                        if self.config.plugin_registries.is_empty() {
                                            self.vi_message = Some(("No repositories configured".to_string(), false));
                                        } else {
                                            self.vi_message = Some((format!("Configured repositories: {}", self.config.plugin_registries.join(", ")), false));
                                        }
                                    }
                                    "add" => {
                                        if let Some(url) = extra {
                                            let _ = self.config.add_plugin_registry(&url);
                                            self.vi_message = Some((format!("Added plugin repository: {}", url), false));
                                        } else {
                                            self.vi_message = Some(("Usage: :plugin repo add <url>".to_string(), true));
                                        }
                                    }
                                    "remove" | "rm" => {
                                        if let Some(url) = extra {
                                            match self.config.remove_plugin_registry(&url) {
                                                Ok(true) => {
                                                    self.vi_message = Some((format!("Removed plugin repository: {}", url), false));
                                                }
                                                Ok(false) => {
                                                    self.vi_message = Some((format!("Repository '{}' not found", url), true));
                                                }
                                                Err(e) => {
                                                    self.vi_message = Some((format!("Failed to remove repository: {}", e), true));
                                                }
                                            }
                                        } else {
                                            self.vi_message = Some(("Usage: :plugin repo remove <url>".to_string(), true));
                                        }
                                    }
                                    _ => {
                                        self.vi_message = Some((format!("Unknown repo command: {}. Available: list, add <url>, remove <url>", action), true));
                                    }
                                }
                            }
                            "install" => {
                                if let Some(id) = arg {
                                    match zee_core::plugin::PluginManager::install_from_registry(&id, None, Some(&self.config.plugin_registries)) {
                                        Ok(_) => {
                                            self.sidebar.reload_plugins();
                                            self.vi_message = Some((format!("Plugin '{}' installed successfully", id), false));
                                        }
                                        Err(e) => {
                                            self.vi_message = Some((format!("Failed to install '{}': {}", id, e), true));
                                        }
                                    }
                                } else {
                                    self.vi_message = Some(("Usage: :plugin install <id>".to_string(), true));
                                }
                            }
                            "uninstall" => {
                                if let Some(id) = arg {
                                    match zee_core::plugin::PluginManager::uninstall_plugin_by_id(&id) {
                                        Ok(true) => {
                                            self.sidebar.reload_plugins();
                                            self.vi_message = Some((format!("Plugin '{}' uninstalled successfully", id), false));
                                        }
                                        Ok(false) => {
                                            self.vi_message = Some((format!("Plugin '{}' not found", id), true));
                                        }
                                        Err(e) => {
                                            self.vi_message = Some((format!("Failed to uninstall '{}': {}", id, e), true));
                                        }
                                    }
                                } else {
                                    self.vi_message = Some(("Usage: :plugin uninstall <id>".to_string(), true));
                                }
                            }
                            "reload" => {
                                self.sidebar.reload_plugins();
                                self.vi_message = Some(("Plugins reloaded successfully".to_string(), false));
                            }
                            _ => {
                                self.vi_message = Some((format!("Unknown plugin command: {}. Available: list, repo [list|add|remove], install <id>, uninstall <id>, reload", subcmd), true));
                            }
                        }
                    }
                    zee_core::ExCommand::Empty => {
                        self.vi_message = None;
                    }
                    zee_core::ExCommand::Unknown(cmd) => {
                        self.vi_message = Some((format!("E492: Not an editor command: :{}", cmd), true));
                    }
                }
            }
            KeyCode::Char(c) => {
                self.vi_cmd.push(zee_core::normalize_vi_char(c));
            }
            KeyCode::Backspace => {
                if self.vi_cmd.chars().count() > 1 {
                    self.vi_cmd.pop();
                } else {
                    self.is_vi_cmd_mode = false;
                    self.vi_cmd.clear();
                }
            }
            _ => {}
        }
    }


    fn handle_editor_key(&mut self, key: KeyEvent) {
        let extend_selection = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Esc => {
                if self.layout.panel_height > 0 {
                    self.layout.panel_height = 0;
                    self.recompute_layout();
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.search_status = None;
                    }
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.selection = None;
                    buffer.selection_anchor = None;
                }
            }
            KeyCode::Left => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_left(extend_selection);
                }
            }
            KeyCode::Right => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_right(extend_selection);
                }
            }
            KeyCode::Up => {
                if self.config.word_wrap {
                    self.move_cursor_vup(extend_selection);
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_up(extend_selection);
                }
            }
            KeyCode::Down => {
                if self.config.word_wrap {
                    self.move_cursor_vdown(extend_selection);
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_down(extend_selection);
                }
            }
            KeyCode::Home => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_home(extend_selection);
                }
            }
            KeyCode::End => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.move_cursor_end(extend_selection);
                }
            }
            KeyCode::PageUp => {
                for _ in 0..20 {
                    if self.config.word_wrap {
                        self.move_cursor_vup(extend_selection);
                    } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.move_cursor_up(extend_selection);
                    }
                }
            }
            KeyCode::PageDown => {
                for _ in 0..20 {
                    if self.config.word_wrap {
                        self.move_cursor_vdown(extend_selection);
                    } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.move_cursor_down(extend_selection);
                    }
                }
            }
            KeyCode::Char(c)
                if (key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT) => {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        if let Some(selection) = buffer.selection.take() {
                            buffer.delete(selection);
                        }
                        buffer.insert(buffer.cursor, &c.to_string());
                    }
                }
            KeyCode::Backspace => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(selection) = buffer.selection.take() {
                        buffer.delete(selection);
                    } else if buffer.cursor > 0 {
                        buffer.delete((buffer.cursor - 1)..buffer.cursor);
                    }
                }
            }
            KeyCode::Delete => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(selection) = buffer.selection.take() {
                        buffer.delete(selection);
                    } else if buffer.cursor < buffer.rope.len_chars() {
                        buffer.delete(buffer.cursor..(buffer.cursor + 1));
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(selection) = buffer.selection.take() {
                        buffer.delete(selection);
                    }
                    buffer.insert(buffer.cursor, "\n");
                }
            }
            KeyCode::Tab => {
                let tab_size = self.config.tab_size.max(1);
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(selection) = buffer.selection.take() {
                        buffer.delete(selection);
                    }
                    let indent = " ".repeat(tab_size);
                    buffer.insert(buffer.cursor, &indent);
                }
            }
            _ => {}
        }
        self.ensure_cursor_visible();
    }

    fn ensure_cursor_visible(&mut self) {
        let (_ex, _ey, ew, eh) = self.layout.editor_bounds();
        let buffer = if let Some(b) = self.buffers.get_mut(self.active_buffer) {
            b
        } else {
            return;
        };

        let (line, col) = buffer.char_to_line_col(buffer.cursor);
        let tab_size = self.config.tab_size;
        
        if self.config.word_wrap {
            // Find visual line of cursor
            let wraps = buffer.wrap_line(line, ew as usize, tab_size);
            let mut v_idx = 0;
            for (i, range) in wraps.iter().enumerate() {
                if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                    v_idx = i;
                    break;
                }
            }

            // Check if (line, v_idx) is BEFORE current scroll
            if line < buffer.scroll_row || (line == buffer.scroll_row && v_idx < buffer.scroll_vrow) {
                buffer.scroll_row = line;
                buffer.scroll_vrow = v_idx;
            } else {
                // Check if (line, v_idx) is AFTER current scroll
                let mut total_vrows = 0;
                let mut current_l = buffer.scroll_row;
                let mut current_v = buffer.scroll_vrow;
                
                while current_l < line {
                    let w = buffer.wrap_line(current_l, ew as usize, tab_size);
                    total_vrows += w.len() - current_v;
                    current_l += 1;
                    current_v = 0;
                }
                total_vrows += v_idx - current_v;
                
                if total_vrows >= eh as usize {
                    // Scroll down
                    let mut target_vrows = total_vrows - eh as usize + 1;
                    while target_vrows > 0 {
                        let w = buffer.wrap_line(buffer.scroll_row, ew as usize, tab_size);
                        let remaining_in_line = w.len() - buffer.scroll_vrow;
                        if target_vrows >= remaining_in_line {
                            target_vrows -= remaining_in_line;
                            buffer.scroll_row += 1;
                            buffer.scroll_vrow = 0;
                            if buffer.scroll_row >= buffer.line_count() {
                                buffer.scroll_row = buffer.line_count() - 1;
                                buffer.scroll_vrow = 0;
                                break;
                            }
                        } else {
                            buffer.scroll_vrow += target_vrows;
                            target_vrows = 0;
                        }
                    }
                }
            }
        } else {
            // Vertical scroll
            if line < buffer.scroll_row {
                buffer.scroll_row = line;
            } else if line >= buffer.scroll_row + eh as usize {
                buffer.scroll_row = line - eh as usize + 1;
            }

            // Horizontal scroll
            let mut visual_col = 0;
            let line_content = buffer.line(line);
            let cursor_in_line = buffer.cursor - buffer.rope.line_to_char(line);
            for (i, c) in line_content.chars().enumerate() {
                if i >= cursor_in_line { break; }
                if c == '\t' {
                    let tab_size = self.config.tab_size;
                    visual_col += tab_size - (visual_col % tab_size);
                } else {
                    visual_col += c.width().unwrap_or(0);
                }
            }

            if visual_col < buffer.scroll_col {
                buffer.scroll_col = visual_col;
            } else if visual_col >= buffer.scroll_col + ew as usize {
                buffer.scroll_col = visual_col - ew as usize + 1;
            }
        }
    }

    fn handle_dialog_key(&mut self, key: KeyEvent) {
        if let Some(ref mut dialog) = self.current_dialog {
            let result = dialog.handle_key(key);
            self.handle_dialog_result(result);
        }
    }

    pub(crate) fn handle_dialog_result(&mut self, result: DialogResult<dialog::Action>) {
        match result {
            DialogResult::Ok(action) => {
                match action {
                    dialog::Action::ConfirmPath(path) => {
                        match self.pending_op {
                            PendingOp::Open => {
                                match self.editor_from_file(&path) {
                                    Ok(mut buffer) => {
                                        let syntax = self.detect_syntax(&path);
                                        buffer.update_syntax(syntax);
                                        self.buffers.push(buffer);
                                        self.active_buffer = self.buffers.len() - 1;
                                        self.update_active_outline();
                                        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                        self.recompute_layout();
                                    }
                                    Err(e) => {
                                        if let Some(ref mut dialog) = self.current_dialog {
                                             dialog.set_error(format!("Error: {}", e));
                                        }
                                        return; // Keep dialog open
                                    }
                                }
                            }
                            PendingOp::SaveAs => {
                                let enc = self.current_dialog.as_ref().and_then(|d| d.selected_encoding());
                                if path.exists() {
                                    self.focus = Focus::Dialog;
                                    self.pending_op = PendingOp::SaveAs; // Keep op
                                    self.target_path = Some(path.clone());
                                    if let Some(enc) = enc {
                                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                            buffer.encoding = enc;
                                        }
                                    }
                                    self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                        self.i18n.get("dialog.overwrite_prompt").to_string(),
                                        self.i18n.get("dialog.overwrite_prompt").replace("{filename}", &path.file_name().unwrap_or_default().to_string_lossy()),
                                        vec![
                                            (self.i18n.get("dialog.yes").to_string(), dialog::Action::Save),
                                            (self.i18n.get("dialog.no").to_string(), dialog::Action::Cancel),
                                        ]
                                    )));
                                    return;
                                }
                                if let Some(enc) = enc {
                                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                        buffer.encoding = enc;
                                    }
                                }
                                if let Err(e) = self.save_as_buffer(self.active_buffer, &path) {
                                    if let Some(ref mut dialog) = self.current_dialog {
                                        dialog.set_error(format!("Error: {}", e));
                                    }
                                    return;
                                }
                                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                self.recompute_layout();
                            }
                            PendingOp::OpenFolder => {
                                self.sidebar.set_root(path);
                                self.sidebar.visible = true;
                                self.config.sidebar = true;
                                let _ = Config::write_key("sidebar", "true");
                                self.sidebar.active_tab = crate::widgets::sidebar::SidebarTab::Files;
                                self.current_dialog = None;
                                self.pending_op = PendingOp::None;
                                self.focus = Focus::Sidebar;
                                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                self.recompute_layout();
                                return;
                            }
                            PendingOp::ExportConfig => {
                                let res = zee_core::export_backup(&path, false);
                                self.pending_op = PendingOp::None;
                                match res {
                                    Ok(report) => {
                                        let title = self.i18n.get("dialog.backup.export_title").to_string();
                                        let msg = self.i18n.get("dialog.backup.export_success")
                                            .replace("{count}", &report.total_files.to_string())
                                            .replace("{path}", &path.to_string_lossy());
                                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                            title,
                                            msg,
                                            vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                                        )));
                                    }
                                    Err(e) => {
                                        let title = self.i18n.get("dialog.backup.error_title").to_string();
                                        let msg = format!("{}", e);
                                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                            title,
                                            msg,
                                            vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                                        )));
                                    }
                                }
                                self.focus = Focus::Dialog;
                                return;
                            }
                            PendingOp::ExportAll => {
                                let res = zee_core::export_backup(&path, true);
                                self.pending_op = PendingOp::None;
                                match res {
                                    Ok(report) => {
                                        let title = self.i18n.get("dialog.backup.export_title").to_string();
                                        let msg = self.i18n.get("dialog.backup.export_success")
                                            .replace("{count}", &report.total_files.to_string())
                                            .replace("{path}", &path.to_string_lossy());
                                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                            title,
                                            msg,
                                            vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                                        )));
                                    }
                                    Err(e) => {
                                        let title = self.i18n.get("dialog.backup.error_title").to_string();
                                        let msg = format!("{}", e);
                                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                            title,
                                            msg,
                                            vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                                        )));
                                    }
                                }
                                self.focus = Focus::Dialog;
                                return;
                            }
                            PendingOp::ImportConfig => {
                                let res = zee_core::import_backup(&path);
                                self.pending_op = PendingOp::None;
                                match res {
                                    Ok(report) => {
                                        self.config = Config::load();
                                        self.i18n = I18n::load(&self.config.language);
                                        self.sidebar.reload_plugins();
                                        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                        self.recompute_layout();

                                        let title = self.i18n.get("dialog.backup.import_title").to_string();
                                        let msg = self.i18n.get("dialog.backup.import_success")
                                            .replace("{count}", &report.total_files.to_string())
                                            .replace("{path}", &path.to_string_lossy());
                                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                            title,
                                            msg,
                                            vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                                        )));
                                    }
                                    Err(e) => {
                                        let title = self.i18n.get("dialog.backup.error_title").to_string();
                                        let msg = format!("{}", e);
                                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                            title,
                                            msg,
                                            vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                                        )));
                                    }
                                }
                                self.focus = Focus::Dialog;
                                return;
                            }
                            _ => {}
                        }
                    }
                    dialog::Action::ConfirmLine(line) => {
                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            buffer.cursor = buffer.rope.line_to_char(line.saturating_sub(1).min(buffer.line_count().saturating_sub(1)));
                            buffer.selection = None;
                            buffer.selection_anchor = None;
                            self.ensure_cursor_visible();
                        }
                    }
                    dialog::Action::FileContextMenuAction { action, path, is_dir: _ } => {
                        match action {
                            "new_file" => {
                                self.pending_op = PendingOp::NewFile;
                                self.target_path = Some(path);
                                self.current_dialog = Some(Box::new(dialog::InputDialog::new(
                                    self.i18n.get("sidebar.new_file").to_string(),
                                    self.i18n.get("sidebar.prop_file").to_string(),
                                    String::new(),
                                )));
                                self.focus = Focus::Dialog;
                                return;
                            }
                            "new_folder" => {
                                self.pending_op = PendingOp::NewFolder;
                                self.target_path = Some(path);
                                self.current_dialog = Some(Box::new(dialog::InputDialog::new(
                                    self.i18n.get("sidebar.new_folder").to_string(),
                                    self.i18n.get("sidebar.new_folder").to_string(),
                                    String::new(),
                                )));
                                self.focus = Focus::Dialog;
                                return;
                            }
                            "rename" => {
                                self.pending_op = PendingOp::Rename;
                                let curr_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                self.target_path = Some(path);
                                self.current_dialog = Some(Box::new(dialog::InputDialog::new(
                                    self.i18n.get("sidebar.rename").to_string(),
                                    self.i18n.get("sidebar.rename").to_string(),
                                    curr_name,
                                )));
                                self.focus = Focus::Dialog;
                                return;
                            }
                            "delete" => {
                                self.pending_op = PendingOp::Delete;
                                self.target_path = Some(path);
                                self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                                    self.i18n.get("sidebar.delete").to_string(),
                                    self.i18n.get("sidebar.delete_confirm").to_string(),
                                    vec![
                                        (self.i18n.get("dialog.yes").to_string(), dialog::Action::Confirm),
                                        (self.i18n.get("dialog.no").to_string(), dialog::Action::Cancel),
                                    ],
                                )));
                                self.focus = Focus::Dialog;
                                return;
                            }
                            "refresh" => {
                                self.sidebar.refresh_files();
                                self.current_dialog = None;
                                self.pending_op = PendingOp::None;
                                self.focus = Focus::Sidebar;
                                return;
                            }
                            "toggle_hidden" => {
                                let new_val = self.sidebar.toggle_show_hidden();
                                self.config.show_hidden = new_val;
                                let _ = zee_core::Config::save_show_hidden(new_val);
                                self.current_dialog = None;
                                self.pending_op = PendingOp::None;
                                self.focus = Focus::Sidebar;
                                return;
                            }
                            _ => {}
                        }
                    }
                    dialog::Action::InputName(name) => {
                        let op = self.pending_op;
                        let target = self.target_path.take();
                        self.pending_op = PendingOp::None;
                        if let Some(target_dir) = target {
                            match op {
                                PendingOp::NewFile => {
                                    if let Ok(new_path) = self.sidebar.file_tree.create_file(&target_dir, &name) {
                                        self.sidebar.refresh_files();
                                        self.sidebar.file_tree.ensure_expanded(&new_path);
                                        self.open_or_switch_to_file(new_path);
                                    }
                                }
                                PendingOp::NewFolder => {
                                    if let Ok(new_path) = self.sidebar.file_tree.create_folder(&target_dir, &name) {
                                        self.sidebar.refresh_files();
                                        self.sidebar.file_tree.ensure_expanded(&new_path);
                                    }
                                }
                                PendingOp::Rename => {
                                    if let Ok(new_path) = self.sidebar.file_tree.rename_item(&target_dir, &name) {
                                        self.sidebar.refresh_files();
                                        // Update open buffer paths if matching
                                        for buf in &mut self.buffers {
                                            if buf.path.as_deref() == Some(&target_dir) {
                                                buf.path = Some(new_path.clone());
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.current_dialog = None;
                        self.focus = Focus::Sidebar;
                        return;
                    }
                    dialog::Action::Confirm => {
                        if self.pending_op == PendingOp::Delete {
                            if let Some(target) = self.target_path.take() {
                                if let Ok(()) = self.sidebar.file_tree.delete_item(&target) {
                                    self.sidebar.refresh_files();
                                    // If deleted file was open, remove buffer
                                    let mut i = 0;
                                    while i < self.buffers.len() {
                                        if self.buffers[i].path.as_deref() == Some(&target) {
                                            self.buffers.remove(i);
                                            if self.buffers.is_empty() {
                                                self.buffers.push(self.new_editor());
                                            }
                                            self.active_buffer = self.active_buffer.min(self.buffers.len() - 1);
                                        } else {
                                            i += 1;
                                        }
                                    }
                                    self.update_active_outline();
                                    self.recompute_layout();
                                }
                            }
                            self.current_dialog = None;
                            self.pending_op = PendingOp::None;
                            self.focus = Focus::Sidebar;
                            return;
                        }
                    }
                    dialog::Action::Save => {
                        let op = self.pending_op;
                        self.pending_op = PendingOp::None;
                        match op {
                            PendingOp::Close | PendingOp::Exit => {
                                if let Ok(()) = self.save_buffer(self.active_buffer) {
                                    if op == PendingOp::Exit {
                                        self.perform_action(Action::Exit);
                                    } else {
                                        self.buffers.remove(self.active_buffer);
                                        if self.buffers.is_empty() {
                                            self.buffers.push(self.new_editor());
                                        }
                                        self.active_buffer = self.active_buffer.min(self.buffers.len() - 1);
                                        self.update_active_outline();
                                        self.recompute_layout();
                                    }
                                }
                            }
                            PendingOp::SaveAs => {
                                if let Some(path) = self.target_path.take() {
                                    let _ = self.save_as_buffer(self.active_buffer, &path);
                                    self.recompute_layout();
                                }
                            }
                            PendingOp::Reload => {
                                let _ = self.save_buffer(self.active_buffer);
                                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                    let _ = buffer.reload_from_disk();
                                }
                                self.update_active_outline();
                                self.recompute_layout();
                            }
                            _ => {
                                self.perform_action(Action::Save);
                            }
                        }
                    }
                    dialog::Action::DontSave | dialog::Action::Discard => {
                        let op = self.pending_op;
                        self.pending_op = PendingOp::None;
                        match op {
                            PendingOp::Reload => {
                                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                                    let _ = buffer.reload_from_disk();
                                    self.update_active_outline();
                                    self.recompute_layout();
                                }
                            }
                            PendingOp::Exit => {
                                self.buffers.remove(self.active_buffer);
                                if self.buffers.is_empty() {
                                    self.buffers.push(self.new_editor());
                                }
                                self.active_buffer = self.active_buffer.min(self.buffers.len() - 1);
                                self.perform_action(Action::Exit);
                            }
                            PendingOp::Close => {
                                self.buffers.remove(self.active_buffer);
                                if self.buffers.is_empty() {
                                    self.buffers.push(self.new_editor());
                                }
                                self.active_buffer = self.active_buffer.min(self.buffers.len() - 1);
                                self.update_active_outline();
                                self.recompute_layout();
                            }
                            PendingOp::None => {
                                // Probably a Reopen or other immediate operation
                                if let Some(enc) = self.target_encoding.take() {
                                    if let Some(buffer) = self.buffers.get(self.active_buffer) {
                                        if let Some(path) = buffer.path.clone() {
                                            if let Ok(new_buffer) = self.editor_from_file(&path) {
                                                let mut b = new_buffer;
                                                b.encoding = enc;
                                                self.buffers[self.active_buffer] = b;
                                                self.update_active_outline();
                                                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                                self.recompute_layout();
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    dialog::Action::SaveSettings(new_cfg) => {
                        let old_theme = self.config.theme.clone();
                        let old_lang = self.config.language.clone();
                        let old_show_hidden = self.config.show_hidden;

                        let _ = Config::write_key("theme", &new_cfg.theme);
                        let _ = Config::write_key("language", &new_cfg.language);
                        let _ = Config::write_key("sidebar_position", &new_cfg.sidebar_position);
                        let _ = Config::write_key("tab_size", &new_cfg.tab_size.to_string());
                        let _ = Config::write_key("expand_tab", &new_cfg.expand_tab.to_string());
                        let _ = Config::write_key("line_numbers", &new_cfg.line_numbers.to_string());
                        let _ = Config::write_key("word_wrap", &new_cfg.word_wrap.to_string());
                        let _ = Config::write_key("vi_mode", &new_cfg.vi_mode.to_string());
                        let _ = Config::save_show_hidden(new_cfg.show_hidden);

                        self.config = *new_cfg;

                        if self.config.show_hidden != old_show_hidden {
                            self.sidebar.file_tree.set_show_hidden(self.config.show_hidden);
                            self.sidebar.refresh_files();
                        }

                        if self.config.theme != old_theme {
                            if let Some(t) = self.themes.iter().find(|t| t.meta.name.to_lowercase().replace(" ", "-") == self.config.theme) {
                                self.theme = t.clone();
                            }
                        }

                        if self.config.language != old_lang {
                            self.i18n = I18n::load(&self.config.language);
                        }

                        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                        self.recompute_layout();
                    }
                    dialog::Action::Cancel => {}
                }
                if let Some(ref d) = self.current_dialog {
                    if d.title() == self.i18n.get("dialog.plugin.title") {
                        self.sidebar.reload_plugins();
                        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    }
                }
                self.current_dialog = None;
                self.pending_op = PendingOp::None;
                self.focus = Focus::Editor;
            }
            DialogResult::Cancel => {
                if let Some(ref d) = self.current_dialog {
                    if d.title() == self.i18n.get("dialog.plugin.title") {
                        self.sidebar.reload_plugins();
                        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    }
                }
                self.current_dialog = None;
                self.pending_op = PendingOp::None;
                self.focus = Focus::Editor;
            }
            _ => {}
        }
    }

    fn handle_panel_key(&mut self, key: KeyEvent) {
        use crate::widgets::find_panel::PanelField;

        match key.code {
            KeyCode::Esc => {
                self.layout.panel_height = 0;
                self.focus = Focus::Editor;
                self.recompute_layout();
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.search_status = None;
                }
            }
            KeyCode::Tab => self.find_panel.next_field(),
            KeyCode::BackTab => self.find_panel.prev_field(),
            KeyCode::Enter => {
                match self.find_panel.focused_field {
                    PanelField::FindInput => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            self.find_prev();
                        } else {
                            self.find_next();
                        }
                    }
                    PanelField::ReplaceInput => {
                        self.replace_current();
                    }
                    PanelField::MatchCase => {
                        self.find_panel.flags.match_case = !self.find_panel.flags.match_case;
                        self.run_search();
                    }
                    PanelField::WholeWord => {
                        self.find_panel.flags.whole_word = !self.find_panel.flags.whole_word;
                        self.run_search();
                    }
                    PanelField::Regex => {
                        self.find_panel.flags.use_regex = !self.find_panel.flags.use_regex;
                        self.run_search();
                    }
                    PanelField::Prev => self.find_prev(),
                    PanelField::Next => self.find_next(),
                    PanelField::Close => {
                        self.layout.panel_height = 0;
                        self.focus = Focus::Editor;
                        self.recompute_layout();
                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            buffer.search_status = None;
                            if self.config.vi_mode {
                                buffer.vi_mode = zee_core::ViMode::Normal;
                            }
                        }
                    }
                    PanelField::ReplaceBtn => self.replace_current(),
                    PanelField::ReplaceAllBtn => self.replace_all(),
                }
            }
            KeyCode::F(3) => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.find_prev();
                } else {
                    self.find_next();
                }
            }
            KeyCode::Char(c) => {
                match self.find_panel.focused_field {
                    PanelField::FindInput => {
                        self.find_panel.find_text.push(c);
                        self.run_search();
                    }
                    PanelField::ReplaceInput => {
                        self.find_panel.replace_text.push(c);
                    }
                    _ => {
                        // Handle space for toggles
                        if c == ' ' {
                            match self.find_panel.focused_field {
                                PanelField::MatchCase => {
                                    self.find_panel.flags.match_case = !self.find_panel.flags.match_case;
                                    self.run_search();
                                }
                                PanelField::WholeWord => {
                                    self.find_panel.flags.whole_word = !self.find_panel.flags.whole_word;
                                    self.run_search();
                                }
                                PanelField::Regex => {
                                    self.find_panel.flags.use_regex = !self.find_panel.flags.use_regex;
                                    self.run_search();
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                match self.find_panel.focused_field {
                    PanelField::FindInput => {
                        self.find_panel.find_text.pop();
                        self.run_search();
                    }
                    PanelField::ReplaceInput => {
                        self.find_panel.replace_text.pop();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn run_search(&mut self) {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            let query = zee_core::search::SearchQuery {
                pattern: self.find_panel.find_text.clone(),
                flags: self.find_panel.flags.clone(),
            };
            buffer.find_results = buffer.search(&query);
            buffer.search_status = None;

            if !buffer.find_results.is_empty() {
                // Find first match at or after cursor
                let cursor = buffer.cursor;
                buffer.current_match_idx = buffer.find_results.iter().position(|m| m.char_range.start >= cursor);
                if buffer.current_match_idx.is_none() {
                    buffer.current_match_idx = Some(0); // Wrap to top
                }

                // Scroll to current match
                if let Some(idx) = buffer.current_match_idx {
                    let m = &buffer.find_results[idx];
                    buffer.cursor = m.char_range.start;
                    self.ensure_cursor_visible();
                }
            } else {
                buffer.current_match_idx = None;
                if !self.find_panel.find_text.is_empty() {
                    buffer.search_status = Some(self.i18n.get("status.no_matches").to_string());
                }
            }
        }
    }

    fn find_next(&mut self) {
        let (next_idx, wrapped) = if let Some(buffer) = self.buffers.get(self.active_buffer) {
            if buffer.find_results.is_empty() { (None, false) }
            else if let Some(idx) = buffer.current_match_idx {
                let next_idx = (idx + 1) % buffer.find_results.len();
                (Some(next_idx), next_idx == 0 && idx != 0)
            } else {
                (Some(0), false)
            }
        } else {
            (None, false)
        };

        if let Some(next_idx) = next_idx {
            let buffer = &mut self.buffers[self.active_buffer];
            buffer.current_match_idx = Some(next_idx);
            let m = &buffer.find_results[next_idx];
            buffer.cursor = m.char_range.start;
            self.ensure_cursor_visible();
            if wrapped {
                let msg = self.i18n.get("status.search_wrapped_top").to_string();
                self.buffers[self.active_buffer].search_status = Some(msg);
            } else {
                self.buffers[self.active_buffer].search_status = None;
            }
        }
    }

    fn find_prev(&mut self) {
        let (prev_idx, wrapped) = if let Some(buffer) = self.buffers.get(self.active_buffer) {
            if buffer.find_results.is_empty() { (None, false) }
            else if let Some(idx) = buffer.current_match_idx {
                let prev_idx = if idx == 0 { buffer.find_results.len() - 1 } else { idx - 1 };
                (Some(prev_idx), prev_idx == buffer.find_results.len() - 1 && idx != buffer.find_results.len() - 1)
            } else {
                (Some(buffer.find_results.len() - 1), false)
            }
        } else {
            (None, false)
        };

        if let Some(prev_idx) = prev_idx {
            let buffer = &mut self.buffers[self.active_buffer];
            buffer.current_match_idx = Some(prev_idx);
            let m = &buffer.find_results[prev_idx];
            buffer.cursor = m.char_range.start;
            self.ensure_cursor_visible();
            if wrapped {
                let msg = self.i18n.get("status.search_wrapped_bottom").to_string();
                self.buffers[self.active_buffer].search_status = Some(msg);
            } else {
                self.buffers[self.active_buffer].search_status = None;
            }
        }
    }

    fn replace_current(&mut self) {
        let mut replaced = false;
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            if let Some(idx) = buffer.current_match_idx {
                let m = buffer.find_results[idx].clone();
                buffer.delete(m.char_range.clone());
                buffer.insert(m.char_range.start, &self.find_panel.replace_text);
                replaced = true;
            }
        }
        self.run_search();
        if replaced {
            let msg = self.i18n.get("status.replaced_count").replace("{n}", "1");
            self.buffers[self.active_buffer].search_status = Some(msg);
        }
    }

    fn replace_all(&mut self) {
        let (pattern, flags, replace_text) = (self.find_panel.find_text.clone(), self.find_panel.flags.clone(), self.find_panel.replace_text.clone());
        let mut count = 0;
        
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            let query = zee_core::search::SearchQuery {
                pattern,
                flags,
            };
            let results = buffer.search(&query);
            if results.is_empty() {
                buffer.search_status = Some(self.i18n.get("status.no_matches").to_string());
                return;
            }

            for m in results.into_iter().rev() {
                buffer.delete(m.char_range.clone());
                buffer.insert(m.char_range.start, &replace_text);
                count += 1;
            }
            let msg = self.i18n.get("status.replaced_count").replace("{n}", &count.to_string());
            buffer.search_status = Some(msg);
        }
        self.run_search();
        if count > 0 {
            let msg = self.i18n.get("status.replaced_count").replace("{n}", &count.to_string());
            self.buffers[self.active_buffer].search_status = Some(msg);
        }
    }


    fn open_menu(&mut self, idx: usize) {
        self.focus = Focus::Menu;
        self.active_menu = Some(idx);
        self.selected_item = 0;
        self.submenu_stack.clear();
    }

    fn close_menu(&mut self) {
        if self.focus == Focus::Menu {
            self.focus = Focus::Editor;
        }
        self.active_menu = None;
        self.submenu_stack.clear();
    }

    fn handle_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if let Some((_, parent_selected)) = self.submenu_stack.pop() {
                    self.selected_item = parent_selected;
                } else {
                    self.close_menu();
                }
            }
            KeyCode::Left => {
                if let Some((_, parent_selected)) = self.submenu_stack.pop() {
                    self.selected_item = parent_selected;
                } else {
                    if let Some(idx) = self.active_menu {
                        let next_idx = if idx == 0 { self.menus.len() - 1 } else { idx - 1 };
                        self.open_menu(next_idx);
                    }
                }
            }
            KeyCode::Right => {
                // Check if current item has a submenu
                let is_submenu = {
                    let menu = self.get_current_active_menu();
                    matches!(menu.items.get(self.selected_item), Some(MenuItem::Submenu { .. }))
                };
                if is_submenu {
                    self.submenu_stack.push((self.active_menu.unwrap(), self.selected_item));
                    self.selected_item = 0;
                } else {
                    if let Some(idx) = self.active_menu {
                        let next_idx = (idx + 1) % self.menus.len();
                        self.open_menu(next_idx);
                    }
                }
            }
            KeyCode::Up => {
                let items_len = self.get_current_active_menu().items.len();
                loop {
                    self.selected_item = if self.selected_item == 0 {
                        items_len - 1
                    } else {
                        self.selected_item - 1
                    };
                    let is_separator = {
                        let menu = self.get_current_active_menu();
                        matches!(menu.items[self.selected_item], MenuItem::Separator)
                    };
                    if !is_separator {
                        break;
                    }
                }
            }
            KeyCode::Down => {
                let items_len = self.get_current_active_menu().items.len();
                loop {
                    self.selected_item = (self.selected_item + 1) % items_len;
                    let is_separator = {
                        let menu = self.get_current_active_menu();
                        matches!(menu.items[self.selected_item], MenuItem::Separator)
                    };
                    if !is_separator {
                        break;
                    }
                }
            }
            KeyCode::Enter => {
                let item = self.get_current_active_menu().items[self.selected_item].clone();
                match item {
                    MenuItem::Action { action, .. } | MenuItem::Toggle { action, .. } => {
                        self.perform_action(action);
                        if self.current_dialog.is_none() && self.focus != Focus::Panel {
                            self.focus = Focus::Editor;
                        }
                        self.active_menu = None;
                        self.submenu_stack.clear();
                    }
                    MenuItem::Submenu { .. } => {
                        self.submenu_stack.push((self.active_menu.unwrap(), self.selected_item));
                        self.selected_item = 0;
                    }
                    MenuItem::Separator => {}
                }
            }
            _ => {}
        }
    }

    fn get_current_active_menu(&self) -> &Menu {
        let mut menu = &self.menus[self.active_menu.unwrap_or(0)];
        for (_m_idx, i_idx) in &self.submenu_stack {
            if let MenuItem::Submenu { menu: ref sub, .. } = menu.items[*i_idx] {
                menu = sub;
            }
        }
        menu
    }

    fn move_cursor_vup(&mut self, extend_selection: bool) {
        let (_ex, _ey, ew, _eh) = self.layout.editor_bounds();
        let tab_size = self.config.tab_size;
        let buffer = &mut self.buffers[self.active_buffer];
        let (line, col) = buffer.char_to_line_col(buffer.cursor);
        let wraps = buffer.wrap_line(line, ew as usize, tab_size);
        
        let mut v_idx = 0;
        for (i, range) in wraps.iter().enumerate() {
            if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                v_idx = i;
                break;
            }
        }

        let current_vcol = buffer.get_visual_col(line, col, &wraps[v_idx], tab_size);

        if v_idx > 0 {
            let target_range = &wraps[v_idx - 1];
            buffer.cursor = buffer.get_char_at_vcol(line, target_range.clone(), current_vcol, tab_size);
        } else if line > 0 {
            let prev_line = line - 1;
            let prev_wraps = buffer.wrap_line(prev_line, ew as usize, tab_size);
            let target_range = prev_wraps.last().unwrap();
            buffer.cursor = buffer.get_char_at_vcol(prev_line, target_range.clone(), current_vcol, tab_size);
        }

        if extend_selection {
            buffer.update_selection();
        } else {
            buffer.selection = None;
        }
    }

    fn move_cursor_vdown(&mut self, extend_selection: bool) {
        let (_ex, _ey, ew, _eh) = self.layout.editor_bounds();
        let tab_size = self.config.tab_size;
        let buffer = &mut self.buffers[self.active_buffer];
        let (line, col) = buffer.char_to_line_col(buffer.cursor);
        let wraps = buffer.wrap_line(line, ew as usize, tab_size);
        
        let mut v_idx = 0;
        for (i, range) in wraps.iter().enumerate() {
            if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                v_idx = i;
                break;
            }
        }

        let current_vcol = buffer.get_visual_col(line, col, &wraps[v_idx], tab_size);

        if v_idx < wraps.len() - 1 {
            let target_range = &wraps[v_idx + 1];
            buffer.cursor = buffer.get_char_at_vcol(line, target_range.clone(), current_vcol, tab_size);
        } else if line < buffer.line_count() - 1 {
            let next_line = line + 1;
            let next_wraps = buffer.wrap_line(next_line, ew as usize, tab_size);
            let target_range = &next_wraps[0];
            buffer.cursor = buffer.get_char_at_vcol(next_line, target_range.clone(), current_vcol, tab_size);
        }

        if extend_selection {
            buffer.update_selection();
        } else {
            buffer.selection = None;
        }
    }

    pub fn open_path(&mut self, path: &std::path::Path) -> bool {
        if let Some(idx) = self.buffers.iter().position(|b| b.path.as_deref() == Some(path)) {
            self.active_buffer = idx;
            self.update_active_outline();
            self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
            self.recompute_layout();
            return true;
        }

        match self.editor_from_file(path) {
            Ok(mut buffer) => {
                let syntax = self.detect_syntax(path);
                buffer.update_syntax(syntax);

                if self.buffers.len() == 1 {
                    let first = &self.buffers[0];
                    if !first.is_modified() && first.path.is_none() && first.rope.len_chars() == 0 {
                        self.buffers[0] = buffer;
                        self.active_buffer = 0;
                        let mut recent = zee_core::recent::RecentFiles::load();
                        recent.add(path);
                        self.update_active_outline();
                        self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                        self.recompute_layout();
                        return true;
                    }
                }

                self.buffers.push(buffer);
                self.active_buffer = self.buffers.len() - 1;
                let mut recent = zee_core::recent::RecentFiles::load();
                recent.add(path);
                self.update_active_outline();
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                self.recompute_layout();
                true
            }
            Err(e) => {
                self.focus = Focus::Dialog;
                self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                    self.i18n.get("dialog.error").to_string(),
                    format!("{}: {}", path.display(), e),
                    vec![(self.i18n.get("dialog.ok").to_string(), dialog::Action::Cancel)],
                )));
                false
            }
        }
    }

    pub fn save_buffer(&mut self, idx: usize) -> anyhow::Result<()> {
        if let Some(buffer) = self.buffers.get_mut(idx) {
            buffer.cleanup_on_save(self.config.trim_trailing_whitespace, self.config.ensure_final_newline);
            let res = buffer.save();
            if res.is_ok() {
                if let Some(path) = &buffer.path {
                    if zee_core::gdrive::GDriveManager::is_gdrive_path(path) {
                        zee_core::gdrive::GDriveManager::sync_in_background(path, None);
                    }
                }
            }
            res
        } else {
            anyhow::bail!("Buffer not found")
        }
    }

    pub fn save_as_buffer(&mut self, idx: usize, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(buffer) = self.buffers.get_mut(idx) {
            buffer.cleanup_on_save(self.config.trim_trailing_whitespace, self.config.ensure_final_newline);
            let res = buffer.save_as(path);
            if res.is_ok() {
                if zee_core::gdrive::GDriveManager::is_gdrive_path(path) {
                    zee_core::gdrive::GDriveManager::sync_in_background(path, None);
                }
            }
            res
        } else {
            anyhow::bail!("Buffer not found")
        }
    }

    fn perform_action(&mut self, action: Action) {
        match action {
            Action::New => {
                self.buffers.push(self.new_editor());
                self.active_buffer = self.buffers.len() - 1;
                self.update_active_outline();
                self.recompute_layout();
            }
            Action::NewFromTemplate(id) => {
                let templates = zee_core::template::Template::load_all();
                if let Some(tpl) = templates.iter().find(|t| t.id == id) {
                    let (content, cursor_offset) = tpl.expand(None);
                    let mut buffer = self.new_editor();
                    buffer.insert(0, &content);
                    buffer.cursor = cursor_offset.min(buffer.rope.len_chars());
                    buffer.selection = None;
                    buffer.selection_anchor = None;

                    if !tpl.extension.is_empty() {
                        let ext_clean = tpl.extension.trim_start_matches('.');
                        if let Some(def) = self.syntax_defs.iter().find(|s| s.meta.extensions.iter().any(|e| e == ext_clean)) {
                            if let Ok(highlighter) = zee_core::syntax::SyntaxHighlighter::new(def.clone()) {
                                buffer.update_syntax(Some(highlighter));
                            }
                        }
                    }

                    if self.buffers.len() == 1 {
                        let first = &self.buffers[0];
                        if !first.is_modified() && first.path.is_none() && first.rope.len_chars() == 0 {
                            self.buffers[0] = buffer;
                            self.active_buffer = 0;
                            self.update_active_outline();
                            self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                            self.recompute_layout();
                            return;
                        }
                    }

                    self.buffers.push(buffer);
                    self.active_buffer = self.buffers.len() - 1;
                    self.update_active_outline();
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                    self.recompute_layout();
                }
            }
            Action::OpenTemplatesFolder => {
                if let Some(dir) = zee_core::template::Template::templates_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = zee_core::selfupdate::open_url(&dir.to_string_lossy());
                }
            }
            Action::Open => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::Open;
                self.current_dialog = Some(Box::new(dialog::OpenDialog::new(&self.i18n)));
            }
            Action::OpenGoogleDrive => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::Open;
                self.current_dialog = Some(Box::new(dialog::GoogleDriveDialog::new(&self.i18n)));
            }
            Action::ReloadFile => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.is_modified() {
                        self.focus = Focus::Dialog;
                        self.pending_op = PendingOp::Reload;
                        let filename = buffer.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy()).unwrap_or_else(|| std::borrow::Cow::Borrowed(self.i18n.get("status.no_name")));
                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                            self.i18n.get("dialog.unsaved_changes_title").to_string(),
                            self.i18n.get("dialog.unsaved_changes").replace("{filename}", &filename),
                            vec![
                                (self.i18n.get("dialog.save").to_string(), dialog::Action::Save),
                                (self.i18n.get("dialog.dont_save").to_string(), dialog::Action::DontSave),
                                (self.i18n.get("dialog.cancel").to_string(), dialog::Action::Cancel),
                            ]
                        )));
                        return;
                    } else {
                        let _ = buffer.reload_from_disk();
                        self.update_active_outline();
                        self.recompute_layout();
                    }
                }
            }
            Action::OpenRecent(path) => {
                self.open_path(&path);
            }
            Action::ClearRecent => {
                let mut recent = zee_core::recent::RecentFiles::load();
                recent.clear();
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
            }
            Action::Save => {
                let needs_save_as = if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    buffer.path.is_none()
                } else {
                    false
                };
                if needs_save_as {
                    self.perform_action(Action::SaveAs);
                } else {
                    let _ = self.save_buffer(self.active_buffer);
                }
            }
            Action::SaveAs => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::SaveAs;
                let buffer = self.buffers.get(self.active_buffer);
                let current_path = buffer.and_then(|b| b.path.as_ref());
                let current_enc = buffer.map(|b| b.encoding).unwrap_or(Encoding::Utf8);
                let default_ext = buffer.and_then(|b| {
                    if let Some(h) = &b.syntax_highlighter {
                        if h.def.meta.name.to_lowercase() == "markdown" {
                            return Some(".md");
                        }
                    }
                    None
                }).unwrap_or(".txt");
                self.current_dialog = Some(Box::new(dialog::SaveAsDialog::new(current_path, Some(default_ext), current_enc, &self.i18n)));
            }
            Action::Close => {
                if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    if buffer.is_modified() {
                        self.focus = Focus::Dialog;
                        self.pending_op = PendingOp::Close;
                        let filename = buffer.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy()).unwrap_or_else(|| std::borrow::Cow::Borrowed(self.i18n.get("status.no_name")));
                        self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                            self.i18n.get("dialog.unsaved_changes_title").to_string(),
                            self.i18n.get("dialog.unsaved_changes").replace("{filename}", &filename),
                            vec![
                                (self.i18n.get("dialog.save").to_string(), dialog::Action::Save),
                                (self.i18n.get("dialog.dont_save").to_string(), dialog::Action::DontSave),
                                (self.i18n.get("dialog.cancel").to_string(), dialog::Action::Cancel),
                            ]
                        )));
                        return;
                    }
                }
                self.buffers.remove(self.active_buffer);
                if self.buffers.is_empty() {
                    self.buffers.push(self.new_editor());
                }
                self.active_buffer = self.active_buffer.min(self.buffers.len() - 1);
                self.update_active_outline();
                self.recompute_layout();
            }
            Action::Find => {
                self.find_panel.is_replace_mode = false;
                self.layout.panel_height = 2;
                self.focus = Focus::Panel;
                self.find_panel.focused_field = crate::widgets::find_panel::PanelField::FindInput;
                self.recompute_layout();
                self.run_search();
            }
            Action::Replace => {
                self.find_panel.is_replace_mode = true;
                self.layout.panel_height = 3;
                self.focus = Focus::Panel;
                self.find_panel.focused_field = crate::widgets::find_panel::PanelField::FindInput;
                self.recompute_layout();
                self.run_search();
            }
            Action::GoToLine => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::None;
                self.current_dialog = Some(Box::new(dialog::GoToLineDialog::new(&self.i18n)));
            }
            Action::Exit => {
                if let Some((idx, buffer)) = self.buffers.iter().enumerate().find(|(_, b)| b.is_modified()) {
                    self.active_buffer = idx; // Switch to the modified buffer to show it
                    self.focus = Focus::Dialog;
                    self.pending_op = PendingOp::Exit;
                    let filename = buffer.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy()).unwrap_or_else(|| std::borrow::Cow::Borrowed(self.i18n.get("status.no_name")));
                    self.current_dialog = Some(Box::new(dialog::MessageDialog::new(
                        self.i18n.get("dialog.unsaved_changes_title").to_string(),
                        self.i18n.get("dialog.unsaved_changes").replace("{filename}", &filename),
                        vec![
                            (self.i18n.get("dialog.save").to_string(), dialog::Action::Save),
                            (self.i18n.get("dialog.dont_save").to_string(), dialog::Action::DontSave),
                            (self.i18n.get("dialog.cancel").to_string(), dialog::Action::Cancel),
                        ]
                    )));
                    self.recompute_layout();
                    return;
                }
                self.running = false;
            }

            Action::Undo => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.undo();
                }
                self.ensure_cursor_visible();
            }
            Action::Redo => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.redo();
                }
                self.ensure_cursor_visible();
            }
            Action::SelectAll => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.select_all();
                }
            }
            Action::Copy => {
                if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    if let Some(range) = buffer.selection.clone() {
                        let text = buffer.rope.slice(range).to_string();
                        let _ = clipboard::set_clipboard(&text);
                    }
                }
            }
            Action::Cut => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if let Some(range) = buffer.selection.clone() {
                        let text = buffer.rope.slice(range.clone()).to_string();
                        let _ = clipboard::set_clipboard(&text);
                        buffer.delete(range);
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
                self.ensure_cursor_visible();
            }
            Action::Paste => {
                if let Some(text) = clipboard::get_clipboard() {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        if let Some(range) = buffer.selection.clone() {
                            buffer.delete(range);
                        }
                        buffer.insert(buffer.cursor, &text);
                        buffer.selection = None;
                        buffer.selection_anchor = None;
                    }
                }
                self.ensure_cursor_visible();
            }
            Action::FormatDocument => {
                self.apply_plugin_transform("format_json");
            }
            Action::SortLines => {
                self.apply_plugin_transform("sort_lines");
            }
            Action::ToUpperCase => {
                self.apply_plugin_transform("to_uppercase");
            }
            Action::ToLowerCase => {
                self.apply_plugin_transform("to_lowercase");
            }
            Action::ToSnakeCase => {
                self.apply_plugin_transform("to_snake_case");
            }
            Action::ToCamelCase => {
                self.apply_plugin_transform("to_camel_case");
            }
            Action::PluginCommand(ref cmd) => {
                self.apply_plugin_transform(cmd);
            }
            Action::ToggleSidebar => {
                self.sidebar.toggle_visibility();
                self.config.sidebar = self.sidebar.visible;
                let _ = Config::write_key("sidebar", &self.config.sidebar.to_string());
                if !self.sidebar.visible && self.focus == Focus::Sidebar {
                    self.focus = Focus::Editor;
                }
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                self.recompute_layout();
            }
            Action::ToggleOutline => {
                if !self.sidebar.visible {
                    self.sidebar.visible = true;
                    self.config.sidebar = true;
                    let _ = Config::write_key("sidebar", "true");
                }
                self.sidebar.active_tab = crate::widgets::sidebar::SidebarTab::Outline;
                self.focus = Focus::Sidebar;
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                self.recompute_layout();
            }
            Action::ToggleLineNumbers => {
                self.config.line_numbers = !self.config.line_numbers;
                let _ = Config::write_key("line_numbers", &self.config.line_numbers.to_string());
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                self.recompute_layout();
            }
            Action::ToggleWordWrap => {
                self.config.word_wrap = !self.config.word_wrap;
                let _ = Config::write_key("word_wrap", &self.config.word_wrap.to_string());
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
            }
            Action::ToggleViMode => {
                if self.layout.panel_height > 0 {
                    self.layout.panel_height = 0;
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.search_status = None;
                    }
                    if self.focus == Focus::Panel {
                        self.focus = Focus::Editor;
                    }
                }

                self.config.vi_mode = !self.config.vi_mode;
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                
                for b in self.buffers.iter_mut() {
                    b.vi_mode = if self.config.vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                    b.selection = None;
                    b.selection_anchor = None;
                }
                self.is_vi_cmd_mode = false;
                self.vi_cmd.clear();
                self.vi_message = None;
                self.recompute_layout();
            }

            Action::ToggleTrimTrailingWhitespace => {
                self.config.trim_trailing_whitespace = !self.config.trim_trailing_whitespace;
                let _ = Config::write_key("trim_trailing_whitespace", &self.config.trim_trailing_whitespace.to_string());
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
            }
            Action::ToggleEnsureFinalNewline => {
                self.config.ensure_final_newline = !self.config.ensure_final_newline;
                let _ = Config::write_key("ensure_final_newline", &self.config.ensure_final_newline.to_string());
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
            }
            Action::About => {
                self.focus = Focus::Dialog;
                self.current_dialog = Some(Box::new(dialog::AboutDialog::new(&self.i18n)));
            }
            Action::CheckForUpdates => {
                self.focus = Focus::Dialog;
                self.current_dialog = Some(Box::new(dialog::UpdateDialog::new(&self.i18n, self.config.include_prerelease)));
            }
            Action::ReopenWithEncoding(enc) => {
                if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    self.target_encoding = Some(enc);
                    if buffer.is_modified() {
                        self.focus = Focus::Dialog;
                        self.pending_op = PendingOp::None;
                        self.current_dialog = Some(Box::new(dialog::ReopenConfirmationDialog::new(&self.i18n)));
                    } else {
                        // Reload immediately
                        if let Some(path) = buffer.path.clone() {
                            if let Ok(new_buffer) = self.editor_from_file(&path) {
                                let mut b = new_buffer;
                                b.encoding = enc;
                                self.buffers[self.active_buffer] = b;
                                self.update_active_outline();
                                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                self.recompute_layout();
                            }
                        }
                    }
                }
            }
            Action::ConvertToEncoding(enc) => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.encoding = enc;
                    buffer.modified_since_save = true;
                    self.menus = Self::build_menus(&self.i18n, &self.config, Some(buffer), &self.themes, &self.syntax_defs);
                }
            }
            Action::SetLineEnding(le) => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.line_ending = le;
                    buffer.modified_since_save = true;
                    self.menus = Self::build_menus(&self.i18n, &self.config, Some(buffer), &self.themes, &self.syntax_defs);
                }
            }
            Action::SetTheme(theme_id) => {
                if let Some(theme) = self.themes.iter().find(|t| t.meta.name.to_lowercase().replace(" ", "-") == theme_id) {
                    self.theme = theme.clone();
                    self.config.theme = theme_id;
                    let _ = Config::write_key("theme", &self.config.theme);
                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                }
            }
            Action::SetSyntax(syntax_name) => {
                if let Some(syntax_def) = self.syntax_defs.iter().find(|s| s.meta.name == syntax_name) {
                    if let Ok(highlighter) = zee_core::syntax::SyntaxHighlighter::new(syntax_def.clone()) {
                        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                            buffer.update_syntax(Some(highlighter));
                        }
                    }
                } else if syntax_name == "Plain Text" {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        buffer.update_syntax(None);
                    }
                }
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
            }
            Action::SetLanguage(lang_id) => {
                self.config.language = lang_id.clone();
                let _ = Config::write_key("language", &self.config.language);
                self.i18n = I18n::load(&self.config.language);
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                self.recompute_layout();
            }
            Action::OpenSettings => {
                self.open_settings_dialog();
            }
            Action::OpenFolder => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::OpenFolder;
                self.current_dialog = Some(Box::new(dialog::OpenFolderDialog::new(Some(&self.sidebar.file_tree.root_path), &self.i18n)));
            }
            Action::ToggleFiles => {
                if !self.sidebar.visible {
                    self.sidebar.visible = true;
                    self.config.sidebar = true;
                    let _ = Config::write_key("sidebar", "true");
                }
                self.sidebar.active_tab = crate::widgets::sidebar::SidebarTab::Files;
                self.focus = Focus::Sidebar;
                self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                self.recompute_layout();
            }
            Action::RefreshFileTree => {
                self.sidebar.refresh_files();
            }
            Action::ManagePlugins => {
                self.focus = Focus::Dialog;
                self.current_dialog = Some(Box::new(dialog::PluginManagerDialog::new(
                    &self.sidebar.plugin_manager,
                    Some(&self.config.plugin_registries),
                    &self.i18n,
                )));
            }
            Action::OpenPluginsFolder => {
                if let Some(dir) = zee_core::plugin::PluginManager::plugins_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = zee_core::selfupdate::open_url(&dir.to_string_lossy());
                }
            }
            Action::ExportConfig => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::ExportConfig;
                self.current_dialog = Some(Box::new(dialog::SaveAsDialog::new(
                    Some(&std::path::PathBuf::from("zee-config.zip")),
                    Some(".zip"),
                    Encoding::Utf8,
                    &self.i18n,
                )));
            }
            Action::ExportAll => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::ExportAll;
                self.current_dialog = Some(Box::new(dialog::SaveAsDialog::new(
                    Some(&std::path::PathBuf::from("zee-backup.zip")),
                    Some(".zip"),
                    Encoding::Utf8,
                    &self.i18n,
                )));
            }
            Action::ImportConfig => {
                self.focus = Focus::Dialog;
                self.pending_op = PendingOp::ImportConfig;
                self.current_dialog = Some(Box::new(dialog::OpenDialog::new(&self.i18n)));
            }
            _ => {} // TODO: other actions
        }
    }

    pub fn open_settings_dialog(&mut self) {
        let dialog = crate::widgets::dialog::SettingsDialog::new(
            self.config.clone(),
            &self.themes,
            &self.i18n,
        );
        self.current_dialog = Some(Box::new(dialog));
        self.focus = Focus::Dialog;
    }

    fn mouse_to_buffer_pos(&self, x: u16, y: u16) -> Option<usize> {
        let (ex, ey, ew, eh) = self.layout.editor_bounds();
        if x < ex || x >= ex + ew || y < ey || y >= ey + eh {
            return None;
        }

        let buffer = &self.buffers[self.active_buffer];
        let tab_size = self.config.tab_size;

        if self.config.word_wrap {
            let mut current_row = 0;
            let target_row = (y - ey) as usize;
            let mut logical_line_idx = buffer.scroll_row;
            let mut vrow_offset = buffer.scroll_vrow;

            while logical_line_idx < buffer.line_count() {
                let wraps = buffer.wrap_line(logical_line_idx, ew as usize, tab_size);
                for (_v_idx, range) in wraps.iter().enumerate().skip(vrow_offset) {
                    if current_row == target_row {
                        // Found the visual line!
                        let target_vcol = (x - ex) as usize;
                        return Some(buffer.get_char_at_vcol(logical_line_idx, range.clone(), target_vcol, tab_size));
                    }
                    current_row += 1;
                    if current_row > target_row { break; }
                }
                if current_row > target_row { break; }
                logical_line_idx += 1;
                vrow_offset = 0;
            }
            Some(buffer.rope.len_chars())
        } else {
            let line_idx = buffer.scroll_row + (y - ey) as usize;
            if line_idx >= buffer.line_count() {
                return Some(buffer.rope.len_chars());
            }

            let line = buffer.line(line_idx);
            let mut visual_x = 0;
            let mut char_idx = buffer.rope.line_to_char(line_idx);
            let target_visual_x = buffer.scroll_col + (x - ex) as usize;

            for c in line.chars() {
                let char_w = if c == '\t' {
                    tab_size - (visual_x % tab_size)
                } else {
                    c.width().unwrap_or(0)
                };

                if visual_x + char_w > target_visual_x {
                    return Some(char_idx);
                }
                
                visual_x += char_w;
                char_idx += 1;
                
                if c == '\n' || c == '\r' {
                    return Some(char_idx.saturating_sub(1));
                }
            }
            
            Some(char_idx)
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        let now = Instant::now();
        let (x, y) = (mouse.column, mouse.row);

        // Modal dialog ALWAYS captures all mouse events!
        if self.focus == Focus::Dialog || self.current_dialog.is_some() {
            self.focus = Focus::Dialog;
            if let Some(ref mut dialog) = self.current_dialog {
                let (dx, dy, dw, dh) = self.layout.dialog_bounds(dialog.dimensions());
                let result = dialog.handle_mouse(mouse, dx, dy, dw, dh);
                self.handle_dialog_result(result);
            }
            return;
        }

        match mouse.kind {
            MouseEventKind::Down(event::MouseButton::Left) | MouseEventKind::Down(event::MouseButton::Middle) => {
                if now.duration_since(self.last_click_time) < Duration::from_millis(300)
                    && self.last_click_pos == (x, y)
                {
                    self.click_count = (self.click_count % 3) + 1;
                } else {
                    self.click_count = 1;
                }
                self.last_click_time = now;
                self.last_click_pos = (x, y);

                let is_middle = mouse.kind == MouseEventKind::Down(event::MouseButton::Middle);
                let is_shift = mouse.modifiers.contains(KeyModifiers::SHIFT);

                // Handle Menu interaction (Left click only)
                if !is_middle && self.focus == Focus::Menu {
                    for (rx, ry, rw, rh, depth, item_idx) in self.dropdown_rects.iter().rev() {
                        if x >= *rx && x < *rx + *rw && y >= *ry && y < *ry + *rh {
                            if *depth <= self.submenu_stack.len() {
                                self.submenu_stack.truncate(*depth);
                                self.selected_item = *item_idx;
                                let menu = self.get_current_active_menu();
                                let item = menu.items[self.selected_item].clone();
                                match item {
                                    MenuItem::Action { action, .. } | MenuItem::Toggle { action, .. } => {
                                        self.perform_action(action);
                                        if self.current_dialog.is_none() && self.focus != Focus::Panel {
                                            self.focus = Focus::Editor;
                                        }
                                        self.active_menu = None;
                                        self.submenu_stack.clear();
                                    }
                                    MenuItem::Submenu { .. } => {
                                        self.submenu_stack.push((self.active_menu.unwrap(), self.selected_item));
                                        self.selected_item = 0;
                                    }
                                    MenuItem::Separator => {}
                                }
                                return;
                            }
                        }
                    }
                }

                let (_mx, my, _mw, mh) = self.layout.menu_bounds();
                let (_tx, ty, _tw, th) = self.layout.tab_bounds();
                if y >= my && y < my + mh {
                    // Menu Bar
                    if !is_middle {
                        for (idx, (_label, start, end)) in self.layout.menu_bar_items.iter().enumerate() {
                            if x >= *start && x < *end {
                                self.open_menu(idx);
                                return;
                            }
                        }
                    }
                    if self.focus == Focus::Menu {
                        self.close_menu();
                    }
                } else if y >= ty && y < ty + th {
                    // Tab Bar
                    if self.focus == Focus::Menu {
                        self.close_menu();
                    }
                    for (idx, start, end) in &self.layout.tab_rects {
                        if x >= *start && x < *end {
                            if is_middle {
                                self.active_buffer = *idx;
                                self.perform_action(Action::Close);
                            } else {
                                if x >= *end - 3 && x < *end - 1 {
                                    self.active_buffer = *idx;
                                    self.perform_action(Action::Close);
                                } else {
                                    self.active_buffer = *idx;
                                    self.update_active_outline();
                                    self.menus = Self::build_menus(&self.i18n, &self.config, self.buffers.get(self.active_buffer), &self.themes, &self.syntax_defs);
                                    self.recompute_layout();
                                    if self.layout.panel_height > 0 {
                                        self.run_search();
                                    }
                                }
                            }
                            return;
                        }
                    }
                } else if y >= self.height.saturating_sub(self.layout.status_height) {
                    // Status Bar
                    if self.focus == Focus::Menu {
                        self.close_menu();
                    }
                } else {
                    // Sidebar
                    let (sx, sy, sw, sh) = self.layout.sidebar_bounds();
                    if sw > 0 && x >= sx && x < sx + sw && y >= sy && y < sy + sh {
                        if self.focus == Focus::Menu {
                            self.close_menu();
                        }
                        let rel_x = x - sx;
                        let rel_y = y - sy;
                        self.focus = Focus::Sidebar;
                        let action = self.sidebar.handle_click(rel_x, rel_y, self.layout.is_right_sidebar, sw, sh.saturating_sub(1) as usize);
                        match action {
                            crate::widgets::sidebar::SidebarAction::OpenFile(path) => {
                                self.open_or_switch_to_file(path);
                                self.focus = Focus::Editor;
                            }
                            crate::widgets::sidebar::SidebarAction::JumpToLine(line) => {
                                if let Some(buf) = self.buffers.get_mut(self.active_buffer) {
                                    let line_idx = line.min(buf.line_count().saturating_sub(1));
                                    buf.cursor = buf.rope.line_to_char(line_idx);
                                    buf.scroll_row = line_idx.saturating_sub(5);
                                    buf.selection = None;
                                    buf.selection_anchor = None;
                                }
                                self.ensure_cursor_visible();
                                self.focus = Focus::Editor;
                            }
                            crate::widgets::sidebar::SidebarAction::ToggleHidden => {
                                let new_val = self.sidebar.toggle_show_hidden();
                                self.config.show_hidden = new_val;
                                let _ = zee_core::Config::save_show_hidden(new_val);
                            }
                            crate::widgets::sidebar::SidebarAction::None => {}
                        }
                        return;
                    }

                    // Editor or Panel
                    if self.focus == Focus::Menu {
                        self.close_menu();
                    } else {
                        let (gx, gy, gw, gh) = self.layout.gutter_bounds();
                        if x >= gx && x < gx + gw && y >= gy && y < gy + gh {
                            // Gutter click
                            let buffer = &mut self.buffers[self.active_buffer];
                            let line_idx = buffer.scroll_row + (y - gy) as usize;
                            buffer.select_line(line_idx);
                            self.focus = Focus::Editor;
                            return;
                        }

                        if let Some(pos) = self.mouse_to_buffer_pos(x, y) {
                            self.focus = Focus::Editor;
                            let buffer = &mut self.buffers[self.active_buffer];
                            if is_shift {
                                buffer.ensure_selection();
                                buffer.cursor = pos;
                                buffer.update_selection();
                            } else {
                                match self.click_count {
                                    1 => {
                                        buffer.cursor = pos;
                                        buffer.selection = None;
                                        buffer.selection_anchor = Some(pos);
                                    }
                                    2 => buffer.select_word(pos),
                                    3 => {
                                        let line = buffer.rope.char_to_line(pos);
                                        buffer.select_line(line);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            MouseEventKind::Drag(event::MouseButton::Left) => {
                let (ex, ey, ew, eh) = self.layout.editor_bounds();
                if ew > 0 && eh > 0 {
                    let buffer = &mut self.buffers[self.active_buffer];
                    if y < ey && buffer.scroll_row > 0 {
                        buffer.scroll_row = buffer.scroll_row.saturating_sub(1);
                    } else if y >= ey + eh && buffer.scroll_row + (eh as usize) < buffer.line_count() {
                        buffer.scroll_row += 1;
                    }

                    let clamped_x = x.clamp(ex, ex + ew - 1);
                    let clamped_y = y.clamp(ey, ey + eh - 1);
                    if let Some(pos) = self.mouse_to_buffer_pos(clamped_x, clamped_y) {
                        let buffer = &mut self.buffers[self.active_buffer];
                        if buffer.selection_anchor.is_none() {
                            buffer.selection_anchor = Some(buffer.cursor);
                        }
                        buffer.cursor = pos;
                        buffer.update_selection();
                    }
                }
            }
            MouseEventKind::Down(event::MouseButton::Right) => {
                let (sx, sy, sw, sh) = self.layout.sidebar_bounds();
                if sw > 0 && x >= sx && x < sx + sw && y >= sy && y < sy + sh {
                    let rel_x = x - sx;
                    let rel_y = y - sy;
                    self.focus = Focus::Sidebar;
                    if let Some((path, is_dir)) = self.sidebar.item_at_click(rel_x, rel_y) {
                        self.current_dialog = Some(Box::new(dialog::FileContextMenuDialog::new(
                            path,
                            is_dir,
                            self.sidebar.file_tree.show_hidden,
                            &self.i18n,
                        )));
                        self.focus = Focus::Dialog;
                    }
                    return;
                }
            }
            MouseEventKind::Up(event::MouseButton::Left) => {
                let buffer = &mut self.buffers[self.active_buffer];
                if buffer.selection.is_none() {
                    buffer.selection_anchor = None;
                }
            }
            MouseEventKind::ScrollUp => {
                let (sx, sy, sw, sh) = self.layout.sidebar_bounds();
                if sw > 0 && x >= sx && x < sx + sw && y >= sy && y < sy + sh {
                    self.sidebar.select_prev(sh.saturating_sub(1) as usize);
                    return;
                }
                let is_shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
                let buffer = &mut self.buffers[self.active_buffer];
                if is_shift {
                    if !self.config.word_wrap && buffer.scroll_col > 0 {
                        buffer.scroll_col = buffer.scroll_col.saturating_sub(4);
                    }
                } else {
                    if buffer.scroll_row > 0 {
                        buffer.scroll_row -= 1;
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                let (sx, sy, sw, sh) = self.layout.sidebar_bounds();
                if sw > 0 && x >= sx && x < sx + sw && y >= sy && y < sy + sh {
                    self.sidebar.select_next(sh.saturating_sub(1) as usize);
                    return;
                }
                let is_shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
                let (_ex, _ey, _ew, eh) = self.layout.editor_bounds();
                let buffer = &mut self.buffers[self.active_buffer];
                if is_shift {
                    if !self.config.word_wrap {
                        buffer.scroll_col += 4;
                    }
                } else {
                    if buffer.scroll_row + (eh as usize) < buffer.line_count() {
                        buffer.scroll_row += 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn render(&mut self, stdout: &mut Stdout) -> Result<()> {
        self.renderer.clear();
        
        // Render regions
        self.render_menu();
        self.render_tabs();
        if self.layout.panel_height > 0 {
            self.render_panel();
        }
        if self.sidebar.visible && self.layout.sidebar_width > 0 {
            self.render_sidebar();
        }
        self.render_editor();
        self.render_status();
        if self.config.vi_mode {
            self.render_command_line();
        }


        // Render open dropdowns
        self.dropdown_rects.clear();
        if let Some(idx) = self.active_menu {
            let start_x = self.layout.menu_bar_items[idx].1;
            let menu = self.menus[idx].clone();
            self.render_dropdown(start_x, 1, &menu, 0);
        }

        // Render dialog if active
        if let Some(ref dialog) = self.current_dialog {
            let (x, y, dw, dh) = self.layout.dialog_bounds(dialog.dimensions());
            dialog.render(&mut self.renderer, &self.theme, x, y, dw, dh);
        }

        self.renderer.present(stdout)?;

        // Move terminal cursor to the logical cursor position for IME
        if self.active_menu.is_none() {
            // Hardware cursor for vi command mode
            if self.is_vi_cmd_mode {
                let (cx, cy, cw, ch) = self.layout.cmdline_bounds();
                if ch > 0 {
                    let mut cur_x = cx;
                    for c in self.vi_cmd.chars() {
                        if cur_x >= cx + cw { break; }
                        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                        cur_x += w as u16;
                    }
                    if cur_x < cx + cw {
                        execute!(stdout, crossterm::cursor::SetCursorStyle::BlinkingBlock)?;
                        execute!(stdout, cursor::Show, cursor::MoveTo(cur_x, cy))?;
                    } else {
                        execute!(stdout, cursor::Hide)?;
                    }
                } else {
                    execute!(stdout, cursor::Hide)?;
                }
            } else if self.focus == Focus::Editor && self.current_dialog.is_none() {
                let (ex, ey, ew, eh) = self.layout.editor_bounds();
                let buffer = &self.buffers[self.active_buffer];
                let (line, col) = buffer.char_to_line_col(buffer.cursor);
                
                if line >= buffer.scroll_row {
                    let mut visual_x = 0;
                    let line_slice = buffer.rope.line(line);
                    let tab_size = self.config.tab_size;
                    for (i, c) in line_slice.chars().enumerate() {
                        if i >= col { break; }
                        if c == '\t' {
                            visual_x += tab_size - (visual_x % tab_size);
                        } else {
                            visual_x += c.width().unwrap_or(0);
                        }
                    }

                    if visual_x >= buffer.scroll_col && visual_x < buffer.scroll_col + ew as usize {
                        let rx = ex + (visual_x - buffer.scroll_col) as u16;
                        let ry = ey + (line - buffer.scroll_row) as u16;
                        if ry < ey + eh {
                            if self.config.vi_mode {
                                let style = match buffer.vi_mode {
                                    zee_core::ViMode::Normal => crossterm::cursor::SetCursorStyle::BlinkingBlock,
                                    zee_core::ViMode::Insert => {
                                        if zee_core::is_cjk_ime_active() {
                                            crossterm::cursor::SetCursorStyle::BlinkingUnderScore
                                        } else {
                                            crossterm::cursor::SetCursorStyle::BlinkingBar
                                        }
                                    }
                                    _ => crossterm::cursor::SetCursorStyle::BlinkingBlock,
                                };
                                execute!(stdout, style)?;
                            }
                            execute!(stdout, cursor::Show, cursor::MoveTo(rx, ry))?;
                        } else {
                            execute!(stdout, cursor::Hide)?;
                        }
                    } else {
                        execute!(stdout, cursor::Hide)?;
                    }
                } else {
                    execute!(stdout, cursor::Hide)?;
                }
            } else if self.focus == Focus::Panel && self.current_dialog.is_none() {
                // Focus hardware cursor on find panel input
                let (px, py, _pw, _ph) = self.layout.panel_bounds();
                let find_label = format!("{}: ", self.i18n.get("panel.find"));
                let rep_label = format!("{}: ", self.i18n.get("panel.replace"));
                let find_label_w: u16 = find_label.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
                let rep_label_w: u16 = rep_label.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
                let label_w = find_label_w.max(rep_label_w);
                let input_x = px + 1 + label_w;
                let cursor_x = match self.find_panel.focused_field {
                    PanelField::FindInput => {
                        let text_w: u16 = self.find_panel.find_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
                        input_x + text_w.min(30)
                    },
                    PanelField::ReplaceInput => {
                        let text_w: u16 = self.find_panel.replace_text.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
                        input_x + text_w.min(30)
                    },
                    _ => 0,
                };
                let cursor_y = match self.find_panel.focused_field {
                    PanelField::ReplaceInput => py + 1,
                    _ => py,
                };
                if cursor_x > 0 {
                    execute!(stdout, cursor::Show, cursor::MoveTo(cursor_x, cursor_y))?;
                } else {
                    execute!(stdout, cursor::Hide)?;
                }
            } else if self.focus == Focus::Dialog || self.current_dialog.is_some() {
                if let Some(ref dialog) = self.current_dialog {
                    if let Some((dx, dy)) = dialog.cursor_pos() {
                        let (x, y, _dw, _dh) = self.layout.dialog_bounds(dialog.dimensions());
                        execute!(stdout, cursor::Show, cursor::MoveTo(x + dx, y + dy))?;
                    } else {
                        execute!(stdout, cursor::Hide)?;
                    }
                } else {
                    execute!(stdout, cursor::Hide)?;
                }
            } else {
                execute!(stdout, cursor::Hide)?;
            }
        }
        stdout.flush()?;
        Ok(())
    }

    fn render_menu(&mut self) {
        let (x, y, w, _h) = self.layout.menu_bounds();
        let bg = self.to_ct_color(self.theme.ui.menu_bar_bg);
        let fg = self.to_ct_color(self.theme.ui.menu_bar_fg);
        let active_bg = self.to_ct_color(self.theme.ui.menu_item_active_bg);
        let active_fg = self.to_ct_color(self.theme.ui.menu_item_active_fg);
        
        // Background
        for dx in 0..w {
            self.renderer.set_cell(x + dx, y, Cell {
                ch: ' ',
                bg,
                ..Default::default()
            });
        }

        // Menu items
        for (idx, (label, start, end)) in self.layout.menu_bar_items.iter().enumerate() {
            let is_active = self.active_menu == Some(idx);
            let item_bg = if is_active { active_bg } else { bg };
            let item_fg = if is_active { active_fg } else { fg };

            let mut cur_l_x = start + 1;
            for c in label.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                self.renderer.set_cell(cur_l_x, y, Cell {
                    ch: c,
                    width: cw as u8,
                    bg: item_bg,
                    fg: item_fg,
                    ..Default::default()
                });
                cur_l_x += cw as u16;
            }
            // Fill padding
            self.renderer.set_cell(*start, y, Cell { ch: ' ', bg: item_bg, ..Default::default() });
            self.renderer.set_cell(*end - 1, y, Cell { ch: ' ', bg: item_bg, ..Default::default() });
        }
    }

    fn calculate_dropdown_width(menu: &Menu) -> u16 {
        let max_width = menu.items.iter().map(|item| match item {
            MenuItem::Action { label, shortcut, .. } => {
                let lw: usize = label.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)).sum();
                let sw: usize = shortcut.as_ref().map(|s| s.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)).sum::<usize>() + 2).unwrap_or(0);
                lw + sw
            }
            MenuItem::Toggle { label, .. } => {
                let lw: usize = label.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)).sum();
                lw + 4
            }
            MenuItem::Submenu { label, .. } => {
                let lw: usize = label.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)).sum();
                lw + 4
            }
            MenuItem::Separator => 5,
        }).max().unwrap_or(10) as u16;
        max_width + 3 + 2 // max_width + padding (3) + borders (2)
    }

    fn render_dropdown(&mut self, mut x: u16, mut y: u16, menu: &Menu, depth: usize) {
        let items = &menu.items;
        let box_w = Self::calculate_dropdown_width(menu);
        let max_width = box_w.saturating_sub(5); // box_w - padding(3) - borders(2)
        let total_h = items.len() as u16 + 2;

        // Clamp dropdown within screen bounds
        if x + box_w > self.width {
            x = self.width.saturating_sub(box_w);
        }
        if y + total_h > self.height {
            y = self.height.saturating_sub(total_h);
        }

        let editor_bg = self.to_ct_color(self.theme.editor.background);
        let mut bg = self.to_ct_color(self.theme.ui.dialog_bg);
        // Ensure menu background is clearly distinct from editor background (e.g. pure black)
        if bg == editor_bg || bg == crossterm::style::Color::Reset || matches!(bg, crossterm::style::Color::Rgb { r: 0, g: 0, b: 0 }) {
            bg = crossterm::style::Color::AnsiValue(236);
        }

        let fg = self.to_ct_color(self.theme.ui.menu_bar_fg);
        let mut active_bg = self.to_ct_color(self.theme.ui.menu_item_active_bg);
        if active_bg == bg || active_bg == crossterm::style::Color::Reset || active_bg == editor_bg {
            active_bg = crossterm::style::Color::AnsiValue(240);
        }
        let active_fg = self.to_ct_color(self.theme.ui.menu_item_active_fg);

        let mut border_fg = self.to_ct_color(self.theme.ui.dialog_border);
        if border_fg == crossterm::style::Color::Reset || border_fg == bg || border_fg == editor_bg {
            border_fg = crossterm::style::Color::AnsiValue(245);
        }
        let sc_fg = self.to_ct_color(self.theme.editor.line_number);

        let box_w = max_width + 2;
        let top_y = y;
        let bot_y = y + 1 + items.len() as u16;

        // Render top border
        self.renderer.set_cell(x, top_y, Cell { ch: '┌', fg: border_fg, bg, ..Default::default() });
        for dx in 1..=max_width {
            self.renderer.set_cell(x + dx, top_y, Cell { ch: '─', fg: border_fg, bg, ..Default::default() });
        }
        self.renderer.set_cell(x + max_width + 1, top_y, Cell { ch: '┐', fg: border_fg, bg, ..Default::default() });

        // Render bottom border
        self.renderer.set_cell(x, bot_y, Cell { ch: '└', fg: border_fg, bg, ..Default::default() });
        for dx in 1..=max_width {
            self.renderer.set_cell(x + dx, bot_y, Cell { ch: '─', fg: border_fg, bg, ..Default::default() });
        }
        self.renderer.set_cell(x + max_width + 1, bot_y, Cell { ch: '┘', fg: border_fg, bg, ..Default::default() });

        let is_current_level = depth == self.submenu_stack.len();
        let selected_at_this_level = if depth < self.submenu_stack.len() {
            Some(self.submenu_stack[depth].1)
        } else if is_current_level {
            Some(self.selected_item)
        } else {
            None
        };

        for (i, item) in items.iter().enumerate() {
            let iy = y + 1 + i as u16;
            let is_selected = selected_at_this_level == Some(i);
            let item_bg = if is_selected { active_bg } else { bg };
            let item_fg = if is_selected { active_fg } else { fg };

            self.dropdown_rects.push((x, iy, box_w, 1, depth, i));

            match item {
                MenuItem::Separator => {
                    self.renderer.set_cell(x, iy, Cell { ch: '├', fg: border_fg, bg, ..Default::default() });
                    for dx in 1..=max_width {
                        self.renderer.set_cell(x + dx, iy, Cell {
                            ch: '─',
                            bg,
                            fg: border_fg,
                            ..Default::default()
                        });
                    }
                    self.renderer.set_cell(x + max_width + 1, iy, Cell { ch: '┤', fg: border_fg, bg, ..Default::default() });
                }
                MenuItem::Action { label, shortcut, .. } => {
                    // Left and right borders
                    self.renderer.set_cell(x, iy, Cell { ch: '│', fg: border_fg, bg: item_bg, ..Default::default() });
                    self.renderer.set_cell(x + max_width + 1, iy, Cell { ch: '│', fg: border_fg, bg: item_bg, ..Default::default() });

                    // Background fill
                    for dx in 1..=max_width {
                        self.renderer.set_cell(x + dx, iy, Cell { ch: ' ', bg: item_bg, ..Default::default() });
                    }

                    let mut cur_ix = x + 2;
                    for c in label.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        self.renderer.set_cell(cur_ix, iy, Cell { ch: c, width: cw as u8, bg: item_bg, fg: item_fg, bold: is_selected, ..Default::default() });
                        cur_ix += cw as u16;
                    }
                    if let Some(s) = shortcut {
                        let sw: u16 = s.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u16).sum();
                        let mut sx = x + max_width - sw;
                        for c in s.chars() {
                            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                            self.renderer.set_cell(sx, iy, Cell { ch: c, width: cw as u8, bg: item_bg, fg: if is_selected { active_fg } else { sc_fg }, ..Default::default() });
                            sx += cw as u16;
                        }
                    }
                }
                MenuItem::Toggle { label, checked, is_radio, .. } => {
                    // Left and right borders
                    self.renderer.set_cell(x, iy, Cell { ch: '│', fg: border_fg, bg: item_bg, ..Default::default() });
                    self.renderer.set_cell(x + max_width + 1, iy, Cell { ch: '│', fg: border_fg, bg: item_bg, ..Default::default() });

                    // Background fill
                    for dx in 1..=max_width {
                        self.renderer.set_cell(x + dx, iy, Cell { ch: ' ', bg: item_bg, ..Default::default() });
                    }

                    let prefix = if *is_radio {
                        if *checked { "● " } else { "○ " }
                    } else {
                        if *checked { "[✓] " } else { "[ ] " }
                    };
                    let mut cur_ix = x + 2;
                    for c in prefix.chars().chain(label.chars()) {
                        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        self.renderer.set_cell(cur_ix, iy, Cell { ch: c, width: cw as u8, bg: item_bg, fg: item_fg, bold: is_selected, ..Default::default() });
                        cur_ix += cw as u16;
                    }
                }
                MenuItem::Submenu { label, menu: sub } => {
                    // Left and right borders
                    self.renderer.set_cell(x, iy, Cell { ch: '│', fg: border_fg, bg: item_bg, ..Default::default() });
                    self.renderer.set_cell(x + max_width + 1, iy, Cell { ch: '│', fg: border_fg, bg: item_bg, ..Default::default() });

                    // Background fill
                    for dx in 1..=max_width {
                        self.renderer.set_cell(x + dx, iy, Cell { ch: ' ', bg: item_bg, ..Default::default() });
                    }

                    let mut cur_ix = x + 2;
                    for c in label.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        self.renderer.set_cell(cur_ix, iy, Cell { ch: c, width: cw as u8, bg: item_bg, fg: item_fg, bold: is_selected, ..Default::default() });
                        cur_ix += cw as u16;
                    }
                    self.renderer.set_cell(x + max_width, iy, Cell { ch: '▶', bg: item_bg, fg: item_fg, ..Default::default() });

                    if selected_at_this_level == Some(i) && depth < self.submenu_stack.len() {
                        let sub_clone = sub.clone();
                        let sub_w = Self::calculate_dropdown_width(&sub_clone);
                        let sub_x = if x + box_w - 1 + sub_w <= self.width {
                            x + box_w - 1
                        } else {
                            x.saturating_sub(sub_w).max(1)
                        };
                        self.render_dropdown(sub_x, iy.saturating_sub(1), &sub_clone, depth + 1);
                    }
                }
            }
        }
    }

    fn render_tabs(&mut self) {
        let (x, y, w, h) = self.layout.tab_bounds();
        let bg = self.to_ct_color(self.theme.ui.tab_bar_bg);
        let active_bg = self.to_ct_color(self.theme.ui.tab_active_bg);
        let active_fg = self.to_ct_color(self.theme.ui.tab_active_fg);
        let inactive_bg = self.to_ct_color(self.theme.ui.tab_inactive_bg);
        let inactive_fg = self.to_ct_color(self.theme.ui.tab_inactive_fg);
        
        // Background for entire tab bar area (including top/bottom margins)
        for dy in 0..h {
            for dx in 0..w {
                self.renderer.set_cell(x + dx, y + dy, Cell {
                    ch: ' ',
                    bg,
                    ..Default::default()
                });
            }
        }

        let total_tabs_width: u16 = self.layout.tab_rects.iter().map(|(_, s, e)| e - s).sum();
        let needs_scroll = total_tabs_width > w;

        let display_w = if needs_scroll { w.saturating_sub(4) } else { w };
        let offset_x = if needs_scroll { 2 } else { 0 };

        let tab_y = y;

        if needs_scroll {
            // Render arrows
            self.renderer.set_cell(x, tab_y, Cell { ch: '<', bg: inactive_bg, fg: inactive_fg, ..Default::default() });
            self.renderer.set_cell(x + 1, tab_y, Cell { ch: ' ', bg: inactive_bg, ..Default::default() });
            self.renderer.set_cell(x + w - 2, tab_y, Cell { ch: ' ', bg: inactive_bg, ..Default::default() });
            self.renderer.set_cell(x + w - 1, tab_y, Cell { ch: '>', bg: inactive_bg, fg: inactive_fg, ..Default::default() });
        }

        let mut current_tab_x = x + offset_x;
        for (i, buffer) in self.buffers.iter().enumerate() {
            let is_active = i == self.active_buffer;
            let tab_bg = if is_active { active_bg } else { inactive_bg };
            let tab_fg = if is_active { active_fg } else { inactive_fg };
            
            let name = buffer.path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.i18n.get("status.no_name").to_string());
            let modified = if buffer.is_modified() { "[+] " } else { "" };
            let ro = if buffer.read_only { "[RO] " } else { "" };
            let label = format!(" {}{}{} × ", ro, modified, name);
            let tab_width: u16 = label.chars().map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u16).sum();

            // Basic scrolling: just hide tabs that don't fit for now
            if current_tab_x + tab_width > x + offset_x + display_w {
                break;
            }

            let mut cur_tx = current_tab_x;

            // Space prefix
            self.renderer.set_cell(cur_tx, tab_y, Cell { ch: ' ', bg: tab_bg, ..Default::default() });
            cur_tx += 1;

            if buffer.read_only {
                for c in "[RO] ".chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    self.renderer.set_cell(cur_tx, tab_y, Cell { ch: c, width: cw as u8, bg: tab_bg, fg: if is_active { Color::Cyan } else { Color::DarkGrey }, bold: is_active, ..Default::default() });
                    cur_tx += cw as u16;
                }
            }

            if buffer.is_modified() {
                for c in "[+] ".chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    self.renderer.set_cell(cur_tx, tab_y, Cell { ch: c, width: cw as u8, bg: tab_bg, fg: Color::Yellow, bold: true, ..Default::default() });
                    cur_tx += cw as u16;
                }
            }

            for c in name.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                self.renderer.set_cell(cur_tx, tab_y, Cell {
                    ch: c,
                    width: cw as u8,
                    bg: tab_bg,
                    fg: tab_fg,
                    bold: is_active,
                    ..Default::default()
                });
                cur_tx += cw as u16;
            }

            // Close button
            self.renderer.set_cell(cur_tx, tab_y, Cell { ch: ' ', bg: tab_bg, ..Default::default() });
            cur_tx += 1;
            self.renderer.set_cell(cur_tx, tab_y, Cell { ch: '×', bg: tab_bg, fg: tab_fg, bold: is_active, ..Default::default() });
            cur_tx += 1;
            self.renderer.set_cell(cur_tx, tab_y, Cell { ch: ' ', bg: tab_bg, ..Default::default() });

            current_tab_x += tab_width;

            // Space between tabs
            if current_tab_x < x + offset_x + display_w {
                self.renderer.set_cell(current_tab_x, tab_y, Cell {
                    ch: ' ',
                    bg,
                    ..Default::default()
                });
                current_tab_x += 1;
            }
        }
    }

    fn render_panel(&mut self) {
        let (x, y, w, h) = self.layout.panel_bounds();
        if h == 0 { return; }

        use crate::widgets::find_panel::PanelField;

        let normal_bg = self.to_ct_color(self.theme.ui.panel_bg);
        let normal_fg = self.to_ct_color(self.theme.ui.panel_fg);
        let focused_bg = self.to_ct_color(self.theme.ui.button_active_bg);
        let focused_fg = self.to_ct_color(self.theme.ui.button_active_fg);
        let error_fg = self.to_ct_color(self.theme.ui.panel_error_fg);

        // Background
        for dy in 0..h {
            for dx in 0..w {
                self.renderer.set_cell(x + dx, y + dy, Cell { ch: ' ', bg: normal_bg, ..Default::default() });
            }
        }

        let find_label = format!("{}: ", self.i18n.get("panel.find"));
        let rep_label = format!("{}: ", self.i18n.get("panel.replace"));
        let find_label_w: u16 = find_label.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let rep_label_w: u16 = rep_label.chars().map(|c| c.width().unwrap_or(0) as u16).sum();
        let label_w = find_label_w.max(rep_label_w);
        let input_x = x + 1 + label_w;
        let input_w = 30;

        // Row 1: Find label
        let mut cur_l_x = x + 1;
        for c in find_label.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_l_x, y, Cell { ch: c, bg: normal_bg, fg: normal_fg, width: cw as u8, ..Default::default() });
            cur_l_x += cw;
        }

        let is_find_focused = self.find_panel.focused_field == PanelField::FindInput;
        let input_bg = if is_find_focused { focused_bg } else { normal_bg };
        let input_fg = if is_find_focused { focused_fg } else { normal_fg };

        // Error color if no matches
        let buffer = &self.buffers[self.active_buffer];
        let input_fg = if !self.find_panel.find_text.is_empty() && buffer.find_results.is_empty() {
            error_fg
        } else {
            input_fg
        };

        for dx in 0..input_w {
            self.renderer.set_cell(input_x + dx, y, Cell { ch: ' ', bg: input_bg, ..Default::default() });
        }
        let mut cur_text_x = input_x;
        for c in self.find_panel.find_text.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            if cur_text_x + cw <= input_x + input_w {
                self.renderer.set_cell(cur_text_x, y, Cell { ch: c, bg: input_bg, fg: input_fg, width: cw as u8, ..Default::default() });
                cur_text_x += cw;
            } else {
                break;
            }
        }

        // Buttons
        let mut cur_x = input_x + input_w + 2;

        let prev_btn = format!(" {} ", self.i18n.get("panel.prev"));
        let is_prev_focused = self.find_panel.focused_field == PanelField::Prev;
        for c in prev_btn.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_x, y, Cell {
                ch: c,
                bg: if is_prev_focused { focused_bg } else { normal_bg },
                fg: if is_prev_focused { focused_fg } else { normal_fg },
                width: cw as u8,
                ..Default::default()
            });
            cur_x += cw;
        }
        cur_x += 1;

        let next_btn = format!(" {} ", self.i18n.get("panel.next"));
        let is_next_focused = self.find_panel.focused_field == PanelField::Next;
        for c in next_btn.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_x, y, Cell {
                ch: c,
                bg: if is_next_focused { focused_bg } else { normal_bg },
                fg: if is_next_focused { focused_fg } else { normal_fg },
                width: cw as u8,
                ..Default::default()
            });
            cur_x += cw;
        }
        cur_x += 1;

        let close_btn = format!(" {} ", self.i18n.get("panel.close"));
        let is_close_focused = self.find_panel.focused_field == PanelField::Close;
        for c in close_btn.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_x, y, Cell {
                ch: c,
                bg: if is_close_focused { focused_bg } else { normal_bg },
                fg: if is_close_focused { focused_fg } else { normal_fg },
                width: cw as u8,
                ..Default::default()
            });
            cur_x += cw;
        }
        cur_x += 2;

        // Match count badge on row 1
        let match_badge = if self.find_panel.find_text.is_empty() {
            "".to_string()
        } else if buffer.find_results.is_empty() {
            self.i18n.get("status.no_matches").to_string()
        } else if let Some(idx) = buffer.current_match_idx {
            self.i18n.get("status.matches")
                .replace("{current}", &(idx + 1).to_string())
                .replace("{total}", &buffer.find_results.len().to_string())
        } else {
            format!("{} matches", buffer.find_results.len())
        };

        if !match_badge.is_empty() {
            let badge_fg = if buffer.find_results.is_empty() { error_fg } else { normal_fg };
            for c in match_badge.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_x + cw < x + w {
                    self.renderer.set_cell(cur_x, y, Cell { ch: c, bg: normal_bg, fg: badge_fg, width: cw as u8, ..Default::default() });
                    cur_x += cw;
                }
            }
        }

        // Row 2: Replace or Toggles
        if self.find_panel.is_replace_mode {
            // Row 2: Replace input
            let mut cur_rep_l_x = x + 1;
            for c in rep_label.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                self.renderer.set_cell(cur_rep_l_x, y + 1, Cell { ch: c, bg: normal_bg, fg: normal_fg, width: cw as u8, ..Default::default() });
                cur_rep_l_x += cw;
            }

            let is_replace_focused = self.find_panel.focused_field == PanelField::ReplaceInput;
            let replace_bg = if is_replace_focused { focused_bg } else { normal_bg };
            let replace_fg = if is_replace_focused { focused_fg } else { normal_fg };

            for dx in 0..input_w {
                self.renderer.set_cell(input_x + dx, y + 1, Cell { ch: ' ', bg: replace_bg, ..Default::default() });
            }
            let mut cur_rep_text_x = input_x;
            for c in self.find_panel.replace_text.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                if cur_rep_text_x + cw <= input_x + input_w {
                    self.renderer.set_cell(cur_rep_text_x, y + 1, Cell { ch: c, bg: replace_bg, fg: replace_fg, width: cw as u8, ..Default::default() });
                    cur_rep_text_x += cw;
                } else {
                    break;
                }
            }

            let mut btn_x = input_x + input_w + 2;
            let replace_btn = format!(" {} ", self.i18n.get("panel.replace_one"));
            let is_rep_focused = self.find_panel.focused_field == PanelField::ReplaceBtn;
            for c in replace_btn.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                self.renderer.set_cell(btn_x, y + 1, Cell {
                    ch: c,
                    bg: if is_rep_focused { focused_bg } else { normal_bg },
                    fg: if is_rep_focused { focused_fg } else { normal_fg },
                    width: cw as u8,
                    ..Default::default()
                });
                btn_x += cw;
            }
            btn_x += 1;

            let replace_all_btn = format!(" {} ", self.i18n.get("panel.replace_all"));
            let is_rep_all_focused = self.find_panel.focused_field == PanelField::ReplaceAllBtn;
            for c in replace_all_btn.chars() {
                let cw = c.width().unwrap_or(0) as u16;
                self.renderer.set_cell(btn_x, y + 1, Cell {
                    ch: c,
                    bg: if is_rep_all_focused { focused_bg } else { normal_bg },
                    fg: if is_rep_all_focused { focused_fg } else { normal_fg },
                    width: cw as u8,
                    ..Default::default()
                });
                btn_x += cw;
            }
            btn_x += 2;

            // Show replace status message (e.g. "Replaced N occurrences" / "1 replacement made") next to Replace All button
            if let Some(ref status) = buffer.search_status {
                for c in status.chars() {
                    let cw = c.width().unwrap_or(0) as u16;
                    if btn_x + cw < x + w {
                        self.renderer.set_cell(btn_x, y + 1, Cell { ch: c, bg: normal_bg, fg: normal_fg, width: cw as u8, ..Default::default() });
                        btn_x += cw;
                    }
                }
            }

            // Row 3: Toggles
            self.render_toggles(x, y + 2);
        } else {
            // Row 2: Toggles
            self.render_toggles(x, y + 1);
        }
    }

    fn render_toggles(&mut self, x: u16, y: u16) {
        use crate::widgets::find_panel::PanelField;
        let normal_bg = self.to_ct_color(self.theme.ui.panel_bg);
        let normal_fg = self.to_ct_color(self.theme.ui.panel_fg);
        let focused_bg = self.to_ct_color(self.theme.ui.button_active_bg);
        let focused_fg = self.to_ct_color(self.theme.ui.button_active_fg);

        let mut cur_x = x + 1;
        
        let match_case = format!("[{}] {}", if self.find_panel.flags.match_case { "x" } else { " " }, self.i18n.get("panel.match_case"));
        let is_mc_focused = self.find_panel.focused_field == PanelField::MatchCase;
        for c in match_case.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_x, y, Cell {
                ch: c,
                bg: if is_mc_focused { focused_bg } else { normal_bg },
                fg: if is_mc_focused { focused_fg } else { normal_fg },
                width: cw as u8,
                ..Default::default()
            });
            cur_x += cw;
        }
        cur_x += 2;

        let whole_word = format!("[{}] {}", if self.find_panel.flags.whole_word { "x" } else { " " }, self.i18n.get("panel.whole_word"));
        let is_ww_focused = self.find_panel.focused_field == PanelField::WholeWord;
        for c in whole_word.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_x, y, Cell {
                ch: c,
                bg: if is_ww_focused { focused_bg } else { normal_bg },
                fg: if is_ww_focused { focused_fg } else { normal_fg },
                width: cw as u8,
                ..Default::default()
            });
            cur_x += cw;
        }
        cur_x += 2;

        let use_regex = format!("[{}] {}", if self.find_panel.flags.use_regex { "x" } else { " " }, self.i18n.get("panel.use_regex"));
        let is_re_focused = self.find_panel.focused_field == PanelField::Regex;
        for c in use_regex.chars() {
            let cw = c.width().unwrap_or(0) as u16;
            self.renderer.set_cell(cur_x, y, Cell {
                ch: c,
                bg: if is_re_focused { focused_bg } else { normal_bg },
                fg: if is_re_focused { focused_fg } else { normal_fg },
                width: cw as u8,
                ..Default::default()
            });
            cur_x += cw;
        }
    }

    fn render_editor(&mut self) {
        let (ex, ey, ew, eh) = self.layout.editor_bounds();
        let (_gx, _gy, gw, gh) = self.layout.gutter_bounds();
        let active_buffer_idx = self.active_buffer;
        let word_wrap = self.config.word_wrap;
        let tab_size = self.config.tab_size;

        let editor_bg = self.to_ct_color(self.theme.editor.background);
        let editor_fg = self.to_ct_color(self.theme.editor.foreground);
        let gutter_bg = editor_bg;
        let gutter_fg = self.to_ct_color(self.theme.editor.line_number);
        let selection_bg = self.to_ct_color(self.theme.editor.selection);
        let selection_fg = editor_fg;
        let cursor_bg = self.to_ct_color(self.theme.editor.cursor);

        let current_line_bg = self.theme.editor.current_line.map(|c| self.to_ct_color(c)).unwrap_or(editor_bg);

        let mut token_colors = std::collections::HashMap::new();
        use zee_core::syntax::TokenType::*;
        for &t in &[Keyword, TypeName, Function, String, Number, Comment, Operator, Punctuation, Constant, Attribute, Error] {
            token_colors.insert(t, self.get_token_color(t));
        }

        let focus = self.focus;
        let App { ref mut renderer, ref mut buffers, .. } = *self;
        let buffer = &mut buffers[active_buffer_idx];

        if word_wrap {
            // Word Wrap rendering
            let mut rendered_rows = 0;
            let mut logical_line_idx = buffer.scroll_row;
            let mut vrow_offset = buffer.scroll_vrow;

            let cursor_line = buffer.rope.char_to_line(buffer.cursor);

            while rendered_rows < eh && logical_line_idx < buffer.line_count() {
                let wraps = buffer.wrap_line(logical_line_idx, ew as usize, tab_size);
                let line_start_char = buffer.rope.line_to_char(logical_line_idx);
                let tokens = buffer.highlight_line(logical_line_idx);

                let is_current_line = logical_line_idx == cursor_line && focus == Focus::Editor;

                for (v_idx, range) in wraps.iter().enumerate().skip(vrow_offset) {
                    if rendered_rows >= eh { break; }
                    let ry = ey + rendered_rows;

                    let line_bg = if is_current_line { current_line_bg } else { editor_bg };

                    // Render gutter for the FIRST visual line of each logical line
                    if gw > 0 {
                        for dx in 0..gw {
                            renderer.set_cell(_gx + dx, ry, Cell { ch: ' ', bg: gutter_bg, ..Default::default() });
                        }
                        if v_idx == 0 {
                            let line_num = (logical_line_idx + 1).to_string();
                            let num_x = _gx + gw - line_num.len() as u16 - 1;
                            for (i, c) in line_num.chars().enumerate() {
                                renderer.set_cell(num_x + i as u16, ry, Cell { ch: c, bg: gutter_bg, fg: gutter_fg, ..Default::default() });
                            }
                        }
                    }

                    // Render visual line content
                    let mut visual_x = 0;
                    let line_slice = buffer.rope.line(logical_line_idx);
                    let mut byte_offset = 0;
                    let mut current_token_idx = 0;
                    
                    for (i, c) in line_slice.chars().enumerate() {
                        let char_len = c.len_utf8();
                        if i < range.start { 
                            byte_offset += char_len;
                            continue; 
                        }
                        if i >= range.end { break; }

                        let char_idx = line_start_char + i;
                        let char_w = if c == '\t' {
                            tab_size as u16 - (visual_x % tab_size as u16)
                        } else {
                            c.width().unwrap_or(0) as u16
                        };

                        let rx = ex + visual_x;
                        let mut bg = line_bg;
                        let mut fg = editor_fg;

                        // Syntax highlighting
                        while current_token_idx < tokens.len() && tokens[current_token_idx].byte_range.end <= byte_offset {
                            current_token_idx += 1;
                        }
                        if current_token_idx < tokens.len() && tokens[current_token_idx].byte_range.start <= byte_offset {
                            fg = *token_colors.get(&tokens[current_token_idx].token).unwrap_or(&editor_fg);
                        }

                        let in_selection = if buffer.vi_mode == zee_core::ViMode::VisualBlock {
                            buffer.get_visual_block_ranges().into_iter().any(|r| char_idx >= r.start && char_idx < r.end)
                        } else if let Some(ref r) = buffer.selection {
                            char_idx >= r.start && char_idx < r.end
                        } else {
                            false
                        };

                        if in_selection {
                            bg = selection_bg;
                            fg = selection_fg;
                        }
                        
                        // Search matches
                        for (idx, m) in buffer.find_results.iter().enumerate() {
                            if char_idx >= m.char_range.start && char_idx < m.char_range.end {
                                if Some(idx) == buffer.current_match_idx {
                                    bg = Color::Green;
                                    fg = Color::Black;
                                } else {
                                    bg = Color::Yellow;
                                    fg = Color::Black;
                                }
                                break;
                            }
                        }

                        if char_idx == buffer.cursor && focus == Focus::Editor {
                            bg = cursor_bg;
                            fg = editor_bg;
                        }

                        if c == '\t' {
                            renderer.set_cell(ex + visual_x, ry, Cell { ch: ' ', bg, fg, width: char_w as u8, ..Default::default() });
                        } else if c != '\n' && c != '\r' {
                            renderer.set_cell(rx, ry, Cell { ch: c, bg, fg, width: char_w as u8, ..Default::default() });
                        } else {
                            // End of logical line, might show cursor/selection
                            if char_idx == buffer.cursor && focus == Focus::Editor {
                                renderer.set_cell(rx, ry, Cell { ch: ' ', bg: cursor_bg, fg: editor_bg, width: 1, ..Default::default() });
                            } else if bg != line_bg {
                                renderer.set_cell(rx, ry, Cell { ch: ' ', bg, fg, width: 1, ..Default::default() });
                            } else if is_current_line {
                                renderer.set_cell(rx, ry, Cell { ch: ' ', bg: line_bg, fg, width: 1, ..Default::default() });
                            }
                        }
                        visual_x += char_w;
                        byte_offset += char_len;
                    }

                    // Handle cursor at the very end of file (after last char of last line)
                    if logical_line_idx == buffer.line_count() - 1 && range.end == line_slice.len_chars() {
                         let last_char_idx = line_start_char + line_slice.len_chars();
                         let ends_with_newline = line_slice.len_chars() > 0 && (line_slice.char(line_slice.len_chars()-1) == '\n' || line_slice.char(line_slice.len_chars()-1) == '\r');
                         if !ends_with_newline && buffer.cursor == last_char_idx && visual_x < ew {
                             renderer.set_cell(ex + visual_x, ry, Cell { ch: ' ', bg: cursor_bg, fg: editor_bg, ..Default::default() });
                             visual_x += 1;
                         }
                    }

                    // Clear rest of row
                    for dx in visual_x..ew {
                        renderer.set_cell(ex + dx, ry, Cell { ch: ' ', bg: line_bg, fg: editor_fg, ..Default::default() });
                    }

                    rendered_rows += 1;
                }
                logical_line_idx += 1;
                vrow_offset = 0;
            }

            // Clear remaining rows
            for dy in rendered_rows..eh {
                if gw > 0 {
                    for dx in 0..gw {
                        renderer.set_cell(_gx + dx, ey + dy, Cell { ch: ' ', bg: gutter_bg, ..Default::default() });
                    }
                }
                for dx in 0..ew {
                    renderer.set_cell(ex + dx, ey + dy, Cell { ch: ' ', bg: editor_bg, ..Default::default() });
                }
            }
        } else {
            // Existing No Wrap rendering
            // Render gutter
            if gw > 0 {
                for dy in 0..gh {
                    let line_idx = buffer.scroll_row + dy as usize;
                    for dx in 0..gw {
                        renderer.set_cell(_gx + dx, _gy + dy, Cell { ch: ' ', bg: gutter_bg, ..Default::default() });
                    }
                    if line_idx < buffer.line_count() {
                        let line_num = (line_idx + 1).to_string();
                        let num_x = _gx + gw - line_num.len() as u16 - 1;
                        for (i, c) in line_num.chars().enumerate() {
                            renderer.set_cell(num_x + i as u16, _gy + dy, Cell { ch: c, bg: gutter_bg, fg: gutter_fg, ..Default::default() });
                        }
                    }
                }
            }

            // Render editor area
            let cursor_line = buffer.rope.char_to_line(buffer.cursor);
            for dy in 0..eh {
                let line_idx = buffer.scroll_row + dy as usize;
                if line_idx >= buffer.line_count() {
                    for dx in 0..ew {
                        renderer.set_cell(ex + dx, ey + dy, Cell { ch: ' ', bg: editor_bg, ..Default::default() });
                    }
                    continue;
                }

                let is_current_line = line_idx == cursor_line && focus == Focus::Editor;
                let line_bg = if is_current_line { current_line_bg } else { editor_bg };

                let tokens = buffer.highlight_line(line_idx);
                let line = buffer.line(line_idx);
                let mut visual_x = 0;
                let mut current_token_idx = 0;
                let mut byte_offset = 0;
                
                for (char_idx, c) in (buffer.rope.line_to_char(line_idx)..).zip(line.chars()) {
                    let char_len = c.len_utf8();
                    let char_w = if c == '\t' {
                        let tab_size = tab_size as u16;
                        tab_size - (visual_x % tab_size)
                    } else {
                        c.width().unwrap_or(0) as u16
                    };

                    if visual_x + char_w > buffer.scroll_col as u16 + ew {
                        break;
                    }

                    if visual_x + char_w > buffer.scroll_col as u16 {
                        let rx = ex + (visual_x.saturating_sub(buffer.scroll_col as u16));
                        let ry = ey + dy;
                        let mut bg = line_bg;
                        let mut fg = editor_fg;

                        // Syntax highlighting
                        while current_token_idx < tokens.len() && tokens[current_token_idx].byte_range.end <= byte_offset {
                            current_token_idx += 1;
                        }
                        if current_token_idx < tokens.len() && tokens[current_token_idx].byte_range.start <= byte_offset {
                            fg = *token_colors.get(&tokens[current_token_idx].token).unwrap_or(&editor_fg);
                        }

                        let in_selection = if buffer.vi_mode == zee_core::ViMode::VisualBlock {
                            buffer.get_visual_block_ranges().into_iter().any(|r| char_idx >= r.start && char_idx < r.end)
                        } else if let Some(ref range) = buffer.selection {
                            char_idx >= range.start && char_idx < range.end
                        } else {
                            false
                        };

                        if in_selection {
                            bg = selection_bg;
                            fg = selection_fg;
                        }

                        // Search matches
                        for (idx, m) in buffer.find_results.iter().enumerate() {
                            if char_idx >= m.char_range.start && char_idx < m.char_range.end {
                                if Some(idx) == buffer.current_match_idx {
                                    bg = Color::Green;
                                    fg = Color::Black;
                                } else {
                                    bg = Color::Yellow;
                                    fg = Color::Black;
                                }
                                break;
                            }
                        }

                        if char_idx == buffer.cursor && focus == Focus::Editor {
                            bg = cursor_bg;
                            fg = editor_bg;
                        }

                        if c == '\t' {
                            for dx in 0..char_w {
                                let vx = visual_x + dx;
                                if vx >= buffer.scroll_col as u16 && vx < buffer.scroll_col as u16 + ew {
                                    renderer.set_cell(ex + (vx - buffer.scroll_col as u16), ry, Cell { ch: ' ', bg, fg, width: 1, ..Default::default() });
                                }
                            }
                        } else if c != '\n' && c != '\r' {
                            let rx = ex + (visual_x.saturating_sub(buffer.scroll_col as u16));
                            let ry = ey + dy;
                            renderer.set_cell(rx, ry, Cell { ch: c, bg, fg, width: char_w as u8, ..Default::default() });
                        } else {
                            if char_idx == buffer.cursor && focus == Focus::Editor {
                                renderer.set_cell(rx, ry, Cell { ch: ' ', bg: cursor_bg, fg: editor_bg, ..Default::default() });
                            } else if bg != line_bg {
                                renderer.set_cell(rx, ry, Cell { ch: ' ', bg, fg, ..Default::default() });
                            } else if is_current_line {
                                renderer.set_cell(rx, ry, Cell { ch: ' ', bg: line_bg, fg, ..Default::default() });
                            }
                        }
                    }
                    visual_x += char_w;
                    byte_offset += char_len;
                }
                // Last line end cursor
                if line_idx == buffer.line_count() - 1 {
                     let last_char_idx = buffer.rope.line_to_char(line_idx) + line.len_chars();
                     let ends_with_newline = line.len_chars() > 0 && (line.char(line.len_chars()-1) == '\n' || line.char(line.len_chars()-1) == '\r');
                     if !ends_with_newline && buffer.cursor == last_char_idx
                         && visual_x >= buffer.scroll_col as u16 && visual_x < buffer.scroll_col as u16 + ew {
                             renderer.set_cell(ex + (visual_x - buffer.scroll_col as u16), ey + dy, Cell { ch: ' ', bg: cursor_bg, fg: editor_bg, ..Default::default() });
                             visual_x += 1;
                         }
                }
                for dx in (visual_x.saturating_sub(buffer.scroll_col as u16))..ew {
                    renderer.set_cell(ex + dx, ey + dy, Cell { ch: ' ', bg: line_bg, fg: editor_fg, ..Default::default() });
                }

            }
        }
    }

    fn render_status(&mut self) {
        let (x, y, w, _h) = self.layout.status_bounds();
        let buffer = &self.buffers[self.active_buffer];
        let bg = self.to_ct_color(self.theme.ui.status_bar_bg);
        let fg = self.to_ct_color(self.theme.ui.status_bar_fg);

        // Background
        for dx in 0..w {
            self.renderer.set_cell(x + dx, y, Cell { ch: ' ', bg, ..Default::default() });
        }

        // Left segment: File status and path
        let modified = if buffer.is_modified() { "[+] " } else { "" };
        let filename = buffer.path.as_ref()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy())
            .unwrap_or_else(|| std::borrow::Cow::Borrowed(self.i18n.get("status.no_name")));
        let left_text = format!(" {}{}", modified, filename);

        // Right segment components
        let (line, col) = buffer.char_to_line_col(buffer.cursor);
        let mut visual_col = col + 1;
        if self.config.word_wrap {
            let (_ex, _ey, ew, _eh) = self.layout.editor_bounds();
            let wraps = buffer.wrap_line(line, ew as usize, self.config.tab_size);
            let char_in_line = col;
            for wrap in wraps {
                if char_in_line >= wrap.start && char_in_line <= wrap.end {
                    let mut w = 0;
                    let line_slice = buffer.rope.line(line);
                    for (i, c) in line_slice.chars().enumerate().skip(wrap.start) {
                        if i >= char_in_line { break; }
                        if c == '\t' {
                            let ts = self.config.tab_size;
                            w += ts - (w % ts);
                        } else {
                            w += c.width().unwrap_or(0);
                        }
                    }
                    visual_col = w + 1;
                    break;
                }
            }
        } else {
            let mut w = 0;
            let line_slice = buffer.rope.line(line);
            for (i, c) in line_slice.chars().enumerate() {
                if i >= col { break; }
                if c == '\t' {
                    let ts = self.config.tab_size;
                    w += ts - (w % ts);
                } else {
                    w += c.width().unwrap_or(0);
                }
            }
            visual_col = w + 1;
        }

        let cursor_info = self.i18n.get("status.cursor")
            .replace("{line}", &(line + 1).to_string())
            .replace("{col}", &visual_col.to_string());
        
        let selection_info = if let Some(ref range) = buffer.selection {
            format!(" {} ", self.i18n.get("status.selection").replace("{n}", &(range.end - range.start).to_string()))
        } else {
            "".to_string()
        };

        let search_info = if let Some(ref status) = buffer.search_status {
            format!(" {} ", status)
        } else if !buffer.find_results.is_empty() {
            let current = buffer.current_match_idx.map(|i| i + 1).unwrap_or(0);
            let total = buffer.find_results.len();
            format!(" {} ", self.i18n.get("status.matches")
                .replace("{current}", &current.to_string())
                .replace("{total}", &total.to_string()))
        } else {
            "".to_string()
        };

        let encoding = match buffer.encoding {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 BOM",
            Encoding::Utf16Le => "UTF-16LE",
            Encoding::Utf16Be => "UTF-16BE",
            Encoding::ShiftJis => "Shift-JIS",
            Encoding::EucJp => "EUC-JP",
            Encoding::Iso2022Jp => "ISO-2022-JP",
            Encoding::Latin1 => "Latin-1",
        };

        let line_ending = match buffer.line_ending {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
            LineEnding::Cr => "CR",
        };

        let syntax = buffer.syntax_highlighter.as_ref()
            .map(|h| h.def.meta.name.as_str())
            .unwrap_or_else(|| self.i18n.get("menu.view.syntax_plain"));

        let (vi_mode_str, vi_badge_bg, vi_badge_fg) = if self.config.vi_mode {
            match buffer.vi_mode {
                zee_core::ViMode::Normal => (" NORMAL", crossterm::style::Color::DarkBlue, crossterm::style::Color::White),
                zee_core::ViMode::Insert => {
                    if zee_core::is_cjk_ime_active() {
                        (" INSERT [あ]", crossterm::style::Color::DarkRed, crossterm::style::Color::White)
                    } else {
                        (" INSERT", crossterm::style::Color::DarkGreen, crossterm::style::Color::White)
                    }
                }
                zee_core::ViMode::Visual => (" VISUAL", crossterm::style::Color::DarkMagenta, crossterm::style::Color::White),
                zee_core::ViMode::VisualLine => (" V-LINE", crossterm::style::Color::DarkMagenta, crossterm::style::Color::White),
                zee_core::ViMode::VisualBlock => (" V-BLOCK", crossterm::style::Color::DarkMagenta, crossterm::style::Color::White),
            }
        } else {
            ("", bg, fg)
        };

        let right_text = format!("{}{} | {} | {} | {} | {} ", 
            search_info, selection_info, cursor_info, encoding, line_ending, syntax);
        let vi_sep = if !vi_mode_str.is_empty() { "| " } else { "" };
        let vi_badge_text = if !vi_mode_str.is_empty() { format!("{} ", vi_mode_str.trim_start()) } else { "".to_string() };

        // Render left
        let mut cur_x = x;
        for c in left_text.chars() {
            let width = c.width().unwrap_or(0) as u16;
            if cur_x + width <= x + w {
                self.renderer.set_cell(cur_x, y, Cell { ch: c, bg, fg, width: width as u8, ..Default::default() });
                cur_x += width;
            }
        }

        // Render right (right-aligned)
        let mut right_visual_width = 0;
        for c in right_text.chars().chain(vi_sep.chars()).chain(vi_badge_text.chars()) {
            right_visual_width += c.width().unwrap_or(0) as u16;
        }

        let mut cur_rx = x + w.saturating_sub(right_visual_width);
        for c in right_text.chars().chain(vi_sep.chars()) {
            let width = c.width().unwrap_or(0) as u16;
            if cur_rx >= x && cur_rx + width <= x + w {
                self.renderer.set_cell(cur_rx, y, Cell { ch: c, bg, fg, width: width as u8, ..Default::default() });
            }
            cur_rx += width;
        }
        for c in vi_badge_text.chars() {
            let width = c.width().unwrap_or(0) as u16;
            if cur_rx >= x && cur_rx + width <= x + w {
                self.renderer.set_cell(cur_rx, y, Cell { ch: c, bg: vi_badge_bg, fg: vi_badge_fg, width: width as u8, ..Default::default() });
            }
            cur_rx += width;
        }
    }

    fn render_command_line(&mut self) {
        let (x, y, w, h) = self.layout.cmdline_bounds();
        if h == 0 {
            return;
        }

        let bg = self.to_ct_color(self.theme.ui.status_bar_bg);
        let fg = self.to_ct_color(self.theme.ui.status_bar_fg);

        // Fill background
        for dx in 0..w {
            self.renderer.set_cell(x + dx, y, Cell { ch: ' ', bg, ..Default::default() });
        }

        if self.is_vi_cmd_mode {
            let mut cur_x = x;
            for c in self.vi_cmd.chars() {
                if cur_x >= x + w { break; }
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                self.renderer.set_cell(cur_x, y, Cell { ch: c, bg, fg, width: cw as u8, ..Default::default() });
                cur_x += cw as u16;
            }
            // Draw block cursor
            if cur_x < x + w {
                self.renderer.set_cell(cur_x, y, Cell { ch: ' ', bg: fg, fg: bg, ..Default::default() });
            }
        } else if let Some((ref msg, is_err)) = self.vi_message {
            let msg_fg = if is_err {
                crossterm::style::Color::Red
            } else {
                fg
            };
            let mut cur_x = x;
            for c in msg.chars() {
                if cur_x >= x + w { break; }
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                self.renderer.set_cell(cur_x, y, Cell { ch: c, bg, fg: msg_fg, width: cw as u8, ..Default::default() });
                cur_x += cw as u16;
            }
        } else if let Some(buf) = self.buffers.get(self.active_buffer) {
            let (hint, hint_fg) = match buf.vi_mode {
                zee_core::ViMode::Insert => {
                    if zee_core::is_cjk_ime_active() {
                        ("-- INSERT [あ] --", crossterm::style::Color::Red)
                    } else {
                        ("-- INSERT --", crossterm::style::Color::Green)
                    }
                }
                zee_core::ViMode::Visual => ("-- VISUAL --", fg),
                zee_core::ViMode::VisualLine => ("-- VISUAL LINE --", fg),
                zee_core::ViMode::VisualBlock => ("-- VISUAL BLOCK --", fg),
                zee_core::ViMode::Normal => ("", fg),
            };
            let mut cur_x = x;
            for c in hint.chars() {
                if cur_x >= x + w { break; }
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                self.renderer.set_cell(cur_x, y, Cell { ch: c, bg, fg: hint_fg, width: cw as u8, ..Default::default() });
                cur_x += cw as u16;
            }
        }
    }


    fn render_too_small(&self, stdout: &mut Stdout) -> Result<()> {
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        let msg = self.i18n.get("status.terminal_too_small")
            .replace("{cols}", &self.width.to_string())
            .replace("{rows}", &self.height.to_string());
        let msg_w = msg.width() as u16;
        let x = (self.width.saturating_sub(msg_w)) / 2;
        let y = self.height / 2;
        execute!(stdout, cursor::MoveTo(x, y))?;
        write!(stdout, "{}", msg)?;
        stdout.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn test_app_new_buffer_and_close() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        assert_eq!(app.buffers.len(), 1);
        assert_eq!(app.focus, Focus::Editor);

        // Perform New Buffer
        app.perform_action(Action::New);
        assert_eq!(app.buffers.len(), 2);
        assert_eq!(app.active_buffer, 1);

        // Close Buffer
        app.perform_action(Action::Close);
        assert_eq!(app.buffers.len(), 1);
        assert_eq!(app.active_buffer, 0);
    }

    #[test]
    fn test_file_open_dialog_workflow() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        
        // Trigger Open File action
        app.perform_action(Action::Open);
        assert_eq!(app.focus, Focus::Dialog);
        assert_eq!(app.pending_op, PendingOp::Open);
        assert!(app.current_dialog.is_some());

        // Cancel dialog
        app.handle_key(make_key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Editor);
        assert!(app.current_dialog.is_none());
    }

    #[test]
    fn test_file_save_and_unsaved_changes_dialog() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        
        // Modify buffer
        if let Some(buffer) = app.buffers.get_mut(app.active_buffer) {
            buffer.insert(0, "Hello World");
            assert!(buffer.is_modified());
        }

        // Try to exit -> Should trigger unsaved changes MessageDialog
        app.perform_action(Action::Exit);
        assert_eq!(app.focus, Focus::Dialog);
        assert_eq!(app.pending_op, PendingOp::Exit);
        assert!(app.current_dialog.is_some());

        // Select "DontSave" -> Discard changes & exit
        app.handle_dialog_result(DialogResult::Ok(dialog::Action::DontSave));
        assert!(!app.running);
    }

    #[test]
    fn test_i18n_language_switch_and_config() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        
        // Verify default language (ja or en)
        let initial_lang = app.config.language.clone();
        
        // Change language config & reload i18n
        app.config.language = if initial_lang == "ja" { "en".to_string() } else { "ja".to_string() };
        app.i18n = I18n::load(&app.config.language);
        app.menus = App::build_menus(&app.i18n, &app.config, app.buffers.get(app.active_buffer), &app.themes, &app.syntax_defs);
        
        // Check menu title updated
        let file_menu_label = app.menus[0].label.clone();
        if app.config.language == "ja" {
            assert_eq!(file_menu_label, "ファイル");
        } else {
            assert_eq!(file_menu_label, "File");
        }
    }

    #[test]
    fn test_find_and_replace_panel_workflow() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        
        if let Some(buffer) = app.buffers.get_mut(app.active_buffer) {
            buffer.insert(0, "foo bar foo");
        }

        // Open Find panel (Ctrl+F)
        app.handle_key(make_ctrl_key(KeyCode::Char('f')));
        assert_eq!(app.focus, Focus::Panel);

        // Input search query 'foo'
        app.handle_key(make_key(KeyCode::Char('f')));
        app.handle_key(make_key(KeyCode::Char('o')));
        app.handle_key(make_key(KeyCode::Char('o')));
        assert_eq!(app.find_panel.find_text, "foo");

        let buffer = &app.buffers[app.active_buffer];
        assert_eq!(buffer.find_results.len(), 2);

        // Close panel with Esc
        app.handle_key(make_key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Editor);

        // Open Replace panel with Ctrl+Shift+F
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
        assert_eq!(app.focus, Focus::Panel);
        assert!(app.find_panel.is_replace_mode);

        // Open Replace panel with Ctrl+R
        app.handle_key(make_ctrl_key(KeyCode::Char('r')));
        assert_eq!(app.focus, Focus::Panel);
        assert!(app.find_panel.is_replace_mode);

        // Close panel with Esc
        app.handle_key(make_key(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Editor);

        // Open About/Help with Ctrl+H
        app.handle_key(make_ctrl_key(KeyCode::Char('h')));
        assert_eq!(app.focus, Focus::Dialog);
        assert!(app.current_dialog.is_some());
    }

    #[test]
    fn test_find_panel_auto_closes_on_vi_mode_toggle() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        assert!(!app.config.vi_mode);

        // Open Find panel with Ctrl+F
        app.handle_key(make_ctrl_key(KeyCode::Char('f')));
        assert_eq!(app.focus, Focus::Panel);
        assert!(app.layout.panel_height > 0);

        // Toggle Vi mode ON with Ctrl+E -> search panel automatically closes!
        app.handle_key(make_ctrl_key(KeyCode::Char('e')));
        assert!(app.config.vi_mode);
        assert_eq!(app.layout.panel_height, 0);
        assert_eq!(app.focus, Focus::Editor);

        // Toggle Vi mode back to standard mode with Ctrl+E
        app.handle_key(make_ctrl_key(KeyCode::Char('e')));
        assert!(!app.config.vi_mode);
        assert_eq!(app.layout.panel_height, 0);
        assert_eq!(app.focus, Focus::Editor);
    }

    #[test]
    fn test_find_panel_esc_closes_from_editor_focus() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.handle_key(make_ctrl_key(KeyCode::Char('f')));
        assert!(app.layout.panel_height > 0);

        // Switch focus to Editor (simulating clicking editor)
        app.focus = Focus::Editor;
        app.handle_key(make_key(KeyCode::Esc));
        assert_eq!(app.layout.panel_height, 0);
    }

    #[test]
    fn test_menu_submenu_navigation() {
        let mut app = App::new(vec![]).expect("Failed to init App");

        // Open View menu (menu index 2: File=0, Edit=1, View=2, Help=3)
        app.open_menu(2);
        assert_eq!(app.focus, Focus::Menu);
        assert_eq!(app.active_menu, Some(2));
        assert_eq!(app.submenu_stack.len(), 0);

        let encoding_label = app.i18n.get("menu.view.encoding").to_string();
        let reopen_label = app.i18n.get("menu.view.reopen_with_encoding").to_string();

        // Find index of Encoding submenu item in View menu
        let encoding_idx = app.menus[2]
            .items
            .iter()
            .position(|item| matches!(item, MenuItem::Submenu { label, .. } if label == &encoding_label))
            .expect("Encoding submenu not found");

        app.selected_item = encoding_idx;

        // Press Right arrow on Encoding submenu -> Should open Encoding submenu
        app.handle_key(make_key(KeyCode::Right));
        assert_eq!(app.active_menu, Some(2));
        assert_eq!(app.submenu_stack.len(), 1);
        assert_eq!(app.submenu_stack[0], (2, encoding_idx));
        assert_eq!(app.selected_item, 0);

        // Current menu is now Encoding (items: Reopen with Encoding, Convert to Encoding)
        let cur_menu = app.get_current_active_menu();
        assert_eq!(cur_menu.label, encoding_label);

        // Press Right arrow on "Reopen with Encoding" submenu
        app.handle_key(make_key(KeyCode::Right));
        assert_eq!(app.submenu_stack.len(), 2);
        let cur_menu2 = app.get_current_active_menu();
        assert_eq!(cur_menu2.label, reopen_label);

        // Press Left arrow -> Should return to Encoding submenu and restore selected_item to 0
        app.handle_key(make_key(KeyCode::Left));
        assert_eq!(app.submenu_stack.len(), 1);
        assert_eq!(app.selected_item, 0);
        assert_eq!(app.get_current_active_menu().label, encoding_label);

        // Press Left arrow -> Should return to View menu and restore selected_item to encoding_idx
        app.handle_key(make_key(KeyCode::Left));
        assert_eq!(app.submenu_stack.len(), 0);
        assert_eq!(app.selected_item, encoding_idx);
        assert_eq!(app.get_current_active_menu().label, app.menus[2].label);

        // Press Left arrow again on top level menu -> Should move to Edit menu (index 1)
        app.handle_key(make_key(KeyCode::Left));
        assert_eq!(app.active_menu, Some(1));
    }

    #[test]
    fn test_mouse_drag_selection_workflow_tui() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.buffers[0].insert(0, "Hello World from Zee editor!\n");
        app.width = 80;
        app.height = 24;
        app.recompute_layout();

        let (ex, ey, _ew, _eh) = app.layout.editor_bounds();

        // 1. Mouse down at 'W' (column 6 in line 0)
        let mouse_down = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: ex + 6,
            row: ey,
            modifiers: KeyModifiers::empty(),
        };
        app.handle_mouse(mouse_down);
        assert_eq!(app.buffers[0].cursor, 6);
        assert_eq!(app.buffers[0].selection_anchor, Some(6));
        assert_eq!(app.buffers[0].selection, None);

        // 2. Mouse drag to 'd' (column 11)
        let mouse_drag1 = MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: ex + 11,
            row: ey,
            modifiers: KeyModifiers::empty(),
        };
        app.handle_mouse(mouse_drag1);
        assert_eq!(app.buffers[0].cursor, 11);
        assert_eq!(app.buffers[0].selection_anchor, Some(6));
        assert_eq!(app.buffers[0].selection, Some(6..11));

        // 3. Mouse drag back to anchor (column 6)
        let mouse_drag_back = MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: ex + 6,
            row: ey,
            modifiers: KeyModifiers::empty(),
        };
        app.handle_mouse(mouse_drag_back);
        assert_eq!(app.buffers[0].cursor, 6);
        assert_eq!(app.buffers[0].selection_anchor, Some(6));
        assert_eq!(app.buffers[0].selection, None);

        // 4. Mouse drag backward to 'H' (column 0)
        let mouse_drag_rev = MouseEvent {
            kind: MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: ex,
            row: ey,
            modifiers: KeyModifiers::empty(),
        };
        app.handle_mouse(mouse_drag_rev);
        assert_eq!(app.buffers[0].cursor, 0);
        assert_eq!(app.buffers[0].selection_anchor, Some(6));
        assert_eq!(app.buffers[0].selection, Some(0..6));

        // 5. Mouse up
        let mouse_up = MouseEvent {
            kind: MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column: ex,
            row: ey,
            modifiers: KeyModifiers::empty(),
        };
        app.handle_mouse(mouse_up);
        assert_eq!(app.buffers[0].selection, Some(0..6));
    }

    #[test]
    fn test_vi_mode_ime_normalization_normal_mode() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.vi_mode = true;
        app.buffers[0].vi_mode = zee_core::ViMode::Normal;
        app.buffers[0].insert(0, "line1\nline2\nline3\n");
        app.buffers[0].cursor = 0;

        // 1. Full-width 'ｊ' moves cursor down
        app.handle_key(make_key(KeyCode::Char('ｊ')));
        assert_eq!(app.buffers[0].char_to_line_col(app.buffers[0].cursor).0, 1);

        // 2. Full-width 'ｋ' moves cursor up
        app.handle_key(make_key(KeyCode::Char('ｋ')));
        assert_eq!(app.buffers[0].char_to_line_col(app.buffers[0].cursor).0, 0);

        // 3. Enter moves cursor down to next line
        app.handle_key(make_key(KeyCode::Enter));
        assert_eq!(app.buffers[0].char_to_line_col(app.buffers[0].cursor).0, 1);

        // 4. Kana 'い' enters insert mode
        app.handle_key(make_key(KeyCode::Char('い')));
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Insert);

        // Return to normal mode
        app.handle_key(make_key(KeyCode::Esc));
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Normal);

        // 5. Kana 'あ' moves cursor right and enters insert mode
        let cur_before = app.buffers[0].cursor;
        app.handle_key(make_key(KeyCode::Char('あ')));
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Insert);
        assert_eq!(app.buffers[0].cursor, cur_before + 1);

        // Return to normal mode
        app.handle_key(make_key(KeyCode::Esc));
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Normal);

        // 6. Japanese sokuon 'っ' deletes line (vi 'dd')
        let lines_before = app.buffers[0].line_count();
        app.handle_key(make_key(KeyCode::Char('っ')));
        assert_eq!(app.buffers[0].line_count(), lines_before - 1);

        // 7. Full-width colon '：' enters vi cmd mode
        app.handle_key(make_key(KeyCode::Char('：')));
        assert!(app.is_vi_cmd_mode);
        assert_eq!(app.vi_cmd, ":");

        // Type full-width 'ｗ' into cmd mode
        app.handle_key(make_key(KeyCode::Char('ｗ')));
        assert_eq!(app.vi_cmd, ":w");

        // Cancel cmd mode with Esc
        app.handle_key(make_key(KeyCode::Esc));
        assert!(!app.is_vi_cmd_mode);
    }

    #[test]
    fn test_app_vi_mode_defaults_off_at_startup() {
        let app = App::new(vec![]).expect("Failed to init App");
        assert!(!app.config.vi_mode, "Vi mode config should default to false");
        assert_eq!(app.buffers.len(), 1);
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Insert, "Initial buffer should be in Insert mode");
    }

    #[test]
    fn test_vi_mode_ex_commands() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.vi_mode = true;
        app.recompute_layout();
        app.buffers[0].vi_mode = zee_core::ViMode::Normal;
        app.buffers[0].insert(0, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n");

        // 1. Jump to line via :3
        app.handle_key(make_key(KeyCode::Char(':')));
        assert!(app.is_vi_cmd_mode);
        app.handle_key(make_key(KeyCode::Char('3')));
        app.handle_key(make_key(KeyCode::Enter));
        assert!(!app.is_vi_cmd_mode);
        let (line, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line, 2); // 0-based index for line 3

        // 2. Set number option via :set nu
        app.handle_key(make_key(KeyCode::Char(':')));
        for c in "set nu".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        app.handle_key(make_key(KeyCode::Enter));
        assert!(app.config.line_numbers);
        assert_eq!(app.vi_message, Some(("number enabled".into(), false)));

        // 3. Unknown command feedback
        app.handle_key(make_key(KeyCode::Char(':')));
        for c in "foo".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        app.handle_key(make_key(KeyCode::Enter));
        // 4. Substitute command via :%s/Line/Item/g
        app.handle_key(make_key(KeyCode::Char(':')));
        for c in "%s/Line/Item/g".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        app.handle_key(make_key(KeyCode::Enter));
        assert_eq!(app.vi_message, Some(("5 substitution(s) made".into(), false)));
        assert_eq!(app.buffers[0].rope.to_string(), "Item 1\nItem 2\nItem 3\nItem 4\nItem 5\n");

        // 5. Substitute with uppercase characters like %s/English/english/g
        let len = app.buffers[0].rope.len_chars();
        app.buffers[0].delete(0..len);
        app.buffers[0].insert(0, "English language and English text\n");
        app.handle_key(make_key(KeyCode::Char(':')));
        for c in "%s/English/english/g".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        app.handle_key(make_key(KeyCode::Enter));
        assert_eq!(app.vi_message, Some(("2 substitution(s) made".into(), false)));
        assert_eq!(app.buffers[0].rope.to_string(), "english language and english text\n");

        // 6. Substitute with Japanese characters :%s/日本語/Japanese/g
        let len = app.buffers[0].rope.len_chars();
        app.buffers[0].delete(0..len);
        app.buffers[0].insert(0, "日本語のテストと日本語のテキスト\n");
        app.handle_key(make_key(KeyCode::Char(':')));
        for c in "%s/日本語/Japanese/g".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        app.handle_key(make_key(KeyCode::Enter));
        assert_eq!(app.vi_message, Some(("2 substitution(s) made".into(), false)));
        assert_eq!(app.buffers[0].rope.to_string(), "JapaneseのテストとJapaneseのテキスト\n");

        // 7. Vi command mode backspace on Japanese multi-byte characters
        app.handle_key(make_key(KeyCode::Char(':')));
        for c in "%s/日本語".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        assert_eq!(app.vi_cmd, ":%s/日本語");
        app.handle_key(make_key(KeyCode::Backspace));
        assert_eq!(app.vi_cmd, ":%s/日本");
        app.handle_key(make_key(KeyCode::Backspace));
        assert_eq!(app.vi_cmd, ":%s/日");
        app.handle_key(make_key(KeyCode::Backspace));
        assert_eq!(app.vi_cmd, ":%s/");
        app.handle_key(make_key(KeyCode::Esc));
        assert!(!app.is_vi_cmd_mode);
        assert_eq!(app.vi_cmd, "");
    }

    #[test]
    fn test_vi_mode_count_motions() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.vi_mode = true;
        app.recompute_layout();
        app.buffers[0].vi_mode = zee_core::ViMode::Normal;
        app.buffers[0].insert(0, "aaa bbb ccc ddd eee\nfff ggg hhh\niii jjj kkk\n");
        app.buffers[0].cursor = 0;

        // 3w -> jump forward 3 words
        app.handle_key(make_key(KeyCode::Char('3')));
        app.handle_key(make_key(KeyCode::Char('w')));
        assert_eq!(app.buffers[0].cursor, 12); // starts of "ddd"

        // 2j -> move down 2 lines
        app.handle_key(make_key(KeyCode::Char('2')));
        app.handle_key(make_key(KeyCode::Char('j')));
        let (line, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line, 2);

        // 1G -> jump to first line
        app.handle_key(make_key(KeyCode::Char('1')));
        app.handle_key(make_key(KeyCode::Char('G')));
        let (line, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line, 0);

        // 2dd -> delete 2 lines
        app.handle_key(make_key(KeyCode::Char('2')));
        app.handle_key(make_key(KeyCode::Char('d')));
        app.handle_key(make_key(KeyCode::Char('d')));
        assert_eq!(app.buffers[0].rope.to_string(), "iii jjj kkk\n");
    }

    #[test]
    fn test_vi_mode_screen_and_sentence_motions() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.vi_mode = true;
        app.height = 24;
        app.recompute_layout();
        app.buffers[0].vi_mode = zee_core::ViMode::Normal;
        
        let text = (0..50).map(|i| format!("Line {}. Another sentence! Yet another?", i)).collect::<Vec<_>>().join("\n");
        app.buffers[0].insert(0, &text);
        app.buffers[0].cursor = 0;

        // Sentence motions: ')' moves to next sentence
        app.handle_key(make_key(KeyCode::Char(')')));
        assert!(app.buffers[0].cursor > 0);

        // Sentence motions: '(' moves back to first sentence
        app.handle_key(make_key(KeyCode::Char('(')));
        assert_eq!(app.buffers[0].cursor, 0);

        // H moves to top line of viewport
        app.buffers[0].scroll_row = 10;
        app.handle_key(make_key(KeyCode::Char('H')));
        let (line_h, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_h, 10);

        // M moves to middle of viewport
        let (_, _, _, eh) = app.layout.editor_bounds();
        app.handle_key(make_key(KeyCode::Char('M')));
        let (line_m, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_m, 10 + (eh as usize) / 2);

        // L moves to bottom of viewport
        app.handle_key(make_key(KeyCode::Char('L')));
        let (line_l, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_l, 10 + (eh as usize).saturating_sub(1));

        // Ctrl-Y scrolls up 1 line
        let old_scroll = app.buffers[0].scroll_row;
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        assert_eq!(app.buffers[0].scroll_row, old_scroll.saturating_sub(1));
    }

    #[test]
    fn test_standard_editor_typing_and_navigation() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.tab_size = 4;
        assert!(!app.config.vi_mode);
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Insert);

        // Type "Hello World"
        for c in "Hello World".chars() {
            app.handle_key(make_key(KeyCode::Char(c)));
        }
        assert_eq!(app.buffers[0].rope.to_string(), "Hello World");
        assert_eq!(app.buffers[0].cursor, 11);

        // Move Left 5 times (to space between Hello and World)
        for _ in 0..5 {
            app.handle_key(make_key(KeyCode::Left));
        }
        assert_eq!(app.buffers[0].cursor, 6);

        // Backspace to delete the space
        app.handle_key(make_key(KeyCode::Backspace));
        assert_eq!(app.buffers[0].rope.to_string(), "HelloWorld");
        assert_eq!(app.buffers[0].cursor, 5);

        // Delete key deletes character under cursor ('W')
        app.handle_key(make_key(KeyCode::Delete));
        assert_eq!(app.buffers[0].rope.to_string(), "Helloorld");

        // Enter key inserts newline
        app.handle_key(make_key(KeyCode::Enter));
        assert_eq!(app.buffers[0].rope.to_string(), "Hello\norld");
        assert_eq!(app.buffers[0].line_count(), 2);

        // Home key moves to start of line
        app.handle_key(make_key(KeyCode::Home));
        let (_, col) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(col, 0);

        // End key moves to end of line
        app.handle_key(make_key(KeyCode::End));
        let (_, col_end) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(col_end, 4);

        // Tab key inserts 4 spaces
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.buffers[0].rope.to_string(), "Hello\norld    ");

        // Add 30 lines to test PageUp and PageDown
        for i in 0..30 {
            let line = format!("\nLine {}", i);
            let len = app.buffers[0].rope.len_chars();
            app.buffers[0].insert(len, &line);
        }
        let last_line = app.buffers[0].line_count() - 1;
        let last_col = app.buffers[0].get_line_max_col(last_line);
        app.buffers[0].cursor = app.buffers[0].line_col_to_char(last_line, last_col);
        let (cur_line_before, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(cur_line_before, 31);

        // PageUp moves up 20 lines
        app.handle_key(make_key(KeyCode::PageUp));
        let (cur_line_up, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(cur_line_up, 11);

        // PageDown moves down 20 lines
        app.handle_key(make_key(KeyCode::PageDown));
        let (cur_line_down, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(cur_line_down, 31);
    }

    #[test]
    fn test_clipboard_and_selection_operations() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.buffers[0].insert(0, "The quick brown fox");

        // Select "quick " (chars 4..10)
        app.buffers[0].selection = Some(4..10);
        app.buffers[0].selection_anchor = Some(4);
        app.buffers[0].cursor = 10;

        // Cut operation removes selection
        app.perform_action(Action::Cut);
        assert_eq!(app.buffers[0].rope.to_string(), "The brown fox");
        assert_eq!(app.buffers[0].selection, None);

        // Select All (via Action::SelectAll)
        app.perform_action(Action::SelectAll);
        assert_eq!(app.buffers[0].selection, Some(0..app.buffers[0].rope.len_chars()));

        // Backspace replaces entire selection
        app.handle_key(make_key(KeyCode::Backspace));
        assert_eq!(app.buffers[0].rope.to_string(), "");
        assert_eq!(app.buffers[0].cursor, 0);
    }

    #[test]
    fn test_ui_menu_and_view_toggles() {
        let mut app = App::new(vec![]).expect("Failed to init App");

        // 1. Toggle Sidebar
        let initial_sidebar = app.sidebar.visible;
        app.perform_action(Action::ToggleSidebar);
        assert_eq!(app.sidebar.visible, !initial_sidebar);
        app.perform_action(Action::ToggleSidebar);
        assert_eq!(app.sidebar.visible, initial_sidebar);

        // 2. Toggle Line Numbers
        let initial_ln = app.config.line_numbers;
        app.perform_action(Action::ToggleLineNumbers);
        assert_eq!(app.config.line_numbers, !initial_ln);
        app.perform_action(Action::ToggleLineNumbers);
        assert_eq!(app.config.line_numbers, initial_ln);

        // 3. Toggle Word Wrap
        let initial_wrap = app.config.word_wrap;
        app.perform_action(Action::ToggleWordWrap);
        assert_eq!(app.config.word_wrap, !initial_wrap);
        app.perform_action(Action::ToggleWordWrap);
        assert_eq!(app.config.word_wrap, initial_wrap);

        // 4. Toggle Vi Mode
        assert!(!app.config.vi_mode);
        app.perform_action(Action::ToggleViMode);
        assert!(app.config.vi_mode);
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Normal);
        app.perform_action(Action::ToggleViMode);
        assert!(!app.config.vi_mode);
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Insert);
    }

    #[test]
    fn test_multi_buffer_tab_navigation() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        assert_eq!(app.buffers.len(), 1);

        // Add 2 more buffers
        app.perform_action(Action::New);
        app.perform_action(Action::New);
        assert_eq!(app.buffers.len(), 3);
        assert_eq!(app.active_buffer, 2);

        // Ctrl+Tab moves forward and wraps around
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));
        assert_eq!(app.active_buffer, 0);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));
        assert_eq!(app.active_buffer, 1);

        // Ctrl+PageUp moves backward
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
        assert_eq!(app.active_buffer, 0);
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
        assert_eq!(app.active_buffer, 2);

        // Close active buffer
        app.perform_action(Action::Close);
        assert_eq!(app.buffers.len(), 2);
    }

    #[test]
    fn test_settings_dialog_open_from_menu_and_keyboard_navigation() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.sidebar_position = "left".to_string();

        // Open File menu (index 0)
        app.open_menu(0);
        assert_eq!(app.focus, Focus::Menu);

        // Find Preferences menu item index
        let pref_idx = app.menus[0].items.iter().position(|item| match item {
            MenuItem::Action { action, .. } => *action == Action::OpenSettings,
            _ => false,
        }).expect("Preferences menu item not found in File menu");

        // Navigate to Preferences in menu
        app.selected_item = pref_idx;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Dialog must be open and focus MUST be Focus::Dialog
        assert!(app.current_dialog.is_some());
        assert_eq!(app.focus, Focus::Dialog);

        // Navigate down to row 2 (Sidebar Position)
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        // Toggle sidebar position using Right arrow
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

        // Save & Apply using 's' key
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

        // Dialog should be closed and setting applied
        assert!(app.current_dialog.is_none());
        assert_eq!(app.focus, Focus::Editor);
        assert_eq!(app.config.sidebar_position, "right");
        assert!(app.layout.is_right_sidebar);
    }

    #[test]
    fn test_settings_dialog_open_and_mouse_click_navigation() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.sidebar_position = "left".to_string();

        // Open settings via Ctrl+, shortcut
        app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::CONTROL));
        assert!(app.current_dialog.is_some());
        assert_eq!(app.focus, Focus::Dialog);

        let (dx, dy, dw, _dh) = {
            let d = app.current_dialog.as_ref().unwrap();
            app.layout.dialog_bounds(d.dimensions())
        };

        // Simulate click on row 2 (Sidebar Position: dy + 2 + 2 = dy + 4)
        // Click on right half to cycle to next value ("right")
        let click_x = dx + dw - 5;
        let click_y = dy + 4;
        let mouse_down = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: click_x,
            row: click_y,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(mouse_down);

        // Simulate click on Save & Apply button (row 10: dy + 2 + 10 = dy + 12)
        // Click left half of buttons row (Save button is on left half)
        let save_click_x = dx + 10;
        let save_click_y = dy + 12;
        let mouse_save = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: save_click_x,
            row: save_click_y,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(mouse_save);

        // Dialog should be closed and settings saved
        assert!(app.current_dialog.is_none());
        assert_eq!(app.config.sidebar_position, "right");
        assert!(app.layout.is_right_sidebar);
    }

    #[test]
    fn test_context_menu_no_refresh_item() {
        let i18n = zee_core::I18n::load("en");
        let path = std::path::PathBuf::from("/test/sample.rs");
        let dialog = crate::widgets::dialog::FileContextMenuDialog::new(path, false, false, &i18n);

        // Ensure "refresh" is NOT present in context menu options
        assert!(!dialog.options.iter().any(|(action, _)| *action == "refresh"));
    }

    #[test]
    fn test_settings_dialog_dropdown_language_selection_in_app() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.config.language = "auto".to_string();

        // Open settings via Ctrl+, shortcut
        app.handle_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::CONTROL));
        assert!(app.current_dialog.is_some());
        assert_eq!(app.focus, Focus::Dialog);

        // Move Down to Language row (row 1)
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        // Press Enter to open dropdown menu
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Move Down inside dropdown to select next language (e.g. "en")
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        // Press Enter to confirm and select from dropdown
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Press 's' to Save & Apply
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

        // Dialog should be closed and setting applied
        assert!(app.current_dialog.is_none());
        assert_eq!(app.focus, Focus::Editor);
        assert_eq!(app.config.language, "en");
    }

    #[test]
    fn test_sidebar_context_menu_new_file_and_open_workflow() {
        let temp_dir = std::env::temp_dir().join("zee_test_tui_sidebar_new_file");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut app = App::new(vec![]).expect("Failed to init App");
        app.sidebar.file_tree.set_root(&temp_dir);

        // 1. Simulate selecting "new_file" action from context menu on root directory
        let action = dialog::Action::FileContextMenuAction {
            action: "new_file",
            path: temp_dir.clone(),
            is_dir: true,
        };
        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Ok(action));

        // Input dialog must now be open
        assert!(app.current_dialog.is_some());
        assert_eq!(app.focus, Focus::Dialog);

        // 2. Type "sample.txt" in InputDialog
        for c in "sample.txt".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }

        // 3. Confirm with Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Dialog should be closed, and file should be created on disk and opened in buffer
        assert!(app.current_dialog.is_none());
        let expected_file = temp_dir.join("sample.txt");
        assert!(expected_file.exists());
        assert_eq!(app.buffers[app.active_buffer].path.as_deref(), Some(expected_file.as_path()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sidebar_context_menu_rename_and_delete_workflow() {
        let temp_dir = std::env::temp_dir().join("zee_test_tui_sidebar_rename_delete");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let initial_file = temp_dir.join("origin.txt");
        std::fs::write(&initial_file, "hello world").unwrap();

        let mut app = App::new(vec![initial_file.clone()]).expect("Failed to init App");
        app.sidebar.file_tree.set_root(&temp_dir);
        assert_eq!(app.buffers[app.active_buffer].path.as_deref(), Some(initial_file.as_path()));

        // 1. Rename file: trigger "rename" from context menu
        let action = dialog::Action::FileContextMenuAction {
            action: "rename",
            path: initial_file.clone(),
            is_dir: false,
        };
        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Ok(action));
        assert!(app.current_dialog.is_some());

        // Backspace to clear "origin.txt" and type "renamed.txt"
        for _ in 0.."origin.txt".len() {
            app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }
        for c in "renamed.txt".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // File on disk must be renamed and buffer path updated
        let renamed_file = temp_dir.join("renamed.txt");
        assert!(!initial_file.exists());
        assert!(renamed_file.exists());
        assert_eq!(app.buffers[app.active_buffer].path.as_deref(), Some(renamed_file.as_path()));

        // 2. Delete file: trigger "delete" from context menu
        let action_delete = dialog::Action::FileContextMenuAction {
            action: "delete",
            path: renamed_file.clone(),
            is_dir: false,
        };
        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Ok(action_delete));
        assert!(app.current_dialog.is_some());

        // Confirm deletion with Enter (Yes button)
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // File should be deleted on disk and removed from buffers
        assert!(!renamed_file.exists());
        assert!(app.current_dialog.is_none());
        assert_ne!(app.buffers[app.active_buffer].path.as_deref(), Some(renamed_file.as_path()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_menu_and_submenu_mouse_click_interaction() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.width = 80;
        app.height = 24;
        app.recompute_layout();

        // 1. Click on "View" menu in menu bar (index 2)
        let view_start = app.layout.menu_bar_items[2].1;
        let menu_bar_click = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: view_start,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(menu_bar_click);

        assert_eq!(app.focus, Focus::Menu);
        assert_eq!(app.active_menu, Some(2));
        assert_eq!(app.submenu_stack.len(), 0);

        // Render to populate dropdown_rects
        let mut sink = std::io::sink();
        let _ = app.renderer.present(&mut sink);
        app.dropdown_rects.clear();
        let menu = app.menus[2].clone();
        app.render_dropdown(view_start, 1, &menu, 0);

        // Find Encoding submenu item in View menu
        let encoding_label = app.i18n.get("menu.view.encoding").to_string();
        let (enc_rx, enc_ry, enc_rw, _enc_rh, enc_depth, enc_idx) = app.dropdown_rects
            .iter()
            .find(|(_rx, _ry, _rw, _rh, depth, idx)| {
                *depth == 0 && matches!(&menu.items[*idx], MenuItem::Submenu { label, .. } if label == &encoding_label)
            })
            .copied()
            .expect("Encoding submenu item not found in dropdown_rects");

        assert_eq!(enc_depth, 0);

        // 2. Click on the Encoding submenu item using mouse
        let click_encoding = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: enc_rx + enc_rw / 2,
            row: enc_ry,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(click_encoding);

        // Submenu should be open!
        assert_eq!(app.focus, Focus::Menu);
        assert_eq!(app.active_menu, Some(2));
        assert_eq!(app.submenu_stack.len(), 1);
        assert_eq!(app.submenu_stack[0], (2, enc_idx));

        // Re-render dropdowns including opened submenu
        app.dropdown_rects.clear();
        app.render_dropdown(view_start, 1, &menu, 0);

        // Both parent menu items (depth 0) and submenu items (depth 1) must be in dropdown_rects!
        let has_depth_0 = app.dropdown_rects.iter().any(|(_, _, _, _, depth, _)| *depth == 0);
        let has_depth_1 = app.dropdown_rects.iter().any(|(_, _, _, _, depth, _)| *depth == 1);
        assert!(has_depth_0, "Parent menu rects should still be registered in dropdown_rects");
        assert!(has_depth_1, "Submenu rects should be registered in dropdown_rects");

        // 3. Click on a submenu item: "Reopen with Encoding"
        let reopen_label = app.i18n.get("menu.view.reopen_with_encoding").to_string();
        let sub_menu = app.get_current_active_menu().clone();
        let (reopen_rx, reopen_ry, reopen_rw, _reopen_rh, reopen_depth, reopen_idx) = app.dropdown_rects
            .iter()
            .find(|(_rx, _ry, _rw, _rh, depth, idx)| {
                *depth == 1 && matches!(&sub_menu.items[*idx], MenuItem::Submenu { label, .. } if label == &reopen_label)
            })
            .copied()
            .expect("Reopen with Encoding submenu not found in dropdown_rects");

        assert_eq!(reopen_depth, 1);

        let click_reopen = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: reopen_rx + reopen_rw / 2,
            row: reopen_ry,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(click_reopen);

        // Should now be at submenu depth 2
        assert_eq!(app.submenu_stack.len(), 2);
        assert_eq!(app.submenu_stack[1], (2, reopen_idx));

        // 4. Click back on the parent menu item at depth 0 (e.g. Word Wrap toggle)
        app.dropdown_rects.clear();
        app.render_dropdown(view_start, 1, &menu, 0);

        let word_wrap_label = app.i18n.get("menu.view.word_wrap").to_string();
        let (ww_rx, ww_ry, ww_rw, _ww_rh, _ww_depth, _ww_idx) = app.dropdown_rects
            .iter()
            .find(|(_rx, _ry, _rw, _rh, depth, idx)| {
                *depth == 0 && matches!(&menu.items[*idx], MenuItem::Toggle { label, .. } if label == &word_wrap_label)
            })
            .copied()
            .expect("Word Wrap toggle not found in dropdown_rects");

        let initial_wrap = app.config.word_wrap;
        let click_word_wrap = MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: ww_rx + ww_rw / 2,
            row: ww_ry,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(click_word_wrap);

        // Clicking an action/toggle in parent menu should execute it, close the menu, and clear submenu_stack!
        assert_eq!(app.active_menu, None);
        assert_eq!(app.focus, Focus::Editor);
        assert_eq!(app.submenu_stack.len(), 0);
        assert_eq!(app.config.word_wrap, !initial_wrap);
    }

    #[test]
    fn test_submenu_edge_screen_bounds_clamping() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        // Narrow terminal to test right-edge clamping
        app.width = 50;
        app.height = 20;
        app.recompute_layout();

        // Open View menu
        app.open_menu(2);
        let encoding_label = app.i18n.get("menu.view.encoding").to_string();
        let encoding_idx = app.menus[2]
            .items
            .iter()
            .position(|item| matches!(item, MenuItem::Submenu { label, .. } if label == &encoding_label))
            .expect("Encoding submenu not found");

        app.selected_item = encoding_idx;
        app.submenu_stack.push((2, encoding_idx));

        // Render dropdown near the right edge
        app.dropdown_rects.clear();
        let menu = app.menus[2].clone();
        app.render_dropdown(35, 1, &menu, 0);

        // Every registered dropdown rectangle must be completely within [0, app.width) horizontally
        // and [0, app.height) vertically!
        for (rx, ry, rw, rh, depth, idx) in &app.dropdown_rects {
            assert!(
                *rx + *rw <= app.width,
                "Rect for depth {} item {} exceeds screen width: rx={}, rw={}, width={}",
                depth, idx, rx, rw, app.width
            );
            assert!(
                *ry + *rh <= app.height,
                "Rect for depth {} item {} exceeds screen height: ry={}, rh={}, height={}",
                depth, idx, ry, rh, app.height
            );
        }
    }

    #[test]
    fn test_sidebar_context_menu_toggle_hidden_workflow() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        let initial_hidden = app.config.show_hidden;

        // Trigger toggle_hidden from context menu action
        let action = dialog::Action::FileContextMenuAction {
            action: "toggle_hidden",
            path: std::path::PathBuf::from("/dummy"),
            is_dir: false,
        };
        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Ok(action));

        assert_eq!(app.config.show_hidden, !initial_hidden);
        assert_eq!(app.sidebar.file_tree.show_hidden, !initial_hidden);
    }

    #[test]
    fn test_tui_tab_key_inserts_spaces_matching_tab_size() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.focus = Focus::Editor;
        app.config.tab_size = 2;

        // Press Tab in standard editor
        app.handle_key(make_key(KeyCode::Tab));
        let buffer = app.buffers.get(app.active_buffer).unwrap();
        assert_eq!(buffer.rope.to_string(), "  ");

        // Change tab_size to 4 and press Tab again
        app.config.tab_size = 4;
        app.handle_key(make_key(KeyCode::Tab));
        let buffer = app.buffers.get(app.active_buffer).unwrap();
        assert_eq!(buffer.rope.to_string(), "      ");
    }

    #[test]
    fn test_tui_ctrl_e_toggles_vi_mode() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.focus = Focus::Editor;
        assert!(!app.config.vi_mode);
        assert_eq!(app.buffers[app.active_buffer].vi_mode, zee_core::ViMode::Insert);

        // Press Ctrl+E to turn Vi mode ON
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert!(app.config.vi_mode);
        assert_eq!(app.buffers[app.active_buffer].vi_mode, zee_core::ViMode::Normal);

        // Press Ctrl+E again to turn Vi mode OFF
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert!(!app.config.vi_mode);
        assert_eq!(app.buffers[app.active_buffer].vi_mode, zee_core::ViMode::Insert);

        // Also test F4 toggle
        app.handle_key(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
        assert!(app.config.vi_mode);
        assert_eq!(app.buffers[app.active_buffer].vi_mode, zee_core::ViMode::Normal);

        app.handle_key(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
        assert!(!app.config.vi_mode);
        assert_eq!(app.buffers[app.active_buffer].vi_mode, zee_core::ViMode::Insert);
    }

    #[test]
    fn test_tui_page_up_down_home_end_navigation() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.focus = Focus::Editor;

        // Populate multi-line content
        for i in 0..50 {
            let line = format!("Line {:02} test content\n", i);
            let len = app.buffers[0].rope.len_chars();
            app.buffers[0].insert(len, &line);
        }

        // 1. Standard mode: Home and End
        app.buffers[0].cursor = 5; // Middle of Line 0
        app.handle_key(make_key(KeyCode::Home));
        assert_eq!(app.buffers[0].cursor, 0);

        app.handle_key(make_key(KeyCode::End));
        let (_, col_end) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(col_end, 20); // Length of "Line 00 test content"

        // Standard mode: PageDown and PageUp
        app.handle_key(make_key(KeyCode::PageDown));
        let (line_pd, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_pd, 20);

        app.handle_key(make_key(KeyCode::PageUp));
        let (line_pu, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_pu, 0);

        // 2. Vi Normal Mode: Home, End, PageDown, PageUp
        app.perform_action(Action::ToggleViMode);
        assert!(app.config.vi_mode);
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Normal);

        app.buffers[0].cursor = 8;
        app.handle_key(make_key(KeyCode::Home));
        assert_eq!(app.buffers[0].cursor, 0);

        app.handle_key(make_key(KeyCode::End));
        let (_, col_vi_end) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(col_vi_end, 20);

        app.handle_key(make_key(KeyCode::PageDown));
        let (line_vi_pd, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_vi_pd, 20);

        app.handle_key(make_key(KeyCode::PageUp));
        let (line_vi_pu, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_vi_pu, 0);

        // 3. Vi Visual Mode: Home, End, PageDown, PageUp with selection expansion
        app.handle_key(make_key(KeyCode::Char('v')));
        assert_eq!(app.buffers[0].vi_mode, zee_core::ViMode::Visual);

        app.handle_key(make_key(KeyCode::End));
        assert!(app.buffers[0].selection.is_some());

        app.handle_key(make_key(KeyCode::PageDown));
        let (line_v_pd, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_v_pd, 20);
        assert!(app.buffers[0].selection.is_some());

        app.handle_key(make_key(KeyCode::PageUp));
        let (line_v_pu, _) = app.buffers[0].char_to_line_col(app.buffers[0].cursor);
        assert_eq!(line_v_pu, 0);
        assert!(app.buffers[0].selection.is_some());
    }

    #[test]
    fn test_open_folder_action_workflow() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        let temp_dir = std::env::temp_dir().join(format!("zee_test_open_folder_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        // 1. Trigger OpenFolder action
        app.perform_action(Action::OpenFolder);
        assert_eq!(app.focus, Focus::Dialog);
        assert_eq!(app.pending_op, PendingOp::OpenFolder);
        assert!(app.current_dialog.is_some());

        // 2. Confirm path from dialog
        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Ok(
            crate::widgets::dialog::Action::ConfirmPath(temp_dir.clone())
        ));

        assert!(app.sidebar.visible);
        assert_eq!(app.sidebar.active_tab, crate::widgets::sidebar::SidebarTab::Files);
        assert_eq!(app.sidebar.file_tree.root_path, temp_dir);
        assert_eq!(app.focus, Focus::Sidebar);
        assert_eq!(app.pending_op, PendingOp::None);
        assert!(app.current_dialog.is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_toggle_files_and_refresh_file_tree_action() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        app.sidebar.visible = false;
        app.sidebar.active_tab = crate::widgets::sidebar::SidebarTab::Outline;

        app.perform_action(Action::ToggleFiles);
        assert!(app.sidebar.visible);
        assert_eq!(app.sidebar.active_tab, crate::widgets::sidebar::SidebarTab::Files);
        assert_eq!(app.focus, Focus::Sidebar);

        // Refresh file tree should run cleanly
        app.perform_action(Action::RefreshFileTree);
    }

    #[test]
    fn test_plugin_manager_action_and_plugins_menu() {
        let mut app = App::new(vec![]).expect("Failed to init App");
        
        // Check that Plugins menu exists in top-level menus
        let plugins_menu_label = app.i18n.get("menu.plugins").to_string();
        let has_plugins_menu = app.menus.iter().any(|m| m.label == plugins_menu_label);
        assert!(has_plugins_menu, "App menus must contain Plugins menu");

        // Trigger ManagePlugins action
        app.perform_action(Action::ManagePlugins);
        assert_eq!(app.focus, Focus::Dialog);
        assert!(app.current_dialog.is_some());

        // Close dialog
        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Cancel);
        assert_eq!(app.focus, Focus::Editor);
        assert!(app.current_dialog.is_none());
    }

    #[test]
    fn test_export_and_import_config_actions() {
        let mut app = App::new(vec![]).expect("Failed to init App");

        // 1. Export Config
        app.perform_action(Action::ExportConfig);
        assert_eq!(app.focus, Focus::Dialog);
        assert_eq!(app.pending_op, PendingOp::ExportConfig);
        assert!(app.current_dialog.is_some());

        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Cancel);
        assert_eq!(app.focus, Focus::Editor);

        // 2. Export All
        app.perform_action(Action::ExportAll);
        assert_eq!(app.focus, Focus::Dialog);
        assert_eq!(app.pending_op, PendingOp::ExportAll);
        assert!(app.current_dialog.is_some());

        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Cancel);
        assert_eq!(app.focus, Focus::Editor);

        // 3. Import Config
        app.perform_action(Action::ImportConfig);
        assert_eq!(app.focus, Focus::Dialog);
        assert_eq!(app.pending_op, PendingOp::ImportConfig);
        assert!(app.current_dialog.is_some());

        app.handle_dialog_result(crate::widgets::dialog::DialogResult::Cancel);
        assert_eq!(app.focus, Focus::Editor);
    }
}



