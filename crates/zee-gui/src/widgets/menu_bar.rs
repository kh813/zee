use gpui::*;
use crate::workspace::Workspace;
use zee_core::i18n::I18n;
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmenuId {
    NewFromTemplate,
    Language,
}

#[allow(dead_code)]
pub struct MenuBar {
    workspace: Entity<Workspace>,
    i18n: I18n,
    pub open_menu: Option<usize>,
    pub open_submenu: Option<(SubmenuId, f32)>,
}

#[allow(dead_code)]
impl MenuBar {
    pub fn new(workspace: Entity<Workspace>, i18n: I18n, _cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            i18n,
            open_menu: None,
            open_submenu: None,
        }
    }

    pub fn toggle_menu(&mut self, idx: usize, _window: &mut Window, cx: &mut Context<Self>) {
        if self.open_menu == Some(idx) {
            self.open_menu = None;
            self.open_submenu = None;
        } else {
            self.open_menu = Some(idx);
            self.open_submenu = None;
        }
        cx.notify();
    }

    pub fn hover_menu(&mut self, idx: usize, _window: &mut Window, cx: &mut Context<Self>) {
        if self.open_menu.is_some() && self.open_menu != Some(idx) {
            self.open_menu = Some(idx);
            self.open_submenu = None;
            cx.notify();
        }
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.open_menu.is_some() || self.open_submenu.is_some() {
            self.open_menu = None;
            self.open_submenu = None;
            cx.notify();
        }
    }

    pub fn open_submenu(&mut self, id: SubmenuId, y: f32, cx: &mut Context<Self>) {
        if self.open_submenu != Some((id, y)) {
            self.open_submenu = Some((id, y));
            cx.notify();
        }
    }

    pub fn close_submenu(&mut self, cx: &mut Context<Self>) {
        if self.open_submenu.is_some() {
            self.open_submenu = None;
            cx.notify();
        }
    }

    pub fn set_i18n(&mut self, i18n: I18n, cx: &mut Context<Self>) {
        self.i18n = i18n;
        cx.notify();
    }
}

impl Render for MenuBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let bg = led_color_to_gpui(theme.ui.menu_bar_bg);
        let fg = led_color_to_gpui(theme.ui.menu_bar_fg);
        let border = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
        let hover_bg = with_alpha(fg, 0.12);
        let active_bg = with_alpha(fg, 0.22);

        let menu_titles = [
            (0, self.i18n.get("menu.file").to_string()),
            (1, self.i18n.get("menu.edit").to_string()),
            (2, self.i18n.get("menu.view").to_string()),
            (3, self.i18n.get("menu.plugins").to_string()),
            (4, self.i18n.get("menu.help").to_string()),
        ];

        let mut bar = div()
            .w_full()
            .h(px(28.0))
            .bg(bg)
            .text_color(fg)
            .text_size(px(12.5))
            .font_family(ui_font_family())
            .border_b_1()
            .border_color(border)
            .flex()
            .items_center()
            .px_2()
            .gap_1();

        for (idx, title) in menu_titles {
            let is_open = self.open_menu == Some(idx);
            let btn_bg = if is_open { active_bg } else { with_alpha(fg, 0.0) };

            bar = bar.child(
                div()
                    .h(px(22.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(btn_bg)
                    .hover(move |s| if !is_open { s.bg(hover_bg) } else { s })
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.toggle_menu(idx, window, cx);
                    }))
                    .on_mouse_move(cx.listener(move |this, _, window, cx| {
                        this.hover_menu(idx, window, cx);
                    }))
                    .child(title)
            );
        }

        bar
    }
}
