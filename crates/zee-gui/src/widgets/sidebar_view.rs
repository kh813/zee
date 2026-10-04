use gpui::*;
use crate::workspace::{Workspace, SidebarTab};
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};
use zee_core::outline::{self, FlatOutlineItem, OutlineNode};
use zee_core::file_tree::FlatFileItem;
use zee_core::buffer::Editor;
use zee_core::i18n::I18n;

#[derive(Clone)]
struct ActiveFileProps {
    file_name: String,
    size_str: String,
    lines: usize,
    chars: usize,
    encoding: &'static str,
    line_ending: &'static str,
}

fn format_file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub struct SidebarView {
    pub workspace: Entity<Workspace>,
    pub i18n: I18n,
    pub focus_handle: FocusHandle,
    pub show_properties: bool,
}

impl SidebarView {
    pub fn new(workspace: Entity<Workspace>, i18n: I18n, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| {
            cx.notify();
        }).detach();

        Self {
            workspace,
            i18n,
            focus_handle: cx.focus_handle(),
            show_properties: true,
        }
    }

    fn toggle_outline_node(nodes: &mut [OutlineNode], line: usize) -> bool {
        for node in nodes.iter_mut() {
            if node.line == line {
                node.is_expanded = !node.is_expanded;
                return true;
            }
            if Self::toggle_outline_node(&mut node.children, line) {
                return true;
            }
        }
        false
    }
}

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (active_tab, is_right_sidebar, bg_color, text_color, active_fg, muted_fg, border_color, hover_bg, sel_bg, file_items, active_path, outline_nodes, active_props) = {
            let workspace = self.workspace.read(cx);
            let theme = &workspace.theme;

            let bg_color = led_color_to_gpui(theme.ui.panel_bg);
            let text_color = led_color_to_gpui(theme.ui.panel_fg);
            let active_fg = led_color_to_gpui(theme.syntax.keyword.unwrap_or(theme.ui.menu_item_active_fg));
            let muted_fg = with_alpha(text_color, 0.55);
            let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
            let hover_bg = with_alpha(led_color_to_gpui(theme.editor.selection), 0.5);
            let sel_bg = with_alpha(led_color_to_gpui(theme.editor.selection), 0.85);

            let active_tab = workspace.sidebar_tab;
            let is_right_sidebar = workspace.config.sidebar_position != "left";
            let file_items = workspace.file_tree.flatten();
            let active_path = workspace.active_editor().and_then(|e| e.path.clone());
            let outline_nodes = workspace.outline_nodes.clone();

            let active_props = workspace.active_editor().map(|editor| {
                let file_name = editor.path.as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.i18n.get("status.no_name").to_string());

                let bytes = editor.path.as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len() as usize)
                    .unwrap_or_else(|| editor.rope.len_bytes());

                ActiveFileProps {
                    file_name,
                    size_str: format_file_size(bytes),
                    lines: editor.rope.len_lines(),
                    chars: editor.rope.len_chars(),
                    encoding: editor.encoding.name(),
                    line_ending: editor.line_ending.name(),
                }
            });

            (active_tab, is_right_sidebar, bg_color, text_color, active_fg, muted_fg, border_color, hover_bg, sel_bg, file_items, active_path, outline_nodes, active_props)
        };

        // Render header tabs
        let header = div()
            .h(px(32.0))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .border_b_1()
            .border_color(border_color)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(if active_tab == SidebarTab::Files { active_fg } else { muted_fg })
                            .font_weight(if active_tab == SidebarTab::Files { FontWeight::BOLD } else { FontWeight::NORMAL })
                            .bg(if active_tab == SidebarTab::Files { with_alpha(active_fg, 0.12) } else { Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 } })
                            .hover(move |s| s.bg(with_alpha(active_fg, 0.18)))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.workspace.update(cx, |w, cx| {
                                    w.sidebar_tab = SidebarTab::Files;
                                    cx.notify();
                                });
                            }))
                            .child("Files")
                    )
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(if active_tab == SidebarTab::Outline { active_fg } else { muted_fg })
                            .font_weight(if active_tab == SidebarTab::Outline { FontWeight::BOLD } else { FontWeight::NORMAL })
                            .bg(if active_tab == SidebarTab::Outline { with_alpha(active_fg, 0.12) } else { Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 } })
                            .hover(move |s| s.bg(with_alpha(active_fg, 0.18)))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.workspace.update(cx, |w, cx| {
                                    w.sidebar_tab = SidebarTab::Outline;
                                    w.update_outline();
                                    cx.notify();
                                });
                            }))
                            .child("Outline")
                    )
            )
            .child(
                div()
                    .px_1()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .text_color(muted_fg)
                    .hover(move |s| s.text_color(active_fg))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.workspace.update(cx, |w, cx| {
                            w.file_tree.refresh();
                            w.update_outline();
                            cx.notify();
                        });
                    }))
                    .child("↻")
            );

        let content = match active_tab {
            SidebarTab::Files => self.render_files(file_items, active_path.as_ref(), text_color, active_fg, muted_fg, hover_bg, sel_bg, cx).into_any_element(),
            SidebarTab::Outline => self.render_outline(&outline_nodes, text_color, active_fg, muted_fg, hover_bg, cx).into_any_element(),
        };

        let mut container = div()
            .w(px(240.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(bg_color)
            .border_color(border_color)
            .font_family(ui_font_family())
            .track_focus(&self.focus_handle);

        if is_right_sidebar {
            container = container.border_l_1();
        } else {
            container = container.border_r_1();
        }

        let properties_panel = self.render_file_properties(
            active_props,
            text_color,
            muted_fg,
            border_color,
            hover_bg,
            cx,
        );

        container
            .child(header)
            .child(
                div()
                    .id("sidebar-content-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .py_1()
                    .child(content)
            )
            .child(properties_panel)
    }
}

impl SidebarView {
    #[allow(clippy::too_many_arguments)]
    fn render_files(
        &self,
        items: Vec<FlatFileItem>,
        active_path: Option<&std::path::PathBuf>,
        text_color: Rgba,
        active_fg: Rgba,
        muted_fg: Rgba,
        hover_bg: Rgba,
        sel_bg: Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if items.is_empty() {
            return div()
                .px_3()
                .py_2()
                .text_size(px(12.0))
                .text_color(muted_fg)
                .child(self.i18n.get("sidebar.no_files").to_string())
                .into_any_element();
        }

        div()
            .min_w_full()
            .flex()
            .flex_col()
            .children(
                items.into_iter().map(|item| {
                    let path = item.path.clone();
                    let is_dir = item.is_dir;
                    let is_current = active_path.is_some_and(|p| *p == item.path);
                    let depth_px = px((item.depth as f32) * 14.0 + 8.0);

                    let item_fg = if is_current {
                        active_fg
                    } else if is_dir {
                        text_color
                    } else {
                        with_alpha(text_color, 0.85)
                    };

                    let icon = if is_dir {
                        if item.is_expanded { "▼" } else { "▶" }
                    } else {
                        "•"
                    };

                    let path_for_click = path.clone();
                    div()
                        .h(px(24.0))
                        .min_w_full()
                        .flex()
                        .items_center()
                        .pl(depth_px)
                        .pr_2()
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .text_color(item_fg)
                        .font_weight(if is_current || is_dir { FontWeight::BOLD } else { FontWeight::NORMAL })
                        .bg(if is_current { sel_bg } else { Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 } })
                        .hover(move |s| s.bg(hover_bg))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            let p = path_for_click.clone();
                            if is_dir {
                                this.workspace.update(cx, |w, cx| {
                                    w.file_tree.toggle_expand(&p);
                                    cx.notify();
                                });
                            } else {
                                this.workspace.update(cx, |w, cx| {
                                    if let Ok(editor) = Editor::from_file(&p) {
                                        w.add_editor(editor);
                                        w.update_outline();
                                        cx.notify();
                                    }
                                });
                            }
                        }))
                        .child(
                            div()
                                .w(px(14.0))
                                .flex_shrink_0()
                                .text_size(px(10.0))
                                .text_color(if is_dir { muted_fg } else { with_alpha(muted_fg, 0.4) })
                                .child(icon)
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .child(item.name)
                        )
                })
            )
            .into_any_element()
    }

    fn render_outline(
        &self,
        outline_nodes: &[OutlineNode],
        text_color: Rgba,
        active_fg: Rgba,
        muted_fg: Rgba,
        hover_bg: Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut flat: Vec<FlatOutlineItem> = Vec::new();
        outline::flatten_outline(outline_nodes, 0, &mut flat);

        if flat.is_empty() {
            return div()
                .px_3()
                .py_2()
                .text_size(px(12.0))
                .text_color(muted_fg)
                .child(self.i18n.get("sidebar.no_headings").to_string())
                .into_any_element();
        }

        div()
            .min_w_full()
            .flex()
            .flex_col()
            .children(
                flat.into_iter().map(|item| {
                    let depth_px = px((item.depth as f32) * 14.0 + 8.0);
                    let target_line = item.line;
                    let has_children = item.has_children;

                    let prefix = if has_children {
                        if item.is_expanded { "▼" } else { "▶" }
                    } else {
                        "•"
                    };

                    let h_badge = format!("H{}", item.level);

                    div()
                        .h(px(24.0))
                        .min_w_full()
                        .flex()
                        .items_center()
                        .pl(depth_px)
                        .pr_2()
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .text_color(if item.level == 1 { active_fg } else { text_color })
                        .font_weight(if item.level <= 2 { FontWeight::BOLD } else { FontWeight::NORMAL })
                        .hover(move |s| s.bg(hover_bg))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            this.workspace.update(cx, |w, cx| {
                                w.jump_to_line(target_line);
                                cx.notify();
                            });
                        }))
                        .child(
                            div()
                                .w(px(14.0))
                                .flex_shrink_0()
                                .text_size(px(10.0))
                                .text_color(muted_fg)
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                    if has_children {
                                        this.workspace.update(cx, |w, cx| {
                                            Self::toggle_outline_node(&mut w.outline_nodes, target_line);
                                            cx.notify();
                                        });
                                    }
                                }))
                                .child(prefix)
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(10.0))
                                .text_color(muted_fg)
                                .mr_1()
                                .child(h_badge)
                        )
                        .child(
                            div()
                                .whitespace_nowrap()
                                .child(item.title)
                        )
                })
            )
            .into_any_element()
    }

    fn render_file_properties(
        &self,
        props: Option<ActiveFileProps>,
        text_color: Rgba,
        muted_fg: Rgba,
        border_color: Rgba,
        hover_bg: Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_expanded = self.show_properties;
        let title = self.i18n.get("sidebar.properties").to_string();

        let header = div()
            .h(px(26.0))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.show_properties = !this.show_properties;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(muted_fg)
                            .child(if is_expanded { "▼" } else { "▶" })
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(muted_fg)
                            .child(title)
                    )
            );

        let mut panel = div()
            .w_full()
            .flex_shrink_0()
            .border_t_1()
            .border_color(border_color)
            .child(header);

        if is_expanded {
            let body = if let Some(p) = props {
                let add_row = |label: String, val: String| {
                    div()
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(11.0))
                        .child(
                            div()
                                .text_color(muted_fg)
                                .child(label)
                        )
                        .child(
                            div()
                                .text_color(text_color)
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .max_w(px(135.0))
                                .child(val)
                        )
                };

                div()
                    .px_2p5()
                    .pb_2()
                    .pt_1()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(add_row(self.i18n.get("sidebar.prop_file").to_string(), p.file_name))
                    .child(add_row(self.i18n.get("sidebar.prop_size").to_string(), p.size_str))
                    .child(add_row(self.i18n.get("sidebar.prop_lines").to_string(), format!("{}", p.lines)))
                    .child(add_row(self.i18n.get("sidebar.prop_chars").to_string(), format!("{}", p.chars)))
                    .child(add_row(self.i18n.get("sidebar.prop_encoding").to_string(), p.encoding.to_string()))
                    .child(add_row(self.i18n.get("sidebar.prop_line_ending").to_string(), p.line_ending.to_string()))
            } else {
                div()
                    .px_2p5()
                    .py_2()
                    .text_size(px(11.0))
                    .text_color(muted_fg)
                    .child(self.i18n.get("sidebar.no_file").to_string())
            };

            panel = panel.child(body);
        }

        panel
    }
}
