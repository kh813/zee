use gpui::*;
use zee_core::config::Config;
use zee_core::i18n::I18n;
use zee_core::theme::Theme;
use crate::window_view::WindowView;
use crate::workspace::Workspace;
use anyhow::Result;
use futures::StreamExt;
use url::Url;

pub fn setup_app(app: &mut App, rx: futures::channel::mpsc::UnboundedReceiver<Vec<String>>) {
    let mut config = Config::load();
    config.vi_mode = false;
    let i18n = I18n::load(&config.language);

    // Global key bindings - must be bound before setup_menu so NSMenu keyEquivalents are properly set
    app.bind_keys(vec![
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-t", NewTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-n", NewWindow {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-o", Open {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-o", OpenFolder {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", Save {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-s", SaveAs {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-r", ReloadFile {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-w", CloseTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-q", Quit {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", Undo {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", Redo {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-y", Redo {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-f", Find {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-r", Replace {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-f", Replace {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-h", About {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-e", ToggleViMode {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-,", OpenSettings {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-=", ZoomIn {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-+", ZoomIn {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd--", ZoomOut {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-0", ResetZoom {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-[", PrevTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-]", NextTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-{", PrevTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-}", NextTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleSidebar {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-1", ToggleFiles {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-2", ToggleOutline {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-tab", NextTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-shift-tab", PrevTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-pageup", PrevTab {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-pagedown", NextTab {}, None),
        KeyBinding::new("f5", RefreshFileTree {}, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-r", RefreshFileTree {}, None),

        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-t", NewTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-n", NewWindow {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-o", Open {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-o", OpenFolder {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", Save {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-s", SaveAs {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-r", ReloadFile {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-w", CloseTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-q", Quit {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", Undo {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", Redo {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f", Find {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-r", Replace {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-f", Replace {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-h", About {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-e", ToggleViMode {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-,", OpenSettings {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-=", ZoomIn {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-+", ZoomIn {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl--", ZoomOut {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-0", ResetZoom {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-tab", NextTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-tab", PrevTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-pageup", PrevTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-pagedown", NextTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-left", PrevTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-right", NextTab {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleSidebar {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-1", ToggleFiles {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-2", ToggleOutline {}, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-r", RefreshFileTree {}, None),
    ]);

    // App-level action handlers to handle actions when no window is open or globally
    let config_new_tab = config.clone();
    let i18n_new_tab = i18n.clone();
    app.on_action(move |_: &NewTab, cx| {
        let mut handled = false;
        if let Some(active_window) = cx.active_window() {
            if let Ok(res) = cx.update_window(active_window, |any_view, _window, cx| {
                if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                    view_handle.update(cx, |view, cx| {
                        view.workspace.update(cx, |w, cx| {
                            w.new_tab();
                            cx.notify();
                        });
                    });
                    true
                } else {
                    false
                }
            }) {
                handled = res;
            }
        }
        if !handled {
            new_window(config_new_tab.clone(), i18n_new_tab.clone(), cx);
        }
    });

    let config_new_win = config.clone();
    let i18n_new_win = i18n.clone();
    app.on_action(move |_: &NewWindow, cx| {
        new_window(config_new_win.clone(), i18n_new_win.clone(), cx);
    });

    let config_new = config.clone();
    let i18n_new = i18n.clone();
    app.on_action(move |_: &New, cx| {
        let mut handled = false;
        if let Some(active_window) = cx.active_window() {
            if let Ok(res) = cx.update_window(active_window, |any_view, _window, cx| {
                if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                    view_handle.update(cx, |view, cx| {
                        view.workspace.update(cx, |w, cx| {
                            w.new_tab();
                            cx.notify();
                        });
                    });
                    true
                } else {
                    false
                }
            }) {
                handled = res;
            }
        }
        if !handled {
            new_window(config_new.clone(), i18n_new.clone(), cx);
        }
    });

    setup_menu(app, &i18n, &config);

    let config_open = config.clone();
    let i18n_open = i18n.clone();
    app.on_action(move |_: &Open, cx| {
        let config = config_open.clone();
        let i18n = i18n_open.clone();
        cx.spawn(|cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let files = rfd::AsyncFileDialog::new().pick_files().await;
                if let Some(files) = files {
                    let paths: Vec<_> = files.into_iter().map(|f| f.path().to_path_buf()).collect();
                    cx.update(|cx| {
                        cx.open_window(centered_window_options(cx), move |window, cx| {
                            let workspace = cx.new(|_| {
                                let mut w = Workspace::new(config.clone());
                                for path in paths {
                                    if let Ok(editor) = zee_core::buffer::Editor::from_file(&path) {
                                        w.add_editor(editor);
                                    }
                                }
                                w
                            });
                            cx.new(|cx| WindowView::new(config.clone(), i18n.clone(), workspace, window, cx))
                        }).expect("Failed to open window");
                    });
                }
            }
        }).detach();
    });

    let i18n_about = i18n.clone();
    app.on_action(move |_: &About, cx| {
        let i18n = i18n_about.clone();
        cx.open_window(centered_window_options(cx), move |window, cx| {
            let workspace = cx.new(|_| Workspace::new(Config::default()));
            cx.new(|cx| {
                let mut view = WindowView::new(Config::default(), i18n.clone(), workspace, window, cx);
                // Immediately show about dialog
                view.handle_about(&About {}, window, cx);
                view
            })
        }).expect("Failed to open about window");
    });

    let i18n_update = i18n.clone();
    app.on_action(move |_: &CheckForUpdates, cx| {
        let i18n = i18n_update.clone();
        if let Some(window_handle) = cx.active_window() {
            let _ = cx.update_window(window_handle, |any_view, window, cx| {
                if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                    view_handle.update(cx, |view, cx| {
                        view.handle_check_for_updates(&CheckForUpdates {}, window, cx);
                    });
                }
            });
        } else {
            cx.open_window(centered_window_options(cx), move |window, cx| {
                let workspace = cx.new(|_| Workspace::new(Config::default()));
                cx.new(|cx| {
                    let mut view = WindowView::new(Config::default(), i18n.clone(), workspace, window, cx);
                    view.handle_check_for_updates(&CheckForUpdates {}, window, cx);
                    view
                })
            }).expect("Failed to open window for update");
        }
    });

    app.on_action(|_: &Quit, cx| {
        for hw in cx.windows() {
            let modified = cx.update_window(hw, |any_view, _window, cx| {
                if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                    view_handle.read(cx).has_modified_buffers(cx)
                } else {
                    false
                }
            }).unwrap_or(false);

            if modified {
                let _ = cx.update_window(hw, |any_view, window, cx| {
                    window.activate_window();
                    if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                        view_handle.update(cx, |view, cx| {
                            view.handle_quit(&Quit {}, window, cx);
                        });
                    }
                });
                return;
            }
        }
        cx.quit();
    });

    app.on_action(|_: &Exit, cx| {
        for hw in cx.windows() {
            let modified = cx.update_window(hw, |any_view, _window, cx| {
                if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                    view_handle.read(cx).has_modified_buffers(cx)
                } else {
                    false
                }
            }).unwrap_or(false);

            if modified {
                let _ = cx.update_window(hw, |any_view, window, cx| {
                    window.activate_window();
                    if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                        view_handle.update(cx, |view, cx| {
                            view.handle_quit(&Quit {}, window, cx);
                        });
                    }
                });
                return;
            }
        }
        cx.quit();
    });

    // Activate the application on launch so its window is brought to the foreground
    app.activate(true);

    // Initial window or CLI argument paths
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let cli_targets = zee_core::cli::parse_file_targets(&raw_args);
    if !cli_targets.is_empty() {
        zee_core::session::UpdateSession::clear();
        open_file_targets(cli_targets, config.clone(), i18n.clone(), app);
    } else if let Some(session) = zee_core::session::UpdateSession::load_and_clear() {
        restore_session_window(session, config.clone(), i18n.clone(), app);
    } else {
        new_window(config.clone(), i18n.clone(), app);
    }

    // Handle files dropped on the Dock icon or opened via Finder
    app.spawn(|cx: &mut AsyncApp| {
        let cx = cx.clone();
        let mut rx = rx;
        async move {
            while let Some(urls) = rx.next().await {
                let config = Config::load();
                let i18n = I18n::load(&config.language);
                let paths: Vec<_> = urls.into_iter()
                    .filter_map(|u| {
                        if let Ok(url) = Url::parse(&u) {
                            url.to_file_path().ok()
                        } else {
                            Some(std::path::PathBuf::from(u))
                        }
                    })
                    .collect();
                
                if !paths.is_empty() {
                    cx.update(|cx| {
                        open_paths(paths, config, i18n, cx);
                    });
                }
            }
        }
    }).detach();
}

fn centered_window_options(cx: &App) -> WindowOptions {
    let window_size = size(px(1012.0), px(800.0));
    let mut origin = Point::default();
    if let Some(display) = cx.primary_display() {
        let display_bounds = display.bounds();
        let base_x = display_bounds.origin.x + (display_bounds.size.width - window_size.width) / 2.0;
        let base_y = display_bounds.origin.y + (display_bounds.size.height - window_size.height) / 2.0;

        let num_windows = cx.windows().len();
        if num_windows > 0 {
            // Offset new window diagonally by 28px per existing window, wrapping around if exceeding screen boundaries
            let offset_step = px(28.0);
            let max_offset_x = (display_bounds.size.width - window_size.width).max(px(0.0));
            let max_offset_y = (display_bounds.size.height - window_size.height).max(px(0.0));
            
            let max_steps_x = ((max_offset_x / 2.0) / offset_step).max(1.0) as usize;
            let max_steps_y = ((max_offset_y / 2.0) / offset_step).max(1.0) as usize;
            let max_steps = max_steps_x.min(max_steps_y).max(1);

            let cascade_idx = num_windows % (max_steps + 1);
            let offset = offset_step * (cascade_idx as f32);

            origin.x = base_x + offset;
            origin.y = base_y + offset;
        } else {
            origin.x = base_x;
            origin.y = base_y;
        }
    }

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
        ..Default::default()
    }
}

pub fn new_window(config: Config, i18n: I18n, cx: &mut App) {
    cx.activate(true);
    let theme_to_use = if config.theme == "terminal-default" || config.theme.is_empty() {
        match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                Theme::find_by_name("tokyo-night").unwrap_or_default()
            }
            WindowAppearance::Light | WindowAppearance::VibrantLight => {
                Theme::find_by_name("catppuccin-latte").unwrap_or_default()
            }
        }
    } else {
        Theme::find_by_name(&config.theme).unwrap_or_default()
    };

    let options = centered_window_options(cx);
    cx.open_window(options, move |window, cx| {
        window.activate_window();
        let workspace = cx.new(|_| {
            let mut w = Workspace::new(config.clone());
            w.theme = theme_to_use;
            w
        });
        cx.new(|cx| WindowView::new(config, i18n, workspace, window, cx))
    }).expect("Failed to open window");
}

pub fn open_file_targets(targets: Vec<zee_core::cli::FileTarget>, config: Config, i18n: I18n, cx: &mut App) {
    cx.activate(true);
    let theme_to_use = if config.theme == "terminal-default" || config.theme.is_empty() {
        match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                Theme::find_by_name("tokyo-night").unwrap_or_default()
            }
            WindowAppearance::Light | WindowAppearance::VibrantLight => {
                Theme::find_by_name("catppuccin-latte").unwrap_or_default()
            }
        }
    } else {
        Theme::find_by_name(&config.theme).unwrap_or_default()
    };

    let options = centered_window_options(cx);
    cx.open_window(options, move |window, cx| {
        window.activate_window();
        let workspace = cx.new(|_| {
            let mut dir_root = None;
            let mut targets_to_open = Vec::new();

            for target in targets {
                if target.path.is_dir() {
                    if dir_root.is_none() {
                        dir_root = Some(target.path);
                    }
                } else {
                    targets_to_open.push(target);
                }
            }

            let mut w = Workspace::new_with_root(config.clone(), dir_root);
            w.theme = theme_to_use;

            let mut opened_any = false;
            for target in targets_to_open {
                if let Ok(editor) = zee_core::buffer::Editor::from_file(&target.path) {
                    w.add_editor(editor);
                    if let Some(line) = target.line {
                        let col = target.col.unwrap_or(1);
                        w.jump_to_line_col(line.saturating_sub(1), col.saturating_sub(1));
                    }
                    opened_any = true;
                }
            }
            if opened_any {
                w.update_outline();
            }
            w
        });
        cx.new(|cx| WindowView::new(config, i18n, workspace, window, cx))
    }).expect("Failed to open window");
}

pub fn open_paths(paths: Vec<std::path::PathBuf>, config: Config, i18n: I18n, cx: &mut App) {
    let targets = paths.into_iter().map(|p| zee_core::cli::FileTarget { path: p, line: None, col: None }).collect();
    open_file_targets(targets, config, i18n, cx);
}

pub fn restore_session_window(session: zee_core::session::UpdateSession, config: Config, i18n: I18n, cx: &mut App) {
    cx.activate(true);
    let theme_to_use = if config.theme == "terminal-default" || config.theme.is_empty() {
        match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                Theme::find_by_name("tokyo-night").unwrap_or_default()
            }
            WindowAppearance::Light | WindowAppearance::VibrantLight => {
                Theme::find_by_name("catppuccin-latte").unwrap_or_default()
            }
        }
    } else {
        Theme::find_by_name(&config.theme).unwrap_or_default()
    };

    let options = centered_window_options(cx);
    cx.open_window(options, move |window, cx| {
        window.activate_window();
        let workspace = cx.new(|_| {
            let mut w = Workspace::new_with_root(config.clone(), session.root_folder.clone());
            w.theme = theme_to_use;
            w.sidebar_visible = session.sidebar_visible;
            w.sidebar_tab = match session.sidebar_tab.as_str() {
                "outline" => crate::workspace::SidebarTab::Outline,
                _ => crate::workspace::SidebarTab::Files,
            };
            if !session.expanded_folders.is_empty() {
                w.file_tree.restore_expanded_paths(&session.expanded_folders);
            }

            let mut opened_any = false;
            for file_state in session.files {
                let editor = if let Some(path) = &file_state.path {
                    if let Ok(mut ed) = zee_core::buffer::Editor::from_file(path) {
                        if let Some(unsaved) = &file_state.unsaved_content {
                            ed.delete(0..ed.rope.len_chars());
                            ed.insert(0, unsaved);
                            ed.modified_since_save = file_state.is_modified;
                        }
                        Some(ed)
                    } else if let Some(unsaved) = &file_state.unsaved_content {
                        let mut ed = zee_core::buffer::Editor::new();
                        ed.path = Some(path.clone());
                        ed.insert(0, unsaved);
                        ed.modified_since_save = file_state.is_modified;
                        Some(ed)
                    } else {
                        None
                    }
                } else if let Some(unsaved) = &file_state.unsaved_content {
                    let mut ed = zee_core::buffer::Editor::new();
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
                    opened_any = true;
                }
            }

            if session.active_index < w.editors.len() {
                w.active_editor_index = session.active_index;
            }

            if opened_any {
                w.update_outline();
            }
            w
        });
        cx.new(|cx| WindowView::new(config, i18n, workspace, window, cx))
    }).expect("Failed to open window");
}

#[cfg(target_os = "macos")]
pub fn setup_menu(app: &mut App, i18n: &I18n, config: &Config) {
    app.set_menus(build_native_menus(i18n, config));
}

#[cfg(not(target_os = "macos"))]
pub fn setup_menu(_app: &mut App, _i18n: &I18n, _config: &Config) {
    // In-window menu bar is handled in window_view.rs
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SetTheme {
    pub name: String,
}

impl Action for SetTheme {
    fn name(&self) -> &'static str { "SetTheme" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "SetTheme" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ReopenWithEncoding {
    pub encoding: String,
}

impl Action for ReopenWithEncoding {
    fn name(&self) -> &'static str { "ReopenWithEncoding" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "ReopenWithEncoding" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ConvertToEncoding {
    pub encoding: String,
}

impl Action for ConvertToEncoding {
    fn name(&self) -> &'static str { "ConvertToEncoding" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "ConvertToEncoding" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SetSyntax {
    pub name: String,
}

impl Action for SetSyntax {
    fn name(&self) -> &'static str { "SetSyntax" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "SetSyntax" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ExecutePluginCommand {
    pub command: String,
}

impl Action for ExecutePluginCommand {
    fn name(&self) -> &'static str { "ExecutePluginCommand" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "ExecutePluginCommand" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct OpenRecent {
    pub path: String,
}

impl Action for OpenRecent {
    fn name(&self) -> &'static str { "OpenRecent" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "OpenRecent" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ApplyTemplate {
    pub id: String,
}

impl Action for ApplyTemplate {
    fn name(&self) -> &'static str { "ApplyTemplate" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "ApplyTemplate" }
}

#[derive(serde::Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct SetLanguage {
    pub id: String,
}

impl Action for SetLanguage {
    fn name(&self) -> &'static str { "SetLanguage" }
    fn boxed_clone(&self) -> Box<dyn Action> { Box::new(self.clone()) }
    fn build(v: gpui::private::serde_json::Value) -> Result<Box<dyn Action>> { Ok(Box::new(serde_json::from_value::<Self>(v)?)) }
    fn partial_eq(&self, _other: &dyn Action) -> bool { false }
    fn name_for_type() -> &'static str { "SetLanguage" }
}

#[cfg(target_os = "macos")]
pub fn build_native_menus(i18n: &I18n, config: &zee_core::config::Config) -> Vec<Menu> {
    let mut theme_items = Vec::new();
    for theme in zee_core::theme::Theme::load_all() {
        let is_current = theme.meta.name == config.theme;
        let theme_name = if is_current {
            format!("✓ {}", theme.meta.name)
        } else {
            theme.meta.name.clone()
        };
        theme_items.push(MenuItem::action(
            theme_name,
            SetTheme { name: theme.meta.name.clone() },
        ));
    }

    let mut syntax_items = Vec::new();
    // In a real app we'd load these from core, but for now we'll hardcode or use builtins if available
    for syntax in ["Plain Text", "Markdown", "Rust", "TOML", "JSON", "Python", "Go", "Swift", "JavaScript", "HTML", "CSS", "XML"] {
        syntax_items.push(MenuItem::action(
            syntax,
            SetSyntax { name: syntax.to_string() },
        ));
    }

    let encodings = ["UTF-8", "UTF-8 with BOM", "UTF-16 LE", "UTF-16 BE", 
        "Shift-JIS", "EUC-JP", "ISO-2022-JP", "Latin-1"];

    let reopen_items = encodings.iter().map(|e| {
        MenuItem::action(e.to_string(), ReopenWithEncoding { encoding: e.to_string() })
    }).collect();

    let convert_items = encodings.iter().map(|e| {
        MenuItem::action(e.to_string(), ConvertToEncoding { encoding: e.to_string() })
    }).collect();

    let line_numbers_label = if config.line_numbers {
        format!("✓ {}", i18n.get("menu.view.line_numbers"))
    } else {
        i18n.get("menu.view.line_numbers").to_string()
    };
    let word_wrap_label = if config.word_wrap {
        format!("✓ {}", i18n.get("menu.view.word_wrap"))
    } else {
        i18n.get("menu.view.word_wrap").to_string()
    };
    let vi_mode_label = if config.vi_mode {
        format!("✓ {}", i18n.get("menu.view.vi_mode"))
    } else {
        i18n.get("menu.view.vi_mode").to_string()
    };

    let mut plugin_menu_items = Vec::new();
    let mut plugin_manager = zee_core::plugin::PluginManager::new();
    let dev_plugin_dir = std::path::PathBuf::from("plugins/zee-plugin-text");
    if dev_plugin_dir.exists() {
        let _ = plugin_manager.load_plugin_dir(&dev_plugin_dir);
    }

    let manifests = plugin_manager.all_manifests();
    if manifests.is_empty() {
        plugin_menu_items.push(MenuItem::action(i18n.get("menu.plugins.no_plugins"), NoOp {}));
    } else {
        for manifest in manifests {
            if manifest.capabilities.commands.is_empty() {
                plugin_menu_items.push(MenuItem::action(format!("✓ {}", manifest.name), NoOp {}));
            } else {
                let mut cmd_items = Vec::new();
                for cmd in &manifest.capabilities.commands {
                    cmd_items.push(MenuItem::action(
                        cmd.clone(),
                        ExecutePluginCommand { command: cmd.clone() },
                    ));
                }
                plugin_menu_items.push(MenuItem::submenu(Menu {
                    name: manifest.name.clone().into(),
                    items: cmd_items,
                    disabled: false,
                }));
            }
        }
    }
    plugin_menu_items.push(MenuItem::separator());
    plugin_menu_items.push(MenuItem::action(i18n.get("menu.plugins.manage"), ManagePlugins {}));
    plugin_menu_items.push(MenuItem::action(i18n.get("menu.plugins.open_folder"), OpenPluginsFolder {}));

    let mut language_items = Vec::new();
    for lang in zee_core::i18n::AVAILABLE_LANGUAGES {
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
        let display_name = if is_current {
            format!("✓ {}", label)
        } else {
            label
        };
        language_items.push(MenuItem::action(
            display_name,
            SetLanguage { id: lang.id.to_string() },
        ));
    }

    vec![
        Menu {
            name: "zee".into(),
            items: vec![
                MenuItem::action(i18n.get("menu.zee.about"), About {}),
                MenuItem::action(i18n.get("menu.help.check_for_updates"), CheckForUpdates {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.app.preferences"), OpenSettings {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.zee.quit"), Quit {}),
            ],
            disabled: false,
        },
        Menu {
            name: i18n.get("menu.file").into(),
            items: {
                let recent = zee_core::recent::RecentFiles::load();
                let mut recent_items = Vec::new();
                if recent.files.is_empty() {
                    recent_items.push(MenuItem::action(i18n.get("menu.file.no_recent"), NoOp {}));
                } else {
                    for p in &recent.files {
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned());
                        recent_items.push(MenuItem::action(
                            name,
                            OpenRecent { path: p.to_string_lossy().into_owned() },
                        ));
                    }
                    recent_items.push(MenuItem::separator());
                    recent_items.push(MenuItem::action(i18n.get("menu.file.clear_recent"), ClearRecent {}));
                }

                let templates = zee_core::template::Template::load_all();
                let mut template_items = Vec::new();
                for tpl in &templates {
                    template_items.push(MenuItem::action(
                        tpl.name.clone(),
                        ApplyTemplate { id: tpl.id.clone() },
                    ));
                }
                template_items.push(MenuItem::separator());
                template_items.push(MenuItem::action(
                    i18n.get("menu.file.open_templates_folder"),
                    OpenTemplatesFolder {},
                ));

                vec![
                    MenuItem::action(i18n.get("menu.file.new_tab"), NewTab {}),
                    MenuItem::submenu(Menu {
                        name: i18n.get("menu.file.new_from_template").into(),
                        items: template_items,
                        disabled: false,
                    }),
                    MenuItem::action(i18n.get("menu.file.new_window"), NewWindow {}),
                    MenuItem::action(i18n.get("menu.file.open"), Open {}),
                    MenuItem::action(i18n.get("menu.file.open_folder"), OpenFolder {}),
                    MenuItem::submenu(Menu {
                        name: i18n.get("menu.file.open_recent").into(),
                        items: recent_items,
                        disabled: false,
                    }),
                    MenuItem::separator(),
                    MenuItem::action(i18n.get("menu.file.reload"), ReloadFile {}),
                    MenuItem::separator(),
                    MenuItem::action(i18n.get("menu.file.save"), Save {}),
                    MenuItem::action(i18n.get("menu.file.save_as"), SaveAs {}),
                    MenuItem::separator(),
                    MenuItem::action(
                        format!("{} {}", if config.trim_trailing_whitespace { "✓" } else { " " }, i18n.get("menu.file.trim_trailing_whitespace")),
                        ToggleTrimTrailingWhitespace {},
                    ),
                    MenuItem::action(
                        format!("{} {}", if config.ensure_final_newline { "✓" } else { " " }, i18n.get("menu.file.ensure_final_newline")),
                        ToggleEnsureFinalNewline {},
                    ),
                    MenuItem::separator(),
                    MenuItem::action(i18n.get("menu.file.export_config"), ExportConfig {}),
                    MenuItem::action(i18n.get("menu.file.import_config"), ImportConfig {}),
                    MenuItem::separator(),
                    MenuItem::action(i18n.get("menu.file.close"), CloseTab {}),
                    MenuItem::separator(),
                    MenuItem::action(i18n.get("menu.file.exit"), Exit {}),
                ]
            },
            disabled: false,
        },
        Menu {
            name: i18n.get("menu.edit").into(),
            items: vec![
                MenuItem::action(i18n.get("menu.edit.undo"), Undo {}),
                MenuItem::action(i18n.get("menu.edit.redo"), Redo {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.edit.cut"), Cut {}),
                MenuItem::action(i18n.get("menu.edit.copy"), Copy {}),
                MenuItem::action(i18n.get("menu.edit.paste"), Paste {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.edit.find"), Find {}),
                MenuItem::action(i18n.get("menu.edit.replace"), Replace {}),
                MenuItem::action(i18n.get("menu.edit.select_all"), SelectAll {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.edit.format_document"), FormatDocument {}),
                MenuItem::action(i18n.get("menu.edit.sort_lines"), SortLines {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.edit.to_uppercase"), ToUpperCase {}),
                MenuItem::action(i18n.get("menu.edit.to_lowercase"), ToLowerCase {}),
                MenuItem::action(i18n.get("menu.edit.to_snake_case"), ToSnakeCase {}),
                MenuItem::action(i18n.get("menu.edit.to_camel_case"), ToCamelCase {}),
            ],
            disabled: false,
        },
        Menu {
            name: i18n.get("menu.view").into(),
            items: vec![
                MenuItem::action(i18n.get("menu.view.sidebar"), ToggleSidebar {}),
                MenuItem::action(i18n.get("menu.view.refresh_files"), RefreshFileTree {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.view.go_to_line"), GoToLine {}),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.view.zoom_in"), ZoomIn {}),
                MenuItem::action(i18n.get("menu.view.zoom_out"), ZoomOut {}),
                MenuItem::action(i18n.get("menu.view.reset_zoom"), ResetZoom {}),
                MenuItem::separator(),
                MenuItem::action(line_numbers_label, ToggleLineNumbers {}),
                MenuItem::action(word_wrap_label, ToggleWordWrap {}),
                MenuItem::action(vi_mode_label, ToggleViMode {}),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: i18n.get("menu.view.encoding").into(),
                    items: vec![
                        MenuItem::submenu(Menu { name: i18n.get("menu.view.reopen_with_encoding").into(), items: reopen_items, disabled: false }),
                        MenuItem::submenu(Menu { name: i18n.get("menu.view.convert_to_encoding").into(), items: convert_items, disabled: false }),
                    ],
                    disabled: false,
                }),
                MenuItem::submenu(Menu {
                    name: i18n.get("menu.view.line_ending").into(),
                    items: vec![
                        MenuItem::action("LF", SetLineEndingLf {}),
                        MenuItem::action("CRLF", SetLineEndingCrlf {}),
                        MenuItem::action("CR", SetLineEndingCr {}),
                    ],
                    disabled: false,
                }),
                MenuItem::submenu(Menu {
                    name: i18n.get("menu.view.theme").into(),
                    items: theme_items,
                    disabled: false,
                }),
                MenuItem::submenu(Menu {
                    name: i18n.get("menu.view.syntax").into(),
                    items: syntax_items,
                    disabled: false,
                }),
                MenuItem::submenu(Menu {
                    name: i18n.get("menu.view.language").into(),
                    items: language_items,
                    disabled: false,
                }),
                MenuItem::separator(),
                MenuItem::action(i18n.get("menu.app.preferences"), OpenSettings {}),
            ],
            disabled: false,
        },
        Menu {
            name: i18n.get("menu.tabs").into(),
            items: vec![
                MenuItem::action(i18n.get("menu.tabs.next"), NextTab {}),
                MenuItem::action(i18n.get("menu.tabs.prev"), PrevTab {}),
            ],
            disabled: false,
        },
        Menu {
            name: i18n.get("menu.plugins").into(),
            items: plugin_menu_items,
            disabled: false,
        },
        Menu {
            name: i18n.get("menu.help").into(),
            items: vec![
                MenuItem::action(i18n.get("menu.help.about"), About {}),
                MenuItem::action(i18n.get("menu.help.check_for_updates"), CheckForUpdates {}),
            ],
            disabled: false,
        }
    ]
}

actions!(zee, [
    // App/File
    About, CheckForUpdates, OpenSettings, Quit, Exit, New, NewTab, NewWindow, Open, OpenFolder, Save, SaveAs, CloseTab,
    ReloadFile, ClearRecent,
    ToggleTrimTrailingWhitespace, ToggleEnsureFinalNewline,
    ExportConfig, ExportAll, ImportConfig, ManagePlugins, OpenPluginsFolder, OpenTemplatesFolder,

    // Edit
    Undo, Redo, Cut, Copy, Paste, Find, Replace, SelectAll,
    FormatDocument, SortLines, ToUpperCase, ToLowerCase, ToSnakeCase, ToCamelCase,
    // Tabs
    NextTab, PrevTab,
    // View
    ToggleSidebar, ToggleOutline, ToggleFiles, RefreshFileTree,
    GoToLine, ZoomIn, ZoomOut, ResetZoom, ToggleLineNumbers, ToggleWordWrap, ToggleViMode,
    SetEncodingUtf8, SetEncodingUtf8Bom, SetEncodingUtf16Le, SetEncodingUtf16Be,
    SetEncodingShiftJis, SetEncodingEucJp, SetEncodingIso2022Jp, SetEncodingLatin1,
    SetLineEndingLf, SetLineEndingCrlf, SetLineEndingCr,
    // Search
    SearchNext, SearchPrev, SearchReplace, SearchReplaceAll, CloseFind,
    ToggleMatchCase, ToggleWholeWord, ToggleRegex,
    // Other
    NoOp
]);
