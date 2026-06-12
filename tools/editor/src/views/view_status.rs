//! Status bar boundary: cursor coords, zoom/grid/snap chips, skill filter.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::boundary::SkillFilter;
use crate::generated::{EditorWindow, StatusController};
use crate::project::save_prefs_now;
use crate::render::apply_damage;
use crate::state::{Damage, SharedState};

/// Common tail of every status-chip handler; the upgrade guards against callbacks firing during teardown.
fn refresh(weak: &slint::Weak<EditorWindow>, shared: &Rc<RefCell<SharedState>>, damage: Damage) {
    if let Some(ui) = weak.upgrade() {
        update_status(&ui, shared);
        apply_damage(&ui, shared, damage);
    }
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_zoom_selected(move |level| {
            let damage = {
                let state = &mut *s.borrow_mut();
                if level <= 0.0 {
                    state.app.zoom_fit()
                } else {
                    state.app.zoom_to(level)
                }
            };
            refresh(&weak, &s, damage);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_grid_selected(move |grid| {
            let damage = s.borrow_mut().app.set_grid(grid);
            refresh(&weak, &s, damage);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>().on_snap_toggled(move || {
        let damage = {
            let state = &mut *s.borrow_mut();
            let on = !state.app.snap;
            state.app.set_snap(on)
        };
        refresh(&weak, &s, damage);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_snap_vertex_toggled(move || {
            {
                let state = &mut *s.borrow_mut();
                let on = !state.app.snap_to_vertex;
                state.app.set_snap_to_vertex(on);
                save_prefs_now(state);
            }
            refresh(&weak, &s, Damage::None);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_snap_line_toggled(move || {
            {
                let state = &mut *s.borrow_mut();
                let on = !state.app.snap_to_line;
                state.app.set_snap_to_line(on);
                save_prefs_now(state);
            }
            refresh(&weak, &s, Damage::None);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_angle_snap_toggled(move || {
            {
                let state = &mut *s.borrow_mut();
                let on = !state.app.angle_snap;
                state.app.set_angle_snap(on);
                save_prefs_now(state);
            }
            refresh(&weak, &s, Damage::None);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_highlight_unenclosed_toggled(move || {
            let damage = {
                let state = &mut *s.borrow_mut();
                let on = !state.app.highlight_unenclosed;
                state.app.set_highlight_unenclosed(on)
            };
            refresh(&weak, &s, damage);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_overlays_toggled(move || {
            let damage = {
                let state = &mut *s.borrow_mut();
                let on = !state.app.overlays_visible;
                state.app.set_overlays_visible(on)
            };
            refresh(&weak, &s, damage);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<StatusController>()
        .on_things_selected(move |filter| {
            let damage = s.borrow_mut().app.set_skill_filter(filter.into());
            refresh(&weak, &s, damage);
        });
}

pub(crate) fn update_status(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let state = shared.borrow();
    let status = ui.global::<StatusController>();
    let [wx, wy] = state.app.cursor_world;
    status.set_coords(slint::format!(
        "({}, {})",
        wx.round() as i32,
        wy.round() as i32
    ));
    status.set_zoom(slint::format!("zoom {:.2}x", state.app.camera.zoom_level()));
    status.set_grid(slint::format!("grid {}", state.app.grid));
    status.set_snap(state.app.snap);
    status.set_snap_vertex(state.app.snap_to_vertex);
    status.set_snap_line(state.app.snap_to_line);
    status.set_angle_snap(state.app.angle_snap);
    status.set_highlight_unenclosed(state.app.highlight_unenclosed);
    status.set_overlays_visible(state.app.overlays_visible);
    let things = match state.app.skill_filter {
        SkillFilter::All => "things: all",
        SkillFilter::Easy => "things: easy",
        SkillFilter::Normal => "things: normal",
        SkillFilter::Hard => "things: hard",
    };
    status.set_things(things.into());
}
