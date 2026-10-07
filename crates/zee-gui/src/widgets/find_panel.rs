use gpui::*;
use crate::workspace::Workspace;
use zee_core::search::{SearchQuery, SearchFlags};
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};

#[derive(Clone, Debug)]
pub enum FindPanelEvent {
    Close,
}

pub struct FindPanel {
    workspace: Entity<Workspace>,
    find_text: String,
    replace_text: String,
    find_focus: FocusHandle,
    replace_focus: FocusHandle,
    is_replace_mode: bool,
    match_case: bool,
    whole_word: bool,
    use_regex: bool,
    is_visible: bool,
    replace_status: Option<String>,
}

impl EventEmitter<FindPanelEvent> for FindPanel {}

impl FindPanel {
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            find_text: String::new(),
            replace_text: String::new(),
            find_focus: cx.focus_handle(),
            replace_focus: cx.focus_handle(),
            is_replace_mode: false,
            match_case: false,
            whole_word: false,
            use_regex: false,
            is_visible: false,
            replace_status: None,
        }
    }

    pub fn show(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.is_visible = true;
        self.is_replace_mode = replace;
        self.replace_status = None;
        if replace {
            self.replace_focus.focus(window, cx);
        } else {
            self.find_focus.focus(window, cx);
        }
        if !self.find_text.is_empty() {
            self.run_search(cx);
        }
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.is_visible = false;
        cx.emit(FindPanelEvent::Close);
        cx.notify();
    }

    fn run_search(&self, cx: &mut Context<Self>) {
        let query = SearchQuery {
            pattern: self.find_text.clone(),
            flags: SearchFlags {
                match_case: self.match_case,
                whole_word: self.whole_word,
                use_regex: self.use_regex,
            },
        };
        self.workspace.update(cx, |w, _| {
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            if query.pattern.is_empty() {
                editor.find_results.clear();
                editor.current_match_idx = None;
                return;
            }
            editor.find_results = editor.search(&query);
            if !editor.find_results.is_empty() {
                editor.current_match_idx = Some(0);
                editor.cursor = editor.find_results[0].char_range.start;
                editor.selection = Some(editor.find_results[0].char_range.clone());
            } else {
                editor.current_match_idx = None;
                editor.selection = None;
            }
        });
    }

    fn handle_search_next(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, _| {
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            if editor.find_results.is_empty() { return; }
            let idx = match editor.current_match_idx {
                Some(i) => (i + 1) % editor.find_results.len(),
                None => 0,
            };
            editor.current_match_idx = Some(idx);
            let m = &editor.find_results[idx];
            editor.cursor = m.char_range.start;
            editor.selection = Some(m.char_range.clone());
        });
        cx.notify();
    }

    fn handle_search_prev(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, _| {
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            if editor.find_results.is_empty() { return; }
            let idx = match editor.current_match_idx {
                Some(i) => if i == 0 { editor.find_results.len() - 1 } else { i - 1 },
                None => 0,
            };
            editor.current_match_idx = Some(idx);
            let m = &editor.find_results[idx];
            editor.cursor = m.char_range.start;
            editor.selection = Some(m.char_range.clone());
        });
        cx.notify();
    }

    fn handle_search_replace(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mut replaced = false;
        self.workspace.update(cx, |w, _| {
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            if let Some(idx) = editor.current_match_idx {
                if idx < editor.find_results.len() {
                    let m = editor.find_results[idx].clone();
                    editor.delete(m.char_range);
                    editor.insert(editor.cursor, &self.replace_text);
                    replaced = true;
                }
            }
        });
        if replaced {
            self.replace_status = Some("Replaced 1 occurrence".to_string());
        } else {
            self.replace_status = Some("No match to replace".to_string());
        }
        self.run_search(cx);
        cx.notify();
    }

    fn handle_search_replace_all(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mut count = 0;
        self.workspace.update(cx, |w, _| {
            let editor = match w.active_editor_mut() {
                Some(e) => e,
                None => return,
            };
            let results = editor.find_results.clone();
            count = results.len();
            for m in results.into_iter().rev() {
                editor.delete(m.char_range);
                editor.insert(editor.cursor, &self.replace_text);
            }
        });
        if count == 0 {
            self.replace_status = Some("No matches found".to_string());
        } else if count == 1 {
            self.replace_status = Some("Replaced 1 occurrence".to_string());
        } else {
            self.replace_status = Some(format!("Replaced {} occurrences", count));
        }
        self.run_search(cx);
        cx.notify();
    }

    fn handle_find_keydown(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => {
                if event.keystroke.modifiers.shift {
                    self.handle_search_prev(window, cx);
                } else {
                    self.handle_search_next(window, cx);
                }
            }
            "tab"
                if self.is_replace_mode => {
                    self.replace_focus.focus(window, cx);
                }
            "escape" => self.hide(cx),
            "backspace" => {
                self.find_text.pop();
                self.run_search(cx);
            }
            k if k.len() == 1 => {
                self.find_text.push_str(k);
                self.run_search(cx);
            }
            _ => {}
        }
        cx.notify();
    }

    fn handle_replace_keydown(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "enter" => {
                self.handle_search_replace(window, cx);
            }
            "tab" => {
                self.find_focus.focus(window, cx);
            }
            "escape" => self.hide(cx),
            "backspace" => {
                self.replace_text.pop();
            }
            k if k.len() == 1 => {
                self.replace_text.push_str(k);
            }
            _ => {}
        }
        cx.notify();
    }
}

impl Render for FindPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_visible {
            return div().h_0();
        }

        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let bg = led_color_to_gpui(theme.ui.panel_bg);
        let fg = led_color_to_gpui(theme.ui.panel_fg);
        let border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
        let input_bg = led_color_to_gpui(theme.editor.background);
        let accent = led_color_to_gpui(theme.syntax.keyword.unwrap_or(theme.editor.cursor));
        let button_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.08);
        let button_hover = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.18);

        let (match_count, current_idx) = if let Some(editor) = workspace.active_editor() {
            (editor.find_results.len(), editor.current_match_idx)
        } else {
            (0, None)
        };
        let match_badge = if self.find_text.is_empty() {
            "".to_string()
        } else if match_count == 0 {
            "No results".to_string()
        } else if let Some(idx) = current_idx {
            format!("{} of {}", idx + 1, match_count)
        } else {
            format!("{} results", match_count)
        };

        let match_case = self.match_case;
        let whole_word = self.whole_word;
        let use_regex = self.use_regex;

        let is_find_focused = self.find_focus.is_focused(window);
        let is_replace_focused = self.replace_focus.is_focused(window);

        div()
            .w_full()
            .flex()
            .flex_col()
            .bg(bg)
            .text_color(fg)
            .border_b_1()
            .border_color(border)
            .font_family(ui_font_family())
            .px_3()
            .py_2()
            .gap_1p5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_grow()
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(input_bg)
                            .border_1()
                            .border_color(if is_find_focused { accent } else { border })
                            .rounded_md()
                            .px_2p5()
                            .track_focus(&self.find_focus)
                            .on_key_down(cx.listener(Self::handle_find_keydown))
                            .child(
                                if self.find_text.is_empty() {
                                    div().text_color(with_alpha(fg, 0.45)).text_size(px(12.0)).child("Find...".to_string())
                                } else {
                                    div().text_size(px(12.0)).child(self.find_text.clone())
                                }
                            )
                            .children(if !match_badge.is_empty() {
                                Some(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(if match_count == 0 { gpui::rgb(0xe53935) } else { with_alpha(fg, 0.65) })
                                        .child(match_badge)
                                )
                            } else {
                                None
                            })
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(26.0))
                                    .px_2()
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .bg(if match_case { accent } else { button_bg })
                                    .text_color(if match_case { gpui::rgb(0xffffff) } else { fg })
                                    .hover(move |s| if !match_case { s.bg(button_hover) } else { s })
                                    .child("Aa")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.match_case = !this.match_case;
                                        this.run_search(cx);
                                        cx.notify();
                                    }))
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(26.0))
                                    .px_2()
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .bg(if whole_word { accent } else { button_bg })
                                    .text_color(if whole_word { gpui::rgb(0xffffff) } else { fg })
                                    .hover(move |s| if !whole_word { s.bg(button_hover) } else { s })
                                    .child("\\b")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.whole_word = !this.whole_word;
                                        this.run_search(cx);
                                        cx.notify();
                                    }))
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(26.0))
                                    .px_2()
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .bg(if use_regex { accent } else { button_bg })
                                    .text_color(if use_regex { gpui::rgb(0xffffff) } else { fg })
                                    .hover(move |s| if !use_regex { s.bg(button_hover) } else { s })
                                    .child(".*")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        this.use_regex = !this.use_regex;
                                        this.run_search(cx);
                                        cx.notify();
                                    }))
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(26.0))
                                    .h(px(26.0))
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(button_hover))
                                    .child("▲")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.handle_search_prev(window, cx)))
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(26.0))
                                    .h(px(26.0))
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(12.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(button_hover))
                                    .child("▼")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.handle_search_next(window, cx)))
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(24.0))
                            .h(px(24.0))
                            .rounded_md()
                            .text_size(px(14.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba(0xffffff22)))
                            .child("×")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.hide(cx)))
                    )
            )
            .child(if self.is_replace_mode {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_grow()
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(input_bg)
                            .border_1()
                            .border_color(if is_replace_focused { accent } else { border })
                            .rounded_md()
                            .px_2p5()
                            .track_focus(&self.replace_focus)
                            .on_key_down(cx.listener(Self::handle_replace_keydown))
                            .child(
                                if self.replace_text.is_empty() {
                                    div().text_color(with_alpha(fg, 0.45)).text_size(px(12.0)).child("Replace with...".to_string())
                                } else {
                                    div().text_size(px(12.0)).child(self.replace_text.clone())
                                }
                            )
                            .children(self.replace_status.as_ref().map(|status| div()
                                        .text_size(px(10.5))
                                        .text_color(with_alpha(fg, 0.7))
                                        .child(status.clone())))
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(26.0))
                                    .px_2p5()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(button_hover))
                                    .child("Replace")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.handle_search_replace(window, cx)))
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(26.0))
                                    .px_2p5()
                                    .rounded_md()
                                    .bg(button_bg)
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(button_hover))
                                    .child("Replace All")
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.handle_search_replace_all(window, cx)))
                            )
                    )
                    .child(
                        // Spacer to align with top close button
                        div().w(px(24.0))
                    )
            } else {
                div()
            })
    }
}

