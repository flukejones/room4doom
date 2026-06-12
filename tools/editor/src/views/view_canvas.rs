//! Map canvas boundary: gesture/key callbacks → model mutations → repaint + panel sync.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle as _, Timer};

use editor_core::{ArenaKey as _, Axis};

use crate::boundary::{SelectMode, Tool};
use crate::generated::{
    CanvasController, ClipboardController, EditorWindow, SectorEditController, ToolController,
    WallEditController,
};
use crate::level_editor::pick3d::PickKind;
use crate::level_editor::preview;
use crate::render::camera3d::Projection;
use crate::render::{apply_damage, regrid_and_paint};
use crate::state::{Damage, SharedState};
use crate::views::view_panels as panels;
use crate::views::view_sector_edit as sector_edit;
use crate::views::view_status::update_status;
use crate::views::view_tex_browser::push_brush_chip;
use crate::views::view_wall_edit as wall_edit;
use crate::{bsp_anim, jobs};

/// 3D-camera ease tic (~60 Hz).
const CAM_EASE_INTERVAL: Duration = Duration::from_millis(16);

thread_local! {
    /// `thread_local` so it arms without a live `SharedState` borrow — its tic closure borrows `shared`.
    static CAM_TIMER: Timer = Timer::default();
}

/// Wire a zero-arg `$ctl` callback: upgrade the weak handle, run `$body` for damage, then `after_edit`; the `ui` arm passes the window to bodies that need it.
macro_rules! key_action {
    ($ui:expr, $shared:expr, $ctl:ty, $setter:ident, ui $body:expr) => {{
        let weak = $ui.as_weak();
        let s = $shared.clone();
        $ui.global::<$ctl>().$setter(move || {
            let Some(ui) = weak.upgrade() else { return };
            let damage = ($body)(&ui, &s);
            after_edit(&ui, &s, damage);
        });
    }};
    ($ui:expr, $shared:expr, $ctl:ty, $setter:ident, $body:expr) => {{
        let weak = $ui.as_weak();
        let s = $shared.clone();
        $ui.global::<$ctl>().$setter(move || {
            let Some(ui) = weak.upgrade() else { return };
            let damage = ($body)(&s);
            after_edit(&ui, &s, damage);
        });
    }};
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let canvas = ui.global::<CanvasController>();

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_tool_click(move |x, y, shift| {
        let Some(ui) = weak.upgrade() else { return };
        // BSP overlay absorbs clicks instead of editing.
        if s.borrow().bsp_anim.is_some() {
            bsp_anim::clear(&ui, &s);
            return;
        }
        let damage = s.borrow_mut().app.tool_click([x, y], shift);
        launch_at(&ui, &s);
        preview::handle_pick(&ui, &s, [x, y]);
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_context_menu_opening(move |x, y| {
        let Some(ui) = weak.upgrade() else { return };
        {
            let state = &mut *s.borrow_mut();
            state.app.cursor_world = state.app.screen_to_world([x, y]);
        }
        sync_selection(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_tool_drag_start(move |x, y, shift| {
        let damage = s.borrow_mut().app.begin_tool_drag([x, y], shift);
        let Some(ui) = weak.upgrade() else { return };
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_tool_drag(move |x, y| {
        let damage = s.borrow_mut().app.drag_to([x, y]);
        let Some(ui) = weak.upgrade() else { return };
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_tool_drag_end(move |x, y| {
        let damage = s.borrow_mut().app.end_drag([x, y]);
        let Some(ui) = weak.upgrade() else { return };
        if s.borrow().app.line_drag_pending().is_some() {
            let canvas = ui.global::<CanvasController>();
            canvas.set_line_drag_x(x);
            canvas.set_line_drag_y(y);
            canvas.set_line_drag_choice(0);
            canvas.set_line_drag_pending(true);
        }
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_line_drag_move(move || {
        let Some(ui) = weak.upgrade() else { return };
        ui.global::<CanvasController>().set_line_drag_pending(false);
        let damage = s.borrow_mut().app.commit_pending_move();
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_line_drag_extrude(move || {
        let Some(ui) = weak.upgrade() else { return };
        ui.global::<CanvasController>().set_line_drag_pending(false);
        let damage = s.borrow_mut().app.commit_pending_extrude();
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_line_drag_cancel(move || {
        let Some(ui) = weak.upgrade() else { return };
        ui.global::<CanvasController>().set_line_drag_pending(false);
        if s.borrow().app.line_drag_pending().is_none() {
            return;
        }
        let damage = s.borrow_mut().app.cancel_pending_drag();
        after_edit(&ui, &s, damage);
    });

    key_action!(
        ui,
        shared,
        CanvasController,
        on_split_line_here,
        |s: &Rc<RefCell<SharedState>>| {
            let mut st = s.borrow_mut();
            let world = st.app.cursor_world;
            st.app.split_selected_line_at(world)
        }
    );
    key_action!(
        ui,
        shared,
        CanvasController,
        on_paste_sector_to_selected,
        |s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.paste()
    );
    key_action!(ui, shared, CanvasController, on_weld, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .weld_selected());
    key_action!(ui, shared, CanvasController, on_merge_lines, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .merge_selected_lines());
    key_action!(ui, shared, CanvasController, on_dissolve, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .dissolve_selected());

    key_action!(ui, shared, CanvasController, on_align_x, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .align_selected(Axis::X));
    key_action!(ui, shared, CanvasController, on_align_y, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .align_selected(Axis::Y));
    key_action!(ui, shared, CanvasController, on_distribute_x, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .distribute_selected(Axis::X));
    key_action!(ui, shared, CanvasController, on_distribute_y, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .distribute_selected(Axis::Y));
    key_action!(ui, shared, CanvasController, on_straighten, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .straighten_selected());
    key_action!(ui, shared, CanvasController, on_rotate_cw, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .rotate_selected_90(true));
    key_action!(ui, shared, CanvasController, on_rotate_ccw, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .rotate_selected_90(false));
    key_action!(ui, shared, CanvasController, on_mirror_h, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .mirror_selected(Axis::X));
    key_action!(ui, shared, CanvasController, on_mirror_v, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .mirror_selected(Axis::Y));

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_fillet(move |radius, segments| {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s
            .borrow_mut()
            .app
            .fillet_selected(radius as f32, segments.max(1) as u32);
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_chamfer(move |dist| {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.chamfer_selected(dist as f32);
        after_edit(&ui, &s, damage);
    });
    key_action!(ui, shared, CanvasController, on_add_sector, |s: &Rc<
        RefCell<SharedState>,
    >| {
        let mut st = s.borrow_mut();
        let world = st.app.cursor_world;
        st.app.add_sector_at(world)
    });
    key_action!(ui, shared, CanvasController, on_merge_sectors, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .merge_selected_sectors());
    key_action!(ui, shared, CanvasController, on_unmerge_sector, |s: &Rc<
        RefCell<SharedState>,
    >| {
        let mut st = s.borrow_mut();
        let world = st.app.cursor_world;
        st.app.unmerge_sector(world)
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_hover(move |x, y| {
        let Some(ui) = weak.upgrade() else { return };
        // Shape-draw takes priority; poly chain pins rubber-line preview.
        let damage = {
            let mut st = s.borrow_mut();
            let shape = st.app.shape_hover([x, y]);
            if matches!(shape, Damage::None) {
                st.app.hover_poly([x, y])
            } else {
                shape
            }
        };
        preview::handle_hover(&ui, &s, [x, y]);
        update_status(&ui, &s);
        ui.global::<CanvasController>()
            .set_can_paste(s.borrow().app.can_paste());
        if !matches!(damage, Damage::None) {
            apply_damage(&ui, &s, damage);
        }
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_double_clicked(move |x, y| {
        let Some(ui) = weak.upgrade() else { return };
        open_editor_at(&ui, &s, [x, y]);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_pan(move |dx, dy| {
        let damage = s.borrow_mut().app.pan(dx, dy);
        let Some(ui) = weak.upgrade() else { return };
        update_status(&ui, &s);
        apply_damage(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_zoom_at(move |delta, x, y| {
        let damage = s.borrow_mut().app.scroll_zoom(delta, [x, y]);
        let Some(ui) = weak.upgrade() else { return };
        update_status(&ui, &s);
        apply_damage(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_orbit(move |dx, dy| {
        let damage = s.borrow_mut().app.orbit(dx, dy);
        let Some(ui) = weak.upgrade() else { return };
        apply_damage(&ui, &s, damage);
    });

    let s = shared.clone();
    canvas.on_orbit_start(move |x, y| {
        s.borrow_mut().app.orbit_start([x, y]);
    });

    let s = shared.clone();
    canvas.on_pinch_started(move || {
        s.borrow_mut().pinch_scale = 1.0;
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_pinch_updated(move |cumulative, cx, cy| {
        let damage = {
            let state = &mut *s.borrow_mut();
            // Delta since the last pinch update.
            let factor = if state.pinch_scale > 0.0 {
                cumulative / state.pinch_scale
            } else {
                1.0
            };
            state.pinch_scale = cumulative;
            state.app.handle_pinch(factor, [cx, cy])
        };
        let Some(ui) = weak.upgrade() else { return };
        update_status(&ui, &s);
        apply_damage(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_resized(move |w, h| {
        let damage = s.borrow_mut().app.set_viewport(w, h);
        let Some(ui) = weak.upgrade() else { return };
        apply_damage(&ui, &s, damage);
    });

    init_keys(ui, shared);
}

/// Named keyboard actions from the canvas.
fn init_keys(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let canvas = ui.global::<CanvasController>();

    key_action!(ui, shared, CanvasController, on_undo, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .undo());
    key_action!(ui, shared, CanvasController, on_redo, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .redo());
    key_action!(ui, shared, CanvasController, on_copy, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .copy_selection());
    key_action!(ui, shared, CanvasController, on_cut, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .cut_selection());
    key_action!(ui, shared, CanvasController, on_paste, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .paste());
    key_action!(ui, shared, CanvasController, on_delete, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .delete_selection());
    key_action!(ui, shared, CanvasController, on_cycle_grid, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .cycle_grid());
    key_action!(
        ui,
        shared,
        CanvasController,
        on_toggle_overlays,
        |s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.toggle_overlays()
    );
    key_action!(ui, shared, CanvasController, on_flip_lines, |s: &Rc<
        RefCell<SharedState>,
    >| {
        let lines = s.borrow().app.selected_lines();
        s.borrow_mut().app.flip_selected_lines(&lines)
    });
    key_action!(
        ui,
        shared,
        CanvasController,
        on_split_intersections,
        |s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.split_selected_at_intersections()
    );

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_pick_brush(move || {
        let Some(ui) = weak.upgrade() else { return };
        if s.borrow_mut().app.pick_brush_from_selection() {
            push_brush_chip(&ui, &s);
        }
    });
    key_action!(ui, shared, CanvasController, on_zoom_in, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .zoom_in());
    key_action!(ui, shared, CanvasController, on_zoom_out, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .zoom_out());

    // Escape: discard draw-in-progress; else clear selection.
    key_action!(
        ui,
        shared,
        CanvasController,
        on_cancel_or_clear,
        ui |ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| {
            bsp_anim::clear(ui, s);
            let state = &mut *s.borrow_mut();
            if state.app.drawing_active() {
                state.app.discard_gesture()
            } else {
                state.app.clear_selection()
            }
        }
    );

    // Enter: commit draw-in-progress; else clear selection.
    key_action!(
        ui,
        shared,
        CanvasController,
        on_commit_or_clear,
        ui |ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| {
            bsp_anim::clear(ui, s);
            let state = &mut *s.borrow_mut();
            // Combine so commit damage is not discarded.
            let committed = state.app.cancel_gesture();
            committed.combine(state.app.clear_selection())
        }
    );

    key_action!(ui, shared, ToolController, on_reset_view, |s: &Rc<
        RefCell<SharedState>,
    >| s
        .borrow_mut()
        .app
        .reset_view_top_down());

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ToolController>()
        .on_set_projection_perspective(move |perspective| {
            let Some(ui) = weak.upgrade() else { return };
            let projection = if perspective {
                Projection::Perspective
            } else {
                Projection::Ortho
            };
            let damage = s.borrow_mut().app.set_projection(projection);
            after_edit(&ui, &s, damage);
        });

    let weak = ui.as_weak();
    canvas.on_save(move || {
        if let Some(ui) = weak.upgrade() {
            ui.invoke_menu_save();
        }
    });
}

/// Ease the 3D camera toward its target each tic until it settles.
pub(crate) fn start_cam_ease(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    CAM_TIMER.with(|t| {
        t.start(slint::TimerMode::Repeated, CAM_EASE_INTERVAL, move || {
            let Some(ui) = weak.upgrade() else { return };
            let more = s.borrow_mut().app.ease_camera();
            // apply_damage would re-arm the timer; repaint directly.
            regrid_and_paint(&ui, &s);
            if !more {
                CAM_TIMER.with(Timer::stop);
            }
        });
    });
}

/// Post-edit refresh: status, damage, panel sync.
pub(crate) fn after_edit(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, damage: Damage) {
    update_status(ui, shared);
    apply_damage(ui, shared, damage);
    sync_sampled_sector(ui, shared);
    sync_clipboard(ui, shared);
    sync_selection(ui, shared);
    panels::sync(ui, shared);
}

/// Push selection counts for canvas context-menu gating.
fn sync_selection(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let state = shared.borrow();
    let canvas = ui.global::<CanvasController>();
    canvas.set_selected_vertex_count(state.app.selected_vertices().len() as i32);
    canvas.set_can_merge_lines(state.app.lines_mergeable());
    canvas.set_can_dissolve(state.app.can_dissolve());
    canvas.set_can_fillet(state.app.can_fillet());
    canvas.set_can_align(state.app.can_align());
    canvas.set_can_straighten(state.app.can_straighten());
    canvas.set_can_transform(state.app.can_transform());
    canvas.set_has_selection(!state.app.selection.is_empty());
    canvas.set_can_merge_sectors(state.app.can_merge_sectors());
    canvas.set_can_add_sector(state.app.can_add_sector(state.app.cursor_world));
    canvas.set_can_unmerge_sector(state.app.can_unmerge_sector(state.app.cursor_world));
    canvas.set_can_paste(state.app.can_paste());
    let tool = ui.global::<ToolController>();
    tool.set_projection_perspective(state.app.projection() == Projection::Perspective);
}

fn sync_clipboard(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let clip = &shared.borrow().app.clipboard;
    let ctl = ui.global::<ClipboardController>();
    ctl.set_has_sector(!clip.sectors.is_empty());
}

fn sync_sampled_sector(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let sampled = shared.borrow_mut().app.sampled_sector.take();
    if sampled.is_some() {
        panels::sync(ui, shared);
    }
}

/// Double-click: populate the wall or sector editor, show it only on success; close stays in Slint.
fn open_editor_at(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, screen: [f32; 2]) {
    let (line, sector) = {
        let state = shared.borrow();
        if !matches!(state.app.tool, Tool::Select(_)) {
            return;
        }
        if state.app.map.is_none() {
            return;
        }
        // Lines first; SelectMode::All would include Thing/Vertex tiers shadowing walls.
        let line = match state
            .app
            .pick_3d_select(screen, SelectMode::Line)
            .map(|h| h.kind)
        {
            Some(PickKind::Linedef(i)) => Some(i),
            _ => None,
        };
        if line.is_some() {
            (line, None)
        } else {
            let sector = match state
                .app
                .pick_3d_select(screen, SelectMode::Sector)
                .map(|h| h.kind)
            {
                Some(PickKind::Sector(s)) => Some(s),
                _ => None,
            };
            (None, sector)
        }
    };
    if let Some(line) = line {
        if wall_edit::open(ui, shared, line.slot() as i32) {
            ui.global::<WallEditController>()
                .set_wall_edit_visible(true);
        }
    } else if let Some(sector) = sector
        && sector_edit::open(ui, shared, sector.slot() as i32)
    {
        ui.global::<SectorEditController>()
            .set_sector_edit_visible(true);
    }
}

/// No-op unless LAUNCH tool active.
fn launch_at(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if shared.borrow().app.tool != Tool::Launch {
        return;
    }
    jobs::start_launch(ui, shared);
}
