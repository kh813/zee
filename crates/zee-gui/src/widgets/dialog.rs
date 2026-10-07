use gpui::*;
use crate::workspace::Workspace;
use crate::app::Quit;
use zee_core::i18n::I18n;
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChangesIntent {
    Quit,
    CloseTab,
    Reload,
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Checking,
    UpToDate { version: String },
    Available { latest_version: String, asset_url: Option<String>, html_url: String },
    Downloading,
    Success,
    Failed { error: String, html_url: Option<String> },
}

pub enum DialogType {
    About,
    Update,
    GoToLine,
    #[allow(dead_code)]
    OpenFile,
    #[allow(dead_code)]
    SaveAs,
    UnsavedChanges { filename: String, intent: UnsavedChangesIntent },
    #[allow(dead_code)]
    Message { title: String, message: String },
    Settings,
    PluginManager,
    NewFile { parent_dir: std::path::PathBuf },
    NewFolder { parent_dir: std::path::PathBuf },
    Rename { target_path: std::path::PathBuf },
    ConfirmDelete { target_path: std::path::PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsDropdown {
    Theme,
    Font,
    Language,
}

pub struct Dialog {
    workspace: Entity<Workspace>,
    i18n: I18n,
    dialog_type: DialogType,
    input_text: String,
    focus_handle: FocusHandle,
    // File browser state
    current_dir: std::path::PathBuf,
    files: Vec<FileEntry>,
    selected_idx: usize,
    show_hidden: bool,
    // Button focus state for UnsavedChanges
    button_idx: usize,
    // Update dialog state
    update_status: Option<UpdateStatus>,
    // Settings dropdown state
    settings_dropdown: Option<SettingsDropdown>,
    // Plugin Manager state
    plugin_tab: usize,
    registry_plugins: Option<Vec<zee_core::plugin::RegistryPlugin>>,
    registry_loading: bool,
    registry_error: Option<String>,
    plugin_show_add_repo: bool,
    plugin_new_repo_url: String,
}

#[derive(Clone)]
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
    #[allow(dead_code)]
    modified: std::time::SystemTime,
}

impl Dialog {
    pub fn new(workspace: Entity<Workspace>, i18n: I18n, dialog_type: DialogType, cx: &mut Context<Self>) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        let mut input_text = String::new();
        if matches!(dialog_type, DialogType::SaveAs) {
            let ws = workspace.read(cx);
            if let Some(editor) = ws.active_editor() {
                if let Some(path) = &editor.path {
                    if let Some(name) = path.file_name() {
                        input_text = name.to_string_lossy().to_string();
                    }
                } else {
                    let ext = if editor.syntax_highlighter.as_ref().map(|h| h.def.meta.name.to_lowercase()).as_deref() == Some("markdown") {
                        ".md"
                    } else {
                        ".txt"
                    };
                    input_text = format!("untitled{}", ext);
                }
            }
        } else if let DialogType::Rename { target_path } = &dialog_type {
            if let Some(name) = target_path.file_name() {
                input_text = name.to_string_lossy().to_string();
            }
        }

        let is_update = matches!(dialog_type, DialogType::Update);

        let mut this = Self {
            workspace,
            i18n,
            dialog_type,
            input_text,
            focus_handle: cx.focus_handle(),
            current_dir,
            files: Vec::new(),
            selected_idx: 0,
            show_hidden: false,
            button_idx: 0,
            update_status: if is_update { Some(UpdateStatus::Checking) } else { None },
            settings_dropdown: None,
            plugin_tab: 0,
            registry_plugins: None,
            registry_loading: false,
            registry_error: None,
            plugin_show_add_repo: false,
            plugin_new_repo_url: String::new(),
        };
        this.refresh_files();
        if is_update {
            this.start_update_check(cx);
        }
        this
    }

    fn start_update_check(&mut self, cx: &mut Context<Self>) {
        self.update_status = Some(UpdateStatus::Checking);
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let res = std::thread::spawn(|| {
                    zee_core::selfupdate::check_latest(zee_core::selfupdate::AppType::Gui)
                }).join().unwrap_or_else(|_| Err(anyhow::anyhow!("Update check thread panicked")));

                let _ = this.update(&mut cx.clone(), |this, cx| {
                    match res {
                        Ok(info) => {
                            if zee_core::selfupdate::is_newer(zee_core::selfupdate::CURRENT_VERSION, &info.version) {
                                this.update_status = Some(UpdateStatus::Available {
                                    latest_version: info.version,
                                    asset_url: info.asset_url,
                                    html_url: info.html_url,
                                });
                            } else {
                                this.update_status = Some(UpdateStatus::UpToDate {
                                    version: info.version,
                                });
                            }
                        }
                        Err(err) => {
                            this.update_status = Some(UpdateStatus::Failed {
                                error: err.to_string(),
                                html_url: Some(format!("https://github.com/{}/releases", zee_core::selfupdate::GITHUB_REPO)),
                            });
                        }
                    }
                    cx.notify();
                });
            }
        }).detach();
    }

    fn start_apply_update(&mut self, asset_url: String, cx: &mut Context<Self>) {
        self.update_status = Some(UpdateStatus::Downloading);
        cx.notify();

        // Save session state so that tabs, cursor positions, root folder, and sidebar state are preserved after relaunch
        let session = {
            let ws = self.workspace.read(cx);
            let mut files = Vec::new();
            for editor in &ws.editors {
                let unsaved_content = if editor.is_modified() || editor.path.is_none() {
                    Some(editor.rope.to_string())
                } else {
                    None
                };
                files.push(zee_core::session::SessionEditorState {
                    path: editor.path.clone(),
                    cursor: editor.cursor,
                    scroll_row: editor.scroll_row,
                    is_modified: editor.is_modified(),
                    unsaved_content,
                });
            }
            let sidebar_tab = match ws.sidebar_tab {
                crate::workspace::SidebarTab::Files => "files".to_string(),
                crate::workspace::SidebarTab::Outline => "outline".to_string(),
            };
            zee_core::session::UpdateSession {
                files,
                active_index: ws.active_editor_index,
                root_folder: Some(ws.file_tree.root_path.clone()),
                expanded_folders: ws.file_tree.expanded_paths(),
                sidebar_visible: ws.sidebar_visible,
                sidebar_tab,
            }
        };
        let _ = session.save();

        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let url = asset_url.clone();
                let res = std::thread::spawn(move || {
                    zee_core::selfupdate::apply_update(&url, zee_core::selfupdate::AppType::Gui)
                }).join().unwrap_or_else(|_| Err(anyhow::anyhow!("Apply update thread panicked")));

                let _ = this.update(&mut cx.clone(), |this, cx| {
                    match res {
                        Ok(()) => {
                            this.update_status = Some(UpdateStatus::Success);
                            cx.notify();
                            cx.spawn(|_, cx: &mut AsyncApp| {
                                let cx = cx.clone();
                                async move {
                                    smol::Timer::after(std::time::Duration::from_millis(500)).await;
                                    cx.update(|cx| {
                                        cx.dispatch_action(&Quit {});
                                    });
                                    smol::Timer::after(std::time::Duration::from_millis(500)).await;
                                    std::process::exit(0);
                                }
                            }).detach();
                        }
                        Err(err) => {
                            zee_core::session::UpdateSession::clear();
                            this.update_status = Some(UpdateStatus::Failed {
                                error: err.to_string(),
                                html_url: Some(format!("https://github.com/{}/releases", zee_core::selfupdate::GITHUB_REPO)),
                            });
                            cx.notify();
                        }
                    }
                });
            }
        }).detach();
    }



    fn fetch_online_plugins(&mut self, cx: &mut Context<Self>) {
        if self.registry_loading {
            return;
        }
        self.registry_loading = true;
        self.registry_error = None;
        cx.notify();

        let sources = self.workspace.read(cx).config.plugin_registries.clone();
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let res = std::thread::spawn(move || {
                    zee_core::plugin::PluginManager::fetch_registries(&sources)
                }).join().unwrap_or_else(|_| Err(anyhow::anyhow!("Registry fetch thread panicked")));

                let _ = this.update(&mut cx.clone(), |this, cx| {
                    this.registry_loading = false;
                    match res {
                        Ok(plugins) => {
                            this.registry_plugins = Some(plugins);
                        }
                        Err(e) => {
                            this.registry_error = Some(e.to_string());
                        }
                    }
                    cx.notify();
                });
            }
        }).detach();
    }

    fn start_install_from_registry(&mut self, plugin_id: String, cx: &mut Context<Self>) {
        self.registry_loading = true;
        self.registry_error = None;
        cx.notify();

        let workspace = self.workspace.clone();
        let sources = self.workspace.read(cx).config.plugin_registries.clone();
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let id_clone = plugin_id.clone();
                let res = std::thread::spawn(move || {
                    zee_core::plugin::PluginManager::install_from_registry(&id_clone, None, Some(&sources))
                }).join().unwrap_or_else(|_| Err(anyhow::anyhow!("Plugin install thread panicked")));

                let _ = this.update(&mut cx.clone(), |this, cx| {
                    this.registry_loading = false;
                    if let Ok(()) = res {
                        workspace.update(cx, |w, cx| {
                            w.reload_plugins();
                            cx.notify();
                        });
                    } else if let Err(e) = res {
                        this.registry_error = Some(e.to_string());
                    }
                    cx.notify();
                });
            }
        }).detach();
    }

    fn refresh_files(&mut self) {
        self.files.clear();
        if let Ok(entries) = std::fs::read_dir(&self.current_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    self.files.push(FileEntry {
                        name,
                        is_dir: meta.is_dir(),
                        size: meta.len(),
                        modified: meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    });
                }
            }
        }
        self.files.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.cmp(&b.name)
            }
        });
        self.selected_idx = 0;
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
    }

    pub fn set_i18n(&mut self, i18n: I18n, cx: &mut Context<Self>) {
        self.i18n = i18n;
        cx.notify();
    }

    fn handle_keydown(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "tab" => {
                if matches!(self.dialog_type, DialogType::UnsavedChanges { .. }) {
                    if event.keystroke.modifiers.shift {
                        self.button_idx = (self.button_idx + 2) % 3; // 0->2, 1->0, 2->1
                    } else {
                        self.button_idx = (self.button_idx + 1) % 3;
                    }
                }
            }
            "left" => {
                if matches!(self.dialog_type, DialogType::UnsavedChanges { .. }) {
                    self.button_idx = self.button_idx.saturating_sub(1);
                }
            }
            "right" => {
                if matches!(self.dialog_type, DialogType::UnsavedChanges { .. }) {
                    self.button_idx = (self.button_idx + 1).min(2);
                }
            }
            "up" => {
                if matches!(self.dialog_type, DialogType::OpenFile | DialogType::SaveAs) {
                    self.selected_idx = self.selected_idx.saturating_sub(1);
                    if !self.files.is_empty() {
                        self.input_text = self.files[self.selected_idx].name.clone();
                    }
                }
            }
            "down"
                if matches!(self.dialog_type, DialogType::OpenFile | DialogType::SaveAs)
                    && !self.files.is_empty() => {
                        self.selected_idx = (self.selected_idx + 1).min(self.files.len() - 1);
                        self.input_text = self.files[self.selected_idx].name.clone();
                    }
            "v" if event.keystroke.modifiers.platform || event.keystroke.modifiers.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    if matches!(self.dialog_type, DialogType::PluginManager) && self.plugin_show_add_repo {
                        self.plugin_new_repo_url.push_str(text.trim());
                    } else {
                        self.input_text.push_str(text.trim());
                    }
                }
            }
            "backspace" => {
                if matches!(self.dialog_type, DialogType::PluginManager) && self.plugin_show_add_repo {
                    self.plugin_new_repo_url.pop();
                } else if matches!(self.dialog_type, DialogType::OpenFile | DialogType::SaveAs) && event.keystroke.modifiers.platform {
                    if let Some(parent) = self.current_dir.parent() {
                        self.current_dir = parent.to_path_buf();
                        self.refresh_files();
                    }
                } else {
                    self.input_text.pop();
                }
            }
            "enter" => {
                if matches!(self.dialog_type, DialogType::PluginManager) && self.plugin_show_add_repo {
                    let url = std::mem::take(&mut self.plugin_new_repo_url);
                    self.plugin_show_add_repo = false;
                    if !url.trim().is_empty() {
                        self.workspace.update(cx, |w, _| {
                            let _ = w.config.add_plugin_registry(&url);
                        });
                        self.fetch_online_plugins(cx);
                    }
                } else if let DialogType::UnsavedChanges { intent, .. } = &self.dialog_type {
                    match self.button_idx {
                        0 => {
                            cx.emit(DialogEvent::Save(*intent));
                            self.close(cx);
                        }
                        1 => {
                            cx.emit(DialogEvent::DontSave(*intent));
                            self.close(cx);
                        }
                        _ => self.close(cx),
                    }
                } else {
                    self.confirm(cx);
                }
            }
            "escape" => {
                if matches!(self.dialog_type, DialogType::PluginManager) && self.plugin_show_add_repo {
                    self.plugin_show_add_repo = false;
                    self.plugin_new_repo_url.clear();
                } else if self.settings_dropdown.is_some() {
                    self.settings_dropdown = None;
                } else {
                    self.close(cx);
                }
            }
            k if k.len() == 1 && !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control => {
                if matches!(self.dialog_type, DialogType::PluginManager) && self.plugin_show_add_repo {
                    self.plugin_new_repo_url.push_str(k);
                } else {
                    self.input_text.push_str(k);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        match &self.dialog_type {
            DialogType::GoToLine => {
                if let Ok(line) = self.input_text.parse::<usize>() {
                    self.workspace.update(cx, |w, _| {
                        if let Some(editor) = w.active_editor_mut() {
                            editor.cursor = editor.rope.line_to_char(line.saturating_sub(1));
                            editor.selection = None;
                        }
                    });
                }
            }
            DialogType::OpenFile | DialogType::SaveAs => {
                if !self.files.is_empty() && self.files[self.selected_idx].is_dir && self.input_text == self.files[self.selected_idx].name {
                    self.current_dir.push(&self.input_text);
                    self.input_text.clear();
                    self.refresh_files();
                    return;
                }

                let mut filename = self.input_text.trim().to_string();
                if matches!(self.dialog_type, DialogType::SaveAs) && !filename.is_empty() && !filename.contains('.') {
                    filename.push_str(".txt");
                }
                let path = self.current_dir.join(&filename);
                if matches!(self.dialog_type, DialogType::OpenFile) {
                    self.workspace.update(cx, |w, _| {
                        if let Ok(editor) = zee_core::buffer::Editor::from_file(&path) {
                            w.add_editor(editor);
                        }
                    });
                } else {
                    self.workspace.update(cx, |w, _| {
                        let _ = w.save_as_active_editor(&path);
                    });
                }
            }
            DialogType::NewFile { parent_dir } => {
                let filename = self.input_text.trim().to_string();
                if !filename.is_empty() {
                    cx.emit(DialogEvent::CreateFile {
                        parent_dir: parent_dir.clone(),
                        filename,
                    });
                }
            }
            DialogType::NewFolder { parent_dir } => {
                let folder_name = self.input_text.trim().to_string();
                if !folder_name.is_empty() {
                    cx.emit(DialogEvent::CreateFolder {
                        parent_dir: parent_dir.clone(),
                        folder_name,
                    });
                }
            }
            DialogType::Rename { target_path } => {
                let new_name = self.input_text.trim().to_string();
                if !new_name.is_empty() {
                    cx.emit(DialogEvent::RenameItem {
                        target_path: target_path.clone(),
                        new_name,
                    });
                }
            }
            DialogType::ConfirmDelete { target_path } => {
                cx.emit(DialogEvent::DeleteItem {
                    target_path: target_path.clone(),
                });
            }
            _ => {}
        }
        self.close(cx);
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(DialogEvent::Close);
    }
}

pub enum DialogEvent {
    Close,
    #[allow(dead_code)]
    Confirm,
    Save(UnsavedChangesIntent),
    DontSave(UnsavedChangesIntent),
    ExportConfig,
    ExportAll,
    ImportBackup,
    CreateFile { parent_dir: std::path::PathBuf, filename: String },
    CreateFolder { parent_dir: std::path::PathBuf, folder_name: String },
    RenameItem { target_path: std::path::PathBuf, new_name: String },
    DeleteItem { target_path: std::path::PathBuf },
}

impl EventEmitter<DialogEvent> for Dialog {}

impl Render for Dialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let bg = led_color_to_gpui(theme.ui.dialog_bg);
        let fg = led_color_to_gpui(theme.editor.foreground);
        let border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);

        let is_wide = matches!(self.dialog_type, DialogType::Settings | DialogType::PluginManager);
        let dialog_width = if is_wide { px(560.0) } else { px(460.0) };
        let dialog_max_h = if is_wide { px(640.0) } else { px(580.0) };

        div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000080)) // Dim backdrop overlay
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.settings_dropdown.is_some() {
                    this.settings_dropdown = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .w(dialog_width)
                    .max_h(dialog_max_h)
                    .bg(bg)
                    .text_color(fg)
                    .font_family(ui_font_family())
                    .border_1()
                    .border_color(border)
                    .rounded_xl()
                    .shadow_2xl()
                    .p_5()
                    .flex()
                    .flex_col()
                    .track_focus(&self.focus_handle)
                    .on_key_down(cx.listener(Self::handle_keydown))
                    .child(self.render_content(cx))
            )
    }
}

impl Dialog {
    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let fg = led_color_to_gpui(theme.editor.foreground);
        let accent = led_color_to_gpui(theme.syntax.keyword.unwrap_or(theme.editor.cursor));
        let button_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.1);
        let button_hover = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.2);
        let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
        let input_bg = led_color_to_gpui(theme.editor.background);

        match &self.dialog_type {
            DialogType::About => {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .py_2()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .child("zee")
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.7))
                            .child(format!("{} {}", self.i18n.get("about.version"), env!("CARGO_PKG_VERSION")))
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.6))
                            .child("Lightweight & fast text editor — MIT License")
                    )
                    .child(
                        div()
                            .mt_4()
                            .h(px(32.0))
                            .px_6()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(accent)
                            .text_color(gpui::rgb(0xffffff))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.9))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                            .child(self.i18n.get("dialog.ok").to_string())
                    )
            }
            DialogType::Update => {
                let status = self.update_status.as_ref().unwrap_or(&UpdateStatus::Checking);
                match status {
                    UpdateStatus::Checking => {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .py_4()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.update.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(with_alpha(fg, 0.7))
                                    .child(self.i18n.get("dialog.update.checking").to_string())
                            )
                    }
                    UpdateStatus::UpToDate { version } => {
                        let msg = self.i18n.get("dialog.update.up_to_date").replace("{version}", version);
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .py_3()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.update.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(with_alpha(fg, 0.8))
                                    .child(msg)
                            )
                            .child(
                                div()
                                    .mt_4()
                                    .h(px(30.0))
                                    .px_6()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    }
                    UpdateStatus::Available { latest_version, asset_url, html_url } => {
                        let msg = self.i18n.get("dialog.update.available").replace("{version}", latest_version);
                        let asset_url_clone = asset_url.clone();
                        let html_url_btn = html_url.clone();

                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .py_3()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.update.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(with_alpha(fg, 0.8))
                                    .child(msg)
                            )
                            .child(
                                div()
                                    .mt_4()
                                    .flex()
                                    .flex_row()
                                    .gap_3()
                                    .child(
                                        if let Some(url) = asset_url_clone {
                                            div()
                                                .h(px(30.0))
                                                .px_4()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_md()
                                                .bg(accent)
                                                .text_color(gpui::rgb(0xffffff))
                                                .text_size(px(13.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .cursor_pointer()
                                                .hover(|s| s.opacity(0.9))
                                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                    this.start_apply_update(url.clone(), cx);
                                                }))
                                                .child(self.i18n.get("dialog.update.btn_update").to_string())
                                        } else {
                                            let h_url = html_url.clone();
                                            div()
                                                .h(px(30.0))
                                                .px_4()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_md()
                                                .bg(accent)
                                                .text_color(gpui::rgb(0xffffff))
                                                .text_size(px(13.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .cursor_pointer()
                                                .hover(|s| s.opacity(0.9))
                                                .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, _, _| {
                                                    let _ = zee_core::selfupdate::open_url(&h_url);
                                                }))
                                                .child(self.i18n.get("dialog.update.btn_open_url").to_string())
                                        }
                                    )
                                    .child(
                                        div()
                                            .h(px(30.0))
                                            .px_4()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .bg(button_bg)
                                            .text_color(fg)
                                            .text_size(px(13.0))
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(button_hover))
                                            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, _, _| {
                                                let _ = zee_core::selfupdate::open_url(&html_url_btn);
                                            }))
                                            .child(self.i18n.get("dialog.update.btn_open_url").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(30.0))
                                            .px_4()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .bg(button_bg)
                                            .text_color(fg)
                                            .text_size(px(13.0))
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(button_hover))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                            .child(self.i18n.get("dialog.cancel").to_string())
                                    )
                            )
                    }
                    UpdateStatus::Downloading => {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .py_4()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.update.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(with_alpha(fg, 0.8))
                                    .child(self.i18n.get("dialog.update.downloading").to_string())
                            )
                    }
                    UpdateStatus::Success => {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .py_3()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.update.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(gpui::rgb(0x44cc44))
                                    .child(self.i18n.get("dialog.update.success").to_string())
                            )
                    }
                    UpdateStatus::Failed { error, html_url } => {
                        let err_msg = self.i18n.get("dialog.update.failed").replace("{error}", error);
                        let html_url_btn = html_url.clone();
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .py_3()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.update.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(gpui::rgb(0xff5555))
                                    .child(err_msg)
                            )
                            .child(
                                div()
                                    .mt_4()
                                    .flex()
                                    .flex_row()
                                    .gap_3()
                                    .child(
                                        if let Some(url) = html_url_btn {
                                            div()
                                                .h(px(30.0))
                                                .px_4()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_md()
                                                .bg(accent)
                                                .text_color(gpui::rgb(0xffffff))
                                                .text_size(px(13.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .cursor_pointer()
                                                .hover(|s| s.opacity(0.9))
                                                .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, _, _| {
                                                    let _ = zee_core::selfupdate::open_url(&url);
                                                }))
                                                .child(self.i18n.get("dialog.update.btn_open_url").to_string())
                                        } else {
                                            div()
                                        }
                                    )
                                    .child(
                                        div()
                                            .h(px(30.0))
                                            .px_4()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .bg(button_bg)
                                            .text_color(fg)
                                            .text_size(px(13.0))
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(button_hover))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                            .child(self.i18n.get("dialog.ok").to_string())
                                    )
                            )
                    }
                }
            }
            DialogType::GoToLine => {
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.i18n.get("dialog.go_to_line").to_string())
                    )
                    .child(
                        div()
                            .h(px(34.0))
                            .bg(input_bg)
                            .border_1()
                            .border_color(border_color)
                            .rounded_md()
                            .px_3()
                            .flex()
                            .items_center()
                            .child(
                                if self.input_text.is_empty() {
                                    div()
                                        .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.45))
                                        .text_size(px(13.0))
                                        .child("Enter line number...")
                                } else {
                                    div()
                                        .text_size(px(13.0))
                                        .child(self.input_text.clone())
                                }
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.confirm(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::OpenFile | DialogType::SaveAs => {
                let title = if matches!(self.dialog_type, DialogType::OpenFile) {
                    self.i18n.get("dialog.open_file")
                } else {
                    self.i18n.get("dialog.save_as")
                };

                div()
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string())
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.6))
                            .child(self.current_dir.to_string_lossy().into_owned())
                    )
                    .child(
                        div()
                            .flex_grow()
                            .min_h(px(200.0))
                            .max_h(px(260.0))
                            .overflow_hidden()
                            .bg(input_bg)
                            .border_1()
                            .border_color(border_color)
                            .rounded_md()
                            .children(self.files.iter().enumerate().map(|(idx, entry)| {
                                let is_selected = idx == self.selected_idx;
                                div()
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .text_size(px(12.0))
                                    .rounded_sm()
                                    .bg(if is_selected { led_color_to_gpui(theme.editor.selection) } else { hsla(0.,0.,0.,0.).into() })
                                    .child(div().child(format!("{}{}", entry.name, if entry.is_dir { "/" } else { "" })))
                                    .child(div().text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.5)).child(if entry.is_dir { "--".to_string() } else { self.format_size(entry.size) }))
                            }))
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .mt_1()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .child(self.i18n.get("dialog.file_browser.filename").to_string())
                            )
                            .child(
                                div()
                                    .flex_grow()
                                    .h(px(28.0))
                                    .bg(input_bg)
                                    .border_1()
                                    .border_color(border_color)
                                    .rounded_md()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.0))
                                    .child(self.input_text.clone())
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.confirm(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::UnsavedChanges { filename, intent: _ } => {
                let button_idx = self.button_idx;
                div()
                    .flex()
                    .flex_col()
                    .gap_3p5()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.i18n.get("dialog.unsaved_changes_title").to_string())
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(with_alpha(led_color_to_gpui(theme.editor.foreground), 0.8))
                            .child(self.i18n.get("dialog.unsaved_changes").replace("{filename}", filename))
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_3()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_3p5()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(if button_idx == 0 { accent } else { button_bg })
                                    .text_color(if button_idx == 0 { gpui::rgb(0xffffff) } else { fg })
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .border_2()
                                    .border_color(if button_idx == 0 { gpui::rgb(0xffffff) } else { hsla(0.,0.,0.,0.).into() })
                                    .hover(move |s| if button_idx != 0 { s.bg(button_hover) } else { s })
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        if let DialogType::UnsavedChanges { intent, .. } = &this.dialog_type {
                                            cx.emit(DialogEvent::Save(*intent));
                                        }
                                        this.close(cx);
                                    }))
                                    .child(self.i18n.get("dialog.save").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_3p5()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(if button_idx == 1 { gpui::rgb(0xd32f2f) } else { button_bg })
                                    .text_color(if button_idx == 1 { gpui::rgb(0xffffff) } else { fg })
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .border_2()
                                    .border_color(if button_idx == 1 { gpui::rgb(0xffffff) } else { hsla(0.,0.,0.,0.).into() })
                                    .hover(move |s| if button_idx != 1 { s.bg(button_hover) } else { s })
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        if let DialogType::UnsavedChanges { intent, .. } = &this.dialog_type {
                                            cx.emit(DialogEvent::DontSave(*intent));
                                        }
                                        this.close(cx);
                                    }))
                                    .child(self.i18n.get("dialog.dont_save").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_3p5()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_color(fg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .border_2()
                                    .border_color(if button_idx == 2 { gpui::rgb(0xffffff) } else { hsla(0.,0.,0.,0.).into() })
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                    )
            }
            DialogType::Message { title, message } => {
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.clone())
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .child(message.clone())
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::Settings => {
                let themes = zee_core::theme::Theme::load_all();
                let current_theme_name = workspace.theme.meta.name.clone();
                let is_theme_open = self.settings_dropdown == Some(SettingsDropdown::Theme);
                let is_font_open = self.settings_dropdown == Some(SettingsDropdown::Font);
                let is_language_open = self.settings_dropdown == Some(SettingsDropdown::Language);

                let current_lang_id = if workspace.config.language.is_empty() {
                    "auto"
                } else {
                    workspace.config.language.as_str()
                };
                let current_lang_name = if current_lang_id == "auto" {
                    self.i18n.get("dialog.settings.language_auto").to_string()
                } else {
                    zee_core::i18n::AVAILABLE_LANGUAGES
                        .iter()
                        .find(|l| l.id == current_lang_id)
                        .map(|l| l.name.to_string())
                        .unwrap_or_else(|| current_lang_id.to_string())
                };

                let sys_default_font = self.i18n.get("dialog.settings.font_system_default").to_string();
                let current_font_label = match &workspace.config.font_family {
                    Some(f) if !f.is_empty() => f.clone(),
                    _ => sys_default_font.clone(),
                };

                let mut font_options: Vec<(Option<String>, String)> = vec![
                    (None, sys_default_font),
                    (Some("Menlo".to_string()), "Menlo".to_string()),
                    (Some("SF Mono".to_string()), "SF Mono".to_string()),
                    (Some("Monaco".to_string()), "Monaco".to_string()),
                    (Some("Fira Code".to_string()), "Fira Code".to_string()),
                    (Some("JetBrains Mono".to_string()), "JetBrains Mono".to_string()),
                    (Some("Cascadia Code".to_string()), "Cascadia Code".to_string()),
                    (Some("Consolas".to_string()), "Consolas".to_string()),
                    (Some("Courier New".to_string()), "Courier New".to_string()),
                    (Some("Inconsolata".to_string()), "Inconsolata".to_string()),
                    (Some("Source Code Pro".to_string()), "Source Code Pro".to_string()),
                    (Some("Hack".to_string()), "Hack".to_string()),
                    (Some("Ubuntu Mono".to_string()), "Ubuntu Mono".to_string()),
                ];
                if let Some(ref cf) = workspace.config.font_family {
                    if !cf.is_empty() && !font_options.iter().any(|(opt, _)| opt.as_deref() == Some(cf.as_str())) {
                        font_options.push((Some(cf.clone()), format!("Custom ({})", cf)));
                    }
                }

                let font_size = workspace.config.font_size;
                let line_height = workspace.config.line_height;
                let ui_font_size = workspace.config.ui_font_size;
                let tab_size = workspace.config.tab_size;
                let expand_tab = workspace.config.expand_tab;
                let line_numbers = workspace.config.line_numbers;
                let word_wrap = workspace.config.word_wrap;
                let sidebar_position = workspace.config.sidebar_position.clone();

                let chip_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.08);
                let chip_active_bg = with_alpha(accent, 0.25);
                let chip_border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
                let dropdown_bg = led_color_to_gpui(theme.ui.panel_bg);

                div()
                    .flex()
                    .flex_col()
                    .gap_3p5()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        if this.settings_dropdown.is_some() {
                            this.settings_dropdown = None;
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.settings.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(with_alpha(fg, 0.6))
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child("✕")
                            )
                    )
                    // Theme dropdown section
                    .child(
                        div()
                            .relative()
                            
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(with_alpha(fg, 0.75))
                                    .child(self.i18n.get("dialog.settings.theme").to_string())
                            )
                            // Anchor container for trigger and floating popup
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .child(
                                        // Dropdown trigger button
                                        div()
                                            .h(px(32.0))
                                            .w_full()
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if is_theme_open { accent } else { chip_border })
                                            .bg(input_bg)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.9))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.settings_dropdown = if this.settings_dropdown == Some(SettingsDropdown::Theme) {
                                                    None
                                                } else {
                                                    Some(SettingsDropdown::Theme)
                                                };
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(current_theme_name.clone())
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(with_alpha(fg, 0.6))
                                                    .child(if is_theme_open { "▲" } else { "▼" })
                                            )
                                    )
                                    // Floating dropdown popup
                                    .children(if is_theme_open {
                                        Some(
                                            deferred(
                                                div()
                                                    .id("settings-theme-dropdown-list")
                                                    .occlude()
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
                                                    .absolute()
                                                    .top(px(36.0))
                                                    .left_0()
                                                    .right_0()
                                                    .w_full()
                                                    .max_h(px(200.0))
                                                    .overflow_y_scroll()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(accent)
                                                    .bg(dropdown_bg)
                                                    .shadow_2xl()
                                                    .p_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_0p5()
                                                    .children(themes.into_iter().map(|t| {
                                                        let is_active = t.meta.name == current_theme_name;
                                                        let t_name = t.meta.name.clone();
                                                        let t_slug = t_name.to_lowercase().replace(' ', "-");
                                                        
                                                        div()
                                                            .h(px(28.0))
                                                            .px_2p5()
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .rounded_sm()
                                                            .bg(if is_active { with_alpha(accent, 0.22) } else { hsla(0.,0.,0.,0.).into() })
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(with_alpha(fg, 0.12)))
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                                cx.stop_propagation();
                                                                this.workspace.update(cx, |w, cx| {
                                                                    if let Some(theme) = zee_core::theme::Theme::find_by_name(&t_slug) {
                                                                        w.theme = theme;
                                                                        let _ = zee_core::config::Config::write_key("theme", &t_slug);
                                                                        cx.notify();
                                                                    }
                                                                });
                                                                this.settings_dropdown = None;
                                                                cx.notify();
                                                            }))
                                                            .child(
                                                                div()
                                                                    .text_size(px(12.0))
                                                                    .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                                                    .child(t_name)
                                                                )
                                                            .child(
                                                                div()
                                                                    .text_size(px(11.0))
                                                                    .text_color(accent)
                                                                    .child(if is_active { "✓" } else { "" })
                                                            )
                                                    }))
                                            ).priority(100)
                                        )
                                    } else {
                                        None
                                    })
                            )
                    )
                    // Language dropdown section
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(with_alpha(fg, 0.75))
                                    .child(self.i18n.get("dialog.settings.language").to_string())
                            )
                            // Anchor container for trigger and floating popup
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .child(
                                        // Dropdown trigger button
                                        div()
                                            .h(px(32.0))
                                            .w_full()
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if is_language_open { accent } else { chip_border })
                                            .bg(input_bg)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.9))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.settings_dropdown = if this.settings_dropdown == Some(SettingsDropdown::Language) {
                                                    None
                                                } else {
                                                    Some(SettingsDropdown::Language)
                                                };
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(current_lang_name.clone())
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(with_alpha(fg, 0.6))
                                                    .child(if is_language_open { "▲" } else { "▼" })
                                            )
                                    )
                                    // Floating dropdown popup
                                    .children(if is_language_open {
                                        Some(
                                            deferred(
                                                div()
                                                    .id("settings-language-dropdown-list")
                                                    .occlude()
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
                                                    .absolute()
                                                    .top(px(36.0))
                                                    .left_0()
                                                    .right_0()
                                                    .w_full()
                                                    .max_h(px(200.0))
                                                    .overflow_y_scroll()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(accent)
                                                    .bg(dropdown_bg)
                                                    .shadow_2xl()
                                                    .p_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_0p5()
                                                    .children(zee_core::i18n::AVAILABLE_LANGUAGES.iter().map(|lang| {
                                                        let is_active = if lang.id == "auto" {
                                                            current_lang_id == "auto"
                                                        } else {
                                                            current_lang_id == lang.id
                                                        };
                                                        let label = if lang.id == "auto" {
                                                            self.i18n.get("dialog.settings.language_auto").to_string()
                                                        } else {
                                                            lang.name.to_string()
                                                        };
                                                        let l_id = lang.id.to_string();

                                                        div()
                                                            .h(px(28.0))
                                                            .px_2p5()
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .rounded_sm()
                                                            .bg(if is_active { with_alpha(accent, 0.22) } else { hsla(0.,0.,0.,0.).into() })
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(with_alpha(fg, 0.12)))
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                                cx.stop_propagation();
                                                                let l_id_clone = l_id.clone();
                                                                this.workspace.update(cx, |w, cx| {
                                                                    w.config.language = l_id_clone.clone();
                                                                    let _ = zee_core::config::Config::write_key("language", &l_id_clone);
                                                                    cx.notify();
                                                                });
                                                                this.i18n = zee_core::i18n::I18n::load(&l_id);
                                                                this.settings_dropdown = None;
                                                                cx.notify();
                                                            }))
                                                            .child(
                                                                div()
                                                                    .text_size(px(12.0))
                                                                    .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                                                    .child(label)
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(11.0))
                                                                    .text_color(accent)
                                                                    .child(if is_active { "✓" } else { "" })
                                                            )
                                                    }))
                                            ).priority(100)
                                        )
                                    } else {
                                        None
                                    })
                            )
                    )
                    // Font Family dropdown section
                    .child(
                        div()
                            .relative()
                            
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(with_alpha(fg, 0.75))
                                    .child(self.i18n.get("dialog.settings.font_family").to_string())
                            )
                            // Anchor container for trigger and floating popup
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .child(
                                        // Dropdown trigger button
                                        div()
                                            .h(px(32.0))
                                            .w_full()
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if is_font_open { accent } else { chip_border })
                                            .bg(input_bg)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.9))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.settings_dropdown = if this.settings_dropdown == Some(SettingsDropdown::Font) {
                                                    None
                                                } else {
                                                    Some(SettingsDropdown::Font)
                                                };
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(current_font_label)
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(with_alpha(fg, 0.6))
                                                    .child(if is_font_open { "▲" } else { "▼" })
                                            )
                                    )
                                    // Floating dropdown popup
                                    .children(if is_font_open {
                                        Some(
                                            deferred(
                                                div()
                                                    .id("settings-font-dropdown-list")
                                                    .occlude()
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
                                                    .absolute()
                                                    .top(px(36.0))
                                                    .left_0()
                                                    .right_0()
                                                    .w_full()
                                                    .max_h(px(200.0))
                                                    .overflow_y_scroll()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(accent)
                                                    .bg(dropdown_bg)
                                                    .shadow_2xl()
                                                    .p_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_0p5()
                                                    .children(font_options.into_iter().map(|(font_opt, label)| {
                                                        let is_active = match (&font_opt, &workspace.config.font_family) {
                                                            (None, None) => true,
                                                            (Some(a), Some(b)) => a == b,
                                                            _ => false,
                                                        };
                                                        let font_val = font_opt.clone();
                                                        
                                                        div()
                                                            .h(px(28.0))
                                                            .px_2p5()
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .rounded_sm()
                                                            .bg(if is_active { with_alpha(accent, 0.22) } else { hsla(0.,0.,0.,0.).into() })
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(with_alpha(fg, 0.12)))
                                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                                cx.stop_propagation();
                                                                this.workspace.update(cx, |w, cx| {
                                                                    w.config.font_family = font_val.clone();
                                                                    let _ = zee_core::config::Config::write_key("font_family", font_val.as_deref().unwrap_or(""));
                                                                    cx.notify();
                                                                });
                                                                this.settings_dropdown = None;
                                                                cx.notify();
                                                            }))
                                                            .child(
                                                                div()
                                                                    .text_size(px(12.0))
                                                                    .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                                                    .child(label)
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(11.0))
                                                                    .text_color(accent)
                                                                    .child(if is_active { "✓" } else { "" })
                                                            )
                                                    }))
                                            ).priority(100)
                                        )
                                    } else {
                                        None
                                    })
                            )
                    )
                    // Steppers (Font Size, Line Height, UI Size)
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            // Font Size
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(with_alpha(fg, 0.75))
                                            .child(self.i18n.get("dialog.settings.font_size").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(30.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded_md()
                                            .bg(input_bg)
                                            .border_1()
                                            .border_color(chip_border)
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.8))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        this.workspace.update(cx, |w, cx| {
                                                            let size = (w.config.font_size - 1.0).max(8.0);
                                                            w.config.font_size = size;
                                                            w.config.line_height = (size * 1.55).round();
                                                            let _ = zee_core::config::Config::write_key("font_size", &format!("{:.1}", size));
                                                            let _ = zee_core::config::Config::write_key("line_height", &format!("{:.1}", w.config.line_height));
                                                            cx.notify();
                                                        });
                                                        cx.notify();
                                                    }))
                                                    .child("-")
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("{:.1} px", font_size))
                                            )
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.8))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        this.workspace.update(cx, |w, cx| {
                                                            let size = (w.config.font_size + 1.0).min(48.0);
                                                            w.config.font_size = size;
                                                            w.config.line_height = (size * 1.55).round();
                                                            let _ = zee_core::config::Config::write_key("font_size", &format!("{:.1}", size));
                                                            let _ = zee_core::config::Config::write_key("line_height", &format!("{:.1}", w.config.line_height));
                                                            cx.notify();
                                                        });
                                                        cx.notify();
                                                    }))
                                                    .child("+")
                                            )
                                    )
                            )
                            // Line Height
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(with_alpha(fg, 0.75))
                                            .child(self.i18n.get("dialog.settings.line_height").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(30.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded_md()
                                            .bg(input_bg)
                                            .border_1()
                                            .border_color(chip_border)
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.8))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        this.workspace.update(cx, |w, cx| {
                                                            let lh = (w.config.line_height - 1.0).max(12.0);
                                                            w.config.line_height = lh;
                                                            let _ = zee_core::config::Config::write_key("line_height", &format!("{:.1}", lh));
                                                            cx.notify();
                                                        });
                                                        cx.notify();
                                                    }))
                                                    .child("-")
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("{:.1} px", line_height))
                                            )
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.8))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        this.workspace.update(cx, |w, cx| {
                                                            let lh = (w.config.line_height + 1.0).min(72.0);
                                                            w.config.line_height = lh;
                                                            let _ = zee_core::config::Config::write_key("line_height", &format!("{:.1}", lh));
                                                            cx.notify();
                                                        });
                                                        cx.notify();
                                                    }))
                                                    .child("+")
                                            )
                                    )
                            )
                            // UI Size
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(with_alpha(fg, 0.75))
                                            .child(self.i18n.get("dialog.settings.ui_font_size").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(30.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .rounded_md()
                                            .bg(input_bg)
                                            .border_1()
                                            .border_color(chip_border)
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.8))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        this.workspace.update(cx, |w, cx| {
                                                            let size = (w.config.ui_font_size - 1.0).max(10.0);
                                                            w.config.ui_font_size = size;
                                                            let _ = zee_core::config::Config::write_key("ui_font_size", &format!("{:.1}", size));
                                                            cx.notify();
                                                        });
                                                        cx.notify();
                                                    }))
                                                    .child("-")
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("{:.1} px", ui_font_size))
                                            )
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.8))
                                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        this.workspace.update(cx, |w, cx| {
                                                            let size = (w.config.ui_font_size + 1.0).min(24.0);
                                                            w.config.ui_font_size = size;
                                                            let _ = zee_core::config::Config::write_key("ui_font_size", &format!("{:.1}", size));
                                                            cx.notify();
                                                        });
                                                        cx.notify();
                                                    }))
                                                    .child("+")
                                            )
                                    )
                            )
                    )
                    // Tab Width & Toggles Row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pt_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(with_alpha(fg, 0.75))
                                            .child(format!("{}:", self.i18n.get("dialog.settings.tab_size")))
                                    )
                                    .children(vec![2, 4, 8].into_iter().map(|ts| {
                                        let is_active = tab_size == ts;
                                        let bg_c = if is_active { chip_active_bg } else { chip_bg };
                                        let border_c = if is_active { accent } else { chip_border };
                                        div()
                                            .h(px(24.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(border_c)
                                            .bg(bg_c)
                                            .text_size(px(11.5))
                                            .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                this.workspace.update(cx, |w, cx| {
                                                    w.config.tab_size = ts;
                                                    let _ = zee_core::config::Config::write_key("tab_size", &ts.to_string());
                                                    cx.notify();
                                                });
                                                cx.notify();
                                            }))
                                            .child(ts.to_string())
                                    }))
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    // Expand tab toggle
                                    .child(
                                        div()
                                            .h(px(24.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if expand_tab { accent } else { chip_border })
                                            .bg(if expand_tab { chip_active_bg } else { chip_bg })
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                this.workspace.update(cx, |w, cx| {
                                                    w.config.expand_tab = !w.config.expand_tab;
                                                    let _ = zee_core::config::Config::write_key("expand_tab", if w.config.expand_tab { "true" } else { "false" });
                                                    cx.notify();
                                                });
                                                cx.notify();
                                            }))
                                            .child(if expand_tab { "✓ Spaces" } else { "Tabs" })
                                    )
                                    // Line Numbers toggle
                                    .child(
                                        div()
                                            .h(px(24.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if line_numbers { accent } else { chip_border })
                                            .bg(if line_numbers { chip_active_bg } else { chip_bg })
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                this.workspace.update(cx, |w, cx| {
                                                    w.config.line_numbers = !w.config.line_numbers;
                                                    let _ = zee_core::config::Config::write_key("line_numbers", if w.config.line_numbers { "true" } else { "false" });
                                                    cx.notify();
                                                });
                                                cx.notify();
                                            }))
                                            .child(if line_numbers { "✓ Lines" } else { "Lines" })
                                    )
                                     // Word Wrap toggle
                                    .child(
                                        div()
                                            .h(px(24.0))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if word_wrap { accent } else { chip_border })
                                            .bg(if word_wrap { chip_active_bg } else { chip_bg })
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                this.workspace.update(cx, |w, cx| {
                                                    w.config.word_wrap = !w.config.word_wrap;
                                                    let _ = zee_core::config::Config::write_key("word_wrap", if w.config.word_wrap { "true" } else { "false" });
                                                    cx.notify();
                                                });
                                                cx.notify();
                                            }))
                                            .child(if word_wrap { "✓ Wrap" } else { "Wrap" })
                                     )
                             )
                     )
                     // Sidebar Position Row
                     .child(
                         div()
                             .flex()
                             .items_center()
                             .justify_between()
                             .pt_0p5()
                             .child(
                                 div()
                                     .text_size(px(11.5))
                                     .font_weight(FontWeight::MEDIUM)
                                     .text_color(with_alpha(fg, 0.75))
                                     .child(format!("{}:", self.i18n.get("dialog.settings.sidebar_position")))
                             )
                             .child(
                                 div()
                                     .flex()
                                     .items_center()
                                     .gap_1p5()
                                     .child({
                                         let is_left = sidebar_position == "left";
                                         div()
                                             .h(px(24.0))
                                             .px_3()
                                             .flex()
                                             .items_center()
                                             .justify_center()
                                             .rounded_md()
                                             .border_1()
                                             .border_color(if is_left { accent } else { chip_border })
                                             .bg(if is_left { chip_active_bg } else { chip_bg })
                                             .text_size(px(11.5))
                                             .font_weight(if is_left { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                             .cursor_pointer()
                                             .hover(|s| s.opacity(0.85))
                                             .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                 this.workspace.update(cx, |w, cx| {
                                                     w.config.sidebar_position = "left".to_string();
                                                     let _ = zee_core::config::Config::write_key("sidebar_position", "left");
                                                     cx.notify();
                                                 });
                                                 cx.notify();
                                             }))
                                             .child(self.i18n.get("dialog.settings.sidebar_left").to_string())
                                     })
                                     .child({
                                         let is_right = sidebar_position != "left";
                                         div()
                                             .h(px(24.0))
                                             .px_3()
                                             .flex()
                                             .items_center()
                                             .justify_center()
                                             .rounded_md()
                                             .border_1()
                                             .border_color(if is_right { accent } else { chip_border })
                                             .bg(if is_right { chip_active_bg } else { chip_bg })
                                             .text_size(px(11.5))
                                             .font_weight(if is_right { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                             .cursor_pointer()
                                             .hover(|s| s.opacity(0.85))
                                             .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                 this.workspace.update(cx, |w, cx| {
                                                     w.config.sidebar_position = "right".to_string();
                                                     let _ = zee_core::config::Config::write_key("sidebar_position", "right");
                                                     cx.notify();
                                                 });
                                                 cx.notify();
                                             }))
                                             .child(self.i18n.get("dialog.settings.sidebar_right").to_string())
                                     })
                             )
                     )
                    // Backup & Restore Section
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .pt_1()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(with_alpha(fg, 0.75))
                                    .child(self.i18n.get("dialog.settings.backup_section").to_string())
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h(px(26.0))
                                            .px_2p5()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(chip_border)
                                            .bg(chip_bg)
                                            .text_size(px(11.5))
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                                                cx.emit(DialogEvent::ExportConfig);
                                            }))
                                            .child(self.i18n.get("dialog.settings.export_config_only").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(26.0))
                                            .px_2p5()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(chip_border)
                                            .bg(chip_bg)
                                            .text_size(px(11.5))
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                                                cx.emit(DialogEvent::ExportAll);
                                            }))
                                            .child(self.i18n.get("dialog.settings.export_all").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(26.0))
                                            .px_2p5()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(accent)
                                            .bg(chip_active_bg)
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                                                cx.emit(DialogEvent::ImportBackup);
                                            }))
                                            .child(self.i18n.get("dialog.settings.import_backup").to_string())
                                    )
                            )
                    )
                    // Footer Actions
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pt_2()
                            .border_t_1()
                            .border_color(chip_border)
                            .child(
                                div()
                                    .h(px(28.0))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.0))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.settings_dropdown = None;
                                        this.workspace.update(cx, |w, cx| {
                                            w.config.font_family = None;
                                            w.config.font_size = 12.0;
                                            w.config.line_height = 19.0;
                                            w.config.ui_font_size = 13.0;
                                            w.config.tab_size = 4;
                                            w.config.expand_tab = true;
                                            w.config.sidebar_position = "right".to_string();
                                            w.config.language = "auto".to_string();
                                            let _ = zee_core::config::Config::write_key("language", "auto");
                                            let _ = zee_core::config::Config::write_key("font_family", "");
                                            let _ = zee_core::config::Config::write_key("font_size", "12.0");
                                            let _ = zee_core::config::Config::write_key("line_height", "19.0");
                                            let _ = zee_core::config::Config::write_key("ui_font_size", "13.0");
                                            let _ = zee_core::config::Config::write_key("tab_size", "4");
                                            let _ = zee_core::config::Config::write_key("expand_tab", "true");
                                            let _ = zee_core::config::Config::write_key("sidebar_position", "right");
                                            cx.notify();
                                        });
                                        cx.notify();
                                    }))
                                    .child(self.i18n.get("dialog.settings.reset_defaults").to_string())
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .px_5()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::PluginManager => {
                let plugins = workspace.plugin_manager.all_manifests();
                let configured_repos = workspace.config.plugin_registries.clone();
                let accent = led_color_to_gpui(theme.syntax.keyword.unwrap_or(theme.ui.menu_item_active_fg));
                let chip_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.08);
                let chip_border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
                let button_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.12);
                let button_hover = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.22);
                let input_bg = with_alpha(led_color_to_gpui(theme.editor.background), 0.7);

                div()
                    .flex()
                    .flex_col()
                    .gap_3p5()
                    .child(
                        // Header
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.i18n.get("dialog.plugin.title").to_string())
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(with_alpha(fg, 0.6))
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child("✕")
                            )
                    )
                    .child(
                        // Tabs row (Installed vs Online Registry)
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .border_b_1()
                            .border_color(chip_border)
                            .pb_1()
                            .child(
                                div()
                                    .pb_1()
                                    .text_size(px(13.0))
                                    .font_weight(if self.plugin_tab == 0 { FontWeight::BOLD } else { FontWeight::NORMAL })
                                    .text_color(if self.plugin_tab == 0 { accent } else { with_alpha(fg, 0.6) })
                                    .border_b_2()
                                    .border_color(if self.plugin_tab == 0 { accent } else { gpui::rgba(0x00000000) })
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.plugin_tab = 0;
                                        cx.notify();
                                    }))
                                    .child(self.i18n.get("dialog.plugin.tab_installed").to_string())
                            )
                            .child(
                                div()
                                    .pb_1()
                                    .text_size(px(13.0))
                                    .font_weight(if self.plugin_tab == 1 { FontWeight::BOLD } else { FontWeight::NORMAL })
                                    .text_color(if self.plugin_tab == 1 { accent } else { with_alpha(fg, 0.6) })
                                    .border_b_2()
                                    .border_color(if self.plugin_tab == 1 { accent } else { gpui::rgba(0x00000000) })
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.plugin_tab = 1;
                                        if this.registry_plugins.is_none() {
                                            this.fetch_online_plugins(cx);
                                        }
                                        cx.notify();
                                    }))
                                    .child(self.i18n.get("dialog.plugin.tab_registry").to_string())
                            )
                    )
                    .child(if self.plugin_tab == 0 {
                        // TAB 0: Installed Plugins
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                // Action buttons bar
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h(px(28.0))
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .bg(accent)
                                            .text_color(gpui::rgb(0xffffff))
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.9))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                let workspace = this.workspace.clone();
                                                cx.spawn(|_, cx: &mut AsyncApp| {
                                                    let cx = cx.clone();
                                                    async move {
                                                        let file = rfd::AsyncFileDialog::new()
                                                            .add_filter("Plugins (*.wasm, *.lua, *.zip)", &["wasm", "lua", "zip"])
                                                            .pick_file()
                                                            .await;

                                                        if let Some(file) = file {
                                                            let path = file.path().to_path_buf();
                                                            let _ = zee_core::plugin::PluginManager::install_plugin_from_path(&path);
                                                            cx.update(|cx| {
                                                                workspace.update(cx, |w, cx| {
                                                                    w.reload_plugins();
                                                                    cx.notify();
                                                                });
                                                            });
                                                        }
                                                    }
                                                }).detach();
                                            }))
                                            .child(self.i18n.get("dialog.plugin.install_local").to_string())
                                    )
                                    .child(
                                        div()
                                            .h(px(28.0))
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .bg(button_bg)
                                            .text_size(px(12.0))
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(button_hover))
                                            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {
                                                if let Some(dir) = zee_core::plugin::PluginManager::plugins_dir() {
                                                    let _ = std::fs::create_dir_all(&dir);
                                                    let _ = zee_core::selfupdate::open_url(&dir.to_string_lossy());
                                                }
                                            }))
                                            .child(self.i18n.get("dialog.plugin.open_dir").to_string())
                                    )
                            )
                            // Plugins list
                            .child(
                                div()
                                    .flex_grow()
                                    .min_h(px(220.0))
                                    .max_h(px(340.0))
                                    .overflow_hidden()
                                    .p_2()
                                    .bg(input_bg)
                                    .border_1()
                                    .border_color(chip_border)
                                    .rounded_md()
                                    .child(if plugins.is_empty() {
                                        div()
                                            .h_full()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.5))
                                            .text_color(with_alpha(fg, 0.5))
                                            .child(self.i18n.get("dialog.plugin.empty").to_string())
                                            .into_any_element()
                                    } else {
                                        let uninstall_label = self.i18n.get("dialog.plugin.uninstall").to_string();
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .w_full()
                                            .children(plugins.into_iter().map(|manifest| {
                                                let plugin_id = manifest.id.clone();
                                                let has_commands = !manifest.capabilities.commands.is_empty();
                                                let has_outline = manifest.capabilities.outline_provider;
                                                let type_label = match manifest.plugin_type {
                                                    zee_core::plugin::PluginType::Lua => "Lua",
                                                    zee_core::plugin::PluginType::Wasm => "WASM",
                                                };
                                                let u_label = uninstall_label.clone();
                                                
                                                div()
                                                    .p_2p5()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(chip_border)
                                                    .bg(chip_bg)
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .child(
                                                                div()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap_2()
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(13.0))
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .child(manifest.name)
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .px_1p5()
                                                                            .py_0p5()
                                                                            .rounded_xs()
                                                                            .bg(with_alpha(accent, 0.15))
                                                                            .text_color(accent)
                                                                            .text_size(px(10.0))
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .child(type_label)
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(11.0))
                                                                            .text_color(with_alpha(fg, 0.55))
                                                                            .child(format!("v{}", manifest.version))
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .px_1p5()
                                                                            .py_0p5()
                                                                            .rounded_sm()
                                                                            .bg(with_alpha(gpui::rgb(0x4caf50), 0.2))
                                                                            .text_color(gpui::rgb(0x4caf50))
                                                                            .text_size(px(10.0))
                                                                            .font_weight(FontWeight::MEDIUM)
                                                                            .child("✓ Active")
                                                                    )
                                                            )
                                                            .child(
                                                                div()
                                                                    .px_2()
                                                                    .py_0p5()
                                                                    .rounded_sm()
                                                                    .bg(with_alpha(gpui::rgb(0xe53935), 0.15))
                                                                    .text_color(gpui::rgb(0xe53935))
                                                                    .text_size(px(11.0))
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.opacity(0.8))
                                                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                                        let id = plugin_id.clone();
                                                                        let _ = zee_core::plugin::PluginManager::uninstall_plugin_by_id(&id);
                                                                        this.workspace.update(cx, |w, cx| {
                                                                            w.reload_plugins();
                                                                            cx.notify();
                                                                        });
                                                                        cx.notify();
                                                                    }))
                                                                    .child(u_label)
                                                            )
                                                    )
                                                    .children(manifest.description.map(|desc| {
                                                        div()
                                                            .text_size(px(11.5))
                                                            .text_color(with_alpha(fg, 0.75))
                                                            .child(desc)
                                                    }))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_1p5()
                                                            .pt_0p5()
                                                            .children(if has_outline {
                                                                Some(div()
                                                                    .px_1p5()
                                                                    .rounded_sm()
                                                                    .bg(with_alpha(accent, 0.15))
                                                                    .text_color(accent)
                                                                    .text_size(px(10.5))
                                                                    .child("Outline Provider"))
                                                            } else {
                                                                None
                                                            })
                                                            .children(if has_commands {
                                                                Some(div()
                                                                    .px_1p5()
                                                                    .rounded_sm()
                                                                    .bg(with_alpha(fg, 0.12))
                                                                    .text_color(with_alpha(fg, 0.8))
                                                                    .text_size(px(10.5))
                                                                    .child(format!("Commands: {}", manifest.capabilities.commands.join(", "))))
                                                            } else {
                                                                None
                                                            })
                                                    )
                                            }))
                                            .into_any_element()
                                    })
                            )
                            .into_any_element()
                    } else {
                        // TAB 1: Online Registry (from zee-plugins)
                        let registry_plugins = self.registry_plugins.clone();
                        let is_loading = self.registry_loading;
                        let reg_error = self.registry_error.clone();

                        div()
                            .flex()
                            .flex_col()
                            .gap_2p5()
                            .child(
                                // Repositories bar
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(with_alpha(fg, 0.75))
                                                    .child(self.i18n.get("dialog.plugin.repositories").to_string())
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .h(px(24.0))
                                                            .px_2()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .rounded_sm()
                                                            .bg(if self.plugin_show_add_repo { accent } else { button_bg })
                                                            .text_color(if self.plugin_show_add_repo { gpui::rgb(0xffffff) } else { fg })
                                                            .text_size(px(11.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .cursor_pointer()
                                                            .hover(move |s| s.opacity(0.85))
                                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                                this.plugin_show_add_repo = !this.plugin_show_add_repo;
                                                                if !this.plugin_show_add_repo {
                                                                    this.plugin_new_repo_url.clear();
                                                                }
                                                                cx.notify();
                                                            }))
                                                            .child(self.i18n.get("dialog.plugin.add_repo_btn").to_string())
                                                    )
                                                    .child(
                                                        div()
                                                            .h(px(24.0))
                                                            .px_2()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .rounded_sm()
                                                            .bg(button_bg)
                                                            .text_size(px(11.0))
                                                            .cursor_pointer()
                                                            .hover(move |s| s.bg(button_hover))
                                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                                this.fetch_online_plugins(cx);
                                                            }))
                                                            .child("↻ Refresh")
                                                    )
                                            )
                                    )
                                    .child(
                                        // Repository chips
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_1p5()
                                            .children(configured_repos.iter().map(|repo_url| {
                                                let url_clone = repo_url.clone();
                                                let display_name = zee_core::plugin::PluginManager::repo_display_name(repo_url);
                                                let can_remove = configured_repos.len() > 1;
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .bg(chip_bg)
                                                    .border_1()
                                                    .border_color(chip_border)
                                                    .text_size(px(11.0))
                                                    .child(display_name)
                                                    .children(if can_remove {
                                                        Some(
                                                            div()
                                                                .text_size(px(10.0))
                                                                .text_color(with_alpha(fg, 0.45))
                                                                .cursor_pointer()
                                                                .hover(|s| s.text_color(gpui::rgb(0xe53935)))
                                                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                                    let u = url_clone.clone();
                                                                    this.workspace.update(cx, |w, _| {
                                                                        let _ = w.config.remove_plugin_registry(&u);
                                                                    });
                                                                    this.fetch_online_plugins(cx);
                                                                }))
                                                                .child("✕")
                                                        )
                                                    } else {
                                                        None
                                                    })
                                            }))
                                    )
                                    .children(if self.plugin_show_add_repo {
                                        let input_val = self.plugin_new_repo_url.clone();
                                        let placeholder = self.i18n.get("dialog.plugin.repo_placeholder").to_string();
                                        let add_btn_text = self.i18n.get("dialog.plugin.repo_add").to_string();
                                        let cancel_btn_text = self.i18n.get("dialog.plugin.repo_cancel").to_string();
                                        Some(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .p_1p5()
                                                .rounded_sm()
                                                .bg(with_alpha(accent, 0.08))
                                                .border_1()
                                                .border_color(with_alpha(accent, 0.3))
                                                .child(
                                                    div()
                                                        .flex_grow()
                                                        .h(px(26.0))
                                                        .px_2()
                                                        .flex()
                                                        .items_center()
                                                        .rounded_xs()
                                                        .bg(input_bg)
                                                        .border_1()
                                                        .border_color(chip_border)
                                                        .text_size(px(11.5))
                                                        .child(if input_val.is_empty() {
                                                            div()
                                                                .text_color(with_alpha(fg, 0.4))
                                                                .child(format!("{} |", placeholder))
                                                                .into_any_element()
                                                        } else {
                                                            div()
                                                                .text_color(fg)
                                                                .child(format!("{}|", input_val))
                                                                .into_any_element()
                                                        })
                                                )
                                                .child(
                                                    div()
                                                        .h(px(26.0))
                                                        .px_2p5()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_xs()
                                                        .bg(accent)
                                                        .text_color(gpui::rgb(0xffffff))
                                                        .text_size(px(11.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .cursor_pointer()
                                                        .hover(|s| s.opacity(0.9))
                                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                            let url = std::mem::take(&mut this.plugin_new_repo_url);
                                                            this.plugin_show_add_repo = false;
                                                            if !url.trim().is_empty() {
                                                                this.workspace.update(cx, |w, _| {
                                                                    let _ = w.config.add_plugin_registry(&url);
                                                                });
                                                                this.fetch_online_plugins(cx);
                                                            }
                                                        }))
                                                        .child(add_btn_text)
                                                )
                                                .child(
                                                    div()
                                                        .h(px(26.0))
                                                        .px_2p5()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .rounded_xs()
                                                        .bg(button_bg)
                                                        .text_size(px(11.0))
                                                        .cursor_pointer()
                                                        .hover(move |s| s.bg(button_hover))
                                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                            this.plugin_show_add_repo = false;
                                                            this.plugin_new_repo_url.clear();
                                                            cx.notify();
                                                        }))
                                                        .child(cancel_btn_text)
                                                )
                                        )
                                    } else {
                                        None
                                    })
                            )
                            .child(
                                div()
                                    .flex_grow()
                                    .min_h(px(220.0))
                                    .max_h(px(340.0))
                                    .overflow_hidden()
                                    .p_2()
                                    .bg(input_bg)
                                    .border_1()
                                    .border_color(chip_border)
                                    .rounded_md()
                                    .child(if is_loading && registry_plugins.is_none() {
                                        div()
                                            .h_full()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.5))
                                            .text_color(with_alpha(fg, 0.6))
                                            .child(self.i18n.get("dialog.plugin.loading").to_string())
                                            .into_any_element()
                                    } else if let Some(ref err) = reg_error {
                                        div()
                                            .h_full()
                                            .w_full()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .justify_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(gpui::rgb(0xe53935))
                                                    .child(format!("Error: {}", err))
                                            )
                                            .into_any_element()
                                    } else if let Some(ref reg_list) = registry_plugins {
                                        if reg_list.is_empty() {
                                            div()
                                                .h_full()
                                                .w_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(12.5))
                                                .text_color(with_alpha(fg, 0.5))
                                                .child(self.i18n.get("dialog.plugin.registry_empty").to_string())
                                                .into_any_element()
                                        } else {
                                            let installed_ids: std::collections::HashSet<String> = plugins.iter().map(|p| p.id.clone()).collect();
                                            let install_online_label = self.i18n.get("dialog.plugin.install_online").to_string();
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .w_full()
                                                .children(reg_list.iter().map(|reg_p| {
                                                    let p_id = reg_p.id.clone();
                                                    let is_installed = installed_ids.contains(&p_id);
                                                    let type_label = match reg_p.plugin_type {
                                                        zee_core::plugin::PluginType::Lua => "Lua",
                                                        zee_core::plugin::PluginType::Wasm => "WASM",
                                                    };
                                                    let inst_label = install_online_label.clone();
                                                    
                                                    div()
                                                        .p_2p5()
                                                        .rounded_md()
                                                        .border_1()
                                                        .border_color(chip_border)
                                                        .bg(chip_bg)
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_between()
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap_2()
                                                                        .child(
                                                                            div()
                                                                                .text_size(px(13.0))
                                                                                .font_weight(FontWeight::BOLD)
                                                                                .child(reg_p.name.clone())
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .px_1p5()
                                                                                .py_0p5()
                                                                                .rounded_xs()
                                                                                .bg(with_alpha(accent, 0.15))
                                                                                .text_color(accent)
                                                                                .text_size(px(10.0))
                                                                                .font_weight(FontWeight::BOLD)
                                                                                .child(type_label)
                                                                        )
                                                                        .children(reg_p.repository.as_ref().map(|repo| {
                                                                            div()
                                                                                .px_1p5()
                                                                                .py_0p5()
                                                                                .rounded_xs()
                                                                                .bg(with_alpha(fg, 0.08))
                                                                                .text_color(with_alpha(fg, 0.7))
                                                                                .text_size(px(10.0))
                                                                                .child(repo.clone())
                                                                        }))
                                                                        .child(
                                                                            div()
                                                                                .text_size(px(11.0))
                                                                                .text_color(with_alpha(fg, 0.55))
                                                                                .child(format!("v{}", reg_p.version))
                                                                        )
                                                                        .children(reg_p.author.as_ref().map(|a| {
                                                                            div()
                                                                                .text_size(px(11.0))
                                                                                .text_color(with_alpha(fg, 0.5))
                                                                                .child(format!("by {}", a))
                                                                        }))
                                                                )
                                                                .child(if is_installed {
                                                                    div()
                                                                        .px_2()
                                                                        .py_0p5()
                                                                        .rounded_sm()
                                                                        .bg(with_alpha(gpui::rgb(0x4caf50), 0.2))
                                                                        .text_color(gpui::rgb(0x4caf50))
                                                                        .text_size(px(11.0))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .child("✓ Installed")
                                                                        .into_any_element()
                                                                } else {
                                                                    div()
                                                                        .px_2p5()
                                                                        .py_0p5()
                                                                        .rounded_sm()
                                                                        .bg(accent)
                                                                        .text_color(gpui::rgb(0xffffff))
                                                                        .text_size(px(11.0))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .cursor_pointer()
                                                                        .hover(|s| s.opacity(0.85))
                                                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                                            this.start_install_from_registry(p_id.clone(), cx);
                                                                        }))
                                                                        .child(inst_label)
                                                                        .into_any_element()
                                                                })
                                                        )
                                                        .children(reg_p.description.as_ref().map(|desc| {
                                                            div()
                                                                .text_size(px(11.5))
                                                                .text_color(with_alpha(fg, 0.75))
                                                                .child(desc.clone())
                                                        }))
                                                }))
                                                .into_any_element()
                                        }
                                    } else {
                                        div()
                                            .h_full()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(self.i18n.get("dialog.plugin.loading").to_string())
                                            .into_any_element()
                                    })
                            )
                            .into_any_element()
                    })
                    // Footer
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .pt_2()
                            .border_t_1()
                            .border_color(chip_border)
                            .child(
                                div()
                                    .h(px(28.0))
                                    .px_5()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::NewFile { parent_dir } => {
                let title = self.i18n.get("sidebar.new_file");
                let label = self.i18n.get("dialog.file_name");
                let parent_display = parent_dir.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| parent_dir.to_string_lossy());
                div()
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string())
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(with_alpha(fg, 0.6))
                            .child(format!("In: {}", parent_display))
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label.to_string())
                    )
                    .child(
                        div()
                            .h(px(34.0))
                            .bg(input_bg)
                            .border_1()
                            .border_color(border_color)
                            .rounded_md()
                            .px_3()
                            .flex()
                            .items_center()
                            .child(
                                if self.input_text.is_empty() {
                                    div()
                                        .text_color(with_alpha(fg, 0.45))
                                        .text_size(px(13.0))
                                        .child("example.txt")
                                } else {
                                    div()
                                        .text_size(px(13.0))
                                        .child(self.input_text.clone())
                                }
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.confirm(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::NewFolder { parent_dir } => {
                let title = self.i18n.get("sidebar.new_folder");
                let label = self.i18n.get("dialog.folder_name");
                let parent_display = parent_dir.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| parent_dir.to_string_lossy());
                div()
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string())
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(with_alpha(fg, 0.6))
                            .child(format!("In: {}", parent_display))
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label.to_string())
                    )
                    .child(
                        div()
                            .h(px(34.0))
                            .bg(input_bg)
                            .border_1()
                            .border_color(border_color)
                            .rounded_md()
                            .px_3()
                            .flex()
                            .items_center()
                            .child(
                                if self.input_text.is_empty() {
                                    div()
                                        .text_color(with_alpha(fg, 0.45))
                                        .text_size(px(13.0))
                                        .child("new_folder")
                                } else {
                                    div()
                                        .text_size(px(13.0))
                                        .child(self.input_text.clone())
                                }
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.confirm(cx)))
                                    .child(self.i18n.get("dialog.ok").to_string())
                            )
                    )
            }
            DialogType::Rename { target_path } => {
                let title = self.i18n.get("sidebar.rename");
                let label = self.i18n.get("dialog.new_name");
                let cur_display = target_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string())
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(with_alpha(fg, 0.6))
                            .child(format!("Current: {}", cur_display))
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label.to_string())
                    )
                    .child(
                        div()
                            .h(px(34.0))
                            .bg(input_bg)
                            .border_1()
                            .border_color(border_color)
                            .rounded_md()
                            .px_3()
                            .flex()
                            .items_center()
                            .child(
                                if self.input_text.is_empty() {
                                    div()
                                        .text_color(with_alpha(fg, 0.45))
                                        .text_size(px(13.0))
                                        .child("new_name")
                                } else {
                                    div()
                                        .text_size(px(13.0))
                                        .child(self.input_text.clone())
                                }
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(accent)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.confirm(cx)))
                                    .child(self.i18n.get("sidebar.rename").to_string())
                            )
                    )
            }
            DialogType::ConfirmDelete { target_path } => {
                let title = self.i18n.get("sidebar.delete");
                let cur_display = target_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                let confirm_msg = format!("{} '{}'?", self.i18n.get("sidebar.delete_confirm"), cur_display);
                let danger_color = led_color_to_gpui(theme.syntax.keyword.unwrap_or(zee_core::theme::Color::Ansi(1)));
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string())
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(with_alpha(fg, 0.85))
                            .child(confirm_msg)
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(button_hover))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close(cx)))
                                    .child(self.i18n.get("dialog.cancel").to_string())
                            )
                            .child(
                                div()
                                    .h(px(30.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .bg(danger_color)
                                    .text_color(gpui::rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.confirm(cx)))
                                    .child(self.i18n.get("sidebar.delete").to_string())
                            )
                    )
            }
        }
    }

    fn format_size(&self, size: u64) -> String {
        if size < 1024 {
            format!("{} B", size)
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f32 / 1024.0)
        } else {
            format!("{:.1} MB", size as f32 / (1024.0 * 1024.0))
        }
    }
}

