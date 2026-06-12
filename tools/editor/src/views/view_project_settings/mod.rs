//! Project Settings dialog: IWAD, node format, LAUNCH thing, last map.

mod conversions;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use editor_core::NodesFormat;
use slint::ComponentHandle as _;

use crate::defaults::{LAUNCH_THING_KINDS, launch_thing_name};
use crate::generated::{EditorWindow, ProjectSettingsController};
use crate::prefs::{self, PopupWindow};
use crate::state::SharedState;
use crate::views::model;
use crate::views::view_window::restore as restore_geom;

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ProjectSettingsController>()
        .on_populate(move || {
            let Some(ui) = weak.upgrade() else { return };
            restore_geom(&ui, &s, PopupWindow::ProjectSettings);
            populate(&ui, &s);
        });

    let weak = ui.as_weak();
    ui.global::<ProjectSettingsController>()
        .on_pick_iwad_path(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("WAD", &["wad", "WAD"])
                .pick_file()
            {
                ui.global::<ProjectSettingsController>()
                    .set_iwad_path(path.display().to_string().into());
            }
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ProjectSettingsController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        apply(&ui, &s);
    });
}

/// Open edge: fill from open project; disabled when none.
fn populate(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<ProjectSettingsController>();
    let state = shared.borrow();
    let Some(project) = &state.project else {
        ctl.set_active(false);
        return;
    };
    let settings = &project.settings;
    ctl.set_active(true);
    ctl.set_iwad_path(settings.iwad.display().to_string().into());
    ctl.set_nodes_format(settings.nodes_format.into());
    let names: Vec<slint::SharedString> = LAUNCH_THING_KINDS
        .iter()
        .map(|&k| launch_thing_name(k).into())
        .collect();
    ctl.set_launch_types(model(names));
    let index = LAUNCH_THING_KINDS
        .iter()
        .position(|&k| k == settings.launch_type)
        .unwrap_or(0);
    ctl.set_launch_type_index(index as i32);
    ctl.set_last_map(settings.last_map.clone().unwrap_or_default().into());
}

/// Apply edge: write fields to project, persist.
fn apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<ProjectSettingsController>();
    let iwad = PathBuf::from(ctl.get_iwad_path().to_string());
    let nodes_format = NodesFormat::from(ctl.get_nodes_format());
    let launch_type = LAUNCH_THING_KINDS
        .get(ctl.get_launch_type_index().max(0) as usize)
        .copied();

    let mut state = shared.borrow_mut();
    let iwad_changed = state.iwad.as_deref() != Some(iwad.as_path());
    let Some(project) = state.project.as_mut() else {
        return;
    };
    project.settings.iwad = iwad.clone();
    project.settings.nodes_format = nodes_format;
    if let Some(launch_type) = launch_type {
        project.settings.launch_type = launch_type;
    }
    if let Err(e) = project.save() {
        log::error!("save project settings: {e}");
    }
    if iwad_changed {
        // invalidates gfx/asset caches
        state.set_iwad(iwad);
    }
    if let Err(e) = prefs::save_prefs(&state.prefs) {
        log::warn!("save project-settings prefs: {e}");
    }
    drop(state);
}
