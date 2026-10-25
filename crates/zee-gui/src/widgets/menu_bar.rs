use gpui::*;
use crate::workspace::Workspace;
use zee_core::i18n::I18n;
use crate::widgets::{ui_font_family, with_alpha};

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
    pub fn new(workspace: Entity<Workspace>, i18n: I18n, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| {
            cx.notify();
        }).detach();
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

#[derive(Clone, Copy, Debug)]
pub struct OsMenuColors {
    pub bar_bg: gpui::Rgba,
    pub bar_fg: gpui::Rgba,
    pub bar_border: gpui::Rgba,
    pub hover_bg: gpui::Rgba,
    pub active_bg: gpui::Rgba,
    pub dropdown_bg: gpui::Rgba,
    pub dropdown_fg: gpui::Rgba,
    pub dropdown_border: gpui::Rgba,
    pub muted_fg: gpui::Rgba,
}

pub fn get_os_menu_colors(window: &gpui::Window) -> OsMenuColors {
    let is_dark = matches!(
        window.appearance(),
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
    );
    if is_dark {
        OsMenuColors {
            bar_bg: gpui::rgb(0x1f1f1f),
            bar_fg: gpui::rgb(0xf0f0f0),
            bar_border: with_alpha(gpui::rgb(0xffffff), 0.12),
            hover_bg: with_alpha(gpui::rgb(0xffffff), 0.08),
            active_bg: with_alpha(gpui::rgb(0xffffff), 0.16),
            dropdown_bg: gpui::rgb(0x252526),
            dropdown_fg: gpui::rgb(0xf0f0f0),
            dropdown_border: with_alpha(gpui::rgb(0xffffff), 0.16),
            muted_fg: with_alpha(gpui::rgb(0xffffff), 0.55),
        }
    } else {
        OsMenuColors {
            bar_bg: gpui::rgb(0xf3f3f3),
            bar_fg: gpui::rgb(0x1e1e1e),
            bar_border: with_alpha(gpui::rgb(0x000000), 0.12),
            hover_bg: with_alpha(gpui::rgb(0x000000), 0.07),
            active_bg: with_alpha(gpui::rgb(0x000000), 0.14),
            dropdown_bg: gpui::rgb(0xffffff),
            dropdown_fg: gpui::rgb(0x1e1e1e),
            dropdown_border: with_alpha(gpui::rgb(0x000000), 0.15),
            muted_fg: with_alpha(gpui::rgb(0x000000), 0.55),
        }
    }
}

impl Render for MenuBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = get_os_menu_colors(window);
        let bg = colors.bar_bg;
        let fg = colors.bar_fg;
        let border = colors.bar_border;
        let hover_bg = colors.hover_bg;
        let active_bg = colors.active_bg;

        let has_git = self.workspace.read(cx).plugin_manager.has_git_easy();
        let mut menu_titles = vec![
            (0, self.i18n.get("menu.file").to_string()),
            (1, self.i18n.get("menu.edit").to_string()),
            (2, self.i18n.get("menu.view").to_string()),
        ];
        if has_git {
            menu_titles.push((5, self.i18n.get("menu.git").to_string()));
        }
        menu_titles.push((3, self.i18n.get("menu.plugins").to_string()));
        menu_titles.push((4, self.i18n.get("menu.help").to_string()));

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

        let workspace = self.workspace.read(cx);
        if let Some(ref ver) = workspace.available_update {
            let label = format!("⬆ v{} {}", ver, self.i18n.get("menu.help.update_available"));
            bar = bar.child(
                div()
                    .ml_auto()
                    .mr_1()
                    .h(px(20.0))
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
                    .child(label)
            );
        }

        bar
    }
}
