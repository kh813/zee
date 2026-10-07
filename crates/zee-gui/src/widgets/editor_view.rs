use gpui::*;
use gpui::prelude::FluentBuilder;
use crate::workspace::Workspace;
use zee_core::syntax::TokenType;
use zee_core::theme::Theme;
use crate::widgets::{led_color_to_gpui, mono_font_family, with_alpha};

const TEXT_AREA_LEFT_PADDING: Pixels = px(6.0);

#[derive(Clone, Copy, Debug)]
struct ScrollbarDrag {
    thumb_grab_offset: Pixels,
}

pub struct EditorView {
    pub workspace: Entity<Workspace>,
    pub focus_handle: FocusHandle,
    last_click_at: Option<std::time::Instant>,
    click_count: usize,
    preedit_text: Option<String>,
    preedit_range: Option<std::ops::Range<usize>>,
    pending_d: bool,
    pending_y: bool,
    pending_c: bool,
    pending_g: bool,
    pending_r: bool,
    pending_f: bool,
    pending_capital_f: bool,
    pending_t: bool,
    pending_capital_t: bool,
    pending_indent: bool,
    pending_unindent: bool,
    pending_m: bool,
    pending_single_quote: bool,
    pending_backtick: bool,
    ignore_next_text_input: bool,
    pub last_wrap_cols: usize,
    pub last_wrap_width_px: f32,
    pub ascii_width_px: f32,
    pub cjk_width_px: f32,
    pub is_mouse_down: bool,
    scrollbar_drag: Option<ScrollbarDrag>,
    pub last_bounds: Option<Bounds<Pixels>>,
    count: usize,
    pending_op_count: usize,
}

impl EditorView {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| {
            cx.notify();
        }).detach();

        let initial_ime = zee_core::is_cjk_ime_active();
        let editor_view_handle = cx.entity().clone();
        cx.spawn(move |_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let mut prev = initial_ime;
                loop {
                    smol::Timer::after(std::time::Duration::from_millis(50)).await;
                    let current = zee_core::is_cjk_ime_active();
                    if current != prev {
                        prev = current;
                        let _ = editor_view_handle.update(&mut cx.clone(), |_this, cx| {
                            cx.notify();
                        });
                    }
                }
            }
        }).detach();
        
        let config = workspace.read(cx).config.clone();
        let font_size = config.font_size;
        let ascii_width = if cfg!(target_os = "macos") { font_size * 0.602 } else { font_size * 0.6 };
        let cjk_width = font_size * 1.0;

        Self {
            workspace,
            focus_handle: cx.focus_handle(),
            last_click_at: None,
            click_count: 0,
            preedit_text: None,
            preedit_range: None,
            pending_d: false,
            pending_y: false,
            pending_c: false,
            pending_g: false,
            pending_r: false,
            pending_f: false,
            pending_capital_f: false,
            pending_t: false,
            pending_capital_t: false,
            pending_indent: false,
            pending_unindent: false,
            pending_m: false,
            pending_single_quote: false,
            pending_backtick: false,
            ignore_next_text_input: false,
            last_wrap_cols: 80,
            last_wrap_width_px: 800.0,
            ascii_width_px: ascii_width,
            cjk_width_px: cjk_width,
            is_mouse_down: false,
            scrollbar_drag: None,
            last_bounds: None,
            count: 0,
            pending_op_count: 0,
        }
    }

    fn token_color(&self, token_type: TokenType, theme: &Theme) -> Rgba {
        let color = match token_type {
            TokenType::Keyword => theme.syntax.keyword,
            TokenType::TypeName => theme.syntax.type_name,
            TokenType::Function => theme.syntax.function,
            TokenType::String => theme.syntax.string,
            TokenType::Number => theme.syntax.number,
            TokenType::Comment => theme.syntax.comment,
            TokenType::Operator => theme.syntax.operator,
            TokenType::Punctuation => theme.syntax.punctuation,
            TokenType::Constant => theme.syntax.constant,
            TokenType::Attribute => theme.syntax.attribute,
            TokenType::Error => theme.syntax.error,
        };
        led_color_to_gpui(color.unwrap_or(theme.editor.foreground))
    }

    fn execute_ex_command(cmd_raw: &str, workspace: &Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) {
        let ex_cmd = zee_core::parse_ex_command(cmd_raw);
        match ex_cmd {
            zee_core::ExCommand::Write { path, force: _ } => {
                workspace.update(cx, |w, cx| {
                    let res = if let Some(ref p) = path {
                        w.save_as_active_editor(p)
                    } else {
                        if let Some(ed) = w.active_editor() {
                            if ed.path.is_none() {
                                w.vi_message = Some(("E32: No file name".into(), true));
                                cx.notify();
                                return;
                            }
                        }
                        w.save_active_editor()
                    };
                    match res {
                        Ok(()) => {
                            let name = path.as_deref().unwrap_or_else(|| {
                                w.active_editor()
                                    .and_then(|e| e.path.as_ref())
                                    .and_then(|p| p.file_name())
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("file")
                            });
                            w.vi_message = Some((format!("\"{}\" written", name), false));
                        }
                        Err(e) => {
                            w.vi_message = Some((format!("E212: Can't open file for writing: {}", e), true));
                        }
                    }
                    cx.notify();
                });
            }
            zee_core::ExCommand::Quit { force } => {
                let is_modified = workspace.read(cx).active_editor().map(|e| e.is_modified()).unwrap_or(false);
                if is_modified && !force {
                    workspace.update(cx, |w, cx| {
                        w.vi_message = Some(("E37: No write since last change (add ! to override)".into(), true));
                        cx.notify();
                    });
                } else {
                    window.dispatch_action(Box::new(crate::app::CloseTab {}), cx);
                }
            }
            zee_core::ExCommand::WriteQuit { path, force: _ } => {
                let mut write_ok = false;
                workspace.update(cx, |w, cx| {
                    let res = if let Some(ref p) = path {
                        w.save_as_active_editor(p)
                    } else {
                        if let Some(ed) = w.active_editor() {
                            if ed.path.is_none() {
                                w.vi_message = Some(("E32: No file name".into(), true));
                                cx.notify();
                                return;
                            }
                        }
                        w.save_active_editor()
                    };
                    match res {
                        Ok(()) => {
                            write_ok = true;
                        }
                        Err(e) => {
                            w.vi_message = Some((format!("E212: Can't open file for writing: {}", e), true));
                        }
                    }
                    cx.notify();
                });
                if write_ok {
                    window.dispatch_action(Box::new(crate::app::CloseTab {}), cx);
                }
            }
            zee_core::ExCommand::QuitAll { force } => {
                let any_modified = workspace.read(cx).has_modified_buffers();
                if any_modified && !force {
                    workspace.update(cx, |w, cx| {
                        w.vi_message = Some(("E37: No write since last change (add ! to override)".into(), true));
                        cx.notify();
                    });
                } else {
                    window.dispatch_action(Box::new(crate::app::Quit {}), cx);
                }
            }
            zee_core::ExCommand::WriteQuitAll { force: _ } => {
                let mut all_ok = true;
                workspace.update(cx, |w, cx| {
                    for ed in &mut w.editors {
                        if ed.is_modified() {
                            let trim = w.config.trim_trailing_whitespace;
                            let ensure_nl = w.config.ensure_final_newline;
                            ed.cleanup_on_save(trim, ensure_nl);
                            if let Err(e) = ed.save() {
                                w.vi_message = Some((format!("Error saving: {}", e), true));
                                all_ok = false;
                                break;
                            }
                        }
                    }
                    cx.notify();
                });
                if all_ok {
                    window.dispatch_action(Box::new(crate::app::Quit {}), cx);
                }
            }
            zee_core::ExCommand::Edit { path, force } => {
                if let Some(p) = path {
                    match zee_core::buffer::Editor::from_file(&p) {
                        Ok(ed) => {
                            workspace.update(cx, |w, cx| {
                                w.add_editor(ed);
                                w.vi_message = Some((format!("\"{}\" opened", p), false));
                                cx.notify();
                            });
                        }
                        Err(e) => {
                            workspace.update(cx, |w, cx| {
                                w.vi_message = Some((format!("E484: Can't open file: {}", e), true));
                                cx.notify();
                            });
                        }
                    }
                } else if force {
                    workspace.update(cx, |w, cx| {
                        if let Some(ed) = w.active_editor_mut() {
                            match ed.reload_from_disk() {
                                Ok(()) => {
                                    w.vi_message = Some(("Reloaded from disk".into(), false));
                                }
                                Err(e) => {
                                    w.vi_message = Some((format!("Failed to reload: {}", e), true));
                                }
                            }
                        }
                        cx.notify();
                    });
                }
            }
            zee_core::ExCommand::GoToLine(line_num) => {
                workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    if let Some(editor) = w.active_editor_mut() {
                        let max_line = editor.line_count().saturating_sub(1);
                        let target_line = line_num.saturating_sub(1).min(max_line);
                        editor.cursor = editor.line_col_to_char(target_line, 0);
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.ensure_cursor_visible(30, 80, word_wrap);
                    }
                    w.vi_message = None;
                    cx.notify();
                });
            }
            zee_core::ExCommand::NoHighlight => {
                workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.find_results.clear();
                        editor.current_match_idx = None;
                        editor.search_status = None;
                    }
                    w.vi_message = None;
                    cx.notify();
                });
            }
            zee_core::ExCommand::BufferNext => {
                workspace.update(cx, |w, cx| {
                    w.next_tab();
                    w.vi_message = None;
                    cx.notify();
                });
            }
            zee_core::ExCommand::BufferPrev => {
                workspace.update(cx, |w, cx| {
                    w.prev_tab();
                    w.vi_message = None;
                    cx.notify();
                });
            }
            zee_core::ExCommand::Set { option, value: _ } => {
                workspace.update(cx, |w, cx| {
                    match option.as_str() {
                        "nu" | "number" => {
                            w.config.line_numbers = true;
                            w.vi_message = Some(("number enabled".into(), false));
                        }
                        "nonu" | "nonumber" => {
                            w.config.line_numbers = false;
                            w.vi_message = Some(("number disabled".into(), false));
                        }
                        "wrap" => {
                            w.config.word_wrap = true;
                            w.vi_message = Some(("wrap enabled".into(), false));
                        }
                        "nowrap" => {
                            w.config.word_wrap = false;
                            w.vi_message = Some(("wrap disabled".into(), false));
                        }
                        _ => {
                            w.vi_message = Some((format!("Unknown option: {}", option), true));
                        }
                    }
                    cx.notify();
                });
            }
            zee_core::ExCommand::Substitute { range, pattern, replacement, global, ignore_case } => {
                workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (start_l, end_l) = match range {
                            zee_core::ExRange::CurrentLine => {
                                let (cur_l, _) = editor.char_to_line_col(editor.cursor);
                                (cur_l + 1, cur_l + 1)
                            }
                            zee_core::ExRange::EntireBuffer => {
                                (1, editor.line_count())
                            }
                            zee_core::ExRange::LineRange(s, e) => (s, e),
                        };
                        match editor.substitute_range(start_l, end_l, &pattern, &replacement, global, ignore_case) {
                            Ok(count) => {
                                w.vi_message = Some((format!("{} substitution(s) made", count), false));
                            }
                            Err(e) => {
                                w.vi_message = Some((format!("E486: {}", e), true));
                            }
                        }
                    }
                    cx.notify();
                });
            }
            zee_core::ExCommand::Plugin { subcmd, arg } => {
                workspace.update(cx, |w, cx| {
                    match subcmd.as_str() {
                        "list" => {
                            let mut names = Vec::new();
                            for p in &w.plugin_manager.plugins {
                                names.push(format!("{} (wasm)", p.manifest.name));
                            }
                            for p in &w.plugin_manager.lua_plugins {
                                names.push(format!("{} (lua)", p.manifest.name));
                            }
                            if names.is_empty() {
                                w.vi_message = Some(("No plugins installed".to_string(), false));
                            } else {
                                w.vi_message = Some((format!("Installed plugins: {}", names.join(", ")), false));
                            }
                        }
                        "install" => {
                            if let Some(id) = arg {
                                match zee_core::plugin::PluginManager::install_from_registry(&id, None) {
                                    Ok(_) => {
                                        w.plugin_manager.load_installed_plugins();
                                        w.vi_message = Some((format!("Plugin '{}' installed successfully", id), false));
                                    }
                                    Err(e) => {
                                        w.vi_message = Some((format!("Failed to install '{}': {}", id, e), true));
                                    }
                                }
                            } else {
                                w.vi_message = Some(("Usage: :plugin install <id>".to_string(), true));
                            }
                        }
                        "uninstall" => {
                            if let Some(id) = arg {
                                match zee_core::plugin::PluginManager::uninstall_plugin_by_id(&id) {
                                    Ok(true) => {
                                        w.plugin_manager.load_installed_plugins();
                                        w.vi_message = Some((format!("Plugin '{}' uninstalled successfully", id), false));
                                    }
                                    Ok(false) => {
                                        w.vi_message = Some((format!("Plugin '{}' not found", id), true));
                                    }
                                    Err(e) => {
                                        w.vi_message = Some((format!("Failed to uninstall '{}': {}", id, e), true));
                                    }
                                }
                            } else {
                                w.vi_message = Some(("Usage: :plugin uninstall <id>".to_string(), true));
                            }
                        }
                        _ => {
                            w.vi_message = Some((format!("Unknown plugin command: {}. Available: list, install <id>, uninstall <id>", subcmd), true));
                        }
                    }
                    cx.notify();
                });
            }
            zee_core::ExCommand::Empty => {
                workspace.update(cx, |w, cx| {
                    w.vi_message = None;
                    cx.notify();
                });
            }
            zee_core::ExCommand::Unknown(cmd) => {
                workspace.update(cx, |w, cx| {
                    w.vi_message = Some((format!("E492: Not an editor command: :{}", cmd), true));
                    cx.notify();
                });
            }
        }
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let raw_key = &event.keystroke.key;
        let shift = event.keystroke.modifiers.shift;
        let control = event.keystroke.modifiers.control;
        let cmd = event.keystroke.modifiers.platform;
        let _alt = event.keystroke.modifiers.alt;

        self.ignore_next_text_input = false;

        let vi_mode_enabled = self.workspace.read(cx).config.vi_mode;
        let current_vi_mode = self.workspace.read(cx).active_editor().map(|e| e.vi_mode);

        let in_vi_cmd = vi_mode_enabled && self.workspace.read(cx).vi_cmd.is_some();
        let effective_vi_mode = if vi_mode_enabled { current_vi_mode } else { None };
        let normalized_key = zee_core::resolve_key_stroke(raw_key, shift, effective_vi_mode, in_vi_cmd);
        let key = &normalized_key;

        // In vi mode (Normal or Visual), detect colon to enter command line
        let is_colon_trigger = key == ":" || (raw_key == ";" && shift) || raw_key == ":" || key == "：";
        if vi_mode_enabled && current_vi_mode != Some(zee_core::ViMode::Insert) && self.workspace.read(cx).vi_cmd.is_none() && is_colon_trigger && !control && !cmd {
            self.ignore_next_text_input = true;
            self.workspace.update(cx, |w, cx| {
                w.vi_cmd = Some(":".into());
                w.vi_message = None;
                cx.notify();
            });
            return;
        }

        if vi_mode_enabled {
            if self.workspace.read(cx).vi_cmd.is_some() {
                match key.as_str() {
                    "escape" => {
                        self.preedit_text = None;
                        self.preedit_range = None;
                        self.workspace.update(cx, |w, cx| {
                            w.vi_cmd = None;
                            w.vi_cmd_preedit = None;
                            cx.notify();
                        });
                        return;
                    }
                    "enter" => {
                        self.preedit_text = None;
                        self.preedit_range = None;
                        let cmd_opt = self.workspace.read(cx).vi_cmd.clone();
                        self.workspace.update(cx, |w, cx| {
                            w.vi_cmd = None;
                            w.vi_cmd_preedit = None;
                            cx.notify();
                        });
                        if let Some(cmd_raw) = cmd_opt {
                            Self::execute_ex_command(&cmd_raw, &self.workspace, window, cx);
                        }
                        return;
                    }
                    "backspace" => {
                        self.preedit_text = None;
                        self.preedit_range = None;
                        self.workspace.update(cx, |w, cx| {
                            w.vi_cmd_preedit = None;
                            if let Some(cmd) = &mut w.vi_cmd {
                                if cmd.chars().count() > 1 {
                                    cmd.pop();
                                } else {
                                    w.vi_cmd = None;
                                }
                            }
                            cx.notify();
                        });
                        return;
                    }
                    _ => {
                        if !control && !cmd {
                            let char_to_add = if key.as_str() == "space" {
                                " ".to_string()
                            } else if key.chars().count() == 1 {
                                let norm = zee_core::normalize_vi_char(key.chars().next().unwrap());
                                if shift && norm.is_ascii_lowercase() {
                                    norm.to_ascii_uppercase().to_string()
                                } else {
                                    norm.to_string()
                                }
                            } else {
                                "".to_string()
                            };
                            if !char_to_add.is_empty() {
                                self.ignore_next_text_input = true;
                                self.workspace.update(cx, |w, cx| {
                                    w.vi_cmd_preedit = None;
                                    if let Some(cmd) = &mut w.vi_cmd {
                                        cmd.push_str(&char_to_add);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                        }
                        return;
                    }
                }
            }


            // Handle Ctrl+V / Cmd+V in Normal/Visual mode for Visual Block
            if (control || cmd) && key.as_str() == "v" {
                if let Some(vi_mode) = current_vi_mode {
                    if vi_mode != zee_core::ViMode::Insert {
                        self.workspace.update(cx, |w, cx| {
                            if let Some(editor) = w.active_editor_mut() {
                                if editor.vi_mode == zee_core::ViMode::VisualBlock {
                                    editor.vi_mode = zee_core::ViMode::Normal;
                                    editor.selection = None;
                                    editor.selection_anchor = None;
                                } else {
                                    editor.vi_mode = zee_core::ViMode::VisualBlock;
                                    editor.ensure_selection();
                                }
                            }
                            cx.notify();
                        });
                        return;
                    }
                }
            }

            // Vi mode Ctrl shortcuts: Ctrl+r (Redo), Ctrl+d, Ctrl+u, Ctrl+f, Ctrl+b
            if control && !cmd {
                if let Some(vi_mode) = current_vi_mode {
                    if vi_mode != zee_core::ViMode::Insert {
                        let is_visual = vi_mode != zee_core::ViMode::Normal;
                        match key.as_str() {
                            "r" => {
                                self.workspace.update(cx, |w, cx| {
                                    if let Some(editor) = w.active_editor_mut() {
                                        editor.redo();
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            "d" => {
                                let max_w = self.last_wrap_width_px;
                                let ascii_w = self.ascii_width_px;
                                let cjk_w = self.cjk_width_px;
                                self.workspace.update(cx, |w, cx| {
                                    let word_wrap = w.config.word_wrap;
                                    let tab_size = w.config.tab_size;
                                    if let Some(editor) = w.active_editor_mut() {
                                        for _ in 0..10 {
                                            if word_wrap {
                                                editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, is_visual);
                                            } else {
                                                editor.move_cursor_down(is_visual);
                                            }
                                        }
                                        editor.ensure_cursor_visible(30, 80, word_wrap);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            "u" => {
                                let max_w = self.last_wrap_width_px;
                                let ascii_w = self.ascii_width_px;
                                let cjk_w = self.cjk_width_px;
                                self.workspace.update(cx, |w, cx| {
                                    let word_wrap = w.config.word_wrap;
                                    let tab_size = w.config.tab_size;
                                    if let Some(editor) = w.active_editor_mut() {
                                        for _ in 0..10 {
                                            if word_wrap {
                                                editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, is_visual);
                                            } else {
                                                editor.move_cursor_up(is_visual);
                                            }
                                        }
                                        editor.ensure_cursor_visible(30, 80, word_wrap);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            "f" => {
                                let max_w = self.last_wrap_width_px;
                                let ascii_w = self.ascii_width_px;
                                let cjk_w = self.cjk_width_px;
                                self.workspace.update(cx, |w, cx| {
                                    let word_wrap = w.config.word_wrap;
                                    let tab_size = w.config.tab_size;
                                    if let Some(editor) = w.active_editor_mut() {
                                        for _ in 0..20 {
                                            if word_wrap {
                                                editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, is_visual);
                                            } else {
                                                editor.move_cursor_down(is_visual);
                                            }
                                        }
                                        editor.ensure_cursor_visible(30, 80, word_wrap);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            "b" => {
                                let max_w = self.last_wrap_width_px;
                                let ascii_w = self.ascii_width_px;
                                let cjk_w = self.cjk_width_px;
                                self.workspace.update(cx, |w, cx| {
                                    let word_wrap = w.config.word_wrap;
                                    let tab_size = w.config.tab_size;
                                    if let Some(editor) = w.active_editor_mut() {
                                        for _ in 0..20 {
                                            if word_wrap {
                                                editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, is_visual);
                                            } else {
                                                editor.move_cursor_up(is_visual);
                                            }
                                        }
                                        editor.ensure_cursor_visible(30, 80, word_wrap);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            "e" => {
                                let max_w = self.last_wrap_width_px;
                                let ascii_w = self.ascii_width_px;
                                let cjk_w = self.cjk_width_px;
                                self.workspace.update(cx, |w, cx| {
                                    let word_wrap = w.config.word_wrap;
                                    let tab_size = w.config.tab_size;
                                    if let Some(editor) = w.active_editor_mut() {
                                        let max_scroll = editor.line_count().saturating_sub(1);
                                        if editor.scroll_row < max_scroll {
                                            editor.scroll_row += 1;
                                            let (line, _) = editor.char_to_line_col(editor.cursor);
                                            if line < editor.scroll_row {
                                                if word_wrap {
                                                    editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, is_visual);
                                                } else {
                                                    editor.move_cursor_down(is_visual);
                                                }
                                            }
                                        }
                                        editor.ensure_cursor_visible(30, 80, word_wrap);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            "y" => {
                                let max_w = self.last_wrap_width_px;
                                let ascii_w = self.ascii_width_px;
                                let cjk_w = self.cjk_width_px;
                                self.workspace.update(cx, |w, cx| {
                                    let word_wrap = w.config.word_wrap;
                                    let tab_size = w.config.tab_size;
                                    if let Some(editor) = w.active_editor_mut() {
                                        if editor.scroll_row > 0 {
                                            editor.scroll_row -= 1;
                                            let (line, _) = editor.char_to_line_col(editor.cursor);
                                            if line >= editor.scroll_row + 30 {
                                                if word_wrap {
                                                    editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, is_visual);
                                                } else {
                                                    editor.move_cursor_up(is_visual);
                                                }
                                            }
                                        }
                                        editor.ensure_cursor_visible(30, 80, word_wrap);
                                    }
                                    cx.notify();
                                });
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // If other Cmd/Ctrl is pressed, key combinations are handled as shortcuts/actions
            if control || cmd {
                return;
            }

            if let Some(vi_mode) = current_vi_mode {
                match vi_mode {
                    zee_core::ViMode::Normal => {
                        self.handle_vi_normal_key(key, shift, cx);
                        self.workspace.update(cx, |w, _| {
                            let word_wrap = w.config.word_wrap;
                            if let Some(editor) = w.active_editor_mut() {
                                editor.ensure_cursor_visible(30, 80, word_wrap);
                            }
                        });
                        return;
                    }
                    zee_core::ViMode::Visual | zee_core::ViMode::VisualLine | zee_core::ViMode::VisualBlock => {
                        self.handle_vi_visual_key(key, cx);
                        self.workspace.update(cx, |w, _| {
                            let word_wrap = w.config.word_wrap;
                            if let Some(editor) = w.active_editor_mut() {
                                editor.ensure_cursor_visible(30, 80, word_wrap);
                            }
                        });
                        return;
                    }
                    zee_core::ViMode::Insert => {
                        if (control && !cmd && key.as_str() == "[") || key.as_str() == "escape" {
                            self.workspace.update(cx, |w, cx| {
                                if let Some(editor) = w.active_editor_mut() {
                                    editor.vi_mode = zee_core::ViMode::Normal;
                                    editor.selection = None;
                                    editor.selection_anchor = None;
                                }
                                cx.notify();
                            });
                            self.preedit_text = None;
                            self.preedit_range = None;
                            return;
                        }
                    }
                }
            }
        } else {
            // If Cmd/Ctrl is pressed, key combinations are handled as shortcuts/actions
            if control || cmd {
                return;
            }
        }

        self.workspace.update(cx, |w, cx| {
            let expand_tab = w.config.expand_tab;
            let tab_size = w.config.tab_size;
            let word_wrap = w.config.word_wrap;
            let max_w = self.last_wrap_width_px;
            let ascii_w = self.ascii_width_px;
            let cjk_w = self.cjk_width_px;
            let vi_mode_enabled = w.config.vi_mode;
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            match key.as_str() {
                "up" => {
                    if word_wrap {
                        editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, shift);
                    } else {
                        editor.move_cursor_up(shift);
                    }
                }
                "down" => {
                    if word_wrap {
                        editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, shift);
                    } else {
                        editor.move_cursor_down(shift);
                    }
                }
                "left" => editor.move_cursor_left(shift),
                "right" => editor.move_cursor_right(shift),
                "home" => editor.move_cursor_home(shift),
                "end" => editor.move_cursor_end(shift),
                "pageup" => {
                    for _ in 0..20 {
                        if word_wrap {
                            editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, shift);
                        } else {
                            editor.move_cursor_up(shift);
                        }
                    }
                }
                "pagedown" => {
                    for _ in 0..20 {
                        if word_wrap {
                            editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, shift);
                        } else {
                            editor.move_cursor_down(shift);
                        }
                    }
                }
                "tab" => {
                    let text = if expand_tab {
                        " ".repeat(tab_size)
                    } else {
                        "\t".to_string()
                    };
                    if let Some(range) = editor.selection.clone() {
                        editor.delete(range);
                    }
                    editor.insert(editor.cursor, &text);
                }
                "backspace" => {
                    if editor.vi_mode == zee_core::ViMode::VisualBlock {
                        editor.delete_visual_block();
                        editor.vi_mode = if vi_mode_enabled { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                    } else if let Some(range) = editor.selection.clone() {
                        editor.delete(range);
                    } else if editor.cursor > 0 {
                        editor.delete(editor.cursor - 1..editor.cursor);
                    }
                }
                "delete" => {
                    if editor.vi_mode == zee_core::ViMode::VisualBlock {
                        editor.delete_visual_block();
                        editor.vi_mode = if vi_mode_enabled { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                    } else if let Some(range) = editor.selection.clone() {
                        editor.delete(range);
                    } else if editor.cursor < editor.rope.len_chars() {
                        editor.delete(editor.cursor..editor.cursor + 1);
                    }
                }
                "enter" => {
                    if let Some(range) = editor.selection.clone() {
                        editor.delete(range);
                    }
                    editor.insert(editor.cursor, "\n");
                }
                _ => {}
            }
            editor.ensure_cursor_visible(30, 80, word_wrap);
            cx.notify();
        });
    }

    fn handle_vi_normal_key(&mut self, key: &str, shift: bool, cx: &mut Context<Self>) {
        if !self.pending_r && !self.pending_f && !self.pending_capital_f && !self.pending_t && !self.pending_capital_t
            && !self.pending_m && !self.pending_single_quote && !self.pending_backtick && !self.pending_indent && !self.pending_unindent
            && !self.pending_g
        {
            if (self.count == 0 && matches!(key, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
                || (self.count > 0 && matches!(key, "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
            {
                if let Ok(digit) = key.parse::<usize>() {
                    self.count = self.count.saturating_mul(10).saturating_add(digit);
                    return;
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

        if self.pending_r {
            if key != "escape" && key.chars().count() == 1 {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.cursor < editor.rope.len_chars() {
                            let (line, col) = editor.char_to_line_col(editor.cursor);
                            let max_col = editor.get_line_max_col(line);
                            if col < max_col {
                                editor.delete(editor.cursor..editor.cursor + 1);
                                editor.insert(editor.cursor, key);
                                editor.cursor = editor.cursor.saturating_sub(1);
                            }
                        }
                    }
                    cx.notify();
                });
            }
            self.pending_r = false;
            return;
        }

        if self.pending_f || self.pending_capital_f || self.pending_t || self.pending_capital_t {
            let forward = self.pending_f || self.pending_t;
            let till = self.pending_t || self.pending_capital_t;
            self.pending_f = false;
            self.pending_capital_f = false;
            self.pending_t = false;
            self.pending_capital_t = false;

            if key != "escape" && key.chars().count() == 1 {
                let target_ch = key.chars().next().unwrap();
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(pos) = editor.find_and_record_inline_char(target_ch, forward, till) {
                            editor.cursor = pos;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        }
                    }
                    cx.notify();
                });
            }
            return;
        }

        if self.pending_m {
            self.pending_m = false;
            if key != "escape" && key.chars().count() == 1 {
                let m = key.chars().next().unwrap();
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.set_mark(m);
                    }
                    cx.notify();
                });
            }
            return;
        }

        if self.pending_single_quote || self.pending_backtick {
            let line_only = self.pending_single_quote;
            self.pending_single_quote = false;
            self.pending_backtick = false;
            if key != "escape" && key.chars().count() == 1 {
                let m = key.chars().next().unwrap();
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.jump_to_mark(m, line_only, false);
                    }
                    cx.notify();
                });
            }
            return;
        }

        if self.pending_indent {
            self.pending_indent = false;
            if key == ">" {
                self.workspace.update(cx, |w, cx| {
                    let expand_tab = w.config.expand_tab;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        editor.indent_line(line, expand_tab, tab_size);
                    }
                    cx.notify();
                });
                return;
            }
        }

        if self.pending_unindent {
            self.pending_unindent = false;
            if key == "<" {
                self.workspace.update(cx, |w, cx| {
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        editor.unindent_line(line, tab_size);
                    }
                    cx.notify();
                });
                return;
            }
        }

        if self.pending_d {
            let mut text_to_copy = None;
            let mut handled = true;

            self.workspace.update(cx, |w, cx| {
                if let Some(editor) = w.active_editor_mut() {
                    let start_pos = editor.cursor;
                    let target_pos = match key {
                        "d" => {
                            let (line, _) = editor.char_to_line_col(editor.cursor);
                            let line_start = editor.rope.line_to_char(line);
                            let target_end_line = (line + repeat).min(editor.rope.len_lines());
                            let next_line_start = if target_end_line < editor.rope.len_lines() {
                                editor.rope.line_to_char(target_end_line)
                            } else {
                                editor.rope.len_chars()
                            };
                            let range = line_start..next_line_start;
                            if !range.is_empty() {
                                text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                                editor.delete(range);
                                editor.cursor = line_start.min(editor.rope.len_chars());
                                let (new_line, _) = editor.char_to_line_col(editor.cursor);
                                let max_col = editor.get_line_max_col(new_line);
                                let col = editor.cursor - editor.rope.line_to_char(new_line);
                                if col > max_col {
                                    editor.cursor = editor.line_col_to_char(new_line, max_col);
                                }
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                            None
                        }
                        "w" => {
                            editor.move_word_forward(false);
                            Some(editor.cursor)
                        }
                        "e" => {
                            editor.move_word_end(false);
                            Some((editor.cursor + 1).min(editor.rope.len_chars()))
                        }
                        "b" => {
                            editor.move_word_backward(false);
                            Some(editor.cursor)
                        }
                        "$" => {
                            let (line, _) = editor.char_to_line_col(editor.cursor);
                            let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                            Some(line_end)
                        }
                        "W" => {
                            editor.move_bigword_forward(false);
                            Some(editor.cursor)
                        }
                        "E" => {
                            editor.move_bigword_end(false);
                            Some((editor.cursor + 1).min(editor.rope.len_chars()))
                        }
                        "B" => {
                            editor.move_bigword_backward(false);
                            Some(editor.cursor)
                        }
                        "{" => {
                            for _ in 0..repeat { editor.move_to_prev_paragraph(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "}" => {
                            for _ in 0..repeat { editor.move_to_next_paragraph(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "(" => {
                            for _ in 0..repeat { editor.move_to_prev_sentence(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        ")" => {
                            for _ in 0..repeat { editor.move_to_next_sentence(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "0" | "^" | "_" => {
                            let line = editor.rope.char_to_line(editor.cursor);
                            let line_start = if key == "0" {
                                editor.rope.line_to_char(line)
                            } else {
                                let line_str = editor.rope.line(line).to_string();
                                let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                                editor.line_col_to_char(line, indent)
                            };
                            Some(line_start)
                        }
                        "h" => {
                            Some(editor.cursor.saturating_sub(1))
                        }
                        "l" => {
                            Some((editor.cursor + 1).min(editor.rope.len_chars()))
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
                            text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                            editor.delete(range.clone());
                            editor.cursor = range.start;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        }
                    }
                }
                cx.notify();
            });

            if let Some(text) = text_to_copy {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            self.pending_d = false;
            self.pending_op_count = 0;
            if handled {
                return;
            }
        }

        if self.pending_c {
            let mut text_to_copy = None;
            let mut handled = true;

            self.workspace.update(cx, |w, cx| {
                if let Some(editor) = w.active_editor_mut() {
                    let start_pos = editor.cursor;
                    let target_pos = match key {
                        "c" => {
                            let (line, _) = editor.char_to_line_col(editor.cursor);
                            let line_start = editor.rope.line_to_char(line);
                            let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                            let range = line_start..line_end;
                            if !range.is_empty() {
                                text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                                editor.delete(range);
                                editor.cursor = line_start;
                            }
                            editor.vi_mode = zee_core::ViMode::Insert;
                            None
                        }
                        "w" => {
                            editor.move_word_forward(false);
                            Some(editor.cursor)
                        }
                        "e" => {
                            editor.move_word_end(false);
                            Some((editor.cursor + 1).min(editor.rope.len_chars()))
                        }
                        "b" => {
                            editor.move_word_backward(false);
                            Some(editor.cursor)
                        }
                        "$" => {
                            let (line, _) = editor.char_to_line_col(editor.cursor);
                            let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                            Some(line_end)
                        }
                        "W" => {
                            editor.move_bigword_forward(false);
                            Some(editor.cursor)
                        }
                        "E" => {
                            editor.move_bigword_end(false);
                            Some((editor.cursor + 1).min(editor.rope.len_chars()))
                        }
                        "B" => {
                            editor.move_bigword_backward(false);
                            Some(editor.cursor)
                        }
                        "{" => {
                            for _ in 0..repeat { editor.move_to_prev_paragraph(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "}" => {
                            for _ in 0..repeat { editor.move_to_next_paragraph(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "(" => {
                            for _ in 0..repeat { editor.move_to_prev_sentence(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        ")" => {
                            for _ in 0..repeat { editor.move_to_next_sentence(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "0" | "^" | "_" => {
                            let line = editor.rope.char_to_line(editor.cursor);
                            let line_start = if key == "0" {
                                editor.rope.line_to_char(line)
                            } else {
                                let line_str = editor.rope.line(line).to_string();
                                let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                                editor.line_col_to_char(line, indent)
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
                            text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                            editor.delete(range.clone());
                            editor.cursor = range.start;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                }
                cx.notify();
            });

            if let Some(text) = text_to_copy {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            self.pending_c = false;
            self.pending_op_count = 0;
            if handled {
                self.ignore_next_text_input = true;
                return;
            }
        }

        if self.pending_y {
            let mut text_to_copy = None;
            let mut handled = true;

            self.workspace.update(cx, |w, cx| {
                if let Some(editor) = w.active_editor_mut() {
                    let start_pos = editor.cursor;
                    let target_pos = match key {
                        "y" => {
                            let (line, _) = editor.char_to_line_col(editor.cursor);
                            editor.select_line(line);
                            if let Some(range) = editor.selection.clone() {
                                text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                            None
                        }
                        "w" => {
                            editor.move_word_forward(false);
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "e" => {
                            editor.move_word_end(false);
                            let pos = (editor.cursor + 1).min(editor.rope.len_chars());
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "b" => {
                            editor.move_word_backward(false);
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "$" => {
                            let (line, _) = editor.char_to_line_col(editor.cursor);
                            let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                            Some(line_end)
                        }
                        "W" => {
                            editor.move_bigword_forward(false);
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "E" => {
                            editor.move_bigword_end(false);
                            let pos = (editor.cursor + 1).min(editor.rope.len_chars());
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "B" => {
                            editor.move_bigword_backward(false);
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "{" => {
                            for _ in 0..repeat { editor.move_to_prev_paragraph(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "}" => {
                            for _ in 0..repeat { editor.move_to_next_paragraph(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "(" => {
                            for _ in 0..repeat { editor.move_to_prev_sentence(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        ")" => {
                            for _ in 0..repeat { editor.move_to_next_sentence(false); }
                            let pos = editor.cursor;
                            editor.cursor = start_pos;
                            Some(pos)
                        }
                        "0" | "^" | "_" => {
                            let line = editor.rope.char_to_line(editor.cursor);
                            let line_start = if key == "0" {
                                editor.rope.line_to_char(line)
                            } else {
                                let line_str = editor.rope.line(line).to_string();
                                let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                                editor.line_col_to_char(line, indent)
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
                            text_to_copy = Some(editor.rope.slice(range).to_string());
                        }
                    }
                }
                cx.notify();
            });

            if let Some(text) = text_to_copy {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            self.pending_y = false;
            self.pending_op_count = 0;
            if handled {
                return;
            }
        }

        match key {
            "i" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "I" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let line = editor.rope.char_to_line(editor.cursor);
                        let line_str = editor.rope.line(line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(line, indent);
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "a" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_right(false);
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "A" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        editor.cursor = editor.line_col_to_char(line, editor.get_line_max_col(line));
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "o" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_end(false);
                        editor.insert(editor.cursor, "\n");
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "O" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_home(false);
                        editor.insert(editor.cursor, "\n");
                        editor.move_cursor_up(false);
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "v" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.vi_mode = zee_core::ViMode::Visual;
                        editor.ensure_selection();
                    }
                    cx.notify();
                });
            }
            "V" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.vi_mode = zee_core::ViMode::VisualLine;
                        editor.ensure_selection();
                        editor.update_selection();
                    }
                    cx.notify();
                });
            }
            "{" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_to_prev_paragraph(false);
                        }
                    }
                    cx.notify();
                });
            }
            "}" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_to_next_paragraph(false);
                        }
                    }
                    cx.notify();
                });
            }
            "(" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_to_prev_sentence(false);
                        }
                    }
                    cx.notify();
                });
            }
            ")" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_to_next_sentence(false);
                        }
                    }
                    cx.notify();
                });
            }
            "H" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let offset = repeat.saturating_sub(1);
                        let target_line = (editor.scroll_row + offset).min(editor.line_count().saturating_sub(1));
                        let line_str = editor.rope.line(target_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(target_line, indent);
                        editor.selection = None;
                        editor.selection_anchor = None;
                    }
                    cx.notify();
                });
            }
            "M" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let half = 15; // 30 lines visible default
                        let target_line = (editor.scroll_row + half).min(editor.line_count().saturating_sub(1));
                        let line_str = editor.rope.line(target_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(target_line, indent);
                        editor.selection = None;
                        editor.selection_anchor = None;
                    }
                    cx.notify();
                });
            }
            "L" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let offset = repeat.saturating_sub(1);
                        let visible_bottom = editor.scroll_row + 29;
                        let target_line = visible_bottom.saturating_sub(offset).min(editor.line_count().saturating_sub(1));
                        let line_str = editor.rope.line(target_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(target_line, indent);
                        editor.selection = None;
                        editor.selection_anchor = None;
                    }
                    cx.notify();
                });
            }
            "h" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_cursor_left(false);
                        }
                    }
                    cx.notify();
                });
            }
            "j" => {
                let max_w = self.last_wrap_width_px;
                let ascii_w = self.ascii_width_px;
                let cjk_w = self.cjk_width_px;
                self.workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            if word_wrap {
                                editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, false);
                            } else {
                                editor.move_cursor_down(false);
                            }
                        }
                    }
                    cx.notify();
                });
            }
            "k" => {
                let max_w = self.last_wrap_width_px;
                let ascii_w = self.ascii_width_px;
                let cjk_w = self.cjk_width_px;
                self.workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            if word_wrap {
                                editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, false);
                            } else {
                                editor.move_cursor_up(false);
                            }
                        }
                    }
                    cx.notify();
                });
            }
            "l" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_cursor_right(false);
                        }
                    }
                    cx.notify();
                });
            }
            "w" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_word_forward(false);
                        }
                    }
                    cx.notify();
                });
            }
            "b" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_word_backward(false);
                        }
                    }
                    cx.notify();
                });
            }
            "e" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_word_end(false);
                        }
                    }
                    cx.notify();
                });
            }
            "0" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let line = editor.rope.char_to_line(editor.cursor);
                        editor.cursor = editor.rope.line_to_char(line);
                    }
                    cx.notify();
                });
            }
            "^" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let line = editor.rope.char_to_line(editor.cursor);
                        let line_str = editor.rope.line(line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(line, indent);
                    }
                    cx.notify();
                });
            }
            "$" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if repeat > 1 {
                            for _ in 1..repeat {
                                editor.move_cursor_down(false);
                            }
                        }
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        editor.cursor = editor.line_col_to_char(line, editor.get_line_max_col(line));
                    }
                    cx.notify();
                });
            }
            "W" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_bigword_forward(false);
                        }
                    }
                    cx.notify();
                });
            }
            "B" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_bigword_backward(false);
                        }
                    }
                    cx.notify();
                });
            }
            "E" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_bigword_end(false);
                        }
                    }
                    cx.notify();
                });
            }
            "_" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if repeat > 1 {
                            for _ in 1..repeat {
                                editor.move_cursor_down(false);
                            }
                        }
                        editor.move_to_first_non_blank(false);
                    }
                    cx.notify();
                });
            }
            "+" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_to_next_line_non_blank(false);
                        }
                    }
                    cx.notify();
                });
            }
            "-" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.move_to_prev_line_non_blank(false);
                        }
                    }
                    cx.notify();
                });
            }
            ";" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            if let Some(pos) = editor.repeat_inline_find(false) {
                                editor.cursor = pos;
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                        }
                    }
                    cx.notify();
                });
            }
            "," => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            if let Some(pos) = editor.repeat_inline_find(true) {
                                editor.cursor = pos;
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                        }
                    }
                    cx.notify();
                });
            }
            "u" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.undo();
                        }
                    }
                    cx.notify();
                });
            }
            "x" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            if editor.cursor < editor.rope.len_chars() {
                                editor.delete(editor.cursor..editor.cursor + 1);
                            }
                        }
                    }
                    cx.notify();
                });
            }
            "X" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            if editor.cursor > 0 {
                                let (_line, col) = editor.char_to_line_col(editor.cursor);
                                if col > 0 {
                                    editor.delete(editor.cursor - 1..editor.cursor);
                                }
                            }
                        }
                    }
                    cx.notify();
                });
            }
            "~" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        for _ in 0..repeat {
                            editor.toggle_case_at_cursor();
                        }
                    }
                    cx.notify();
                });
            }
            "%" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(pos) = editor.find_matching_bracket(editor.cursor) {
                            editor.cursor = pos;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        }
                    }
                    cx.notify();
                });
            }
            "f" => {
                self.pending_f = true;
            }
            "F" => {
                self.pending_capital_f = true;
            }
            "t" => {
                self.pending_t = true;
            }
            "T" => {
                self.pending_capital_t = true;
            }
            "m" => {
                self.pending_m = true;
            }
            "'" => {
                self.pending_single_quote = true;
            }
            "`" => {
                self.pending_backtick = true;
            }
            ">" => {
                self.pending_indent = true;
            }
            "<" => {
                self.pending_unindent = true;
            }
            ":" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    w.vi_cmd = Some(":".into());
                    cx.notify();
                });
            }
            "/" | "?" => {
                self.ignore_next_text_input = true;
                cx.dispatch_action(&crate::app::Find {});
            }
            "n" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.find_next_match();
                    }
                    cx.notify();
                });
            }
            "N" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.find_prev_match();
                    }
                    cx.notify();
                });
            }
            "*" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.search_word_at_cursor(true);
                    }
                    cx.notify();
                });
            }
            "#" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.search_word_at_cursor(false);
                    }
                    cx.notify();
                });
            }
            "r" => {
                self.pending_r = true;
            }
            "s" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.cursor < editor.rope.len_chars() {
                            editor.delete(editor.cursor..editor.cursor + 1);
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "S" => {
                self.ignore_next_text_input = true;
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        let line_start = editor.rope.line_to_char(line);
                        let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                        let range = line_start..line_end;
                        if !range.is_empty() {
                            text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                            editor.delete(range);
                            editor.cursor = line_start;
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "C" => {
                self.ignore_next_text_input = true;
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                        let range = editor.cursor..line_end;
                        if !range.is_empty() {
                            text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                            editor.delete(range);
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "D" => {
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                        let range = editor.cursor..line_end;
                        if !range.is_empty() {
                            text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                            editor.delete(range);
                        }
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "Y" => {
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        editor.select_line(line);
                        if let Some(range) = editor.selection.clone() {
                            text_to_copy = Some(editor.rope.slice(range).to_string());
                            editor.selection = None;
                            editor.selection_anchor = None;
                        }
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "J" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        if line + 1 < editor.line_count() {
                            let line_end = editor.line_col_to_char(line, editor.get_line_max_col(line));
                            let next_line_start = editor.rope.line_to_char(line + 1);
                            let next_line_str = editor.rope.line(line + 1).to_string();
                            let next_indent = next_line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                            let next_text_start = next_line_start + next_indent;
                            editor.delete(line_end..next_text_start);
                            editor.insert(line_end, " ");
                            editor.cursor = line_end;
                        }
                    }
                    cx.notify();
                });
            }
            "d" => {
                self.pending_d = true;
                self.pending_op_count = repeat;
            }
            "c" => {
                self.pending_c = true;
                self.pending_op_count = repeat;
            }
            "y" => {
                self.pending_y = true;
                self.pending_op_count = repeat;
            }
            "p" => {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.clone();
                        self.workspace.update(cx, |w, cx| {
                            if let Some(editor) = w.active_editor_mut() {
                                if let Some(range) = editor.selection.clone() {
                                    editor.delete(range);
                                }
                                if text.ends_with('\n') {
                                    // Line paste below
                                    let (line, _) = editor.char_to_line_col(editor.cursor);
                                    let next_line_start = if line + 1 < editor.line_count() {
                                        editor.rope.line_to_char(line + 1)
                                    } else {
                                        editor.rope.len_chars()
                                    };
                                    editor.insert(next_line_start, &text);
                                    editor.cursor = next_line_start;
                                } else {
                                    editor.move_cursor_right(false);
                                    editor.insert(editor.cursor, &text);
                                }
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                            cx.notify();
                        });
                    }
                }
            }
            "P" => {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.clone();
                        self.workspace.update(cx, |w, cx| {
                            if let Some(editor) = w.active_editor_mut() {
                                if let Some(range) = editor.selection.clone() {
                                    editor.delete(range);
                                }
                                if text.ends_with('\n') {
                                    // Line paste above
                                    let (line, _) = editor.char_to_line_col(editor.cursor);
                                    let line_start = editor.rope.line_to_char(line);
                                    editor.insert(line_start, &text);
                                    editor.cursor = line_start;
                                } else {
                                    editor.insert(editor.cursor, &text);
                                }
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                            cx.notify();
                        });
                    }
                }
            }
            "g" => {
                if self.pending_g {
                    self.workspace.update(cx, |w, cx| {
                        if let Some(editor) = w.active_editor_mut() {
                            editor.cursor = 0;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        }
                        cx.notify();
                    });
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
            }
            "G" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let line = if has_count {
                            (repeat.saturating_sub(1)).min(editor.line_count().saturating_sub(1))
                        } else {
                            editor.line_count().saturating_sub(1)
                        };
                        let col = editor.get_line_max_col(line);
                        editor.cursor = editor.line_col_to_char(line, col);
                        editor.selection = None;
                        editor.selection_anchor = None;
                    }
                    cx.notify();
                });
            }
            "escape" => {
                self.count = 0;
                self.pending_op_count = 0;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.selection = None;
                        editor.selection_anchor = None;
                    }
                    w.vi_cmd = None;
                    cx.notify();
                });
                self.preedit_text = None;
                self.preedit_range = None;
                self.pending_d = false;
                self.pending_y = false;
                self.pending_c = false;
                self.pending_g = false;
                self.pending_r = false;
                self.pending_f = false;
                self.pending_capital_f = false;
                self.pending_t = false;
                self.pending_capital_t = false;
                self.pending_indent = false;
                self.pending_unindent = false;
                self.pending_m = false;
                self.pending_single_quote = false;
                self.pending_backtick = false;
            }
            "enter" => {
                self.preedit_text = None;
                self.preedit_range = None;
                self.workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    if let Some(editor) = w.active_editor_mut() {
                        let line = editor.rope.char_to_line(editor.cursor);
                        if line + 1 < editor.rope.len_lines() {
                            let next_line = line + 1;
                            let line_str = editor.rope.line(next_line).to_string();
                            let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                            editor.cursor = editor.line_col_to_char(next_line, indent);
                            editor.selection = None;
                            editor.selection_anchor = None;
                            editor.ensure_cursor_visible(30, 80, word_wrap);
                        }
                    }
                    cx.notify();
                });
            }
            "dd" => {
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let (line, _) = editor.char_to_line_col(editor.cursor);
                        let line_start = editor.rope.line_to_char(line);
                        let target_end_line = (line + repeat).min(editor.rope.len_lines());
                        let next_line_start = if target_end_line < editor.rope.len_lines() {
                            editor.rope.line_to_char(target_end_line)
                        } else {
                            editor.rope.len_chars()
                        };
                        let range = line_start..next_line_start;
                        if !range.is_empty() {
                            text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                            editor.delete(range);
                            editor.cursor = line_start.min(editor.rope.len_chars());
                            let (new_line, _) = editor.char_to_line_col(editor.cursor);
                            let max_col = editor.get_line_max_col(new_line);
                            let col = editor.cursor - editor.rope.line_to_char(new_line);
                            if col > max_col {
                                editor.cursor = editor.line_col_to_char(new_line, max_col);
                            }
                        }
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown" => {
                let max_w = self.last_wrap_width_px;
                let ascii_w = self.ascii_width_px;
                let cjk_w = self.cjk_width_px;
                self.workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        match key {
                            "up" => {
                                if word_wrap {
                                    editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, shift);
                                } else {
                                    editor.move_cursor_up(shift);
                                }
                            }
                            "down" => {
                                if word_wrap {
                                    editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, shift);
                                } else {
                                    editor.move_cursor_down(shift);
                                }
                            }
                            "left" => editor.move_cursor_left(shift),
                            "right" => editor.move_cursor_right(shift),
                            "home" => editor.move_cursor_home(shift),
                            "end" => editor.move_cursor_end(shift),
                            "pageup" => {
                                for _ in 0..20 {
                                    if word_wrap {
                                        editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, shift);
                                    } else {
                                        editor.move_cursor_up(shift);
                                    }
                                }
                            }
                            "pagedown" => {
                                for _ in 0..20 {
                                    if word_wrap {
                                        editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, shift);
                                    } else {
                                        editor.move_cursor_down(shift);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    cx.notify();
                });
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
                self.pending_indent = false;
                self.pending_unindent = false;
            }
        }
    }

    fn handle_vi_visual_key(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.pending_single_quote || self.pending_backtick {
            let line_only = self.pending_single_quote;
            self.pending_single_quote = false;
            self.pending_backtick = false;
            if key != "escape" && key.chars().count() == 1 {
                let m = key.chars().next().unwrap();
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.jump_to_mark(m, line_only, true);
                    }
                    cx.notify();
                });
            }
            return;
        }

        let current_mode = self.workspace.read(cx).active_editor().map(|e| e.vi_mode);
        let is_block = current_mode == Some(zee_core::ViMode::VisualBlock);

        match key {
            "escape" => {
                self.pending_single_quote = false;
                self.pending_backtick = false;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.vi_mode = zee_core::ViMode::Normal;
                        editor.selection = None;
                        editor.selection_anchor = None;
                    }
                    cx.notify();
                });
            }
            "v" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.vi_mode == zee_core::ViMode::Visual {
                            editor.vi_mode = zee_core::ViMode::Normal;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        } else {
                            editor.vi_mode = zee_core::ViMode::Visual;
                            editor.update_selection();
                        }
                    }
                    cx.notify();
                });
            }
            "V" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.vi_mode == zee_core::ViMode::VisualLine {
                            editor.vi_mode = zee_core::ViMode::Normal;
                            editor.selection = None;
                            editor.selection_anchor = None;
                        } else {
                            editor.vi_mode = zee_core::ViMode::VisualLine;
                            editor.update_selection();
                        }
                    }
                    cx.notify();
                });
            }
            "{" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_prev_paragraph(true);
                    }
                    cx.notify();
                });
            }
            "}" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_next_paragraph(true);
                    }
                    cx.notify();
                });
            }
            "(" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_prev_sentence(true);
                    }
                    cx.notify();
                });
            }
            ")" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_next_sentence(true);
                    }
                    cx.notify();
                });
            }
            "H" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let target_line = editor.scroll_row.min(editor.line_count().saturating_sub(1));
                        let line_str = editor.rope.line(target_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(target_line, indent);
                        editor.update_selection();
                    }
                    cx.notify();
                });
            }
            "M" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let half = 15;
                        let target_line = (editor.scroll_row + half).min(editor.line_count().saturating_sub(1));
                        let line_str = editor.rope.line(target_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(target_line, indent);
                        editor.update_selection();
                    }
                    cx.notify();
                });
            }
            "L" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let visible_bottom = editor.scroll_row + 29;
                        let target_line = visible_bottom.min(editor.line_count().saturating_sub(1));
                        let line_str = editor.rope.line(target_line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(target_line, indent);
                        editor.update_selection();
                    }
                    cx.notify();
                });
            }
            "h" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_left(true);
                    }
                    cx.notify();
                });
            }
            "j" => {
                let max_w = self.last_wrap_width_px;
                let ascii_w = self.ascii_width_px;
                let cjk_w = self.cjk_width_px;
                self.workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        if word_wrap {
                            editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, true);
                        } else {
                            editor.move_cursor_down(true);
                        }
                    }
                    cx.notify();
                });
            }
            "k" => {
                let max_w = self.last_wrap_width_px;
                let ascii_w = self.ascii_width_px;
                let cjk_w = self.cjk_width_px;
                self.workspace.update(cx, |w, cx| {
                    let word_wrap = w.config.word_wrap;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        if word_wrap {
                            editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, true);
                        } else {
                            editor.move_cursor_up(true);
                        }
                    }
                    cx.notify();
                });
            }
            "l" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_right(true);
                    }
                    cx.notify();
                });
            }
            "w" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_word_forward(true);
                    }
                    cx.notify();
                });
            }
            "b" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_word_backward(true);
                    }
                    cx.notify();
                });
            }
            "e" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_word_end(true);
                    }
                    cx.notify();
                });
            }
            "0" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_home(true);
                    }
                    cx.notify();
                });
            }
            "$" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_cursor_end(true);
                    }
                    cx.notify();
                });
            }
            "W" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_bigword_forward(true);
                    }
                    cx.notify();
                });
            }
            "B" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_bigword_backward(true);
                    }
                    cx.notify();
                });
            }
            "E" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_bigword_end(true);
                    }
                    cx.notify();
                });
            }
            "_" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_first_non_blank(true);
                    }
                    cx.notify();
                });
            }
            "+" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_next_line_non_blank(true);
                    }
                    cx.notify();
                });
            }
            "-" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.move_to_prev_line_non_blank(true);
                    }
                    cx.notify();
                });
            }
            ";" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(pos) = editor.repeat_inline_find(false) {
                            editor.cursor = pos;
                            editor.update_selection();
                        }
                    }
                    cx.notify();
                });
            }
            "," => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(pos) = editor.repeat_inline_find(true) {
                            editor.cursor = pos;
                            editor.update_selection();
                        }
                    }
                    cx.notify();
                });
            }
            "'" => {
                self.pending_single_quote = true;
            }
            "`" => {
                self.pending_backtick = true;
            }
            "d" | "x" => {
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.vi_mode == zee_core::ViMode::VisualBlock {
                            text_to_copy = Some(editor.get_visual_block_text());
                            editor.delete_visual_block();
                        } else {
                            if let Some(range) = editor.selection.clone() {
                                text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                                editor.delete(range);
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                        }
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "c" | "s" => {
                self.ignore_next_text_input = true;
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.vi_mode == zee_core::ViMode::VisualBlock {
                            text_to_copy = Some(editor.get_visual_block_text());
                            editor.delete_visual_block();
                        } else {
                            if let Some(range) = editor.selection.clone() {
                                text_to_copy = Some(editor.rope.slice(range.clone()).to_string());
                                editor.delete(range);
                                editor.selection = None;
                                editor.selection_anchor = None;
                            }
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "y" => {
                let mut text_to_copy = None;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.vi_mode == zee_core::ViMode::VisualBlock {
                            text_to_copy = Some(editor.get_visual_block_text());
                        } else {
                            if let Some(range) = editor.selection.clone() {
                                text_to_copy = Some(editor.rope.slice(range).to_string());
                            }
                        }
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
                if let Some(text) = text_to_copy {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            "p" => {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        let text = text.clone();
                        self.workspace.update(cx, |w, cx| {
                            if let Some(editor) = w.active_editor_mut() {
                                if editor.vi_mode == zee_core::ViMode::VisualBlock {
                                    editor.delete_visual_block();
                                } else if let Some(range) = editor.selection.clone() {
                                    editor.delete(range);
                                }
                                editor.insert(editor.cursor, &text);
                                editor.selection = None;
                                editor.selection_anchor = None;
                                editor.vi_mode = zee_core::ViMode::Normal;
                            }
                            cx.notify();
                        });
                    }
                }
            }
            "I" if is_block => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(anchor) = editor.selection_anchor {
                            let (anchor_line, anchor_col) = editor.char_to_line_col(anchor);
                            let (cursor_line, cursor_col) = editor.char_to_line_col(editor.cursor);
                            let target_line = anchor_line.min(cursor_line);
                            let target_col = anchor_col.min(cursor_col);
                            editor.cursor = editor.line_col_to_char(target_line, target_col);
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            "A" if is_block => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(anchor) = editor.selection_anchor {
                            let (anchor_line, anchor_col) = editor.char_to_line_col(anchor);
                            let (cursor_line, cursor_col) = editor.char_to_line_col(editor.cursor);
                            let target_line = anchor_line.min(cursor_line);
                            let target_col = anchor_col.max(cursor_col) + 1;
                            editor.cursor = editor.line_col_to_char(target_line, target_col);
                        }
                        editor.vi_mode = zee_core::ViMode::Insert;
                    }
                    cx.notify();
                });
            }
            ">" => {
                self.workspace.update(cx, |w, cx| {
                    let expand_tab = w.config.expand_tab;
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(range) = editor.selection.clone() {
                            let start_line = editor.rope.char_to_line(range.start);
                            let end_line = editor.rope.char_to_line(range.end.saturating_sub(1));
                            editor.indent_range(start_line, end_line, expand_tab, tab_size);
                        }
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
            }
            "<" => {
                self.workspace.update(cx, |w, cx| {
                    let tab_size = w.config.tab_size;
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(range) = editor.selection.clone() {
                            let start_line = editor.rope.char_to_line(range.start);
                            let end_line = editor.rope.char_to_line(range.end.saturating_sub(1));
                            editor.unindent_range(start_line, end_line, tab_size);
                        }
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
            }
            "~" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(range) = editor.selection.clone() {
                            editor.change_case_range(range, None);
                        }
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
            }
            "u" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(range) = editor.selection.clone() {
                            editor.change_case_range(range, Some(false));
                        }
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
            }
            "U" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(range) = editor.selection.clone() {
                            editor.change_case_range(range, Some(true));
                        }
                        editor.selection = None;
                        editor.selection_anchor = None;
                        editor.vi_mode = zee_core::ViMode::Normal;
                    }
                    cx.notify();
                });
            }
            "%" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        if let Some(pos) = editor.find_matching_bracket(editor.cursor) {
                            editor.cursor = pos;
                            editor.update_selection();
                        }
                    }
                    cx.notify();
                });
            }
            "^" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        let line = editor.rope.char_to_line(editor.cursor);
                        let line_str = editor.rope.line(line).to_string();
                        let indent = line_str.chars().take_while(|c| c.is_whitespace() && *c != '\n' && *c != '\r').count();
                        editor.cursor = editor.line_col_to_char(line, indent);
                        editor.update_selection();
                    }
                    cx.notify();
                });
            }
            "g" => {
                if self.pending_g {
                    self.workspace.update(cx, |w, cx| {
                        if let Some(editor) = w.active_editor_mut() {
                            editor.cursor = 0;
                            editor.update_selection();
                        }
                        cx.notify();
                    });
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
            }
            "G" => {
                self.workspace.update(cx, |w, cx| {
                    if let Some(editor) = w.active_editor_mut() {
                        editor.cursor = editor.rope.len_chars();
                        let line = editor.line_count().saturating_sub(1);
                        let col = editor.get_line_max_col(line);
                        editor.cursor = editor.line_col_to_char(line, col);
                        editor.update_selection();
                    }
                    cx.notify();
                });
            }
            ":" => {
                self.ignore_next_text_input = true;
                self.workspace.update(cx, |w, cx| {
                    w.vi_cmd = Some(":".into());
                    cx.notify();
                });
            }
            _ => {}
        }
    }


    fn mouse_pos_to_char_pos(&self, position: Point<Pixels>, cx: &mut Context<Self>) -> usize {
        let workspace = self.workspace.read(cx);
        let editor = match workspace.active_editor() {
            Some(e) => e,
            None => return 0,
        };
        
        let line_height = px(workspace.config.line_height);
        let font_size = workspace.config.font_size;
        let gutter_width = if workspace.config.line_numbers { px(52.0) } else { px(0.0) };
        let text_left_padding = if workspace.config.line_numbers { TEXT_AREA_LEFT_PADDING } else { px(0.0) };
        let sidebar_width = if workspace.sidebar_visible { px(240.0) } else { px(0.0) };
        let char_width = px(font_size * 0.6);
        let tab_size = workspace.config.tab_size;
        let word_wrap = workspace.config.word_wrap;

        let tab_bar_height = px(36.0);
        #[cfg(not(target_os = "macos"))]
        let menu_bar_height = px(28.0);
        #[cfg(target_os = "macos")]
        let menu_bar_height = px(0.0);

        let top_offset = tab_bar_height + menu_bar_height;
        let relative_y = position.y - top_offset;
        let left_offset = sidebar_width + gutter_width + text_left_padding;
        let relative_x = (position.x - left_offset).max(px(0.0));

        if !word_wrap {
            let line_idx = (relative_y / line_height).floor() as i32 + editor.scroll_row as i32;
            let line_idx = line_idx.max(0).min(editor.line_count() as i32 - 1) as usize;

            let relative_x = relative_x + px(editor.scroll_col as f32 * font_size * 0.6);
            let col_idx = (relative_x / char_width).round() as i32;
            let col_idx = col_idx.max(0) as usize;

            editor.line_col_to_char(line_idx, col_idx)
        } else {
            let visual_row = (relative_y / line_height).floor() as i32;
            if visual_row < 0 {
                return editor.line_col_to_char(editor.scroll_row, 0);
            }
            let mut current_vrow = 0;
            let target_vx: f32 = relative_x / px(1.0);

            for line_idx in editor.scroll_row..editor.line_count() {
                let wraps = editor.wrap_line_px(
                    line_idx,
                    self.last_wrap_width_px,
                    self.ascii_width_px,
                    self.cjk_width_px,
                    tab_size,
                );
                let line_vrows = wraps.len();
                if visual_row < current_vrow + line_vrows as i32 {
                    let v_idx = (visual_row - current_vrow).max(0) as usize;
                    let v_idx = v_idx.min(line_vrows.saturating_sub(1));
                    let range = wraps[v_idx].clone();
                    return editor.get_char_at_v_px(
                        line_idx,
                        range,
                        target_vx,
                        self.ascii_width_px,
                        self.cjk_width_px,
                        tab_size,
                    );
                }
                current_vrow += line_vrows as i32;
            }

            editor.rope.len_chars()
        }
    }

    fn get_track_bounds(&self, window: &Window) -> (Pixels, Pixels) {
        if let Some(b) = self.last_bounds {
            (b.origin.y, b.size.height)
        } else {
            let tab_bar_height = px(36.0);
            #[cfg(not(target_os = "macos"))]
            let menu_bar_height = px(28.0);
            #[cfg(target_os = "macos")]
            let menu_bar_height = px(0.0);
            let top = tab_bar_height + menu_bar_height;
            let h = (window.viewport_size().height - top - px(26.0)).max(px(50.0));
            (top, h)
        }
    }

    fn handle_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        let (track_top, track_height) = self.get_track_bounds(window);

        let workspace = self.workspace.read(cx);
        let editor = match workspace.active_editor() {
            Some(e) => e,
            None => return,
        };
        let line_height_px = px(workspace.config.line_height);
        let line_count = editor.line_count().max(1);
        let visible_lines = (track_height / line_height_px).floor().max(1.0);
        let max_scroll_row = self.max_scroll_row(
            editor,
            workspace.config.line_height,
            workspace.config.word_wrap,
            workspace.config.tab_size,
            window.viewport_size().height,
        );
        if max_scroll_row == 0 {
            return;
        }

        let ratio = (visible_lines / line_count as f32).clamp(0.04, 0.95);
        let thumb_height = (track_height * ratio).max(px(24.0)).min(track_height);
        let scrollable_track = (track_height - thumb_height).max(px(0.0));
        let scroll_ratio = (editor.scroll_row as f32 / max_scroll_row as f32).clamp(0.0, 1.0);
        let thumb_top = scrollable_track * scroll_ratio;

        let click_y = event.position.y - track_top;

        if click_y >= thumb_top && click_y <= (thumb_top + thumb_height) {
            let grab_offset = click_y - thumb_top;
            self.scrollbar_drag = Some(ScrollbarDrag {
                thumb_grab_offset: grab_offset,
            });
        } else {
            let new_thumb_top = (click_y - thumb_height / 2.0).clamp(px(0.0), scrollable_track);
            let new_scroll_ratio = if scrollable_track > px(0.0) {
                (new_thumb_top / scrollable_track).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let target_scroll_row = (new_scroll_ratio * max_scroll_row as f32).round() as usize;

            self.workspace.update(cx, |w, cx| {
                if let Some(editor) = w.active_editor_mut() {
                    editor.scroll_row = target_scroll_row.min(max_scroll_row);
                }
                cx.notify();
            });

            self.scrollbar_drag = Some(ScrollbarDrag {
                thumb_grab_offset: thumb_height / 2.0,
            });
        }

        cx.notify();
    }

    fn handle_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(bounds) = self.last_bounds {
            let scrollbar_left = bounds.origin.x + bounds.size.width - px(14.0);
            if event.position.x >= scrollbar_left && event.position.x <= bounds.origin.x + bounds.size.width {
                self.handle_scrollbar_mouse_down(event, window, cx);
                return;
            }
        }

        self.focus_handle.focus(window, cx);
        self.is_mouse_down = true;
        let now = std::time::Instant::now();
        if let Some(last) = self.last_click_at {
            if now.duration_since(last).as_millis() < 300 {
                self.click_count += 1;
            } else {
                self.click_count = 1;
            }
        } else {
            self.click_count = 1;
        }
        self.last_click_at = Some(now);

        let char_pos = self.mouse_pos_to_char_pos(event.position, cx);
        let workspace_read = self.workspace.read(cx);
        let gutter_width = if workspace_read.config.line_numbers { px(52.0) } else { px(0.0) };
        let sidebar_width = if workspace_read.sidebar_visible { px(240.0) } else { px(0.0) };
        let is_gutter_click = event.position.x >= sidebar_width && event.position.x < (sidebar_width + gutter_width);

        self.workspace.update(cx, |w, cx| {
            let vi_mode_enabled = w.config.vi_mode;
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            if is_gutter_click {
                let (line, _) = editor.char_to_line_col(char_pos);
                editor.select_line(line);
            } else if event.modifiers.alt {
                // Alt + Drag: Visual Block (Rectangular) selection
                editor.vi_mode = zee_core::ViMode::VisualBlock;
                editor.cursor = char_pos;
                editor.selection_anchor = Some(char_pos);
                editor.selection = Some(char_pos..char_pos);
            } else {
                if editor.vi_mode == zee_core::ViMode::VisualBlock {
                    editor.vi_mode = if vi_mode_enabled { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                }
                match self.click_count {
                    1 => {
                        editor.cursor = char_pos;
                        if event.modifiers.shift {
                            editor.ensure_selection();
                            editor.update_selection();
                        } else {
                            editor.selection = None;
                            editor.selection_anchor = Some(char_pos);
                        }
                    }
                    2 => editor.select_word(char_pos),
                    3 => {
                        let (line, _) = editor.char_to_line_col(char_pos);
                        editor.select_line(line);
                    }
                    _ => {
                        editor.cursor = char_pos;
                        editor.selection = None;
                        editor.selection_anchor = Some(char_pos);
                    }
                }
            }
            cx.notify();
        });
    }

    fn handle_mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(drag) = self.scrollbar_drag {
            let (track_top, track_height) = self.get_track_bounds(window);
            let workspace = self.workspace.read(cx);
            let editor = match workspace.active_editor() {
                Some(e) => e,
                None => return,
            };
            let line_height_px = px(workspace.config.line_height);
            let line_count = editor.line_count().max(1);
            let visible_lines = (track_height / line_height_px).floor().max(1.0);
            let max_scroll_row = self.max_scroll_row(
                editor,
                workspace.config.line_height,
                workspace.config.word_wrap,
                workspace.config.tab_size,
                window.viewport_size().height,
            );
            if max_scroll_row == 0 {
                return;
            }

            let ratio = (visible_lines / line_count as f32).clamp(0.04, 0.95);
            let thumb_height = (track_height * ratio).max(px(24.0)).min(track_height);
            let scrollable_track = (track_height - thumb_height).max(px(0.0));

            let current_y = event.position.y - track_top;
            let new_thumb_top = (current_y - drag.thumb_grab_offset).clamp(px(0.0), scrollable_track);
            let new_scroll_ratio = if scrollable_track > px(0.0) {
                (new_thumb_top / scrollable_track).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let target_scroll_row = (new_scroll_ratio * max_scroll_row as f32).round() as usize;

            self.workspace.update(cx, |w, cx| {
                if let Some(editor) = w.active_editor_mut() {
                    editor.scroll_row = target_scroll_row.min(max_scroll_row);
                }
                cx.notify();
            });
            return;
        }

        if self.is_mouse_down || event.pressed_button == Some(MouseButton::Left) {
            let tab_bar_height = px(36.0);
            #[cfg(not(target_os = "macos"))]
            let menu_bar_height = px(28.0);
            #[cfg(target_os = "macos")]
            let menu_bar_height = px(0.0);
            let top_offset = tab_bar_height + menu_bar_height;
            let viewport_height = window.viewport_size().height;

            let char_pos = self.mouse_pos_to_char_pos(event.position, cx);
            self.workspace.update(cx, |w, cx| {
                let line_height = w.config.line_height;
                let word_wrap = w.config.word_wrap;
                let tab_size = w.config.tab_size;

                let editor = match w.active_editor_mut() {
                    Some(e) => e,
                    None => return,
                };

                // Auto-scroll when dragging near top or bottom
                let max_scroll_row = self.max_scroll_row(editor, line_height, word_wrap, tab_size, viewport_height);
                if event.position.y < top_offset + px(24.0) && editor.scroll_row > 0 {
                    editor.scroll_row = editor.scroll_row.saturating_sub(1);
                } else if event.position.y > viewport_height - px(24.0) && editor.scroll_row < max_scroll_row {
                    editor.scroll_row += 1;
                }

                if editor.selection_anchor.is_none() {
                    editor.selection_anchor = Some(editor.cursor);
                }
                editor.cursor = char_pos;
                editor.update_selection();
                cx.notify();
            });
        }
    }

    fn handle_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.scrollbar_drag.is_some() {
            self.scrollbar_drag = None;
            cx.notify();
            return;
        }
        self.is_mouse_down = false;
        self.workspace.update(cx, |w, cx| {
            let vi_mode_enabled = w.config.vi_mode;
            if let Some(editor) = w.active_editor_mut() {
                if editor.vi_mode == zee_core::ViMode::VisualBlock {
                    if let Some(anchor) = editor.selection_anchor {
                        if anchor == editor.cursor {
                            // Clicked without dragging: clear block selection
                            editor.selection = None;
                            editor.selection_anchor = None;
                            editor.vi_mode = if vi_mode_enabled { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                        }
                    } else {
                        editor.vi_mode = if vi_mode_enabled { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                    }
                } else if editor.selection.is_none() {
                    editor.selection_anchor = None;
                }
            }
            cx.notify();
        });
    }

    pub fn max_scroll_row(
        &self,
        editor: &zee_core::buffer::Editor,
        line_height: f32,
        word_wrap: bool,
        tab_size: usize,
        viewport_height: Pixels,
    ) -> usize {
        let line_height_px = px(line_height);
        let tab_bar_height = px(36.0);
        #[cfg(not(target_os = "macos"))]
        let menu_bar_height = px(28.0);
        #[cfg(target_os = "macos")]
        let menu_bar_height = px(0.0);

        let chrome_height = tab_bar_height + menu_bar_height + px(26.0);
        let editor_height = (viewport_height - chrome_height).max(line_height_px);
        let visible_lines = (editor_height / line_height_px).floor().max(1.0) as usize;

        let line_count = editor.line_count();
        if !word_wrap {
            line_count.saturating_sub(visible_lines)
        } else {
            let mut total_vrows = 0;
            let mut target_line = 0;
            for l in (0..line_count).rev() {
                let vrows = editor.wrap_line_px(
                    l,
                    self.last_wrap_width_px,
                    self.ascii_width_px,
                    self.cjk_width_px,
                    tab_size,
                ).len().max(1);
                total_vrows += vrows;
                if total_vrows >= visible_lines {
                    target_line = l;
                    break;
                }
            }
            target_line
        }
    }

    fn handle_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let line_height_px = px(self.workspace.read(cx).config.line_height);
        let char_width_px = px(self.workspace.read(cx).config.font_size * 0.6);
        let viewport_height = window.viewport_size().height;

        self.workspace.update(cx, |w, cx| {
            let line_height = w.config.line_height;
            let word_wrap = w.config.word_wrap;
            let tab_size = w.config.tab_size;

            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            let max_scroll_row = self.max_scroll_row(editor, line_height, word_wrap, tab_size, viewport_height);
            let delta = event.delta.pixel_delta(line_height_px);
            
            // Vertical scroll
            if delta.y != px(0.0) {
                let rows = (delta.y / line_height_px).floor() as i32;
                if rows > 0 {
                    editor.scroll_row = editor.scroll_row.saturating_sub(rows as usize);
                } else {
                    editor.scroll_row = (editor.scroll_row + (-rows) as usize).min(max_scroll_row);
                }
            }

            // Horizontal scroll - only when word_wrap is false
            if !word_wrap && delta.x != px(0.0) {
                let cols = (delta.x / char_width_px).floor() as i32;
                if cols > 0 {
                    editor.scroll_col = editor.scroll_col.saturating_sub(cols as usize);
                } else {
                    editor.scroll_col += (-cols) as usize;
                }
            }
            cx.notify();
        });
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(&mut self, range: std::ops::Range<usize>, _actual_range: &mut Option<std::ops::Range<usize>>, _window: &mut Window, cx: &mut Context<Self>) -> Option<String> {
        let workspace = self.workspace.read(cx);
        if let Some(cmd) = &workspace.vi_cmd {
            let u16_chars: Vec<u16> = cmd.encode_utf16().collect();
            if range.start > u16_chars.len() || range.end > u16_chars.len() {
                return None;
            }
            return String::from_utf16(&u16_chars[range.start..range.end]).ok();
        }
        let editor = workspace.active_editor()?;
        let total_chars = editor.rope.len_chars();
        if range.start > total_chars || range.end > total_chars {
            return None;
        }
        Some(editor.rope.slice(range).to_string())
    }

    fn selected_text_range(&mut self, ignore_disabled_input: bool, _window: &mut Window, cx: &mut Context<Self>) -> Option<UTF16Selection> {
        let workspace = self.workspace.read(cx);
        let vi_mode_enabled = workspace.config.vi_mode;
        let in_vi_cmd = vi_mode_enabled && workspace.vi_cmd.is_some();
        if in_vi_cmd {
            let cmd_len = workspace.vi_cmd.as_ref().map(|s| s.encode_utf16().count()).unwrap_or(0);
            return Some(UTF16Selection {
                range: cmd_len..cmd_len,
                reversed: false,
            });
        }

        let editor = workspace.active_editor()?;
        let is_insert = editor.vi_mode == zee_core::ViMode::Insert;
        let input_enabled = !vi_mode_enabled || is_insert || self.pending_r;

        if !ignore_disabled_input && !input_enabled {
            return None;
        }
        
        let range = editor.selection.clone().unwrap_or(editor.cursor..editor.cursor);
        Some(UTF16Selection {
            range,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<std::ops::Range<usize>> {
        self.preedit_range.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.preedit_text = None;
        self.preedit_range = None;
        self.workspace.update(cx, |w, cx| {
            w.vi_cmd_preedit = None;
            cx.notify();
        });
        cx.notify();
    }

    fn accepts_text_input(&self, _window: &mut Window, cx: &mut Context<Self>) -> bool {
        let workspace = self.workspace.read(cx);
        if !workspace.config.vi_mode {
            return true;
        }
        if workspace.vi_cmd.is_some() {
            return true;
        }
        let is_insert = workspace.active_editor().map(|e| e.vi_mode == zee_core::ViMode::Insert).unwrap_or(true);
        is_insert || self.pending_r
    }

    fn replace_text_in_range(&mut self, replacement_range: Option<std::ops::Range<usize>>, text: &str, _window: &mut Window, cx: &mut Context<Self>) {
        if self.ignore_next_text_input {
            self.ignore_next_text_input = false;
            return;
        }

        let vi_mode_enabled = self.workspace.read(cx).config.vi_mode;
        let in_vi_cmd = vi_mode_enabled && self.workspace.read(cx).vi_cmd.is_some();
        let is_normal_or_visual = self.workspace.read(cx).active_editor().map(|e| e.vi_mode != zee_core::ViMode::Insert).unwrap_or(false);

        if in_vi_cmd {
            self.workspace.update(cx, |w, cx| {
                if let Some(cmd) = &mut w.vi_cmd {
                    cmd.push_str(text);
                }
                w.vi_cmd_preedit = None;
                cx.notify();
            });
            self.preedit_text = None;
            self.preedit_range = None;
            cx.notify();
            return;
        }

        if vi_mode_enabled && is_normal_or_visual && !self.pending_r {
            self.preedit_text = None;
            self.preedit_range = None;
            cx.notify();
            return;
        }

        self.workspace.update(cx, |w, cx| {
            let word_wrap = w.config.word_wrap;
            let vi_mode_enabled = w.config.vi_mode;
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            if editor.vi_mode == zee_core::ViMode::VisualBlock {
                editor.delete_visual_block();
                editor.vi_mode = if vi_mode_enabled { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                editor.insert(editor.cursor, text);
            } else if let Some(range) = replacement_range {
                editor.delete(range);
                editor.insert(editor.cursor, text);
            } else if let Some(range) = editor.selection.clone() {
                editor.delete(range);
                editor.insert(editor.cursor, text);
            } else {
                editor.insert(editor.cursor, text);
            }
            editor.ensure_cursor_visible(30, 80, word_wrap);
            self.preedit_text = None;
            self.preedit_range = None;
            cx.notify();
        });
    }

    fn replace_and_mark_text_in_range(&mut self, range_to_replace: Option<std::ops::Range<usize>>, text: &str, _marked_range: Option<std::ops::Range<usize>>, _window: &mut Window, cx: &mut Context<Self>) {
        if self.ignore_next_text_input {
            self.ignore_next_text_input = false;
            return;
        }

        let vi_mode_enabled = self.workspace.read(cx).config.vi_mode;
        let in_vi_cmd = vi_mode_enabled && self.workspace.read(cx).vi_cmd.is_some();
        let is_normal_or_visual = self.workspace.read(cx).active_editor().map(|e| e.vi_mode != zee_core::ViMode::Insert).unwrap_or(false);

        if in_vi_cmd {
            if text.is_empty() {
                self.unmark_text(_window, cx);
                return;
            }
            let cmd_len = self.workspace.read(cx).vi_cmd.as_ref().map(|s| s.encode_utf16().count()).unwrap_or(0);
            let text_u16_len = text.encode_utf16().count();
            self.preedit_text = Some(text.to_string());
            self.preedit_range = Some(cmd_len..cmd_len + text_u16_len);
            self.workspace.update(cx, |w, cx| {
                w.vi_cmd_preedit = Some(text.to_string());
                cx.notify();
            });
            cx.notify();
            return;
        }

        if vi_mode_enabled && is_normal_or_visual {
            self.preedit_text = None;
            self.preedit_range = None;
            cx.notify();
            return;
        }

        if text.is_empty() {
            self.unmark_text(_window, cx);
            return;
        }
        
        let workspace = self.workspace.read(cx);
        let editor = match workspace.active_editor() {
            Some(e) => e,
            None => return,
        };
        let start_pos = range_to_replace.map(|r| r.start).unwrap_or(editor.cursor);
        
        self.preedit_text = Some(text.to_string());
        self.preedit_range = Some(start_pos..start_pos + text.chars().count());
        cx.notify();
    }

    fn bounds_for_range(&mut self, range_utf16: std::ops::Range<usize>, bounds: Bounds<Pixels>, _window: &mut Window, cx: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let workspace = self.workspace.read(cx);
        let vi_mode_enabled = workspace.config.vi_mode;
        let in_vi_cmd = vi_mode_enabled && workspace.vi_cmd.is_some();

        if in_vi_cmd {
            use unicode_width::UnicodeWidthChar;
            let window_left = if workspace.sidebar_visible && workspace.config.sidebar_position == "left" {
                bounds.origin.x - px(240.0)
            } else {
                bounds.origin.x
            };
            let cmd_text = workspace.vi_cmd.as_deref().unwrap_or("");
            let mono_char_width = 12.0 * 0.6; // approx 7.2px per ASCII char at text_size 12.0
            let mut visual_cols = 0.0;
            for c in cmd_text.chars() {
                let w = UnicodeWidthChar::width(c).unwrap_or(1);
                visual_cols += w as f32;
            }
            let cmd_x = window_left + px(12.0) + px(visual_cols * mono_char_width);
            let cmd_y = bounds.bottom() + px(24.0);
            let preedit_w = (range_utf16.end.saturating_sub(range_utf16.start)).max(1) as f32 * mono_char_width;

            return Some(Bounds {
                origin: Point::new(cmd_x, cmd_y),
                size: Size::new(px(preedit_w), px(22.0)),
            });
        }

        let editor = workspace.active_editor()?;
        
        let line_height = px(workspace.config.line_height);
        let gutter_width = if workspace.config.line_numbers { px(52.0) } else { px(0.0) };
        let text_left_padding = if workspace.config.line_numbers { TEXT_AREA_LEFT_PADDING } else { px(0.0) };
        let char_width = px(workspace.config.font_size * 0.6);

        let (line, col) = editor.char_to_line_col(range_utf16.start);
        
        if line < editor.scroll_row {
            return None;
        }

        let word_wrap = workspace.config.word_wrap;
        let tab_size = workspace.config.tab_size;
        let (visual_row, visual_x) = if !word_wrap {
            let vr = line - editor.scroll_row;
            let vc = (col as i32) - (editor.scroll_col as i32);
            if vc < 0 {
                return None;
            }
            (vr, char_width * vc as f32)
        } else {
            let mut vrow = 0;
            for l in editor.scroll_row..line {
                let wraps = editor.wrap_line_px(
                    l,
                    self.last_wrap_width_px,
                    self.ascii_width_px,
                    self.cjk_width_px,
                    tab_size,
                );
                vrow += wraps.len();
            }
            let wraps = editor.wrap_line_px(
                line,
                self.last_wrap_width_px,
                self.ascii_width_px,
                self.cjk_width_px,
                tab_size,
            );
            let wraps_len = wraps.len();
            let mut found = false;
            let mut target_vx = 0.0;
            for (v_idx, r) in wraps.into_iter().enumerate() {
                if col >= r.start && (col < r.end || v_idx == wraps_len.saturating_sub(1)) {
                    vrow += v_idx;
                    target_vx = editor.get_visual_px(
                        line,
                        col,
                        &r,
                        self.ascii_width_px,
                        self.cjk_width_px,
                        tab_size,
                    );
                    found = true;
                    break;
                }
            }
            if !found { (vrow, px(0.0)) } else { (vrow, px(target_vx)) }
        };

        let origin_x = bounds.origin.x + gutter_width + text_left_padding + visual_x;
        let origin_y = bounds.origin.y + (line_height * visual_row as f32);

        Some(Bounds {
            origin: Point::new(origin_x, origin_y),
            size: Size::new(char_width * (range_utf16.end.saturating_sub(range_utf16.start)).max(1) as f32, line_height),
        })
    }

    fn character_index_for_point(&mut self, _point: Point<Pixels>, _window: &mut Window, _cx: &mut Context<Self>) -> Option<usize> {
        None
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;

        if workspace.editors.is_empty() {
            return div()
                .track_focus(&self.focus_handle)
                .key_context("Editor")
                .w_full()
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .bg(led_color_to_gpui(theme.editor.background))
                .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.4))
                .font_family(crate::widgets::ui_font_family())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_size(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.6))
                                .child("zee")
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .flex()
                                .items_center()
                                .gap_2()
                                .child("⌘T : New Tab")
                                .child("•")
                                .child("⌘N : New Window")
                                .child("•")
                                .child("⌘O : Open File")
                        )
                )
                .into_any_element();
        }

        let editor = workspace.active_editor().unwrap();
        let font_family: SharedString = match &workspace.config.font_family {
            Some(f) => SharedString::from(f.clone()),
            None => SharedString::from(mono_font_family()),
        };
        let font_size = px(workspace.config.font_size);
        let line_height = px(workspace.config.line_height);

        let gutter_width = if workspace.config.line_numbers { px(52.0) } else { px(0.0) };
        let text_left_padding = if workspace.config.line_numbers { TEXT_AREA_LEFT_PADDING } else { px(0.0) };
        let sidebar_width = if workspace.sidebar_visible { px(240.0) } else { px(0.0) };
        let font_size_val = workspace.config.font_size;
        let ascii_width = if cfg!(target_os = "macos") { font_size_val * 0.602 } else { font_size_val * 0.6 };
        let cjk_width = font_size_val * 1.0;
        self.ascii_width_px = ascii_width;
        self.cjk_width_px = cjk_width;
        let viewport_width = window.viewport_size().width;
        let available_width_px = (viewport_width - sidebar_width - gutter_width - text_left_padding - px(16.0)).max(px(100.0));
        let available_width_val: f32 = available_width_px / px(1.0);
        self.last_wrap_width_px = available_width_val;
        let wrap_cols = ((available_width_px / px(ascii_width)).floor() as usize).max(20);
        self.last_wrap_cols = wrap_cols;

        let focus_handle = self.focus_handle.clone();
        let entity = cx.entity().clone();

        div()
            .track_focus(&self.focus_handle)
            .key_context("Editor")
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_key_down(event, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event, window, cx| {
                this.handle_mouse_down(event, window, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, event, window, cx| {
                this.handle_mouse_up(event, window, cx);
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, event, window, cx| {
                this.handle_mouse_up(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event, window, cx| {
                this.handle_mouse_move(event, window, cx);
            }))
            .on_scroll_wheel(cx.listener(|this, event, window, cx| {
                this.handle_scroll(event, window, cx);
            }))
            .w_full()
            .h_full()
            .relative()
            .bg(led_color_to_gpui(theme.editor.background))
            .text_color(led_color_to_gpui(theme.editor.foreground))
            .text_size(font_size)
            .line_height(line_height)
            .font_family(font_family.clone())
            .child(
                canvas(
                    move |_bounds, _window, _cx| {
                        
                    },
                    move |bounds, (), window, cx| {
                        entity.update(cx, |this, _| {
                            this.last_bounds = Some(bounds);
                        });
                        if focus_handle.is_focused(window) {
                            window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity.clone()), cx);
                        }
                    }
                )
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h_full()
            )
            .child(
                div()
                    .w_full()
                    .h_full()
                    .font_family(font_family)
                    .child(self.render_lines(workspace, editor))
            )
            .child(self.render_scrollbar(workspace, editor, window, cx))
            .into_any_element()
    }
}

impl EditorView {
    fn render_scrollbar(
        &self,
        workspace: &Workspace,
        editor: &zee_core::buffer::Editor,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let (_track_top, track_height) = self.get_track_bounds(window);
        let line_count = editor.line_count().max(1);
        let line_height_px = px(workspace.config.line_height);
        let visible_lines = (track_height / line_height_px).floor().max(1.0);

        let max_scroll_row = self.max_scroll_row(
            editor,
            workspace.config.line_height,
            workspace.config.word_wrap,
            workspace.config.tab_size,
            window.viewport_size().height,
        );

        if max_scroll_row == 0 || line_count <= visible_lines as usize {
            return div().w_0().h_0().into_any_element();
        }

        let ratio = (visible_lines / line_count as f32).clamp(0.04, 0.95);
        let thumb_height = (track_height * ratio).max(px(24.0)).min(track_height);
        let scrollable_track = (track_height - thumb_height).max(px(0.0));
        let scroll_ratio = (editor.scroll_row as f32 / max_scroll_row as f32).clamp(0.0, 1.0);
        let thumb_top = scrollable_track * scroll_ratio;

        let theme = &workspace.theme;
        let is_dragging = self.scrollbar_drag.is_some();
        let thumb_color = with_alpha(
            led_color_to_gpui(theme.ui.status_bar_fg),
            if is_dragging { 0.55 } else { 0.28 },
        );
        let thumb_hover = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.55);
        let track_hover = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.08);

        div()
            .id("editor-scrollbar")
            .absolute()
            .top_0()
            .right_0()
            .w(px(14.0))
            .h_full()
            .cursor_default()
            .hover(move |s| s.bg(track_hover))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_scrollbar_mouse_down(event, window, cx);
            }))
            .child(
                div()
                    .absolute()
                    .right(px(3.0))
                    .w(px(8.0))
                    .top(thumb_top)
                    .h(thumb_height)
                    .rounded_full()
                    .bg(thumb_color)
                    .hover(move |s| s.bg(thumb_hover))
            )
            .into_any_element()
    }

    fn render_lines(&self, workspace: &Workspace, editor: &zee_core::buffer::Editor) -> impl IntoElement {
        let line_count = editor.line_count();
        let scroll_row = editor.scroll_row;
        let word_wrap = workspace.config.word_wrap;
        let tab_size = workspace.config.tab_size;

        let mut rows = Vec::new();
        let max_visual_rows = 100;

        for line_idx in scroll_row..line_count {
            if rows.len() >= max_visual_rows {
                break;
            }
            if word_wrap {
                let wraps = editor.wrap_line_px(
                    line_idx,
                    self.last_wrap_width_px,
                    self.ascii_width_px,
                    self.cjk_width_px,
                    tab_size,
                );
                let wraps_len = wraps.len();
                for (v_idx, range) in wraps.into_iter().enumerate() {
                    rows.push(self.render_visual_line(line_idx, v_idx, wraps_len, range, workspace, editor).into_any_element());
                    if rows.len() >= max_visual_rows {
                        break;
                    }
                }
            } else {
                let len = editor.line(line_idx).len_chars();
                rows.push(self.render_visual_line(line_idx, 0, 1, 0..len, workspace, editor).into_any_element());
            }
        }

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .children(rows)
    }

    fn render_visual_line(
        &self,
        line_idx: usize,
        v_idx: usize,
        wraps_len: usize,
        range: std::ops::Range<usize>,
        workspace: &Workspace,
        editor: &zee_core::buffer::Editor,
    ) -> impl IntoElement {
        let theme = &workspace.theme;
        let word_wrap = workspace.config.word_wrap;
        let line_height = px(workspace.config.line_height);
        let char_width = self.ascii_width_px;

        let line = editor.rope.line(line_idx);
        let mut line_str = line.to_string();
        // Strip line endings for rendering
        if line_str.ends_with('\n') {
            line_str.pop();
            if line_str.ends_with('\r') {
                line_str.pop();
            }
        } else if line_str.ends_with('\r') {
            line_str.pop();
        }

        let (cursor_line, _) = editor.char_to_line_col(editor.cursor);
        let is_cursor_line = line_idx == cursor_line;

        let editor_bg = led_color_to_gpui(theme.editor.background);
        let bg: Rgba = if is_cursor_line {
            theme.editor.current_line.map(led_color_to_gpui).unwrap_or(editor_bg)
        } else {
            editor_bg
        };

        let gutter_width = if workspace.config.line_numbers { px(52.0) } else { px(0.0) };
        let text_left_padding = if workspace.config.line_numbers { TEXT_AREA_LEFT_PADDING } else { px(0.0) };
        let gutter_border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.2);

        div()
            .w_full()
            .h(line_height)
            .flex()
            .flex_row()
            .bg(bg)
            .text_color(led_color_to_gpui(theme.editor.foreground))
            .child(
                div()
                    .flex_none()
                    .w(gutter_width)
                    .h(line_height)
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_2p5()
                    .border_r_1()
                    .border_color(if workspace.config.line_numbers { gutter_border } else { rgba(0x00000000) })
                    .text_color(led_color_to_gpui(theme.editor.line_number))
                    .font_family(mono_font_family())
                    .child(
                        if workspace.config.line_numbers && v_idx == 0 {
                            (line_idx + 1).to_string()
                        } else {
                            "".to_string()
                        }
                    )
            )
            .child(
                div()
                    .flex_grow()
                    .h(line_height)
                    .relative()
                    .overflow_hidden()
                    .child({
                        let cursor_overlay = self.render_cursor_overlay(
                            line_idx,
                            v_idx,
                            wraps_len,
                            &range,
                            workspace,
                            editor,
                            is_cursor_line,
                        );

                        let mut line_container = div()
                            .when(!word_wrap, |d| {
                                d.absolute()
                                    .top_0()
                                    .left(text_left_padding - px(editor.scroll_col as f32 * char_width))
                                    .h_full()
                            })
                            .when(word_wrap, |d| {
                                d.absolute()
                                    .top_0()
                                    .left(text_left_padding)
                                    .h_full()
                            })
                            .child(
                                div()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .children(self.render_visual_line_content(
                                        line_idx,
                                        range.clone(),
                                        &line_str,
                                        workspace,
                                        editor,
                                    ))
                            );

                        if let Some(cursor) = cursor_overlay {
                            line_container = line_container.child(cursor);
                        }

                        line_container
                    })
            )
    }

    fn render_visual_line_content(
        &self,
        line_idx: usize,
        range: std::ops::Range<usize>,
        line_str: &str,
        workspace: &Workspace,
        editor: &zee_core::buffer::Editor,
    ) -> Vec<AnyElement> {
        let theme = &workspace.theme;
        let line_height = px(workspace.config.line_height);

        let char_offsets: Vec<usize> = line_str.char_indices().map(|(b, _)| b).collect();
        let total_chars = char_offsets.len();
        let byte_start = if range.start >= total_chars {
            line_str.len()
        } else {
            char_offsets[range.start]
        };
        let byte_end = if range.end >= total_chars {
            line_str.len()
        } else {
            char_offsets[range.end]
        };
        let v_text = &line_str[byte_start..byte_end];
        let v_len_chars = v_text.chars().count();

        let line_start_char = editor.rope.line_to_char(line_idx);
        let v_start_char = line_start_char + range.start;
        let v_end_char = line_start_char + range.end;

        let global_selection = if editor.vi_mode == zee_core::ViMode::VisualBlock {
            let ranges = editor.get_visual_block_ranges();
            ranges.into_iter().find(|r| (r.start < v_end_char && r.end > v_start_char) || (r.start >= v_start_char && r.start <= v_end_char))
        } else {
            editor.selection.clone()
        };

        let v_selection: Option<std::ops::Range<usize>> = if let Some(ref sel) = global_selection {
            let sel_min = sel.start.min(sel.end);
            let sel_max = sel.start.max(sel.end);
            if sel_min < v_end_char && sel_max > v_start_char {
                let s = sel_min.saturating_sub(v_start_char).min(v_len_chars);
                let e = sel_max.saturating_sub(v_start_char).min(v_len_chars);
                if s < e {
                    Some(s..e)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut elements = Vec::new();

        let render_chunk = |text: &str, chunk_start_char: usize, token_color: Option<Rgba>, elements: &mut Vec<AnyElement>| {
            if text.is_empty() { return; }
            self.render_chunk_internal(text, chunk_start_char, token_color, v_selection.clone(), theme, line_height, elements);
        };

        if let Some(Some(tokens)) = editor.line_tokens.get(line_idx) {
            let mut last_v_offset = 0;
            for token in tokens {
                let tok_start = token.byte_range.start.clamp(byte_start, byte_end);
                let tok_end = token.byte_range.end.clamp(byte_start, byte_end);
                if tok_end > tok_start {
                    let v_tok_start = tok_start - byte_start;
                    let v_tok_end = tok_end - byte_start;

                    if v_tok_start > last_v_offset {
                        let text = &v_text[last_v_offset..v_tok_start];
                        let start_char = v_text[..last_v_offset].chars().count();
                        render_chunk(text, start_char, None, &mut elements);
                    }

                    let text = &v_text[v_tok_start..v_tok_end];
                    let start_char = v_text[..v_tok_start].chars().count();
                    render_chunk(text, start_char, Some(self.token_color(token.token, theme)), &mut elements);
                    last_v_offset = v_tok_end;
                }
            }
            if last_v_offset < v_text.len() {
                let text = &v_text[last_v_offset..];
                let start_char = v_text[..last_v_offset].chars().count();
                render_chunk(text, start_char, None, &mut elements);
            }
        } else {
            render_chunk(v_text, 0, None, &mut elements);
        }

        elements
    }

    #[allow(clippy::too_many_arguments)]
    fn render_cursor_overlay(
        &self,
        line_idx: usize,
        v_idx: usize,
        wraps_len: usize,
        range: &std::ops::Range<usize>,
        workspace: &Workspace,
        editor: &zee_core::buffer::Editor,
        is_cursor_line: bool,
    ) -> Option<AnyElement> {
        if !is_cursor_line {
            return None;
        }

        let (_cursor_line, cursor_col) = editor.char_to_line_col(editor.cursor);
        let is_last_vrow = v_idx == wraps_len - 1;
        let is_cursor_on_vrow = if is_last_vrow {
            cursor_col >= range.start && cursor_col <= range.end
        } else {
            cursor_col >= range.start && cursor_col < range.end
        };

        if !is_cursor_on_vrow {
            return None;
        }

        let cursor_x = editor.get_visual_px(
            line_idx,
            cursor_col,
            range,
            self.ascii_width_px,
            self.cjk_width_px,
            workspace.config.tab_size,
        );

        let theme = &workspace.theme;
        let line_height = px(workspace.config.line_height);

        let is_insert = !workspace.config.vi_mode || editor.vi_mode == zee_core::ViMode::Insert;
        if is_insert {
            if let Some(ref preedit) = self.preedit_text {
                return Some(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(cursor_x))
                        .h(line_height)
                        .flex()
                        .items_center()
                        .text_color(gpui::rgb(0xffffff))
                        .bg(gpui::rgb(0x0000ff))
                        .border_b_1()
                        .border_color(gpui::rgb(0xffffff))
                        .font_family(mono_font_family())
                        .child(preedit.clone())
                        .into_any_element(),
                );
            }
        }

        let is_block_cursor = workspace.config.vi_mode && editor.vi_mode != zee_core::ViMode::Insert;
        let cursor_color = led_color_to_gpui(theme.editor.cursor);

        if is_block_cursor {
            let line = editor.rope.line(line_idx);
            let char_at_cursor = line.chars().nth(cursor_col);
            use unicode_width::UnicodeWidthChar;
            let cursor_w = match char_at_cursor {
                Some('\t') => {
                    let tab_width_px = workspace.config.tab_size as f32 * self.ascii_width_px;
                    let col_px = cursor_x % tab_width_px;
                    tab_width_px - col_px
                }
                Some(c) if c.width() == Some(2) => self.cjk_width_px,
                _ => self.ascii_width_px,
            };

            Some(
                div()
                    .absolute()
                    .top_0()
                    .left(px(cursor_x))
                    .w(px(cursor_w))
                    .h(line_height)
                    .bg(with_alpha(cursor_color, 0.5))
                    .border_1()
                    .border_color(cursor_color)
                    .into_any_element(),
            )
        } else {
            let is_ime_active = zee_core::is_cjk_ime_active() || self.preedit_text.is_some();
            if is_ime_active {
                let line = editor.rope.line(line_idx);
                let char_at_cursor = line.chars().nth(cursor_col);
                use unicode_width::UnicodeWidthChar;
                let cursor_w = match char_at_cursor {
                    Some(c) if c.width() == Some(2) => self.cjk_width_px,
                    _ => self.ascii_width_px,
                };
                let ime_cursor_color = gpui::rgb(0xffa726); // Vibrant amber/orange
                let underscore_height = px(3.0);
                let underscore_top = line_height - underscore_height;

                Some(
                    div()
                        .absolute()
                        .top(underscore_top)
                        .left(px(cursor_x))
                        .w(px(cursor_w))
                        .h(underscore_height)
                        .bg(ime_cursor_color)
                        .rounded_xs()
                        .into_any_element(),
                )
            } else {
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(cursor_x))
                        .w(px(2.0))
                        .h(line_height)
                        .bg(cursor_color)
                        .into_any_element(),
                )
            }
        }
    }


    #[allow(clippy::too_many_arguments)]
    fn render_chunk_internal(
        &self,
        text: &str,
        start_char: usize,
        token_color: Option<Rgba>,
        selection: Option<std::ops::Range<usize>>,
        theme: &Theme,
        line_height: Pixels,
        elements: &mut Vec<AnyElement>,
    ) {
        if text.is_empty() { return; }

        let text_color = token_color.unwrap_or(led_color_to_gpui(theme.editor.foreground));
        let sel_bg = with_alpha(led_color_to_gpui(theme.editor.selection), 0.75);

        let chunk_chars: Vec<char> = text.chars().collect();
        let chunk_len = chunk_chars.len();

        if let Some(ref sel) = selection {
            let sel_start = sel.start.saturating_sub(start_char);
            let sel_end = sel.end.saturating_sub(start_char);

            if sel_start < chunk_len && sel_end > 0 {
                let highlight_start = sel_start;
                let highlight_end = sel_end.min(chunk_len);

                if highlight_start > 0 {
                    elements.push(
                        div()
                            .h(line_height)
                            .flex()
                            .items_center()
                            .text_color(text_color)
                            .font_family(mono_font_family())
                            .child(chunk_chars[..highlight_start].iter().collect::<String>())
                            .into_any_element()
                    );
                }

                elements.push(
                    div()
                        .h(line_height)
                        .flex()
                        .items_center()
                        .bg(sel_bg)
                        .text_color(text_color)
                        .font_family(mono_font_family())
                        .child(chunk_chars[highlight_start..highlight_end].iter().collect::<String>())
                        .into_any_element()
                );

                if highlight_end < chunk_len {
                    elements.push(
                        div()
                            .h(line_height)
                            .flex()
                            .items_center()
                            .text_color(text_color)
                            .font_family(mono_font_family())
                            .child(chunk_chars[highlight_end..].iter().collect::<String>())
                            .into_any_element()
                    );
                }
                return;
            }
        }

        elements.push(
            div()
                .h(line_height)
                .flex()
                .items_center()
                .text_color(text_color)
                .font_family(mono_font_family())
                .child(text.to_string())
                .into_any_element()
        );
    }
}
