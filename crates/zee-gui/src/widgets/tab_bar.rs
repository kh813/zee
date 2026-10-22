use gpui::*;
use crate::workspace::Workspace;
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};

#[derive(Clone, Debug)]
pub enum TabBarEvent {
    Select(usize),
    Close(usize),
    New,
}

pub struct TabBar {
    workspace: Entity<Workspace>,
    scroll_offset: Pixels,
}

impl EventEmitter<TabBarEvent> for TabBar {}

impl TabBar {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| {
            cx.notify();
        }).detach();
        Self { 
            workspace,
            scroll_offset: px(0.0),
        }
    }

    fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(1.0)).x;
        self.scroll_offset = (self.scroll_offset + delta).min(px(0.0));
        cx.notify();
    }
}

impl Render for TabBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let active_index = workspace.active_editor_index;

        let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
        let hover_tab_bg = with_alpha(led_color_to_gpui(theme.ui.tab_active_bg), 0.4);

        let mut bar = div()
            .h(px(36.0))
            .w_full()
            .flex()
            .items_end()
            .px_2()
            .bg(led_color_to_gpui(theme.ui.tab_bar_bg))
            .border_b_1()
            .border_color(border_color)
            .font_family(ui_font_family())
            .on_scroll_wheel(cx.listener(|this, event, _, cx| {
                this.handle_scroll(event, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_grow()
                    .h_full()
                    .items_end()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .items_end()
                            .gap_1()
                            .ml(self.scroll_offset)
                            .children(
                                workspace.editors.iter().enumerate().map(|(idx, editor)| {
                                    let is_active = idx == active_index;
                                    let file_name = editor.path.as_ref()
                                        .and_then(|p| p.file_name())
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("[No Name]");
                                    let is_modified = editor.is_modified();
                                    let is_ro = editor.read_only;

                                    let bg_color = if is_active { 
                                        led_color_to_gpui(theme.ui.tab_active_bg) 
                                    } else { 
                                        led_color_to_gpui(theme.ui.tab_inactive_bg) 
                                    };
                                    let text_color = if is_active { 
                                        led_color_to_gpui(theme.ui.tab_active_fg) 
                                    } else { 
                                        led_color_to_gpui(theme.ui.tab_inactive_fg) 
                                    };

                                    div()
                                        .flex()
                                        .items_center()
                                        .h(px(30.0))
                                        .px_3()
                                        .rounded_t_md()
                                        .bg(bg_color)
                                        .text_color(text_color)
                                        .text_size(px(12.5))
                                        .cursor_pointer()
                                        .border_t_1()
                                        .border_l_1()
                                        .border_r_1()
                                        .border_color(if is_active { border_color } else { rgba(0x00000000) })
                                        .hover(move |s| {
                                            if !is_active {
                                                s.bg(hover_tab_bg)
                                            } else {
                                                s
                                            }
                                        })
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                            this.workspace.update(cx, |w, cx| {
                                                w.active_editor_index = idx;
                                                cx.notify();
                                            });
                                            cx.emit(TabBarEvent::Select(idx));
                                        }))
                                        .child(
                                            div()
                                                 .flex()
                                                 .items_center()
                                                 .gap_2()
                                                 .child(file_name.to_string())
                                                 .children(if is_ro {
                                                     Some(
                                                         div()
                                                             .text_size(px(10.0))
                                                             .px_1()
                                                             .rounded_sm()
                                                             .bg(with_alpha(border_color, 0.4))
                                                             .child("RO")
                                                     )
                                                 } else {
                                                     None
                                                 })
                                                 .children(if is_modified {
                                                     Some(
                                                         div()
                                                             .w(px(6.0))
                                                             .h(px(6.0))
                                                             .rounded_full()
                                                             .bg(text_color)
                                                     )
                                                 } else {
                                                     None
                                                 })
                                        )
                                        .child(
                                            div()
                                                 .flex()
                                                 .items_center()
                                                 .justify_center()
                                                 .w(px(16.0))
                                                 .h(px(16.0))
                                                 .ml_2()
                                                 .rounded_sm()
                                                 .text_size(px(13.0))
                                                 .hover(|s| s.bg(rgba(0xffffff22)))
                                                 .child("×")
                                                 .on_mouse_down(MouseButton::Left, cx.listener(move |_this, _, _, cx| {
                                                     cx.stop_propagation();
                                                     cx.emit(TabBarEvent::Close(idx));
                                                 }))
                                        )
                                }).collect::<Vec<_>>()
                            )
                    )
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(26.0))
                    .h(px(26.0))
                    .mb_1()
                    .ml_2()
                    .rounded_md()
                    .text_size(px(16.0))
                    .text_color(led_color_to_gpui(theme.ui.tab_inactive_fg))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(0xffffff18)).text_color(gpui::rgb(0xffffff)))
                    .child("+")
                    .on_mouse_down(MouseButton::Left, cx.listener(|_this, _, _, cx| {
                        cx.stop_propagation();
                        cx.emit(TabBarEvent::New);
                    }))
            );

        #[cfg(target_os = "macos")]
        if let Some(ref ver) = workspace.available_update {
            let i18n = zee_core::i18n::I18n::load(&workspace.config.language);
            let label = format!("⬆ v{} {}", ver, i18n.get("menu.help.update_available"));
            bar = bar.child(
                div()
                    .ml_auto()
                    .mr_2()
                    .mb_1()
                    .h(px(22.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(with_alpha(gpui::rgb(0x2563eb), 0.2))
                    .border_1()
                    .border_color(with_alpha(gpui::rgb(0x3b82f6), 0.4))
                    .text_color(gpui::rgb(0x60a5fa))
                    .text_size(px(11.0))
                    .hover(|s| s.bg(with_alpha(gpui::rgb(0x2563eb), 0.35)).border_color(with_alpha(gpui::rgb(0x3b82f6), 0.7)))
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::app::CheckForUpdates {}), cx);
                    }))
                    .child(label),
            );
        }

        bar
    }
}


