use gpui::*;
use zee_core::config::Config;
use zee_core::i18n::I18n;
use crate::widgets::editor_view::EditorView;
use crate::widgets::tab_bar::{TabBar, TabBarEvent};
use crate::widgets::status_bar::StatusBar;
use crate::widgets::find_panel::{FindPanel, FindPanelEvent};
use crate::widgets::sidebar_view::SidebarView;
use crate::workspace::Workspace;
use crate::app::*;
use zee_core::buffer::Editor;

#[cfg(not(target_os = "macos"))]
use crate::widgets::menu_bar::MenuBar;
#[cfg(not(target_os = "macos"))]
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};

use crate::widgets::dialog::{Dialog, DialogType, DialogEvent, UnsavedChangesIntent};

pub struct WindowView {
    config: Config,
    i18n: I18n,
    pub(crate) workspace: Entity<Workspace>,
    editor: Entity<EditorView>,
    sidebar: Entity<SidebarView>,
    tab_bar: Entity<TabBar>,
    status_bar: Entity<StatusBar>,
    find_panel: Entity<FindPanel>,
    #[cfg(not(target_os = "macos"))]
    menu_bar: Entity<MenuBar>,
    dialog: Option<Entity<Dialog>>,
    focus_handle: FocusHandle,
}

impl WindowView {
    pub fn new(config: Config, i18n: I18n, workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| EditorView::new(workspace.clone(), cx));
        let sidebar = cx.new(|cx| SidebarView::new(workspace.clone(), i18n.clone(), cx));
        let tab_bar = cx.new(|cx| TabBar::new(workspace.clone(), cx));
        let status_bar = cx.new(|cx| StatusBar::new(workspace.clone(), cx));
        let find_panel = cx.new(|cx| FindPanel::new(workspace.clone(), cx));
        #[cfg(not(target_os = "macos"))]
        let menu_bar = cx.new(|cx| MenuBar::new(workspace.clone(), i18n.clone(), cx));
        
        workspace.update(cx, |w, _| w.update_outline());
        
        let focus_handle = cx.focus_handle();
        
        // Focus the editor by default
        editor.update(cx, |editor, cx| {
            editor.focus_handle.focus(window, cx);
        });

        cx.observe(&workspace, |this, workspace, cx| {
            let w_config = &workspace.read(cx).config;
            #[allow(unused_mut)]
            let mut menu_changed = false;
            if this.config.word_wrap != w_config.word_wrap {
                this.config.word_wrap = w_config.word_wrap;
                menu_changed = true;
            }
            if this.config.line_numbers != w_config.line_numbers {
                this.config.line_numbers = w_config.line_numbers;
                menu_changed = true;
            }
            if this.config.vi_mode != w_config.vi_mode {
                this.config.vi_mode = w_config.vi_mode;
                menu_changed = true;
            }
            if this.config.theme != w_config.theme {
                this.config.theme = w_config.theme.clone();
                menu_changed = true;
            }
            if this.config.language != w_config.language {
                this.config.language = w_config.language.clone();
                this.i18n = I18n::load(&this.config.language);
                this.sidebar.update(cx, |sb, cx| {
                    sb.i18n = this.i18n.clone();
                    cx.notify();
                });
                #[cfg(not(target_os = "macos"))]
                this.menu_bar.update(cx, |mb, cx| {
                    mb.set_i18n(this.i18n.clone(), cx);
                });
                if let Some(dialog) = &this.dialog {
                    dialog.update(cx, |d, cx| {
                        d.set_i18n(this.i18n.clone(), cx);
                    });
                }
                menu_changed = true;
            }
            if menu_changed {
                #[cfg(target_os = "macos")]
                cx.set_menus(crate::app::build_native_menus(&this.i18n, &this.config));
            }
            cx.notify();
        }).detach();

        #[cfg(not(target_os = "macos"))]
        cx.observe(&menu_bar, |_, _, cx| {
            cx.notify();
        }).detach();

        cx.subscribe(&tab_bar, |this, _tab_bar, event: &TabBarEvent, cx| {
            match event {
                TabBarEvent::Select(idx) => {
                    this.workspace.update(cx, |w, cx| {
                        w.active_editor_index = *idx;
                        cx.notify();
                    });
                }
                TabBarEvent::New => {
                    this.workspace.update(cx, |w, cx| {
                        w.new_tab();
                        cx.notify();
                    });
                }
                TabBarEvent::Close(idx) => {
                    let idx = *idx;
                    let is_modified = this.workspace.read(cx).editors.get(idx).map(|e| e.is_modified()).unwrap_or(false);
                    if is_modified {
                        this.workspace.update(cx, |w, cx| {
                            w.active_editor_index = idx;
                            cx.notify();
                        });
                        let filename = this.workspace.read(cx).editors[idx].path.as_ref()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or(this.i18n.get("status.no_name").to_string());
                        this.show_dialog(DialogType::UnsavedChanges { filename, intent: UnsavedChangesIntent::CloseTab }, None, cx);
                    } else {
                        this.workspace.update(cx, |w, cx| {
                            w.close_editor(idx);
                            cx.notify();
                        });
                    }
                }
            }
        }).detach();

        cx.subscribe(&sidebar, |this, _sidebar, event: &crate::widgets::sidebar_view::SidebarEvent, cx| {
            match event {
                crate::widgets::sidebar_view::SidebarEvent::NewFile(parent) => {
                    this.show_dialog(DialogType::NewFile { parent_dir: parent.clone() }, None, cx);
                }
                crate::widgets::sidebar_view::SidebarEvent::NewFolder(parent) => {
                    this.show_dialog(DialogType::NewFolder { parent_dir: parent.clone() }, None, cx);
                }
                crate::widgets::sidebar_view::SidebarEvent::Rename(target) => {
                    this.show_dialog(DialogType::Rename { target_path: target.clone() }, None, cx);
                }
                crate::widgets::sidebar_view::SidebarEvent::Delete(target) => {
                    this.show_dialog(DialogType::ConfirmDelete { target_path: target.clone() }, None, cx);
                }
                crate::widgets::sidebar_view::SidebarEvent::Refresh => {
                    this.workspace.update(cx, |w, cx| {
                        w.file_tree.refresh();
                        w.update_outline();
                        cx.notify();
                    });
                }
                crate::widgets::sidebar_view::SidebarEvent::ToggleShowHidden => {
                    this.workspace.update(cx, |w, cx| {
                        let new_val = w.file_tree.toggle_show_hidden();
                        w.config.show_hidden = new_val;
                        let _ = zee_core::config::Config::save_show_hidden(new_val);
                        cx.notify();
                    });
                }
            }
        }).detach();

        cx.subscribe(&find_panel, |_this, _find_panel, event: &FindPanelEvent, cx| {
            match event {
                FindPanelEvent::Close => {
                    cx.spawn(|_, cx: &mut AsyncApp| {
                        let cx = cx.clone();
                        async move {
                            cx.update(|cx| {
                                if let Some(window_handle) = cx.active_window() {
                                    let _ = cx.update_window(window_handle, |any_view, window, cx| {
                                        if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                                            view_handle.update(cx, |view, cx| {
                                                view.editor.update(cx, |editor, cx| {
                                                    editor.focus_handle.focus(window, cx);
                                                });
                                            });
                                        }
                                    });
                                }
                            });
                        }
                    }).detach();
                }
            }
        }).detach();

        Self {
            config,
            i18n,
            workspace,
            editor,
            sidebar,
            tab_bar,
            status_bar,
            find_panel,
            #[cfg(not(target_os = "macos"))]
            menu_bar,
            dialog: None,
            focus_handle,
        }
    }

    fn show_dialog(&mut self, dialog_type: DialogType, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| Dialog::new(self.workspace.clone(), self.i18n.clone(), dialog_type, cx));
        cx.subscribe(&dialog, |this, _dialog, event, cx| {
            match event {
                DialogEvent::Close => {
                    this.dialog = None;
                    cx.notify();
                    
                    // Defer focus restoration to avoid re-entrancy
                    cx.spawn(|_, cx: &mut AsyncApp| {
                        let cx = cx.clone();
                        async move {
                            cx.update(|cx| {
                                if let Some(window_handle) = cx.active_window() {
                                    let _ = cx.update_window(window_handle, |any_view, window, cx| {
                                        if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                                            view_handle.update(cx, |view, cx| {
                                                view.editor.update(cx, |editor, cx| {
                                                    editor.focus_handle.focus(window, cx);
                                                });
                                            });
                                        }
                                    });
                                }
                            });
                        }
                    }).detach();
                }
                DialogEvent::Save(intent) => {
                    let intent = *intent;
                    this.workspace.update(cx, |w, cx| {
                        let _ = w.save_active_editor();
                        if intent == UnsavedChangesIntent::Reload {
                            let _ = w.reload_active_editor();
                        }
                        cx.notify();
                    });
                    this.dialog = None;
                    cx.notify();
                    
                    cx.spawn(move |_, cx: &mut AsyncApp| {
                        let cx = cx.clone();
                        async move {
                            cx.update(|cx| {
                                // Restore focus
                                if let Some(window_handle) = cx.active_window() {
                                    let _ = cx.update_window(window_handle, |any_view, window, cx| {
                                        if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                                            view_handle.update(cx, |view, cx| {
                                                view.editor.update(cx, |editor, cx| {
                                                    editor.focus_handle.focus(window, cx);
                                                });
                                            });
                                        }
                                    });
                                }
                                
                                // Dispatch action
                                match intent {
                                    UnsavedChangesIntent::Quit => cx.dispatch_action(&Quit {}),
                                    UnsavedChangesIntent::CloseTab => cx.dispatch_action(&CloseTab {}),
                                    UnsavedChangesIntent::Reload => {},
                                }
                            });
                        }
                    }).detach();
                }
                DialogEvent::DontSave(intent) => {
                    let intent = *intent;
                    this.workspace.update(cx, |w, cx| {
                        if intent == UnsavedChangesIntent::Reload {
                            let _ = w.reload_active_editor();
                        } else {
                            w.close_active_editor();
                        }
                        cx.notify();
                    });
                    this.dialog = None;
                    cx.notify();
                    
                    cx.spawn(move |_, cx: &mut AsyncApp| {
                        let cx = cx.clone();
                        async move {
                            cx.update(|cx| {
                                // Restore focus
                                if let Some(window_handle) = cx.active_window() {
                                    let _ = cx.update_window(window_handle, |any_view, window, cx| {
                                        if let Ok(view_handle) = any_view.downcast::<WindowView>() {
                                            view_handle.update(cx, |view, cx| {
                                                view.editor.update(cx, |editor, cx| {
                                                    editor.focus_handle.focus(window, cx);
                                                });
                                            });
                                        }
                                    });
                                }
                                
                                // Dispatch action
                                match intent {
                                    UnsavedChangesIntent::Quit => cx.dispatch_action(&Quit {}),
                                    UnsavedChangesIntent::CloseTab => {},
                                    UnsavedChangesIntent::Reload => {},
                                }
                            });
                        }
                    }).detach();
                }
                DialogEvent::ExportConfig => {
                    this.dialog = None;
                    cx.notify();
                    Self::trigger_export(false, this.i18n.clone(), cx);
                }
                DialogEvent::ExportAll => {
                    this.dialog = None;
                    cx.notify();
                    Self::trigger_export(true, this.i18n.clone(), cx);
                }
                DialogEvent::ImportBackup => {
                    this.dialog = None;
                    cx.notify();
                    Self::trigger_import(this.i18n.clone(), cx);
                }
                DialogEvent::CreateFile { parent_dir, filename } => {
                    this.dialog = None;
                    let parent_dir = parent_dir.clone();
                    let filename = filename.clone();
                    this.workspace.update(cx, |w, cx| {
                        match w.file_tree.create_file(&parent_dir, &filename) {
                            Ok(path) => {
                                if let Ok(editor) = zee_core::buffer::Editor::from_file(&path) {
                                    w.add_editor(editor);
                                    w.update_outline();
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to create file: {}", e);
                            }
                        }
                        cx.notify();
                    });
                    cx.notify();
                }
                DialogEvent::CreateFolder { parent_dir, folder_name } => {
                    this.dialog = None;
                    let parent_dir = parent_dir.clone();
                    let folder_name = folder_name.clone();
                    this.workspace.update(cx, |w, cx| {
                        if let Err(e) = w.file_tree.create_folder(&parent_dir, &folder_name) {
                            eprintln!("Failed to create folder: {}", e);
                        }
                        cx.notify();
                    });
                    cx.notify();
                }
                DialogEvent::RenameItem { target_path, new_name } => {
                    this.dialog = None;
                    let target_path = target_path.clone();
                    let new_name = new_name.clone();
                    this.workspace.update(cx, |w, cx| {
                        match w.file_tree.rename_item(&target_path, &new_name) {
                            Ok(new_path) => {
                                for editor in &mut w.editors {
                                    if editor.path.as_ref() == Some(&target_path) {
                                        editor.path = Some(new_path.clone());
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to rename item: {}", e);
                            }
                        }
                        cx.notify();
                    });
                    cx.notify();
                }
                DialogEvent::DeleteItem { target_path } => {
                    this.dialog = None;
                    let target_path = target_path.clone();
                    this.workspace.update(cx, |w, cx| {
                        if let Ok(()) = w.file_tree.delete_item(&target_path) {
                            let to_close: Vec<usize> = w.editors.iter().enumerate()
                                .filter_map(|(idx, e)| {
                                    if let Some(p) = &e.path {
                                        if p == &target_path || p.starts_with(&target_path) {
                                            return Some(idx);
                                        }
                                    }
                                    None
                                })
                                .collect();
                            for idx in to_close.into_iter().rev() {
                                w.close_editor(idx);
                            }
                        }
                        cx.notify();
                    });
                    cx.notify();
                }
                _ => {}
            }
        }).detach();
        self.dialog = Some(dialog.clone());
        if let Some(window) = window {
            dialog.update(cx, |d, cx| d.focus(window, cx));
        } else {
            cx.spawn(|_, cx: &mut AsyncApp| {
                let cx = cx.clone();
                async move {
                    cx.update(|cx| {
                        if let Some(window_handle) = cx.active_window() {
                            let _ = cx.update_window(window_handle, |_any_view, window, cx| {
                                dialog.update(cx, |d, cx| {
                                    d.focus(window, cx);
                                });
                            });
                        }
                    });
                }
            }).detach();
        }
        cx.notify();
    }

    fn handle_new(&mut self, _: &New, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.new_tab();
            cx.notify();
        });
    }

    fn handle_new_tab(&mut self, _: &NewTab, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.new_tab();
            cx.notify();
        });
    }

    fn handle_new_window(&mut self, _: &NewWindow, _window: &mut Window, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let i18n = self.i18n.clone();
        cx.spawn(|_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                cx.update(|cx| {
                    crate::app::new_window(config, i18n, cx);
                });
            }
        }).detach();
    }

    fn handle_open(&mut self, _: &Open, _window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        cx.spawn(|_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let files = rfd::AsyncFileDialog::new()
                    .pick_files()
                    .await;
                
                if let Some(files) = files {
                    for file in files {
                        let path = file.path().to_path_buf();
                        if let Ok(editor) = Editor::from_file(&path) {
                            cx.update(|cx| {
                                workspace.update(cx, |w, cx| {
                                    w.add_editor(editor);
                                    cx.notify();
                                });
                            });
                        }
                    }
                }
            }
        }).detach();
    }

    fn handle_open_folder(&mut self, _: &OpenFolder, _window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        cx.spawn(|_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let folder = rfd::AsyncFileDialog::new()
                    .pick_folder()
                    .await;

                if let Some(folder) = folder {
                    let path = folder.path().to_path_buf();
                    cx.update(|cx| {
                        workspace.update(cx, |w, cx| {
                            w.set_root_path(path);
                            cx.notify();
                        });
                    });
                }
            }
        }).detach();
    }

    fn handle_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let path_opt = workspace.read(cx).active_editor().and_then(|e| e.path.clone());
        if path_opt.is_none() && workspace.read(cx).active_editor().is_none() {
            return;
        }
        if let Some(_path) = path_opt {
            workspace.update(cx, |w, cx| {
                let _ = w.save_active_editor();
                cx.notify();
            });
        } else {
            cx.spawn(|_, cx: &mut AsyncApp| {
                let cx = cx.clone();
                async move {
                    let file = rfd::AsyncFileDialog::new()
                        .save_file()
                        .await;
                    if let Some(file) = file {
                        let path = file.path().to_path_buf();
                        cx.update(|cx| {
                            workspace.update(cx, |w, cx| {
                                let _ = w.save_as_active_editor(&path);
                                cx.notify();
                            });
                        });
                    }
                }
            }).detach();
        }
    }

    fn handle_save_as(&mut self, _: &SaveAs, _window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        if workspace.read(cx).active_editor().is_none() {
            return;
        }
        cx.spawn(|_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .save_file()
                    .await;
                
                if let Some(file) = file {
                    let path = file.path().to_path_buf();
                    cx.update(|cx| {
                        workspace.update(cx, |w, cx| {
                            let _ = w.save_as_active_editor(&path);
                            cx.notify();
                        });
                    });
                }
            }
        }).detach();
    }

    fn handle_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        if workspace.editors.is_empty() {
            window.remove_window();
            return;
        }
        let editor = match workspace.active_editor() {
            Some(e) => e,
            None => {
                window.remove_window();
                return;
            }
        };
        let is_modified = editor.is_modified();
        if is_modified {
            let filename = editor.path.as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or(self.i18n.get("status.no_name").to_string());
            self.show_dialog(DialogType::UnsavedChanges { filename, intent: UnsavedChangesIntent::CloseTab }, Some(window), cx);
        } else {
            self.workspace.update(cx, |w, cx| {
                w.close_active_editor();
                cx.notify();
            });
        }
    }

    fn handle_next_tab(&mut self, _: &NextTab, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.next_tab();
            cx.notify();
        });
    }

    fn handle_prev_tab(&mut self, _: &PrevTab, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.prev_tab();
            cx.notify();
        });
    }

    fn handle_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.undo();
            }
            cx.notify();
        });
    }

    fn handle_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.redo();
            }
            cx.notify();
        });
    }

    fn handle_cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        let mut text_to_copy = None;
        self.workspace.update(cx, |w, cx| {
            let vi_mode = w.config.vi_mode;
            if let Some(editor) = w.active_editor_mut() {
                if editor.vi_mode == zee_core::ViMode::VisualBlock {
                    text_to_copy = Some(editor.get_visual_block_text());
                    editor.delete_visual_block();
                    editor.vi_mode = if vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                } else if let Some(range) = editor.selection.clone() {
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

    fn handle_copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        if let Some(editor) = workspace.active_editor() {
            if editor.vi_mode == zee_core::ViMode::VisualBlock {
                let text = editor.get_visual_block_text();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            } else if let Some(range) = editor.selection.clone() {
                let text = editor.rope.slice(range).to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    fn handle_paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                let text = text.clone();
                self.workspace.update(cx, |w, cx| {
                    let vi_mode = w.config.vi_mode;
                    if let Some(editor) = w.active_editor_mut() {
                        if editor.vi_mode == zee_core::ViMode::VisualBlock {
                            editor.delete_visual_block();
                            editor.vi_mode = if vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                        } else if let Some(range) = editor.selection.clone() {
                            editor.delete(range);
                        }
                        editor.insert(editor.cursor, &text);
                    }
                    cx.notify();
                });
            }
        }
    }

    fn handle_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.select_all();
            }
            cx.notify();
        });
    }

    fn handle_format_document(&mut self, _: &crate::app::FormatDocument, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform("format_json");
            cx.notify();
        });
    }

    fn handle_sort_lines(&mut self, _: &crate::app::SortLines, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform("sort_lines");
            cx.notify();
        });
    }

    fn handle_to_uppercase(&mut self, _: &crate::app::ToUpperCase, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform("to_uppercase");
            cx.notify();
        });
    }

    fn handle_to_lowercase(&mut self, _: &crate::app::ToLowerCase, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform("to_lowercase");
            cx.notify();
        });
    }

    fn handle_to_snake_case(&mut self, _: &crate::app::ToSnakeCase, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform("to_snake_case");
            cx.notify();
        });
    }

    fn handle_to_camel_case(&mut self, _: &crate::app::ToCamelCase, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform("to_camel_case");
            cx.notify();
        });
    }

    fn handle_find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.find_panel.update(cx, |p, cx| p.show(false, window, cx));
    }

    fn handle_replace(&mut self, _: &Replace, window: &mut Window, cx: &mut Context<Self>) {
        self.find_panel.update(cx, |p, cx| p.show(true, window, cx));
    }

    fn handle_toggle_sidebar(&mut self, _: &ToggleSidebar, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.toggle_sidebar();
            cx.notify();
        });
        cx.notify();
    }

    fn handle_toggle_outline(&mut self, _: &ToggleOutline, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.sidebar_visible = true;
            w.sidebar_tab = crate::workspace::SidebarTab::Outline;
            w.update_outline();
            cx.notify();
        });
        cx.notify();
    }

    fn handle_toggle_files(&mut self, _: &ToggleFiles, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.sidebar_visible = true;
            w.sidebar_tab = crate::workspace::SidebarTab::Files;
            cx.notify();
        });
        cx.notify();
    }

    fn handle_refresh_file_tree(&mut self, _: &RefreshFileTree, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.file_tree.refresh();
            w.update_outline();
            cx.notify();
        });
        cx.notify();
    }

    fn handle_toggle_line_numbers(&mut self, _: &ToggleLineNumbers, _window: &mut Window, cx: &mut Context<Self>) {
        self.config.line_numbers = !self.config.line_numbers;
        let line_numbers = self.config.line_numbers;
        let _ = Config::write_key("line_numbers", &line_numbers.to_string());
        self.workspace.update(cx, |w, cx| {
            w.config.line_numbers = line_numbers;
            cx.notify();
        });
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_toggle_word_wrap(&mut self, _: &ToggleWordWrap, _window: &mut Window, cx: &mut Context<Self>) {
        self.config.word_wrap = !self.config.word_wrap;
        let word_wrap = self.config.word_wrap;
        let _ = Config::write_key("word_wrap", &word_wrap.to_string());
        self.workspace.update(cx, |w, cx| {
            w.config.word_wrap = word_wrap;
            cx.notify();
        });
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_toggle_vi_mode(&mut self, _: &ToggleViMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.config.vi_mode = !self.config.vi_mode;
        let vi_mode = self.config.vi_mode;
        self.workspace.update(cx, |w, cx| {
            w.config.vi_mode = vi_mode;
            for editor in w.editors.iter_mut() {
                editor.vi_mode = if vi_mode { zee_core::ViMode::Normal } else { zee_core::ViMode::Insert };
                editor.selection = None;
                editor.selection_anchor = None;
            }
            cx.notify();
        });
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_toggle_trim_trailing_whitespace(&mut self, _: &ToggleTrimTrailingWhitespace, _window: &mut Window, cx: &mut Context<Self>) {
        self.config.trim_trailing_whitespace = !self.config.trim_trailing_whitespace;
        let val = self.config.trim_trailing_whitespace;
        let _ = Config::write_key("trim_trailing_whitespace", &val.to_string());
        self.workspace.update(cx, |w, cx| {
            w.config.trim_trailing_whitespace = val;
            cx.notify();
        });
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_toggle_ensure_final_newline(&mut self, _: &ToggleEnsureFinalNewline, _window: &mut Window, cx: &mut Context<Self>) {
        self.config.ensure_final_newline = !self.config.ensure_final_newline;
        let val = self.config.ensure_final_newline;
        let _ = Config::write_key("ensure_final_newline", &val.to_string());
        self.workspace.update(cx, |w, cx| {
            w.config.ensure_final_newline = val;
            cx.notify();
        });
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_set_encoding_utf8(&mut self, _: &SetEncodingUtf8, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::Utf8;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_shift_jis(&mut self, _: &SetEncodingShiftJis, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::ShiftJis;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_euc_jp(&mut self, _: &SetEncodingEucJp, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::EucJp;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_utf8_bom(&mut self, _: &SetEncodingUtf8Bom, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::Utf8Bom;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_utf16_le(&mut self, _: &SetEncodingUtf16Le, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::Utf16Le;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_utf16_be(&mut self, _: &SetEncodingUtf16Be, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::Utf16Be;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_iso_2022_jp(&mut self, _: &SetEncodingIso2022Jp, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::Iso2022Jp;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_encoding_latin1(&mut self, _: &SetEncodingLatin1, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = zee_core::Encoding::Latin1;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_line_ending_lf(&mut self, _: &SetLineEndingLf, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.line_ending = zee_core::LineEnding::Lf;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_line_ending_crlf(&mut self, _: &SetLineEndingCrlf, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.line_ending = zee_core::LineEnding::Crlf;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_line_ending_cr(&mut self, _: &SetLineEndingCr, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.line_ending = zee_core::LineEnding::Cr;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_theme(&mut self, action: &SetTheme, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_theme(&action.name, cx);
    }

    fn handle_set_syntax(&mut self, action: &SetSyntax, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Some(_buffer) = w.active_editor_mut() {
                println!("Setting syntax to {}", action.name);
            }
            cx.notify();
        });
        cx.notify();
    }

    fn handle_set_language(&mut self, action: &crate::app::SetLanguage, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_language(&action.id, cx);
    }

    fn set_language(&mut self, lang_id: &str, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.config.language = lang_id.to_string();
            let _ = Config::write_key("language", lang_id);
            cx.notify();
        });
    }

    fn set_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        let theme = zee_core::theme::Theme::find_by_name(name)
            .unwrap_or_default();
        
        self.workspace.update(cx, |w, cx| {
            w.theme = theme.clone();
            cx.notify();
        });
        
        let theme_slug = theme.meta.name.to_lowercase().replace(' ', "-");
        self.config.theme = theme.meta.name.clone();
        let _ = Config::write_key("theme", &theme_slug);
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_go_to_line(&mut self, _: &GoToLine, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(DialogType::GoToLine, Some(window), cx);
    }

    fn handle_open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(DialogType::Settings, Some(window), cx);
    }

    fn handle_zoom_in(&mut self, _: &ZoomIn, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            let size = (w.config.font_size + 1.0).min(48.0);
            w.config.font_size = size;
            w.config.line_height = (size * 1.55).round();
            let _ = Config::write_key("font_size", &format!("{:.1}", size));
            let _ = Config::write_key("line_height", &format!("{:.1}", w.config.line_height));
            cx.notify();
        });
        cx.notify();
    }

    fn handle_zoom_out(&mut self, _: &ZoomOut, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            let size = (w.config.font_size - 1.0).max(8.0);
            w.config.font_size = size;
            w.config.line_height = (size * 1.55).round();
            let _ = Config::write_key("font_size", &format!("{:.1}", size));
            let _ = Config::write_key("line_height", &format!("{:.1}", w.config.line_height));
            cx.notify();
        });
        cx.notify();
    }

    fn handle_reset_zoom(&mut self, _: &ResetZoom, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.config.font_size = 12.0;
            w.config.line_height = 19.0;
            let _ = Config::write_key("font_size", "12.0");
            let _ = Config::write_key("line_height", "19.0");
            cx.notify();
        });
        cx.notify();
    }

    pub fn has_modified_buffers(&self, cx: &App) -> bool {
        self.workspace.read(cx).has_modified_buffers()
    }

    pub fn handle_about(&mut self, _: &About, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(DialogType::About, Some(window), cx);
    }

    pub fn handle_check_for_updates(&mut self, _: &CheckForUpdates, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(DialogType::Update, Some(window), cx);
    }

    pub fn handle_quit(&mut self, _action: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        let mut modified_file = None;
        let mut target_idx = None;
        
        self.workspace.update(cx, |w, _| {
            for (idx, editor) in w.editors.iter().enumerate() {
                if editor.is_modified() {
                    modified_file = Some(editor.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or(self.i18n.get("status.no_name").to_string()));
                    target_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = target_idx {
                w.active_editor_index = idx;
            }
        });

        if let Some(filename) = modified_file {
            self.show_dialog(DialogType::UnsavedChanges { filename, intent: UnsavedChangesIntent::Quit }, Some(window), cx);
        } else {
            cx.propagate();
        }
    }

    pub fn handle_exit(&mut self, _action: &Exit, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_modified_buffers(cx) {
            self.handle_quit(&Quit {}, window, cx);
        } else {
            cx.propagate();
        }
    }

    fn handle_export_config(&mut self, _: &ExportConfig, _window: &mut Window, cx: &mut Context<Self>) {
        Self::trigger_export(false, self.i18n.clone(), cx);
    }

    fn handle_export_all(&mut self, _: &ExportAll, _window: &mut Window, cx: &mut Context<Self>) {
        Self::trigger_export(true, self.i18n.clone(), cx);
    }

    fn handle_import_config(&mut self, _: &ImportConfig, _window: &mut Window, cx: &mut Context<Self>) {
        Self::trigger_import(self.i18n.clone(), cx);
    }

    fn handle_manage_plugins(&mut self, _: &ManagePlugins, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(DialogType::PluginManager, Some(window), cx);
    }

    fn handle_open_plugins_folder(&mut self, _: &OpenPluginsFolder, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(dir) = zee_core::plugin::PluginManager::plugins_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = zee_core::selfupdate::open_url(&dir.to_string_lossy());
        }
    }

    fn handle_reload_file(&mut self, _: &ReloadFile, window: &mut Window, cx: &mut Context<Self>) {
        let is_modified = self.workspace.read(cx).active_editor().map(|e| e.is_modified()).unwrap_or(false);
        let file_name = self.workspace.read(cx).active_editor().and_then(|e| e.path.as_ref()).map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".to_string());

        if is_modified {
            self.show_dialog(DialogType::UnsavedChanges { filename: file_name, intent: UnsavedChangesIntent::Reload }, Some(window), cx);
        } else {
            self.workspace.update(cx, |w, cx| {
                let _ = w.reload_active_editor();
                w.update_outline();
                cx.notify();
            });
            cx.notify();
        }
    }

    fn handle_open_recent(&mut self, action: &OpenRecent, _window: &mut Window, cx: &mut Context<Self>) {
        let path = std::path::PathBuf::from(&action.path);
        if path.exists() {
            if let Ok(editor) = Editor::from_file(&path) {
                self.workspace.update(cx, |w, cx| {
                    w.add_editor(editor);
                    w.update_outline();
                    cx.notify();
                });
                #[cfg(target_os = "macos")]
                cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
                cx.notify();
            }
        }
    }

    fn handle_clear_recent(&mut self, _: &ClearRecent, _window: &mut Window, cx: &mut Context<Self>) {
        let mut recent = zee_core::recent::RecentFiles::load();
        recent.clear();
        #[cfg(target_os = "macos")]
        cx.set_menus(crate::app::build_native_menus(&self.i18n, &self.config));
        cx.notify();
    }

    fn handle_apply_template(&mut self, action: &ApplyTemplate, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.new_from_template(&action.id);
            w.update_outline();
            cx.notify();
        });
        cx.notify();
    }

    fn handle_open_templates_folder(&mut self, _: &OpenTemplatesFolder, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(dir) = zee_core::template::Template::templates_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = zee_core::selfupdate::open_url(&dir.to_string_lossy());
        }
    }

    fn trigger_export(include_plugins: bool, _i18n: I18n, cx: &mut Context<Self>) {
        let default_name = if include_plugins { "zee-backup.zip" } else { "zee-config.zip" };
        let view_handle = cx.entity().clone();
        cx.spawn(move |_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_file_name(default_name)
                    .add_filter("Zip Archive", &["zip"])
                    .add_filter("Tar Gz Archive", &["tar.gz", "tgz"])
                    .save_file()
                    .await;

                if let Some(file) = file {
                    let path = file.path().to_path_buf();
                    let res = zee_core::export_backup(&path, include_plugins);
                    cx.update(|cx| {
                        view_handle.update(cx, |this, cx| {
                            match res {
                                Ok(report) => {
                                    let title = this.i18n.get("dialog.backup.export_title").to_string();
                                    let msg = this.i18n.get("dialog.backup.export_success")
                                        .replace("{count}", &report.total_files.to_string())
                                        .replace("{path}", &path.to_string_lossy());
                                    this.show_dialog(DialogType::Message { title, message: msg }, None, cx);
                                }
                                Err(e) => {
                                    let title = this.i18n.get("dialog.backup.error_title").to_string();
                                    let msg = format!("{}", e);
                                    this.show_dialog(DialogType::Message { title, message: msg }, None, cx);
                                }
                            }
                        });
                    });
                }
            }
        }).detach();
    }

    fn trigger_import(_i18n: I18n, cx: &mut Context<Self>) {
        let view_handle = cx.entity().clone();
        cx.spawn(move |_, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .add_filter("Zee Backup / Config", &["zip", "tar.gz", "tgz", "toml"])
                    .pick_file()
                    .await;

                if let Some(file) = file {
                    let path = file.path().to_path_buf();
                    let res = zee_core::import_backup(&path);
                    cx.update(|cx| {
                        view_handle.update(cx, |this, cx| {
                            match res {
                                Ok(report) => {
                                    this.workspace.update(cx, |w, cx| {
                                        w.reload_config();
                                        w.reload_plugins();
                                        cx.notify();
                                    });
                                    let title = this.i18n.get("dialog.backup.import_title").to_string();
                                    let msg = this.i18n.get("dialog.backup.import_success")
                                        .replace("{count}", &report.total_files.to_string());
                                    this.show_dialog(DialogType::Message { title, message: msg }, None, cx);
                                }
                                Err(e) => {
                                    let title = this.i18n.get("dialog.backup.error_title").to_string();
                                    let msg = format!("{}", e);
                                    this.show_dialog(DialogType::Message { title, message: msg }, None, cx);
                                }
                            }
                        });
                    });
                }
            }
        }).detach();
    }

    fn handle_execute_plugin_command(&mut self, action: &ExecutePluginCommand, _window: &mut Window, cx: &mut Context<Self>) {
        let cmd = action.command.clone();
        self.workspace.update(cx, |w, cx| {
            w.apply_plugin_transform(&cmd);
            cx.notify();
        });
    }

    fn handle_reopen_with_encoding(&mut self, action: &ReopenWithEncoding, _window: &mut Window, cx: &mut Context<Self>) {
        let enc = self.parse_encoding(&action.encoding);
        self.workspace.update(cx, |w, cx| {
            if let Some(buffer) = w.active_editor() {
                if let Some(path) = buffer.path.clone() {
                    if let Ok(mut new_buffer) = Editor::from_file(&path) {
                        new_buffer.encoding = enc;
                        w.editors[w.active_editor_index] = new_buffer;
                        cx.notify();
                    }
                }
            }
        });
        cx.notify();
    }

    fn handle_convert_to_encoding(&mut self, action: &ConvertToEncoding, _window: &mut Window, cx: &mut Context<Self>) {
        let enc = self.parse_encoding(&action.encoding);
        self.workspace.update(cx, |w, cx| {
            if let Some(editor) = w.active_editor_mut() {
                editor.encoding = enc;
            }
            cx.notify();
        });
        cx.notify();
    }

    fn parse_encoding(&self, name: &str) -> zee_core::Encoding {
        match name {
            "UTF-8" => zee_core::Encoding::Utf8,
            "UTF-8 with BOM" => zee_core::Encoding::Utf8Bom,
            "UTF-16 LE" => zee_core::Encoding::Utf16Le,
            "UTF-16 BE" => zee_core::Encoding::Utf16Be,
            "Shift-JIS" => zee_core::Encoding::ShiftJis,
            "EUC-JP" => zee_core::Encoding::EucJp,
            "ISO-2022-JP" => zee_core::Encoding::Iso2022Jp,
            "Latin-1" => zee_core::Encoding::Latin1,
            _ => zee_core::Encoding::Utf8,
        }
    }

    fn led_color_to_gpui(&self, color: zee_core::theme::Color) -> Rgba {
        match color {
            zee_core::theme::Color::Rgb(r, g, b) => {
                Rgba {
                    r: r as f32 / 255.0,
                    g: g as f32 / 255.0,
                    b: b as f32 / 255.0,
                    a: 1.0,
                }
            }
            zee_core::theme::Color::Ansi(i) => {
                let (r, g, b) = match i {
                    0 => (0, 0, 0),
                    1 => (170, 0, 0),
                    2 => (0, 170, 0),
                    3 => (170, 170, 0),
                    4 => (0, 0, 170),
                    5 => (170, 0, 170),
                    6 => (0, 170, 170),
                    7 => (170, 170, 170),
                    8 => (85, 85, 85),
                    9 => (255, 85, 85),
                    10 => (85, 255, 85),
                    11 => (255, 255, 85),
                    12 => (85, 85, 255),
                    13 => (255, 85, 255),
                    14 => (85, 255, 255),
                    15 => (255, 255, 255),
                    _ => (128, 128, 128),
                };
                Rgba {
                    r: r as f32 / 255.0,
                    g: g as f32 / 255.0,
                    b: b as f32 / 255.0,
                    a: 1.0,
                }
            }
        }
    }
}

impl Render for WindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let bg = self.led_color_to_gpui(theme.editor.background);

        let root = div()
            .w_full()
            .h_full()
            .relative() // So dialog can be absolute
            .bg(bg)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_new))
            .on_action(cx.listener(Self::handle_new_tab))
            .on_action(cx.listener(Self::handle_new_window))
            .on_action(cx.listener(Self::handle_open))
            .on_action(cx.listener(Self::handle_open_folder))
            .on_action(cx.listener(Self::handle_save))
            .on_action(cx.listener(Self::handle_save_as))
            .on_action(cx.listener(Self::handle_close_tab))
            .on_action(cx.listener(Self::handle_next_tab))
            .on_action(cx.listener(Self::handle_prev_tab))
            .on_action(cx.listener(Self::handle_undo))
            .on_action(cx.listener(Self::handle_redo))
            .on_action(cx.listener(Self::handle_cut))
            .on_action(cx.listener(Self::handle_copy))
            .on_action(cx.listener(Self::handle_paste))
            .on_action(cx.listener(Self::handle_select_all))
            .on_action(cx.listener(Self::handle_format_document))
            .on_action(cx.listener(Self::handle_sort_lines))
            .on_action(cx.listener(Self::handle_to_uppercase))
            .on_action(cx.listener(Self::handle_to_lowercase))
            .on_action(cx.listener(Self::handle_to_snake_case))
            .on_action(cx.listener(Self::handle_to_camel_case))
            .on_action(cx.listener(Self::handle_find))
            .on_action(cx.listener(Self::handle_replace))
            .on_action(cx.listener(Self::handle_toggle_sidebar))
            .on_action(cx.listener(Self::handle_toggle_outline))
            .on_action(cx.listener(Self::handle_toggle_files))
            .on_action(cx.listener(Self::handle_refresh_file_tree))
            .on_action(cx.listener(Self::handle_toggle_line_numbers))
            .on_action(cx.listener(Self::handle_toggle_word_wrap))
            .on_action(cx.listener(Self::handle_toggle_vi_mode))
            .on_action(cx.listener(Self::handle_toggle_trim_trailing_whitespace))
            .on_action(cx.listener(Self::handle_toggle_ensure_final_newline))
            .on_action(cx.listener(Self::handle_set_encoding_utf8))
            .on_action(cx.listener(Self::handle_set_encoding_utf8_bom))
            .on_action(cx.listener(Self::handle_set_encoding_utf16_le))
            .on_action(cx.listener(Self::handle_set_encoding_utf16_be))
            .on_action(cx.listener(Self::handle_set_encoding_shift_jis))
            .on_action(cx.listener(Self::handle_set_encoding_euc_jp))
            .on_action(cx.listener(Self::handle_set_encoding_iso_2022_jp))
            .on_action(cx.listener(Self::handle_set_encoding_latin1))
            .on_action(cx.listener(Self::handle_reopen_with_encoding))
            .on_action(cx.listener(Self::handle_convert_to_encoding))
            .on_action(cx.listener(Self::handle_set_line_ending_lf))
            .on_action(cx.listener(Self::handle_set_line_ending_crlf))
            .on_action(cx.listener(Self::handle_set_line_ending_cr))
            .on_action(cx.listener(Self::handle_set_theme))
            .on_action(cx.listener(Self::handle_set_syntax))
            .on_action(cx.listener(Self::handle_set_language))
            .on_action(cx.listener(Self::handle_go_to_line))
            .on_action(cx.listener(Self::handle_open_settings))
            .on_action(cx.listener(Self::handle_export_config))
            .on_action(cx.listener(Self::handle_export_all))
            .on_action(cx.listener(Self::handle_import_config))
            .on_action(cx.listener(Self::handle_manage_plugins))
            .on_action(cx.listener(Self::handle_open_plugins_folder))
            .on_action(cx.listener(Self::handle_execute_plugin_command))
            .on_action(cx.listener(Self::handle_zoom_in))
            .on_action(cx.listener(Self::handle_zoom_out))
            .on_action(cx.listener(Self::handle_reset_zoom))
            .on_action(cx.listener(Self::handle_about))
            .on_action(cx.listener(Self::handle_check_for_updates))
            .on_action(cx.listener(Self::handle_quit))
            .on_action(cx.listener(Self::handle_exit))
            .on_action(cx.listener(Self::handle_reload_file))
            .on_action(cx.listener(Self::handle_open_recent))
            .on_action(cx.listener(Self::handle_clear_recent))
            .on_action(cx.listener(Self::handle_apply_template))
            .on_action(cx.listener(Self::handle_open_templates_folder))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                let mut opened_any = false;
                let mut dir_to_open = None;
                for path in paths.paths() {
                    if path.is_dir() {
                        if dir_to_open.is_none() {
                            dir_to_open = Some(path.clone());
                        }
                    } else if let Ok(editor) = Editor::from_file(path) {
                        this.workspace.update(cx, |w, cx| {
                            w.add_editor(editor);
                            w.update_outline();
                            cx.notify();
                        });
                        opened_any = true;
                    }
                }
                if let Some(dir) = dir_to_open {
                    this.workspace.update(cx, |w, cx| {
                        w.set_root_path(dir);
                        cx.notify();
                    });
                    opened_any = true;
                }
                if opened_any {
                    cx.notify();
                }
            }))
            .child(self.render_layout(cx));

        #[cfg(not(target_os = "macos"))]
        let root = root.child(self.render_menu_dropdown(cx));

        root.child(if let Some(ref dialog) = self.dialog {
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h_full()
                .child(dialog.clone())
        } else {
            div()
        })
    }
}

impl WindowView {
    fn render_layout(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div().w_full().h_full().flex().flex_col();

        #[cfg(not(target_os = "macos"))]
        let container = container.child(self.menu_bar.clone());

        let (is_sidebar_visible, is_right_sidebar) = {
            let w = self.workspace.read(cx);
            (w.sidebar_visible, w.config.sidebar_position != "left")
        };

        let main_area = div()
            .flex_grow()
            .flex()
            .flex_row()
            .w_full()
            .h_full()
            .overflow_hidden();

        let editor_wrapper = div().flex_grow().h_full().child(self.editor.clone());

        let main_area = if is_sidebar_visible {
            if is_right_sidebar {
                main_area
                    .child(editor_wrapper)
                    .child(self.sidebar.clone())
            } else {
                main_area
                    .child(self.sidebar.clone())
                    .child(editor_wrapper)
            }
        } else {
            main_area.child(editor_wrapper)
        };

        container
            .child(self.tab_bar.clone())
            .child(self.find_panel.clone())
            .child(main_area)
            .child(self.status_bar.clone())
    }
}

#[cfg(not(target_os = "macos"))]
impl WindowView {
    fn render_menu_dropdown(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_bar = self.menu_bar.read(cx);
        let open_menu = menu_bar.open_menu;

        if let Some(idx) = open_menu {
            let (bg, fg, border, hover_bg, muted_fg) = {
                let workspace = self.workspace.read(cx);
                let theme = &workspace.theme;
                let bg = led_color_to_gpui(theme.ui.menu_bar_bg);
                let fg = led_color_to_gpui(theme.ui.menu_bar_fg);
                let border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
                let hover_bg = with_alpha(fg, 0.15);
                let muted_fg = with_alpha(fg, 0.55);
                (bg, fg, border, hover_bg, muted_fg)
            };

            let left_pos = match idx {
                0 => px(8.0),
                1 => px(48.0),
                2 => px(90.0),
                3 => px(136.0),
                _ => px(196.0),
            };

            let menu_content = match idx {
                0 => self.render_file_menu(fg, hover_bg, muted_fg, border, cx).into_any_element(),
                1 => self.render_edit_menu(fg, hover_bg, muted_fg, border, cx).into_any_element(),
                2 => self.render_view_menu(fg, hover_bg, muted_fg, border, cx).into_any_element(),
                3 => self.render_plugins_menu(fg, hover_bg, muted_fg, border, cx).into_any_element(),
                _ => self.render_help_menu(fg, hover_bg, muted_fg, cx).into_any_element(),
            };

            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h_full()
                .child(
                    div()
                        .absolute()
                        .top(px(28.0))
                        .left_0()
                        .w_full()
                        .h_full()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                            this.menu_bar.update(cx, |m, cx| m.close_menu(cx));
                        }))
                )
                .child(
                    div()
                        .occlude()
                        .absolute()
                        .top(px(28.0))
                        .left(left_pos)
                        .w(px(230.0))
                        .bg(bg)
                        .text_color(fg)
                        .font_family(ui_font_family())
                        .text_size(px(12.5))
                        .border_1()
                        .border_color(border)
                        .rounded_sm()
                        .shadow_lg()
                        .py_1()
                        .child(menu_content)
                )
        } else {
            div()
        }
    }

    fn render_menu_item<A: Action + Clone + 'static>(
        &self,
        label: String,
        shortcut: Option<&'static str>,
        checked: bool,
        action: A,
        _fg: Rgba,
        hover_bg: Rgba,
        muted_fg: Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let action = action.clone();
        div()
            .h(px(26.0))
            .px_2()
            .mx_1()
            .flex()
            .items_center()
            .justify_between()
            .rounded_sm()
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                this.menu_bar.update(cx, |m, cx| m.close_menu(cx));
                window.dispatch_action(Box::new(action.clone()), cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .w(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .child(if checked { "✓" } else { "" })
                    )
                    .child(label)
            )
            .child(if let Some(sc) = shortcut {
                div().text_size(px(11.0)).text_color(muted_fg).child(sc)
            } else {
                div()
            })
    }

    fn render_menu_sep(&self, border: Rgba) -> impl IntoElement {
        div().h(px(1.0)).bg(border).my_1().mx_2()
    }

    fn render_file_menu(&self, fg: Rgba, hover_bg: Rgba, muted_fg: Rgba, border: Rgba, cx: &mut Context<Self>) -> impl IntoElement {
        let recent = zee_core::recent::RecentFiles::load();
        let templates = zee_core::template::Template::load_all();
        let mut menu = div()
            .flex()
            .flex_col()
            .child(self.render_menu_item(self.i18n.get("menu.file.new_tab").to_string(), Some("Ctrl+T"), false, NewTab {}, fg, hover_bg, muted_fg, cx));

        for tpl in templates.iter().take(4) {
            menu = menu.child(self.render_menu_item(
                format!("  + {}", tpl.name),
                None,
                false,
                ApplyTemplate { id: tpl.id.clone() },
                fg,
                hover_bg,
                muted_fg,
                cx,
            ));
        }

        menu = menu
            .child(self.render_menu_item(self.i18n.get("menu.file.new_window").to_string(), Some("Ctrl+N"), false, NewWindow {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.file.open").to_string(), Some("Ctrl+O"), false, Open {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.file.open_folder").to_string(), Some("Ctrl+Shift+O"), false, OpenFolder {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.file.reload").to_string(), Some("Ctrl+Shift+R"), false, ReloadFile {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border));

        if !recent.files.is_empty() {
            for p in recent.files.iter().take(5) {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned());
                menu = menu.child(self.render_menu_item(
                    format!("  {}", name),
                    None,
                    false,
                    OpenRecent { path: p.to_string_lossy().into_owned() },
                    fg,
                    hover_bg,
                    muted_fg,
                    cx,
                ));
            }
            menu = menu.child(self.render_menu_item(self.i18n.get("menu.file.clear_recent").to_string(), None, false, ClearRecent {}, fg, hover_bg, muted_fg, cx));
            menu = menu.child(self.render_menu_sep(border));
        }

        menu
            .child(self.render_menu_item(self.i18n.get("menu.file.save").to_string(), Some("Ctrl+S"), false, Save {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.file.save_as").to_string(), Some("Ctrl+Shift+S"), false, SaveAs {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.file.trim_trailing_whitespace").to_string(), None, self.config.trim_trailing_whitespace, ToggleTrimTrailingWhitespace {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.file.ensure_final_newline").to_string(), None, self.config.ensure_final_newline, ToggleEnsureFinalNewline {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.file.export_config").to_string(), None, false, ExportConfig {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.file.import_config").to_string(), None, false, ImportConfig {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.file.close").to_string(), Some("Ctrl+W"), false, CloseTab {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.file.exit").to_string(), Some("Ctrl+Q"), false, Exit {}, fg, hover_bg, muted_fg, cx))
    }

    fn render_edit_menu(&self, fg: Rgba, hover_bg: Rgba, muted_fg: Rgba, border: Rgba, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.render_menu_item(self.i18n.get("menu.edit.undo").to_string(), Some("Ctrl+Z"), false, Undo {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.redo").to_string(), Some("Ctrl+Y"), false, Redo {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.edit.cut").to_string(), Some("Ctrl+X"), false, Cut {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.copy").to_string(), Some("Ctrl+C"), false, Copy {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.paste").to_string(), Some("Ctrl+V"), false, Paste {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.edit.find").to_string(), Some("Ctrl+F"), false, Find {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.replace").to_string(), Some("Ctrl+H"), false, Replace {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.edit.select_all").to_string(), Some("Ctrl+A"), false, SelectAll {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.edit.format_document").to_string(), None, false, crate::app::FormatDocument {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.sort_lines").to_string(), None, false, crate::app::SortLines {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.edit.to_uppercase").to_string(), None, false, crate::app::ToUpperCase {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.to_lowercase").to_string(), None, false, crate::app::ToLowerCase {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.to_snake_case").to_string(), None, false, crate::app::ToSnakeCase {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.edit.to_camel_case").to_string(), None, false, crate::app::ToCamelCase {}, fg, hover_bg, muted_fg, cx))
    }

    fn render_view_menu(&self, fg: Rgba, hover_bg: Rgba, muted_fg: Rgba, border: Rgba, cx: &mut Context<Self>) -> impl IntoElement {
        let line_numbers_checked = self.config.line_numbers;
        let word_wrap_checked = self.config.word_wrap;
        let vi_mode_checked = self.config.vi_mode;

        let mut menu = div()
            .flex()
            .flex_col()
            .child(self.render_menu_item(self.i18n.get("menu.view.go_to_line").to_string(), None, false, GoToLine {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.view.zoom_in").to_string(), Some("Ctrl+="), false, ZoomIn {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.view.zoom_out").to_string(), Some("Ctrl+-"), false, ZoomOut {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.view.reset_zoom").to_string(), Some("Ctrl+0"), false, ResetZoom {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.view.sidebar").to_string(), Some("Ctrl+B"), false, ToggleSidebar {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.view.line_numbers").to_string(), None, line_numbers_checked, ToggleLineNumbers {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.view.word_wrap").to_string(), None, word_wrap_checked, ToggleWordWrap {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.view.vi_mode").to_string(), None, vi_mode_checked, ToggleViMode {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_sep(border));

        for lang in zee_core::i18n::AVAILABLE_LANGUAGES {
            let is_current = if lang.id == "auto" {
                self.config.language == "auto" || self.config.language.is_empty()
            } else {
                self.config.language == lang.id
            };
            let label = if lang.id == "auto" {
                format!("  {}", self.i18n.get("dialog.settings.language_auto"))
            } else {
                format!("  {}", lang.name)
            };
            menu = menu.child(self.render_menu_item(
                label,
                None,
                is_current,
                crate::app::SetLanguage { id: lang.id.to_string() },
                fg,
                hover_bg,
                muted_fg,
                cx,
            ));
        }

        menu
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.app.preferences").to_string(), Some("Ctrl+,"), false, OpenSettings {}, fg, hover_bg, muted_fg, cx))
    }

    fn render_plugins_menu(&self, fg: Rgba, hover_bg: Rgba, muted_fg: Rgba, border: Rgba, cx: &mut Context<Self>) -> impl IntoElement {
        let plugins = self.workspace.read(cx).plugin_manager.all_manifests();
        let mut menu = div()
            .flex()
            .flex_col();

        if plugins.is_empty() {
            menu = menu.child(self.render_menu_item(self.i18n.get("menu.plugins.no_plugins").to_string(), None, false, NoOp {}, fg, hover_bg, muted_fg, cx));
        } else {
            for manifest in plugins {
                if manifest.capabilities.commands.is_empty() {
                    menu = menu.child(self.render_menu_item(format!("✓ {}", manifest.name), None, false, NoOp {}, fg, hover_bg, muted_fg, cx));
                } else {
                    for cmd in &manifest.capabilities.commands {
                        menu = menu.child(self.render_menu_item(
                            format!("{}: {}", manifest.name, cmd),
                            None,
                            false,
                            ExecutePluginCommand { command: cmd.clone() },
                            fg,
                            hover_bg,
                            muted_fg,
                            cx,
                        ));
                    }
                }
            }
        }

        menu
            .child(self.render_menu_sep(border))
            .child(self.render_menu_item(self.i18n.get("menu.plugins.manage").to_string(), None, false, ManagePlugins {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.plugins.open_folder").to_string(), None, false, OpenPluginsFolder {}, fg, hover_bg, muted_fg, cx))
    }

    fn render_help_menu(&self, fg: Rgba, hover_bg: Rgba, muted_fg: Rgba, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.render_menu_item(self.i18n.get("menu.help.about").to_string(), None, false, About {}, fg, hover_bg, muted_fg, cx))
            .child(self.render_menu_item(self.i18n.get("menu.help.check_for_updates").to_string(), None, false, CheckForUpdates {}, fg, hover_bg, muted_fg, cx))
    }
}
