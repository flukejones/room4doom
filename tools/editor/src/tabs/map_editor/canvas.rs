//! Native map canvas gestures, context menus and transient overlays.

use super::{MapCommand, MapEditor};
use crate::components::forms::flush_input_registry;
use crate::components::types::{SectorPreview, WallPreview};
use crate::gfx::image::UiImage;
use crate::level_editor::LevelEditorState;
use crate::render::input::SelItem;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Context, DispatchPhase, Div, Entity, FocusHandle,
    KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels,
    Point, ScrollDelta, ScrollWheelEvent, TouchPhase, WeakEntity, Window, canvas, div, point,
    prelude::*, px, size,
};
use std::cell::Cell;
use std::rc::Rc;
use ui_common::ActiveTheme as _;
use ui_common::widgets::{ContextMenu, Label, LabelSize, Toggleable as _, right_click_menu};

pub(crate) struct CanvasData {
    pub(crate) coords: String,
    pub(crate) wgpu_img: UiImage,
    pub(crate) wall_preview: WallPreview,
    pub(crate) sector_preview: SectorPreview,
    pub(crate) fillet_radius: i32,
    pub(crate) fillet_segments: i32,
    pub(crate) chamfer_dist: i32,
    pub(crate) line_drag_pending: bool,
    pub(crate) line_drag_x: f32,
    pub(crate) line_drag_y: f32,
    pub(crate) line_drag_choice: i32,
}

impl Default for CanvasData {
    fn default() -> Self {
        Self {
            coords: String::new(),
            wgpu_img: Default::default(),
            wall_preview: Default::default(),
            sector_preview: Default::default(),
            fillet_radius: 32,
            fillet_segments: 4,
            chamfer_dist: 16,
            line_drag_pending: Default::default(),
            line_drag_x: Default::default(),
            line_drag_y: Default::default(),
            line_drag_choice: 0,
        }
    }
}

#[derive(Clone, Copy)]
enum GestureKind {
    Edit,
    Pan,
    Orbit,
}

struct Gesture {
    kind: GestureKind,
    start: Point<Pixels>,
    last: Point<Pixels>,
    toggle: bool,
    dragging: bool,
}

#[derive(Clone, Copy)]
enum Trim {
    Fillet,
    Chamfer,
}

pub(crate) struct CanvasState {
    bounds: Rc<Cell<Bounds<Pixels>>>,
    scale: Rc<Cell<f32>>,
    focus: Option<FocusHandle>,
    gesture: Option<Gesture>,
    hover: Option<Point<Pixels>>,
    scroll_orbit: bool,
    pinch_scale: f32,
    trim: Option<Trim>,
    trim_focus: Option<FocusHandle>,
}

impl CanvasState {
    pub fn new() -> Self {
        Self {
            bounds: Rc::new(Cell::new(Bounds::default())),
            focus: None,
            scale: Rc::new(Cell::new(0.)),
            gesture: None,
            hover: None,
            scroll_orbit: false,
            pinch_scale: 1.,
            trim: None,
            trim_focus: None,
        }
    }

    fn local(&self, point: Point<Pixels>) -> [f32; 2] {
        let point = point - self.bounds.get().origin;
        [point.x.into(), point.y.into()]
    }
}

struct ContextCapabilities {
    selected_vertex_count: usize,
    can_split_line: bool,
    can_merge_lines: bool,
    can_dissolve: bool,
    can_fillet: bool,
    can_align: bool,
    can_straighten: bool,
    can_transform: bool,
    has_selection: bool,
    can_add_sector: bool,
    can_merge_sectors: bool,
    can_unmerge_sector: bool,
    can_paste: bool,
    has_sector: bool,
}

impl ContextCapabilities {
    fn new(editor: &LevelEditorState) -> Self {
        Self {
            selected_vertex_count: editor.selected_vertices().len(),
            can_split_line: editor.selection.items().iter().any(|item| {
                matches!(item, SelItem::Line(key)
                    if editor.map.as_ref().is_some_and(|map| map.lines.contains(*key)))
            }),
            can_merge_lines: editor.lines_mergeable(),
            can_dissolve: editor.can_dissolve(),
            can_fillet: editor.can_fillet(),
            can_align: editor.can_align(),
            can_straighten: editor.can_straighten(),
            can_transform: editor.can_transform(),
            has_selection: !editor.selection.is_empty(),
            can_add_sector: editor.can_add_sector(editor.cursor_world),
            can_merge_sectors: editor.can_merge_sectors(),
            can_unmerge_sector: editor.can_unmerge_sector(editor.cursor_world),
            can_paste: editor.can_paste(),
            has_sector: !editor.clipboard.sectors.is_empty(),
        }
    }
}

impl MapEditor {
    pub(crate) fn canvas_property_popup_active(&self) -> bool {
        self.canvas.trim.is_some()
    }

    pub(crate) fn canvas_property_popup_focus(&self) -> Option<&FocusHandle> {
        self.canvas_property_popup_active()
            .then_some(self.canvas.trim_focus.as_ref())
            .flatten()
    }

    pub(crate) fn focus_main_content(&self, window: &mut Window, cx: &mut App) {
        let focus = self.canvas.focus.as_ref().unwrap_or(&self.forms.focus);
        window.focus(focus, cx);
    }

    pub(crate) fn cancel_canvas_gesture(&mut self, cx: &mut Context<Self>) {
        let editing =
            self.canvas.gesture.take().is_some_and(|gesture| {
                matches!(gesture.kind, GestureKind::Edit) && gesture.dragging
            });
        self.canvas.scroll_orbit = false;
        self.canvas.pinch_scale = 1.;
        self.canvas.hover = None;
        if editing {
            self.command(MapCommand::CancelOrClear, cx);
        }
    }

    pub(crate) fn map_canvas(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let focus = self
            .canvas
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let size_probe = self.canvas_size_probe(cx);

        let image = self.presentation.canvas.wgpu_img.clone();
        #[cfg(target_os = "macos")]
        let image = if let Some(surface) = image.surface() {
            Some(gpui::surface(surface).size_full().into_any_element())
        } else {
            self.forms.image("map-canvas", image)
        };
        #[cfg(not(target_os = "macos"))]
        let image = self.forms.image("map-canvas", image);

        let down = cx.listener(Self::canvas_down);
        let inputs = self.forms.inputs.clone();
        let mut content = div()
            .id("map-canvas")
            .debug_selector(|| "map-canvas".into())
            .relative()
            .size_full()
            .overflow_hidden()
            .key_context("EditorCanvas")
            .track_focus(&focus)
            .child(size_probe)
            .children(image)
            .on_any_mouse_down(move |event, window, cx| {
                flush_input_registry(&inputs, window, cx);
                down(event, window, cx);
            })
            .on_mouse_move(cx.listener(Self::canvas_move))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if !hovered && this.canvas.hover.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::canvas_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::canvas_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::canvas_up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::canvas_up))
            .on_scroll_wheel(cx.listener(Self::canvas_scroll))
            .on_action(cx.listener(Self::canvas_action));
        content
            .interactivity()
            .on_pinch(cx.listener(Self::canvas_pinch));

        content = content.children(self.canvas_preview(cx));
        content = content.children(self.canvas_line_choice(cx));
        content = content.children(self.canvas_trim(window, cx));
        let coordinates = self.cursor_coordinates();
        let weak = cx.weak_entity();
        let bounds = self.canvas.bounds.clone();
        // Stretch the context-menu wrapper on both axes so the percentage-sized
        // canvas has a definite viewport instead of collapsing to zero height.
        div()
            .grid()
            .grid_cols(1)
            .grid_rows(1)
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                right_click_menu("map-edit-menu")
                    .trigger(move |menu_active, _, _| {
                        content.when(!menu_active, |content| content.children(coordinates))
                    })
                    .menu(move |window, cx| {
                        Self::canvas_context_menu(&weak, bounds.get(), window, cx)
                    }),
            )
            .into_any_element()
    }

    fn canvas_size_probe(&self, cx: &Context<Self>) -> AnyElement {
        let bounds = self.canvas.bounds.clone();
        let old_scale = self.canvas.scale.clone();
        let weak = cx.weak_entity();
        let drag_move = cx.listener(|this, event: &MouseMoveEvent, window, cx| {
            if this.canvas.gesture.is_some() {
                this.canvas_move(event, window, cx);
                cx.stop_propagation();
            }
        });
        canvas(
            move |new_bounds, window, cx| {
                let previous = bounds.replace(new_bounds);
                let scale = window.scale_factor();
                let scale_changed = (old_scale.replace(scale) - scale).abs() > f32::EPSILON;
                if previous.size != new_bounds.size || scale_changed {
                    weak.update(cx, |this, cx| {
                        this.presentation.pixel_ratio = scale;
                        this.command(
                            MapCommand::Resized(
                                new_bounds.size.width.into(),
                                new_bounds.size.height.into(),
                            ),
                            cx,
                        );
                    })
                    .ok();
                }
            },
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        drag_move(event, window, cx);
                    }
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn cursor_coordinates(&self) -> Option<AnyElement> {
        if self.canvas.hover.is_none()
            || self.modal_active
            || self.canvas.trim.is_some()
            || self.presentation.canvas.line_drag_pending
        {
            return None;
        }
        let text = self.presentation.canvas.coords.clone();
        Some(
            canvas(
                move |bounds, window, cx| {
                    let cursor = window.mouse_position();
                    if !bounds.contains(&cursor) || !window.is_window_active() {
                        return None;
                    }
                    let mut label = div()
                        .flex()
                        .flex_none()
                        .max_w((bounds.size.width - px(8.)).max(px(0.)))
                        .max_h((bounds.size.height - px(8.)).max(px(0.)))
                        .overflow_hidden()
                        .child(
                            Label::new(text)
                                .size(LabelSize::Small)
                                .single_line()
                                .truncate(),
                        )
                        .into_any_element();
                    let extent = label.layout_as_root(
                        size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                        window,
                        cx,
                    );
                    if extent.width + px(8.) > bounds.size.width
                        || extent.height + px(8.) > bounds.size.height
                    {
                        return None;
                    }
                    let place = |cursor: Pixels, extent: Pixels, start: Pixels, end: Pixels| {
                        let after = cursor + px(12.);
                        let wanted = if after + extent <= end - px(4.) {
                            after
                        } else {
                            cursor - extent - px(12.)
                        };
                        wanted.clamp(start + px(4.), (end - extent - px(4.)).max(start + px(4.)))
                    };
                    let origin = point(
                        place(cursor.x, extent.width, bounds.left(), bounds.right()),
                        (cursor.y - extent.height).clamp(
                            bounds.top(),
                            (bounds.bottom() - extent.height).max(bounds.top()),
                        ),
                    );
                    label.prepaint_at(origin, window, cx);
                    Some(label)
                },
                |_, label, window, cx| {
                    if let Some(mut label) = label {
                        label.paint(window, cx);
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full()
            .into_any_element(),
        )
    }

    fn canvas_context_menu(
        weak: &WeakEntity<Self>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<ContextMenu> {
        let inputs = weak.update(cx, |view, _| view.forms.inputs.clone()).ok();
        if let Some(inputs) = inputs {
            flush_input_registry(&inputs, window, cx);
        }
        let local = window.mouse_position() - bounds.origin;
        let state = weak
            .update(cx, |view, cx| {
                view.command(
                    MapCommand::ContextMenuOpening(local.x.into(), local.y.into()),
                    cx,
                );
                ContextCapabilities::new(&view.shared.borrow().map_editor)
            })
            .ok();
        let weak = weak.clone();
        ContextMenu::build(window, cx, move |mut menu, _, _| {
            let Some(canvas) = state else {
                return menu;
            };
            macro_rules! entry {
                ($when:expr, $label:expr, $command:ident) => {
                    if $when {
                        let weak = weak.clone();
                        menu = menu.entry($label, None, move |_, cx| {
                            weak.update(cx, |view, cx| view.command(MapCommand::$command, cx))
                                .ok();
                        });
                    }
                };
            }
            entry!(canvas.can_split_line, "Split Line Here", SplitLineHere);
            entry!(canvas.selected_vertex_count > 1, "Weld Vertices", Weld);
            entry!(canvas.can_merge_lines, "Merge Lines", MergeLines);
            entry!(canvas.can_dissolve, "Dissolve Collinear", Dissolve);
            if canvas.can_fillet {
                for (label, kind) in [("Fillet…", Trim::Fillet), ("Chamfer…", Trim::Chamfer)] {
                    let weak = weak.clone();
                    menu = menu.entry(label, None, move |_, cx| {
                        weak.update(cx, |view, cx| {
                            view.canvas.trim = Some(kind);
                            cx.notify();
                        })
                        .ok();
                    });
                }
            }
            entry!(canvas.can_align, "Align X", AlignX);
            entry!(canvas.can_align, "Align Y", AlignY);
            entry!(canvas.can_align, "Distribute X", DistributeX);
            entry!(canvas.can_align, "Distribute Y", DistributeY);
            entry!(canvas.can_straighten, "Straighten", Straighten);
            entry!(canvas.can_transform, "Rotate Clockwise", RotateCw);
            entry!(canvas.can_transform, "Rotate Counterclockwise", RotateCcw);
            entry!(canvas.can_transform, "Mirror Horizontal", MirrorH);
            entry!(canvas.can_transform, "Mirror Vertical", MirrorV);
            entry!(canvas.can_add_sector, "Add Sector", AddSector);
            entry!(canvas.can_merge_sectors, "Merge Sectors", MergeSectors);
            entry!(canvas.can_unmerge_sector, "Unmerge Sector", UnmergeSector);
            menu = menu.separator();
            entry!(canvas.has_selection, "Copy", Copy);
            entry!(canvas.has_selection, "Cut", Cut);
            entry!(canvas.can_paste, "Paste", Paste);
            entry!(
                canvas.has_sector,
                "Paste to Selected",
                PasteSectorToSelected
            );
            entry!(canvas.has_selection, "Delete", Delete);
            menu
        })
    }

    fn canvas_preview(&mut self, cx: &App) -> Option<Div> {
        let preview = self.presentation.canvas.wall_preview.clone();
        if preview.visible {
            let mut card = div()
                .absolute()
                .flex()
                .gap_2()
                .p_2()
                .bg(cx.theme().colors().panel_background);
            if preview.pinned {
                card = card.left_2().bottom_2();
            } else {
                let size = self.canvas.bounds.get().size;
                card = card
                    .left(px(preview
                        .x
                        .max(0.)
                        .min(f32::from(size.width) - 160.)
                        .max(0.)))
                    .top(px(preview
                        .y
                        .max(0.)
                        .min(f32::from(size.height) - 160.)
                        .max(0.)));
            }
            for (name, side) in [("wall-front", preview.front), ("wall-back", preview.back)] {
                if side.visible {
                    card = card.child(
                        div()
                            .w(px(side.w))
                            .h(px(side.h))
                            .children(self.forms.image(name, side.img)),
                    );
                }
            }
            let sector = self.presentation.canvas.sector_preview.clone();
            if sector.visible {
                card = card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .w(px(sector.side))
                                .h(px(sector.side))
                                .children(self.forms.image("hover-ceiling", sector.ceil_img)),
                        )
                        .child(
                            div()
                                .w(px(sector.side))
                                .h(px(sector.side))
                                .children(self.forms.image("hover-floor", sector.floor_img)),
                        ),
                );
            }
            Some(card)
        } else {
            None
        }
    }

    fn canvas_line_choice(&self, cx: &Context<Self>) -> Option<Div> {
        let canvas = &self.presentation.canvas;
        if !canvas.line_drag_pending {
            return None;
        }
        let size = self.canvas.bounds.get().size;
        Some(
            div()
                .absolute()
                .left(px(canvas
                    .line_drag_x
                    .min(f32::from(size.width) - 124.)
                    .max(0.)))
                .top(px(canvas
                    .line_drag_y
                    .min(f32::from(size.height) - 100.)
                    .max(0.)))
                .w(px(120.))
                .flex()
                .flex_col()
                .p_2()
                .bg(cx.theme().colors().panel_background)
                .child(
                    self.forms
                        .button(
                            "line-move",
                            "Move",
                            cx.listener(|this, _, _, cx| {
                                this.command(MapCommand::LineDragMove, cx);
                            }),
                        )
                        .toggle_state(canvas.line_drag_choice == 0),
                )
                .child(
                    self.forms
                        .button(
                            "line-extrude",
                            "Extrude",
                            cx.listener(|this, _, _, cx| {
                                this.command(MapCommand::LineDragExtrude, cx);
                            }),
                        )
                        .toggle_state(canvas.line_drag_choice == 1),
                ),
        )
    }

    fn canvas_trim(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(trim) = self.canvas.trim {
            let focus = self
                .canvas
                .trim_focus
                .get_or_insert_with(|| cx.focus_handle())
                .clone();
            let mut panel = div()
                .id("canvas-trim-popup")
                .track_focus(&focus)
                .tab_group()
                .absolute()
                .top_4()
                .right_4()
                .w(px(240.))
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .bg(cx.theme().colors().panel_background)
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    if event.button == MouseButton::Left {
                        this.canvas.trim = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }));
            panel = panel.child(self.canvas_trim_fields(trim, window, cx));
            panel = panel
                .child(self.forms.button(
                    "trim-apply",
                    "Apply",
                    cx.listener(move |this, _, _, cx| {
                        let canvas = &this.presentation.canvas;
                        let command = match trim {
                            Trim::Fillet => {
                                MapCommand::Fillet(canvas.fillet_radius, canvas.fillet_segments)
                            }
                            Trim::Chamfer => MapCommand::Chamfer(canvas.chamfer_dist),
                        };
                        this.command(command, cx);
                        this.canvas.trim = None;
                        cx.notify();
                    }),
                ))
                .child(self.forms.button(
                    "trim-cancel",
                    "Cancel",
                    cx.listener(|this, _, _, cx| {
                        this.canvas.trim = None;
                        cx.notify();
                    }),
                ));
            Some(panel.into_any_element())
        } else {
            None
        }
    }

    fn canvas_trim_fields(
        &mut self,
        trim: Trim,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let radius = self.presentation.canvas.fillet_radius;
        let segments = self.presentation.canvas.fillet_segments;
        let distance = self.presentation.canvas.chamfer_dist;
        let mut panel = div().flex().flex_col().gap_2();
        match trim {
            Trim::Fillet => {
                panel = panel
                    .child(self.forms.field(
                        "fillet-radius",
                        "Radius",
                        radius.to_string(),
                        cx.listener(|this, text: &String, _, cx| {
                            if let Ok(value) = text.parse() {
                                this.presentation.canvas.fillet_radius = value;
                                cx.notify();
                            }
                        }),
                        |_, _, _| {},
                        window,
                        cx,
                    ))
                    .child(self.forms.field(
                        "fillet-segments",
                        "Segments",
                        segments.to_string(),
                        cx.listener(|this, text: &String, _, cx| {
                            if let Ok(value) = text.parse() {
                                this.presentation.canvas.fillet_segments = value;
                                cx.notify();
                            }
                        }),
                        |_, _, _| {},
                        window,
                        cx,
                    ));
            }
            Trim::Chamfer => {
                panel = panel.child(self.forms.field(
                    "chamfer-distance",
                    "Distance",
                    distance.to_string(),
                    cx.listener(|this, text: &String, _, cx| {
                        if let Ok(value) = text.parse() {
                            this.presentation.canvas.chamfer_dist = value;
                            cx.notify();
                        }
                    }),
                    |_, _, _| {},
                    window,
                    cx,
                ));
            }
        }
        panel
    }

    fn canvas_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_active || self.canvas.trim.is_some() {
            return;
        }
        self.flush_inspector(window, cx);
        if let Some(focus) = &self.canvas.focus {
            window.focus(focus, cx);
        }
        let [x, y] = self.canvas.local(event.position);
        if self.presentation.canvas.line_drag_pending {
            return;
        }
        if event.button == MouseButton::Right {
            self.command(MapCommand::ContextMenuOpening(x, y), cx);
            return;
        }
        if event.click_count == 2 && event.button == MouseButton::Left {
            self.command(MapCommand::DoubleClicked(x, y), cx);
            cx.notify();
            return;
        }
        let kind = match event.button {
            MouseButton::Left if event.modifiers.shift => {
                self.command(MapCommand::OrbitStart(x, y), cx);
                GestureKind::Orbit
            }
            MouseButton::Middle => GestureKind::Pan,
            MouseButton::Left if event.modifiers.alt => GestureKind::Pan,
            MouseButton::Left => GestureKind::Edit,
            _ => return,
        };
        self.canvas.gesture = Some(Gesture {
            kind,
            start: event.position,
            last: event.position,
            toggle: event.modifiers.shift || event.modifiers.control || event.modifiers.platform,
            dragging: false,
        });
        cx.notify();
    }

    fn canvas_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.modal_active || self.canvas.trim.is_some() {
            return;
        }
        let [x, y] = self.canvas.local(event.position);
        if let Some(mut gesture) = self.canvas.gesture.take() {
            let delta = event.position - gesture.last;
            match gesture.kind {
                GestureKind::Pan => {
                    self.command(MapCommand::Pan(delta.x.into(), delta.y.into()), cx);
                }
                GestureKind::Orbit => {
                    self.command(MapCommand::Orbit(delta.x.into(), delta.y.into()), cx);
                }
                GestureKind::Edit => {
                    let distance = event.position - gesture.start;
                    if !gesture.dragging
                        && (f32::from(distance.x).abs() > 3. || f32::from(distance.y).abs() > 3.)
                    {
                        let [sx, sy] = self.canvas.local(gesture.start);
                        self.command(MapCommand::ToolDragStart(sx, sy, gesture.toggle), cx);
                        gesture.dragging = true;
                    }
                    if gesture.dragging {
                        self.command(MapCommand::ToolDrag(x, y), cx);
                    }
                }
            }
            gesture.last = event.position;
            self.canvas.gesture = Some(gesture);
            self.canvas.hover = self
                .canvas
                .bounds
                .get()
                .contains(&event.position)
                .then_some(event.position);
        } else if self.canvas.hover != Some(event.position) {
            self.canvas.hover = Some(event.position);
            self.canvas.scroll_orbit = false;
            self.command(MapCommand::Hover(x, y), cx);
        } else {
            return;
        }
        cx.notify();
    }

    fn canvas_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.modal_active || self.canvas.trim.is_some() {
            self.cancel_canvas_gesture(cx);
            return;
        }
        if let Some(gesture) = self.canvas.gesture.take() {
            let [x, y] = self.canvas.local(event.position);
            if matches!(gesture.kind, GestureKind::Edit) {
                if gesture.dragging {
                    self.command(MapCommand::ToolDragEnd(x, y), cx);
                } else {
                    self.command(MapCommand::ToolClick(x, y, gesture.toggle), cx);
                }
            }
            cx.notify();
        }
    }

    fn canvas_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.modal_active || self.canvas.trim.is_some() {
            cx.stop_propagation();
            return;
        }
        if matches!(event.touch_phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.canvas.scroll_orbit = false;
            cx.stop_propagation();
            return;
        }
        if event.touch_phase == TouchPhase::Started {
            self.canvas.scroll_orbit = false;
        }
        let [x, y] = self.canvas.local(event.position);
        let delta = event.delta.pixel_delta(px(40.));
        if event.modifiers.shift {
            if !self.canvas.scroll_orbit {
                self.command(MapCommand::OrbitStart(x, y), cx);
                self.canvas.scroll_orbit = true;
            }
            self.command(MapCommand::Orbit(delta.x.into(), delta.y.into()), cx);
        } else if event.modifiers.control || matches!(event.delta, ScrollDelta::Lines(_)) {
            self.canvas.scroll_orbit = false;
            self.command(MapCommand::ZoomAt(delta.y.into(), x, y), cx);
        } else {
            self.canvas.scroll_orbit = false;
            self.command(MapCommand::Pan(delta.x.into(), delta.y.into()), cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn canvas_pinch(&mut self, event: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.modal_active {
            return;
        }
        if event.phase == TouchPhase::Started {
            self.canvas.pinch_scale = 1.;
            self.command(MapCommand::PinchStarted, cx);
        }
        if matches!(event.phase, TouchPhase::Started | TouchPhase::Moved) {
            let [x, y] = self.canvas.local(event.position);
            self.canvas.pinch_scale *= (1. + event.delta).max(0.01);
            self.command(MapCommand::PinchUpdated(self.canvas.pinch_scale, x, y), cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn canvas_action(&mut self, action: &CanvasAction, _: &mut Window, cx: &mut Context<Self>) {
        if self.modal_active {
            return;
        }
        let command = action.0;
        if self.canvas.trim.is_some() {
            if command == MapCommand::CancelOrClear {
                self.canvas.trim = None;
                cx.notify();
            }
            return;
        }
        if self.presentation.canvas.line_drag_pending {
            match command {
                MapCommand::HeightStep {
                    ..
                } => {
                    self.presentation.canvas.line_drag_choice =
                        1 - self.presentation.canvas.line_drag_choice;
                    cx.notify();
                }
                MapCommand::CommitOrClear => self.command(
                    if self.presentation.canvas.line_drag_choice == 0 {
                        MapCommand::LineDragMove
                    } else {
                        MapCommand::LineDragExtrude
                    },
                    cx,
                ),
                MapCommand::CancelOrClear => self.command(MapCommand::LineDragCancel, cx),
                _ => {}
            }
        } else {
            self.command(command, cx);
        }
    }
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = editor_canvas, no_json)]
pub(super) struct CanvasAction(MapCommand);

pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some("EditorCanvas");
    cx.bind_keys(
        [
            (
                "up",
                MapCommand::HeightStep {
                    raise: true,
                    opposite: false,
                },
            ),
            (
                "down",
                MapCommand::HeightStep {
                    raise: false,
                    opposite: false,
                },
            ),
            (
                "shift-up",
                MapCommand::HeightStep {
                    raise: true,
                    opposite: true,
                },
            ),
            (
                "shift-down",
                MapCommand::HeightStep {
                    raise: false,
                    opposite: true,
                },
            ),
            ("space", MapCommand::ToggleOverlays),
            ("escape", MapCommand::CancelOrClear),
            ("enter", MapCommand::CommitOrClear),
            ("backspace", MapCommand::Delete),
            ("delete", MapCommand::Delete),
            ("g", MapCommand::CycleGrid),
            ("f", MapCommand::FlipLines),
            ("i", MapCommand::SplitIntersections),
            ("b", MapCommand::PickBrush),
            ("+", MapCommand::ZoomIn),
            ("=", MapCommand::ZoomIn),
            ("-", MapCommand::ZoomOut),
            ("3", MapCommand::ResetView),
        ]
        .map(|(key, command)| KeyBinding::new(key, CanvasAction(command), context)),
    );
}
