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
    TogglePin(usize),
    ContextMenu {
        tab_index: usize,
        position: Point<Pixels>,
    },
    ToggleTabListDropdown {
        anchor_x: Pixels,
    },
}

/// Payload carried by GPUI's drag system while a tab is being dragged.
#[derive(Clone, Debug)]
pub struct DraggedTab {
    pub source_window: AnyWindowHandle,
    pub tab_index: usize,
    pub source_tab_count: usize,
    pub is_pinned: bool,
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
    pub last_global_pos: Option<Point<Pixels>>,
}

impl Global for GlobalTabDragState {}

/// Floating visual preview of a tab while being dragged.
pub struct DraggedTabGhost {
    title: String,
    is_modified: bool,
    is_read_only: bool,
    is_pinned: bool,
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
            is_pinned: tab.is_pinned,
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
            .children(if self.is_pinned {
                Some(
                    div()
                        .text_size(px(10.0))
                        .text_color(self.accent_color)
                        .child("●"),
                )
            } else {
                None
            })
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

/// Hover tooltip view for displaying a tab's full path and status flags.
struct TabTooltip {
    text: String,
    bg: Rgba,
    fg: Rgba,
    border: Rgba,
}

impl Render for TabTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(self.bg)
            .text_color(self.fg)
            .text_size(px(11.5))
            .font_family(ui_font_family())
            .border_1()
            .border_color(self.border)
            .shadow_md()
            .child(self.text.clone())
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

/// Compute the minimum (most negative) horizontal scroll offset for a tab strip.
pub fn compute_tab_min_scroll_offset(
    viewport_bounds: Bounds<Pixels>,
    tab_bounds: &[Bounds<Pixels>],
) -> Pixels {
    if tab_bounds.is_empty() {
        return px(0.0);
    }
    let content_width = tab_bounds[tab_bounds.len() - 1].right() - tab_bounds[0].origin.x;
    (viewport_bounds.size.width - content_width).min(px(0.0))
}

/// Return `true` when the painted tabs exceed the visible tab viewport width.
pub fn is_tab_strip_overflowing(
    viewport_bounds: Option<Bounds<Pixels>>,
    tab_bounds: &[Bounds<Pixels>],
) -> bool {
    let Some(vp) = viewport_bounds else {
        return false;
    };
    if tab_bounds.is_empty() {
        return false;
    }
    let content_width = tab_bounds[tab_bounds.len() - 1].right() - tab_bounds[0].origin.x;
    content_width > vp.size.width + px(2.0)
}

/// Adjust horizontal `current_offset` (`<= 0`) so that `tab_bounds[active_index]`
/// is completely visible inside `viewport_bounds`, clamping dead space on the right.
pub fn compute_auto_scroll_offset(
    current_offset: Pixels,
    viewport_bounds: Bounds<Pixels>,
    tab_bounds: &[Bounds<Pixels>],
    active_index: usize,
) -> Pixels {
    if tab_bounds.is_empty() || active_index >= tab_bounds.len() {
        return px(0.0);
    }
    let min_offset = compute_tab_min_scroll_offset(viewport_bounds, tab_bounds);
    let mut offset = current_offset.clamp(min_offset, px(0.0));

    // Recover unscrolled coordinates relative to viewport left edge
    let active = tab_bounds[active_index];
    let active_left = active.origin.x - viewport_bounds.origin.x - current_offset;
    let active_right = active.right() - viewport_bounds.origin.x - current_offset;

    if active_left + offset < px(0.0) {
        offset = -active_left;
    } else if active_right + offset > viewport_bounds.size.width {
        offset = viewport_bounds.size.width - active_right;
    }

    offset.clamp(min_offset, px(0.0))
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
    last_active_index: usize,
    last_tab_count: usize,
    pub(crate) tab_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    pub(crate) bar_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
    pub(crate) viewport_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
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
            last_active_index: 0,
            last_tab_count: 0,
            tab_bounds: Rc::new(RefCell::new(Vec::new())),
            bar_bounds: Rc::new(RefCell::new(None)),
            viewport_bounds: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn snapshot_geometry(&self) -> (Option<Bounds<Pixels>>, Vec<Bounds<Pixels>>) {
        (
            *self.bar_bounds.borrow(),
            self.tab_bounds.borrow().clone(),
        )
    }

    fn clamp_scroll_offset(&mut self, delta: Pixels) {
        let min_offset = self
            .viewport_bounds
            .borrow()
            .map(|vp| compute_tab_min_scroll_offset(vp, &self.tab_bounds.borrow()))
            .unwrap_or(px(0.0));
        self.scroll_offset = (self.scroll_offset + delta).clamp(min_offset, px(0.0));
    }

    fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let px_delta = event.delta.pixel_delta(px(1.0));
        let delta = if px_delta.x.abs() > px(0.1) {
            px_delta.x
        } else {
            px_delta.y
        };
        self.clamp_scroll_offset(delta);
        cx.notify();
    }

    fn nudge_scroll(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        self.clamp_scroll_offset(delta);
        cx.notify();
    }
}

impl Render for TabBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.read(cx);
        let theme = &workspace.theme;
        let active_index = workspace.active_editor_index;
        let tab_count = workspace.editors.len();
        let pinned_count = workspace.pinned_count();
        let current_window = window.window_handle();

        // Auto-scroll to keep the active tab visible whenever active tab or tab count changes
        if active_index != self.last_active_index || tab_count != self.last_tab_count {
            self.last_active_index = active_index;
            self.last_tab_count = tab_count;
            if let Some(vp) = *self.viewport_bounds.borrow() {
                let painted = self.tab_bounds.borrow();
                if painted.len() == tab_count {
                    self.scroll_offset =
                        compute_auto_scroll_offset(self.scroll_offset, vp, &painted, active_index);
                } else {
                    let min_off = compute_tab_min_scroll_offset(vp, &painted);
                    self.scroll_offset = self.scroll_offset.clamp(min_off, px(0.0));
                }
            }
        }

        let display_labels = Workspace::compute_tab_display_labels(&workspace.editors);
        let border_color = with_alpha(led_color_to_gpui(theme.editor.line_number), 0.35);
        let hover_tab_bg = with_alpha(led_color_to_gpui(theme.ui.tab_active_bg), 0.4);
        let accent_color = led_color_to_gpui(theme.editor.cursor);
        let tip_bg = led_color_to_gpui(theme.ui.dialog_bg);
        let tip_fg = led_color_to_gpui(theme.editor.foreground);

        let drag_state = cx
            .try_global::<GlobalTabDragState>()
            .cloned()
            .unwrap_or_default();
        let dragging_from_here = drag_state
            .active
            .as_ref()
            .filter(|d| d.source_window == current_window)
            .map(|d| (d.tab_index, d.is_pinned));
        let is_cross_window_target = drag_state
            .active
            .as_ref()
            .map(|d| d.source_window != current_window)
            .unwrap_or(false)
            && drag_state.target_window == Some(current_window);

        let insertion_slot_to_draw = if drag_state.active.is_some()
            && drag_state.target_window == Some(current_window)
        {
            drag_state.target_insert_index.and_then(|slot| {
                if let Some((from_idx, is_pinned)) = dragging_from_here {
                    let clamped = if is_pinned {
                        slot.min(pinned_count)
                    } else {
                        slot.max(pinned_count).min(tab_count)
                    };
                    Workspace::resolve_same_window_drop_index(from_idx, clamped, tab_count)
                        .map(|_| clamped)
                } else {
                    let is_pinned = drag_state
                        .active
                        .as_ref()
                        .map(|d| d.is_pinned)
                        .unwrap_or(false);
                    let clamped = if is_pinned {
                        slot.min(pinned_count)
                    } else {
                        slot.max(pinned_count).min(tab_count)
                    };
                    Some(clamped)
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
        let viewport_bounds_cell = self.viewport_bounds.clone();

        let is_overflowing = is_tab_strip_overflowing(
            *self.viewport_bounds.borrow(),
            &self.tab_bounds.borrow(),
        ) || self.scroll_offset < px(0.0);

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

        let tab_bounds_for_dblclick = self.tab_bounds.clone();

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
                    .relative()
                    .flex()
                    .flex_grow()
                    .h_full()
                    .items_end()
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_this, event: &MouseDownEvent, _, cx| {
                            if event.click_count >= 2 {
                                let last_right = tab_bounds_for_dblclick
                                    .borrow()
                                    .last()
                                    .map(|b| b.right())
                                    .unwrap_or(px(0.0));
                                if event.position.x > last_right {
                                    cx.emit(TabBarEvent::New);
                                }
                            }
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                *viewport_bounds_cell.borrow_mut() = Some(bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
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
                                    let is_being_dragged =
                                        dragging_from_here.map(|(i, _)| i) == Some(idx);
                                    let label = &display_labels[idx];
                                    let is_modified = editor.is_modified();
                                    let is_ro = editor.read_only;
                                    let is_pinned = editor.pinned;
                                    let is_last_pinned =
                                        is_pinned && pinned_count > 0 && idx + 1 == pinned_count && tab_count > pinned_count;

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

                                    let mut status_tags: Vec<&str> = Vec::new();
                                    if is_pinned {
                                        status_tags.push("Pinned");
                                    }
                                    if is_modified {
                                        status_tags.push("Modified");
                                    }
                                    if is_ro {
                                        status_tags.push("Read-Only");
                                    }
                                    let base_tip = label
                                        .full_path
                                        .clone()
                                        .unwrap_or_else(|| label.name.clone());
                                    let tooltip_text = if status_tags.is_empty() {
                                        base_tip
                                    } else {
                                        format!("{}  ({})", base_tip, status_tags.join(" · "))
                                    };

                                    let dragged_payload = DraggedTab {
                                        source_window: current_window,
                                        tab_index: idx,
                                        source_tab_count: tab_count,
                                        is_pinned,
                                        title: label.name.clone(),
                                        is_modified,
                                        is_read_only: is_ro,
                                        bg_color,
                                        text_color,
                                        border_color,
                                        accent_color,
                                    };

                                    let disambiguator = label.disambiguator.clone();
                                    let tab_name = label.name.clone();

                                    div()
                                        .id(SharedString::from(format!("zee-tab-{}", idx)))
                                        .flex()
                                        .items_center()
                                        .h(px(30.0))
                                        .px(if is_pinned { px(9.0) } else { px(12.0) })
                                        .mr(if is_last_pinned { px(6.0) } else { px(0.0) })
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
                                        .tooltip(move |_window, cx| {
                                            cx.new(|_| TabTooltip {
                                                text: tooltip_text.clone(),
                                                bg: tip_bg,
                                                fg: tip_fg,
                                                border: border_color,
                                            })
                                            .into()
                                        })
                                        .on_drag(
                                            dragged_payload,
                                            move |tab, cursor_offset, window, cx| {
                                                let src_win = window.window_handle();
                                                cx.set_global(GlobalTabDragState {
                                                    active: Some(tab.clone()),
                                                    grab_offset: cursor_offset,
                                                    target_window: Some(src_win),
                                                    target_insert_index: Some(tab.tab_index),
                                                    over_tab_bar: true,
                                                    target_local_pos: None,
                                                    last_global_pos: None,
                                                });
                                                window.refresh();
                                                cx.new(|_| DraggedTabGhost::from_dragged(tab))
                                            },
                                        )
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
                                        .on_mouse_down(
                                            MouseButton::Middle,
                                            cx.listener(move |_this, _, _, cx| {
                                                cx.stop_propagation();
                                                if !is_pinned {
                                                    cx.emit(TabBarEvent::Close(idx));
                                                }
                                            }),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            cx.listener(
                                                move |_this, event: &MouseDownEvent, _, cx| {
                                                    cx.stop_propagation();
                                                    cx.emit(TabBarEvent::ContextMenu {
                                                        tab_index: idx,
                                                        position: event.position,
                                                    });
                                                },
                                            ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1p5()
                                                .children(if is_pinned {
                                                    Some(
                                                        div()
                                                            .w(px(5.0))
                                                            .h(px(5.0))
                                                            .rounded_full()
                                                            .bg(accent_color),
                                                    )
                                                } else {
                                                    None
                                                })
                                                .child(tab_name)
                                                .children(disambiguator.map(|parent| {
                                                    div()
                                                        .text_size(px(10.5))
                                                        .text_color(with_alpha(text_color, 0.58))
                                                        .child(parent)
                                                }))
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
                                        .child(if is_pinned {
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .w(px(16.0))
                                                .h(px(16.0))
                                                .ml_1p5()
                                                .rounded_sm()
                                                .text_size(px(10.5))
                                                .text_color(with_alpha(text_color, 0.7))
                                                .hover(|s| s.bg(rgba(0xffffff22)))
                                                .child("📌")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |_this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        cx.emit(TabBarEvent::TogglePin(idx));
                                                    }),
                                                )
                                        } else {
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
                                                )
                                        })
                                })
                                .collect::<Vec<_>>(),
                            ),
                    ),
            )
            // Overflow scroll arrows (< >) when tabs exceed viewport width
            .children(if is_overflowing {
                let inactive_fg = led_color_to_gpui(theme.ui.tab_inactive_fg);
                Some(
                    div()
                        .flex()
                        .items_center()
                        .mb_1()
                        .ml_1()
                        .gap_0p5()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(22.0))
                                .h(px(24.0))
                                .rounded_sm()
                                .text_size(px(12.0))
                                .text_color(inactive_fg)
                                .cursor_pointer()
                                .hover(|s| s.bg(rgba(0xffffff18)))
                                .child("‹")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.nudge_scroll(px(140.0), cx);
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(22.0))
                                .h(px(24.0))
                                .rounded_sm()
                                .text_size(px(12.0))
                                .text_color(inactive_fg)
                                .cursor_pointer()
                                .hover(|s| s.bg(rgba(0xffffff18)))
                                .child("›")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.nudge_scroll(px(-140.0), cx);
                                    }),
                                ),
                        ),
                )
            } else {
                None
            })
            // New tab (+) button
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(26.0))
                    .h(px(26.0))
                    .mb_1()
                    .ml_1()
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
            // Tab list dropdown (▾) button for instant navigation across all open tabs
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(24.0))
                    .px_1p5()
                    .mb_1()
                    .ml_1()
                    .rounded_md()
                    .text_size(px(11.0))
                    .text_color(led_color_to_gpui(theme.ui.tab_inactive_fg))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(0xffffff18)).text_color(gpui::rgb(0xffffff)))
                    .child(if tab_count > 1 {
                        format!("{} ▾", tab_count)
                    } else {
                        "▾".to_string()
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(TabBarEvent::ToggleTabListDropdown {
                                anchor_x: event.position.x,
                            });
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
    use gpui::{point, px, size, Bounds, Pixels};
    use super::{
        compute_auto_scroll_offset, compute_insertion_indicator_x, compute_tab_insert_slot,
        compute_tab_min_scroll_offset, hit_test_window_stack, is_tab_strip_overflowing,
        window_content_screen_origin, window_outer_screen_bounds,
    };

    #[::core::prelude::v1::test]
    fn test_compute_tab_insert_slot_and_indicator_positions() {
        let tabs = vec![
            Bounds::new(point(px(8.0), px(6.0)), size(px(100.0), px(30.0))),
            Bounds::new(point(px(112.0), px(6.0)), size(px(100.0), px(30.0))),
            Bounds::new(point(px(216.0), px(6.0)), size(px(100.0), px(30.0))),
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
    fn test_tab_strip_auto_scroll_and_overflow_detection() {
        let viewport = Bounds::new(point(px(10.0), px(0.0)), size(px(250.0), px(36.0)));
        // 4 tabs of 100px each, gap 4px -> total width = 412px > 250px (overflowing)
        let tabs_at_zero = vec![
            Bounds::new(point(px(10.0), px(6.0)), size(px(100.0), px(30.0))),  // [0..100]
            Bounds::new(point(px(114.0), px(6.0)), size(px(100.0), px(30.0))), // [104..204]
            Bounds::new(point(px(218.0), px(6.0)), size(px(100.0), px(30.0))), // [208..308]
            Bounds::new(point(px(322.0), px(6.0)), size(px(100.0), px(30.0))), // [312..412]
        ];

        assert!(is_tab_strip_overflowing(Some(viewport), &tabs_at_zero));
        assert_eq!(compute_tab_min_scroll_offset(viewport, &tabs_at_zero), px(-162.0));

        // Active tab 0 is already visible -> stays at 0
        assert_eq!(compute_auto_scroll_offset(px(0.0), viewport, &tabs_at_zero, 0), px(0.0));
        // Active tab 3 ends at 412 -> must scroll left to 250 - 412 = -162
        assert_eq!(compute_auto_scroll_offset(px(0.0), viewport, &tabs_at_zero, 3), px(-162.0));

        // Now suppose tabs were painted at offset = -162 and user switches back to tab 0
        let tabs_scrolled: Vec<Bounds<Pixels>> = tabs_at_zero
            .iter()
            .map(|b| Bounds::new(point(b.origin.x - px(162.0), b.origin.y), b.size))
            .collect();
        assert_eq!(
            compute_auto_scroll_offset(px(-162.0), viewport, &tabs_scrolled, 0),
            px(0.0)
        );

        // Non-overflowing strip (2 tabs = 204px <= 250px) -> always 0
        assert!(!is_tab_strip_overflowing(Some(viewport), &tabs_at_zero[..2]));
        assert_eq!(compute_auto_scroll_offset(px(-50.0), viewport, &tabs_at_zero[..2], 1), px(0.0));
    }

    #[::core::prelude::v1::test]
    fn test_cross_window_screen_coordinate_mapping_and_z_order_hit_test() {
        let win_a_bounds = Bounds::new(point(px(100.0), px(100.0)), size(px(800.0), px(600.0)));
        let win_a_viewport = size(px(800.0), px(572.0));
        let disp_primary = point(px(0.0), px(0.0));
        let outer_a = window_outer_screen_bounds(win_a_bounds, disp_primary);
        let content_a = window_content_screen_origin(win_a_bounds, win_a_viewport, disp_primary);
        assert_eq!(content_a, point(px(100.0), px(128.0)));

        let win_b_bounds = Bounds::new(point(px(50.0), px(40.0)), size(px(800.0), px(600.0)));
        let win_b_viewport = size(px(800.0), px(572.0));
        let disp_secondary = point(px(1920.0), px(200.0));
        let outer_b = window_outer_screen_bounds(win_b_bounds, disp_secondary);
        let content_b = window_content_screen_origin(win_b_bounds, win_b_viewport, disp_secondary);
        assert_eq!(outer_b.origin, point(px(1970.0), px(240.0)));
        assert_eq!(content_b, point(px(1970.0), px(268.0)));

        let win_c_bounds = Bounds::new(point(px(500.0), px(100.0)), size(px(600.0), px(500.0)));
        let win_c_viewport = size(px(600.0), px(472.0));
        let outer_c = window_outer_screen_bounds(win_c_bounds, disp_primary);
        let content_c = window_content_screen_origin(win_c_bounds, win_c_viewport, disp_primary);

        let stack = vec![
            ("C", outer_c, content_c),
            ("A", outer_a, content_a),
            ("B", outer_b, content_b),
        ];

        let hit_overlap = hit_test_window_stack(point(px(600.0), px(150.0)), &stack);
        assert_eq!(hit_overlap, Some(("C", point(px(100.0), px(22.0)))));

        let hit_b = hit_test_window_stack(point(px(2070.0), px(288.0)), &stack);
        assert_eq!(hit_b, Some(("B", point(px(100.0), px(20.0)))));

        let hit_none = hit_test_window_stack(point(px(1500.0), px(10.0)), &stack);
        assert_eq!(hit_none, None);

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
