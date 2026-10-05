use gpui::*;
use crate::workspace::Workspace;
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};
use crate::app::GoToLine;

pub struct StatusBar {
    workspace: Entity<Workspace>,
}

impl StatusBar {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| {
            cx.notify();
        }).detach();
        Self { workspace }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.3);

        let editor = match workspace.active_editor() {
            Some(e) => e,
            None => {
                return div()
                    .h(px(26.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .bg(led_color_to_gpui(theme.ui.status_bar_bg))
                    .text_color(with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.5))
                    .text_size(px(11.5))
                    .font_family(ui_font_family())
                    .border_t_1()
                    .border_color(border_color)
                    .child(div().child("No open tabs"));
            }
        };
        
        let file_name = editor.path.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("[No Name]");
        
        let is_modified = editor.is_modified();
        let vi_mode_enabled = workspace.config.vi_mode;
        
        let (line, col) = editor.char_to_line_col(editor.cursor);

        let selection_info = editor.selection.as_ref().map(|sel| format!("{} chars", (sel.end as isize - sel.start as isize).abs()));

        let encoding = format!("{:?}", editor.encoding).to_uppercase();
        let line_ending = format!("{:?}", editor.line_ending).to_uppercase();
        let syntax = editor.syntax_highlighter.as_ref().map(|h| h.def.meta.name.clone()).unwrap_or_else(|| "Plain Text".to_string());

        let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.3);
        let hover_pill_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.12);
        let modified_color = led_color_to_gpui(theme.syntax.keyword.unwrap_or(theme.editor.cursor));

        let status_row = div()
            .h(px(24.0))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .bg(led_color_to_gpui(theme.ui.status_bar_bg))
            .text_color(led_color_to_gpui(theme.ui.status_bar_fg))
            .text_size(px(11.5))
            .font_family(ui_font_family())
            .border_t_1()
            .border_color(border_color)
            .child(
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                if is_modified {
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded_full()
                                        .bg(modified_color)
                                } else {
                                    div()
                                }
                            )
                            .child(file_name.to_string())
                    )
                    .children(if vi_mode_enabled {
                        let (badge_text, bg_color) = match editor.vi_mode {
                            zee_core::ViMode::Insert => ("INSERT", gpui::rgb(0x2e7d32)),
                            zee_core::ViMode::Visual => ("VISUAL", gpui::rgb(0x7b1fa2)),
                            zee_core::ViMode::VisualLine => ("V-LINE", gpui::rgb(0x8e24aa)),
                            zee_core::ViMode::VisualBlock => ("V-BLOCK", gpui::rgb(0x6a1b9a)),
                            zee_core::ViMode::Normal => ("NORMAL", gpui::rgb(0x1565c0)),
                        };
                        Some(
                            div()
                                .px_1p5()
                                .py(px(1.0))
                                .rounded_sm()
                                .text_size(px(10.0))
                                .bg(bg_color)
                                .text_color(gpui::rgb(0xffffff))
                                .child(badge_text)
                        )
                    } else {
                        None
                    })
            )
            .child(
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .children(selection_info.map(|info| {
                        div()
                            .h(px(20.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .bg(with_alpha(border_color, 0.5))
                            .child(info)
                    }))
                    .child(
                        div()
                            .h(px(20.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover_pill_bg))
                            .child(format!("Ln {}, Col {}", line + 1, col + 1))
                            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                                cx.dispatch_action(&GoToLine {});
                            }))
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .child(encoding)
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .child(line_ending)
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_sm()
                            .child(syntax)
                    )
            );

        if !vi_mode_enabled {
            return div().w_full().child(status_row);
        }

        // --- Bottom Row: Vim Command Line & Feedback Message Area ---
        let cmdline_bg = with_alpha(led_color_to_gpui(theme.ui.status_bar_bg), 0.95);
        let cmdline_border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.15);

        let cmdline_content = if let Some(ref cmd) = workspace.vi_cmd {
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .child(
                    div()
                        .font_family(crate::widgets::mono_font_family())
                        .text_size(px(12.0))
                        .text_color(led_color_to_gpui(theme.ui.status_bar_fg))
                        .child(cmd.clone())
                )
                .child(
                    div()
                        .w(px(7.0))
                        .h(px(14.0))
                        .bg(led_color_to_gpui(theme.editor.cursor))
                )
        } else if let Some((ref msg, is_err)) = workspace.vi_message {
            let msg_color = if is_err {
                gpui::rgb(0xff5555)
            } else {
                led_color_to_gpui(theme.ui.status_bar_fg)
            };
            div()
                .font_family(crate::widgets::mono_font_family())
                .text_size(px(12.0))
                .text_color(msg_color)
                .child(msg.clone())
        } else {
            let hint = match editor.vi_mode {
                zee_core::ViMode::Insert => "-- INSERT --",
                zee_core::ViMode::Visual => "-- VISUAL --",
                zee_core::ViMode::VisualLine => "-- VISUAL LINE --",
                zee_core::ViMode::VisualBlock => "-- VISUAL BLOCK --",
                zee_core::ViMode::Normal => "",
            };
            div()
                .font_family(crate::widgets::mono_font_family())
                .text_size(px(12.0))
                .text_color(with_alpha(led_color_to_gpui(theme.ui.status_bar_fg), 0.7))
                .child(hint)
        };

        let cmdline_row = div()
            .h(px(22.0))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .bg(cmdline_bg)
            .border_t_1()
            .border_color(cmdline_border)
            .child(cmdline_content);

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(status_row)
            .child(cmdline_row)
    }
}

