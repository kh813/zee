use std::cell::RefCell;
use std::rc::Rc;

use gpui::*;
use crate::workspace::Workspace;
use crate::widgets::{led_color_to_gpui, ui_font_family, with_alpha};

#[derive(Clone, Debug)]
pub enum TabBarEvent {
    Select(usize),
    Close(usize),
    New,
}

/// Payload carried by GPUI's drag system while a tab is being dragged.
#[derive(Clone, Debug)]
pub struct DraggedTab {
    pub source_window: AnyWindowHandle,
    pub tab_index: usize,
    pub title: String,
    pub is_modified: bool,
    pub is_read_only: bool,
    pub bg_color: Rgba,
    pub text_color: Rgba,
    pub border_color: Rgba,
    pub accent_color: Rgba,
}

/// App-wide preview state for active tab drag operations so both the source
/// window and any hovered target window can draw real-time drop indicators.
#[derive(Clone, Debug, Default)]
pub struct GlobalTabDragState {
    pub active: Option<DraggedTab>,
    pub grab_offset: Point<Pixels>,
    pub target_window: Option<AnyWindowHandle>,
    pub target_insert_index: Option<usize>,
    pub over_tab_bar: bool,
    pub target_local_pos: Option<Point<Pixels>>,
}

impl Global for GlobalTabDragState {}

/// Floating visual preview of a tab while being dragged.
pub struct DraggedTabGhost {
    title: String,
    is_modified: bool,
    is_read_only: bool,
    bg_color: Rgba,
    text_color: Rgba,
    border_color: Rgba,
    accent_color: Rgba,
}

impl DraggedTabGhost {
    pub fn from_dragged(tab: &DraggedTab) -> Self {
        Self {
            title: tab.title.clone(),
            is_modified: tab.is_modified,
            is_read_only: tab.is_read_only,
            bg_color: tab.bg_color,
            text_color: tab.text_color,
            border_color: tab.border_color,
            accent_color: tab.accent_color,
        }
    }

    pub fn render_pill(&self) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(30.0))
            .px_3()
            .rounded_md()
            .bg(with_alpha(self.bg_color, 0.92))
            .text_color(self.text_color)
            .text_size(px(12.5))
            .font_family(ui_font_family())
            .border_1()
            .border_color(self.accent_color)
            .shadow_md()
            .child(self.title.clone())
            .children(if self.is_read_only {
                Some(
                    div()
                        .text_size(px(10.0))
                        .px_1()
                        .rounded_sm()
                        .bg(with_alpha(self.border_color, 0.4))
                        .child("RO"),
                )
            } else {
                None
            })
            .children(if self.is_modified {
                Some(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(self.text_color),
                )
            } else {
                None
            })
    }
}

impl Render for DraggedTabGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_pill()
    }
}

/// Compute the insertion slot (`0..=tab_bounds.len()`) for a pointer at `local_x`
/// based on the painted bounds of each tab chip.
pub fn compute_tab_insert_slot(tab_bounds: &[Bounds<Pixels>], local_x: Pixels) -> usize {
    for (idx, bounds) in tab_bounds.iter().enumerate() {
        let mid_x = bounds.origin.x + bounds.size.width / 2.0;
        if local_x < mid_x {
            return idx;
        }
    }
    tab_bounds.len()
}

/// Compute the X coordinate (in window-local pixels) for rendering the vertical
/// insertion indicator bar at `insert_slot`.
pub fn compute_insertion_indicator_x(
    tab_bounds: &[Bounds<Pixels>],
    insert_slot: usize,
    fallback_left: Pixels,
) -> Pixels {
    if tab_bounds.is_empty() {
        return fallback_left;
    }
    if insert_slot == 0 {
        return tab_bounds[0].origin.x;
    }
    if insert_slot >= tab_bounds.len() {
        return tab_bounds[tab_bounds.len() - 1].right();
    }
    let prev_right = tab_bounds[insert_slot - 1].right();
    let next_left = tab_bounds[insert_slot].origin.x;
    prev_right + (next_left - prev_right) / 2.0
}

/// Convert a window's GPUI bounds + viewport size + global display origin into
/// the screen-space origin of the window's content area (top-left of GPUI coordinates).
pub fn window_content_screen_origin(
    window_bounds: Bounds<Pixels>,
    viewport_size: Size<Pixels>,
    display_origin: Point<Pixels>,
) -> Point<Pixels> {
    let titlebar_height = (window_bounds.size.height - viewport_size.height).max(px(0.0));
    point(
        display_origin.x + window_bounds.origin.x,
        display_origin.y + window_bounds.origin.y + titlebar_height,
    )
}

/// Convert a window's GPUI outer bounds + global display origin into screen-space
/// outer bounds (including OS titlebar).
pub fn window_outer_screen_bounds(
    window_bounds: Bounds<Pixels>,
    display_origin: Point<Pixels>,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            display_origin.x + window_bounds.origin.x,
            display_origin.y + window_bounds.origin.y,
        ),
        window_bounds.size,
    )
}

/// Return the global screen origin of the display hosting `window`.
///
/// On macOS, GPUI's `window.bounds()` is relative to the window's current `NSScreen`,
/// so we add `CGDisplayBounds(display_id).origin` to obtain consistent multi-monitor
/// global coordinates. On Windows/Linux, `window.bounds()` is already in global
/// virtual screen coordinates.
pub fn screen_display_origin(window: &Window, cx: &App) -> Point<Pixels> {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[repr(C)]
        struct CGSize {
            width: f64,
            height: f64,
        }
        #[repr(C)]
        struct CGRect {
            origin: CGPoint,
            size: CGSize,
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGDisplayBounds(display: u32) -> CGRect;
        }
        if let Some(display) = window.display(cx) {
            let cg_id = u64::from(display.id()) as u32;
            let rect = unsafe { CGDisplayBounds(cg_id) };
            return point(px(rect.origin.x as f32), px(rect.origin.y as f32));
        }
        point(px(0.0), px(0.0))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, cx);
        point(px(0.0), px(0.0))
    }
}

/// Hit-test a global screen point against a front-to-back ordered list of candidate
/// windows `(handle, outer_screen_bounds, content_screen_origin)`.
/// Returns the topmost matching window handle along with the point converted into
/// that window's local content-area coordinates.
pub fn hit_test_window_stack<H: Copy>(
    global_pos: Point<Pixels>,
    candidates: &[(H, Bounds<Pixels>, Point<Pixels>)],
) -> Option<(H, Point<Pixels>)> {
    for &(handle, outer_bounds, content_origin) in candidates {
        if outer_bounds.contains(&global_pos) {
            return Some((handle, global_pos - content_origin));
        }
    }
    None
}

pub struct TabBar {
    workspace: Entity<Workspace>,
    scroll_offset: Pixels,
    pub(crate) tab_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    pub(crate) bar_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
}

impl EventEmitter<TabBarEvent> for TabBar {}

impl TabBar {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |_, _, cx| {
            cx.notify();
        })
        .detach();
        Self {
            workspace,
            scroll_offset: px(0.0),
            tab_bounds: Rc::new(RefCell::new(Vec::new())),
            bar_bounds: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn snapshot_geometry(&self) -> (Option<Bounds<Pixels>>, Vec<Bounds<Pixels>>) {
        (
            *self.bar_bounds.borrow(),
            self.tab_bounds.borrow().clone(),
        )
    }

    fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(1.0)).x;
        self.scroll_offset = (self.scroll_offset + delta).min(px(0.0));
        cx.notify();
    }
}

impl Render for TabBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let active_index = workspace.active_editor_index;
        let tab_count = workspace.editors.len();
        let current_window = window.window_handle();

        let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
        let hover_tab_bg = with_alpha(led_color_to_gpui(theme.ui.tab_active_bg), 0.4);
        let accent_color = led_color_to_gpui(theme.editor.cursor);

        let drag_state = cx.try_global::<GlobalTabDragState>().cloned().unwrap_or_default();
        let dragging_from_here_idx = drag_state
            .active
            .as_ref()
            .filter(|d| d.source_window == current_window)
            .map(|d| d.tab_index);
        let is_cross_window_target = drag_state
            .active
            .as_ref()
            .map(|d| d.source_window != current_window)
            .unwrap_or(false)
            && drag_state.target_window == Some(current_window);

        let insertion_slot_to_draw = if drag_state.active.is_some()
            && drag_state.target_window == Some(current_window)
        {
            drag_state.target_insert_index.filter(|&slot| {
                if let Some(from_idx) = dragging_from_here_idx {
                    Workspace::resolve_same_window_drop_index(from_idx, slot, tab_count).is_some()
                } else {
                    true
                }
            })
        } else {
            None
        };

        let bar_bg = if is_cross_window_target && drag_state.over_tab_bar {
            with_alpha(accent_color, 0.14)
        } else {
            led_color_to_gpui(theme.ui.tab_bar_bg)
        };

        let bar_bounds_cell = self.bar_bounds.clone();
        let tab_bounds_cell = self.tab_bounds.clone();

        let indicator_x = insertion_slot_to_draw.map(|slot| {
            let bar_left = self
                .bar_bounds
                .borrow()
                .map(|b| b.origin.x)
                .unwrap_or(px(0.0));
            let painted_tabs = self.tab_bounds.borrow();
            let raw_x = compute_insertion_indicator_x(&painted_tabs, slot, bar_left + px(8.0));
            (raw_x - bar_left).max(px(2.0))
        });

        #[allow(unused_mut)]
        let mut bar = div()
            .relative()
            .h(px(36.0))
            .w_full()
            .flex()
            .items_end()
            .px_2()
            .bg(bar_bg)
            .border_b_1()
            .border_color(if is_cross_window_target {
                accent_color
            } else {
                border_color
            })
            .font_family(ui_font_family())
            .on_scroll_wheel(cx.listener(|this, event, _, cx| {
                this.handle_scroll(event, cx);
            }))
            .child(
                canvas(
                    move |bounds, _, _| {
                        *bar_bounds_cell.borrow_mut() = Some(bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
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
                            .on_children_prepainted(move |bounds, _, _| {
                                *tab_bounds_cell.borrow_mut() = bounds;
                            })
                            .children(
                                workspace.editors.iter().enumerate().map(|(idx, editor)| {
                                    let is_active = idx == active_index;
                                    let is_being_dragged = dragging_from_here_idx == Some(idx);
                                    let file_name = editor
                                        .path
                                        .as_ref()
                                        .and_then(|p| p.file_name())
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("[No Name]")
                                        .to_string();
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

                                    let dragged_payload = DraggedTab {
                                        source_window: current_window,
                                        tab_index: idx,
                                        title: file_name.clone(),
                                        is_modified,
                                        is_read_only: is_ro,
                                        bg_color,
                                        text_color,
                                        border_color,
                                        accent_color,
                                    };

                                    div()
                                        .id(SharedString::from(format!("zee-tab-{}", idx)))
                                        .flex()
                                        .items_center()
                                        .h(px(30.0))
                                        .px_3()
                                        .rounded_t_md()
                                        .bg(if is_being_dragged {
                                            with_alpha(bg_color, 0.45)
                                        } else {
                                            bg_color
                                        })
                                        .text_color(if is_being_dragged {
                                            with_alpha(text_color, 0.45)
                                        } else {
                                            text_color
                                        })
                                        .text_size(px(12.5))
                                        .cursor_pointer()
                                        .border_t_1()
                                        .border_l_1()
                                        .border_r_1()
                                        .border_color(if is_active {
                                            border_color
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .hover(move |s| {
                                            if !is_active {
                                                s.bg(hover_tab_bg)
                                            } else {
                                                s
                                            }
                                        })
                                        .on_drag(dragged_payload, move |tab, cursor_offset, window, cx| {
                                            let src_win = window.window_handle();
                                            cx.set_global(GlobalTabDragState {
                                                active: Some(tab.clone()),
                                                grab_offset: cursor_offset,
                                                target_window: Some(src_win),
                                                target_insert_index: Some(tab.tab_index),
                                                over_tab_bar: true,
                                                target_local_pos: None,
                                            });
                                            window.refresh();
                                            cx.new(|_| DraggedTabGhost::from_dragged(tab))
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, _, cx| {
                                                this.workspace.update(cx, |w, cx| {
                                                    w.active_editor_index = idx;
                                                    cx.notify();
                                                });
                                                cx.emit(TabBarEvent::Select(idx));
                                            }),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(file_name)
                                                .children(if is_ro {
                                                    Some(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .px_1()
                                                            .rounded_sm()
                                                            .bg(with_alpha(border_color, 0.4))
                                                            .child("RO"),
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
                                                            .bg(text_color),
                                                    )
                                                } else {
                                                    None
                                                }),
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
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |_this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        cx.emit(TabBarEvent::Close(idx));
                                                    }),
                                                ),
                                        )
                                })
                                .collect::<Vec<_>>(),
                            ),
                    ),
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_this, _, _, cx| {
                            cx.stop_propagation();
                            cx.emit(TabBarEvent::New);
                        }),
                    ),
            )
            .children(indicator_x.map(|x| {
                div()
                    .absolute()
                    .left(x - px(1.5))
                    .top(px(5.0))
                    .bottom(px(2.0))
                    .w(px(3.0))
                    .rounded_full()
                    .bg(accent_color)
            }));

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
                    .hover(|s| {
                        s.bg(with_alpha(gpui::rgb(0x2563eb), 0.35))
                            .border_color(with_alpha(gpui::rgb(0x3b82f6), 0.7))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::app::CheckForUpdates {}), cx);
                        }),
                    )
                    .child(label),
            );
        }

        bar
    }
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, size, Bounds};
    use super::{
        compute_insertion_indicator_x, compute_tab_insert_slot, hit_test_window_stack,
        window_content_screen_origin, window_outer_screen_bounds,
    };

    #[::core::prelude::v1::test]
    fn test_compute_tab_insert_slot_and_indicator_positions() {
        let tabs = vec![
            Bounds::new(point(px(8.0), px(6.0)), size(px(100.0), px(30.0))),   // [8..108], mid=58
            Bounds::new(point(px(112.0), px(6.0)), size(px(100.0), px(30.0))), // [112..212], mid=162
            Bounds::new(point(px(216.0), px(6.0)), size(px(100.0), px(30.0))), // [216..316], mid=266
        ];

        assert_eq!(compute_tab_insert_slot(&tabs, px(0.0)), 0);
        assert_eq!(compute_tab_insert_slot(&tabs, px(57.9)), 0);
        assert_eq!(compute_tab_insert_slot(&tabs, px(58.0)), 1);
        assert_eq!(compute_tab_insert_slot(&tabs, px(161.9)), 1);
        assert_eq!(compute_tab_insert_slot(&tabs, px(162.0)), 2);
        assert_eq!(compute_tab_insert_slot(&tabs, px(265.9)), 2);
        assert_eq!(compute_tab_insert_slot(&tabs, px(266.0)), 3);
        assert_eq!(compute_tab_insert_slot(&tabs, px(500.0)), 3);

        assert_eq!(compute_insertion_indicator_x(&tabs, 0, px(8.0)), px(8.0));
        assert_eq!(compute_insertion_indicator_x(&tabs, 1, px(8.0)), px(110.0));
        assert_eq!(compute_insertion_indicator_x(&tabs, 2, px(8.0)), px(214.0));
        assert_eq!(compute_insertion_indicator_x(&tabs, 3, px(8.0)), px(316.0));
        assert_eq!(compute_insertion_indicator_x(&[], 0, px(8.0)), px(8.0));
    }

    #[::core::prelude::v1::test]
    fn test_cross_window_screen_coordinate_mapping_and_z_order_hit_test() {
        // Primary monitor window A at (100, 100), size 800x600 with 28px macOS titlebar
        let win_a_bounds = Bounds::new(point(px(100.0), px(100.0)), size(px(800.0), px(600.0)));
        let win_a_viewport = size(px(800.0), px(572.0));
        let disp_primary = point(px(0.0), px(0.0));
        let outer_a = window_outer_screen_bounds(win_a_bounds, disp_primary);
        let content_a = window_content_screen_origin(win_a_bounds, win_a_viewport, disp_primary);
        assert_eq!(content_a, point(px(100.0), px(128.0)));

        // Secondary monitor at x=1920, y=200 hosting window B at display-local (50, 40)
        let win_b_bounds = Bounds::new(point(px(50.0), px(40.0)), size(px(800.0), px(600.0)));
        let win_b_viewport = size(px(800.0), px(572.0));
        let disp_secondary = point(px(1920.0), px(200.0));
        let outer_b = window_outer_screen_bounds(win_b_bounds, disp_secondary);
        let content_b = window_content_screen_origin(win_b_bounds, win_b_viewport, disp_secondary);
        assert_eq!(outer_b.origin, point(px(1970.0), px(240.0)));
        assert_eq!(content_b, point(px(1970.0), px(268.0)));

        // Overlapping window C on primary monitor in front of A
        let win_c_bounds = Bounds::new(point(px(500.0), px(100.0)), size(px(600.0), px(500.0)));
        let win_c_viewport = size(px(600.0), px(472.0));
        let outer_c = window_outer_screen_bounds(win_c_bounds, disp_primary);
        let content_c = window_content_screen_origin(win_c_bounds, win_c_viewport, disp_primary);

        // Front-to-back z-order: C, A, B
        let stack = vec![
            ("C", outer_c, content_c),
            ("A", outer_a, content_a),
            ("B", outer_b, content_b),
        ];

        // Overlap region (600, 150) must hit frontmost window C
        let hit_overlap = hit_test_window_stack(point(px(600.0), px(150.0)), &stack);
        assert_eq!(hit_overlap, Some(("C", point(px(100.0), px(22.0)))));

        // Dragging from A into secondary monitor window B at global (2070, 288)
        let hit_b = hit_test_window_stack(point(px(2070.0), px(288.0)), &stack);
        assert_eq!(hit_b, Some(("B", point(px(100.0), px(20.0)))));

        // Outside all windows returns None
        let hit_none = hit_test_window_stack(point(px(1500.0), px(10.0)), &stack);
        assert_eq!(hit_none, None);

        // Negative-coordinate secondary monitor placed to the LEFT of primary (x=-1920)
        let disp_left = point(px(-1920.0), px(0.0));
        let win_left_bounds = Bounds::new(point(px(100.0), px(80.0)), size(px(900.0), px(700.0)));
        let win_left_viewport = size(px(900.0), px(672.0));
        let outer_left = window_outer_screen_bounds(win_left_bounds, disp_left);
        let content_left =
            window_content_screen_origin(win_left_bounds, win_left_viewport, disp_left);
        assert_eq!(outer_left.origin, point(px(-1820.0), px(80.0)));
        assert_eq!(content_left, point(px(-1820.0), px(108.0)));
        let left_stack = vec![("LEFT", outer_left, content_left)];
        let hit_left = hit_test_window_stack(point(px(-1700.0), px(120.0)), &left_stack);
        assert_eq!(hit_left, Some(("LEFT", point(px(120.0), px(12.0)))));
    }
}
