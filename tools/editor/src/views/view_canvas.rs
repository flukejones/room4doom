//! Map canvas boundary: gesture/key callbacks → model mutations → repaint + panel sync.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle as _, Timer};

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
use crate::views::view_status::update_status;
use crate::views::view_tex_browser::push_brush_chip;
use crate::{bsp_anim, jobs};

/// 3D-camera ease tic (~60 Hz).
const CAM_EASE_INTERVAL: Duration = Duration::from_millis(16);

thread_local! {
    /// `thread_local` so it arms without a live `SharedState` borrow — its tic closure borrows `shared`.
    static CAM_TIMER: Timer = Timer::default();
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
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_pick_at(move |x, y| {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.pick_at([x, y]);
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_split_line_here(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = {
            let mut st = s.borrow_mut();
            let world = st.app.cursor_world;
            st.app.split_selected_line_at(world)
        };
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_paste_sector_to_selected(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.paste();
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_weld(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.weld_selected();
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_merge_lines(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.merge_selected_lines();
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_add_sector(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = {
            let mut st = s.borrow_mut();
            let world = st.app.cursor_world;
            st.app.add_sector_at(world)
        };
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_merge_sectors(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = s.borrow_mut().app.merge_selected_sectors();
        after_edit(&ui, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_unmerge_sector(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = {
            let mut st = s.borrow_mut();
            let world = st.app.cursor_world;
            st.app.unmerge_sector(world)
        };
        after_edit(&ui, &s, damage);
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
            // Delta since last pinch update.
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

    macro_rules! key_action {
        ($setter:ident, $body:expr) => {{
            let weak = ui.as_weak();
            let s = shared.clone();
            canvas.$setter(move || {
                let Some(ui) = weak.upgrade() else { return };
                let damage = ($body)(&ui, &s);
                after_edit(&ui, &s, damage);
            });
        }};
    }

    key_action!(
        on_undo,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.undo()
    );
    key_action!(
        on_redo,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.redo()
    );
    key_action!(
        on_copy,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.copy_selection()
    );
    key_action!(
        on_cut,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.cut_selection()
    );
    key_action!(
        on_paste,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.paste()
    );
    key_action!(
        on_delete,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.delete_active()
    );
    key_action!(
        on_cycle_grid,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.cycle_grid()
    );
    key_action!(
        on_toggle_overlays,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.toggle_overlays()
    );
    key_action!(
        on_flip_lines,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| {
            let lines = s.borrow().app.selected_lines();
            s.borrow_mut().app.flip_selected_lines(&lines)
        }
    );
    key_action!(
        on_split_intersections,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s
            .borrow_mut()
            .app
            .split_selected_at_intersections()
    );

    let weak = ui.as_weak();
    let s = shared.clone();
    canvas.on_pick_brush(move || {
        let Some(ui) = weak.upgrade() else { return };
        if s.borrow_mut().app.pick_brush_from_selection() {
            push_brush_chip(&ui, &s);
        }
    });
    key_action!(
        on_zoom_in,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.zoom_in()
    );
    key_action!(
        on_zoom_out,
        |_ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| s.borrow_mut().app.zoom_out()
    );

    // Escape: discard draw-in-progress; else clear selection.
    key_action!(
        on_cancel_or_clear,
        |ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| {
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
        on_commit_or_clear,
        |ui: &EditorWindow, s: &Rc<RefCell<SharedState>>| {
            bsp_anim::clear(ui, s);
            let state = &mut *s.borrow_mut();
            // Combine so commit damage is not discarded.
            let committed = state.app.cancel_gesture();
            committed.combine(state.app.clear_selection())
        }
    );

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ToolController>().on_reset_view(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = {
            let mut st = s.borrow_mut();
            st.app.reset_view_top_down()
        };
        after_edit(&ui, &s, damage);
    });

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
    ctl.set_has_lines(!clip.fragment.lines.is_empty());
    ctl.set_has_things(!clip.fragment.things.is_empty());
}

fn sync_sampled_sector(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let sampled = shared.borrow_mut().app.sampled_sector.take();
    if sampled.is_some() {
        panels::sync(ui, shared);
    }
}

/// Double-click: open wall or sector editor. Rust sets `*-edit-visible`; close stays in Slint.
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
        let ctl = ui.global::<WallEditController>();
        ctl.set_line_index(line as i32);
        ctl.set_wall_edit_visible(true);
    } else if let Some(sector) = sector {
        let ctl = ui.global::<SectorEditController>();
        ctl.set_sector_index(sector as i32);
        ctl.set_sector_edit_visible(true);
    }
}

/// No-op unless LAUNCH tool active.
fn launch_at(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if shared.borrow().app.tool != Tool::Launch {
        return;
    }
    jobs::start_launch(ui, shared);
}
