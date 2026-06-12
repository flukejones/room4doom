//! Menu-bar callbacks. Delegates to `project` (file lifecycle) and `jobs` (background export).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::generated::{EditorWindow, NewMapController};
use crate::jobs;
use crate::png_export::PNG_SCALE_PRESETS;
use crate::project::{
    WadLoad, close_project, confirm_discard, import_dpr, load_wad_file, new_map, new_project,
    open_project, open_wad, refresh_all, refresh_map_tab_title, save_prefs_now, save_project,
};
use crate::state::SharedState;

pub(crate) fn set_callbacks_menu(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_new_project(move || {
        let Some(ui) = weak.upgrade() else { return };
        new_project(&ui, &s);
        refresh_all(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<NewMapController>().on_confirmed(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        if name.is_empty() {
            return;
        }
        new_map(&ui, &s, name.as_str());
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_close_project(move || {
        let Some(ui) = weak.upgrade() else { return };
        close_project(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_open_wad(move || {
        let Some(ui) = weak.upgrade() else { return };
        if s.borrow().iwad.is_none() {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("WAD", &["wad", "WAD"])
                .pick_file()
            else {
                return;
            };
            if !matches!(load_wad_file(&s, &path), WadLoad::Iwad) {
                return;
            }
        }
        open_wad(&ui, &s, None);
        refresh_all(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_open_project(move || {
        let Some(ui) = weak.upgrade() else { return };
        if !confirm_discard(&s) {
            return;
        }
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Open project folder")
            .pick_folder()
        else {
            return;
        };
        open_project(&ui, &s, &dir);
        refresh_all(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_import_dpr(move || {
        let Some(ui) = weak.upgrade() else { return };
        if !confirm_discard(&s) {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("DoomEd project", &["dpr"])
            .pick_file()
        else {
            return;
        };
        import_dpr(&s, &path);
        refresh_all(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_open_recent(move |path| {
        let Some(ui) = weak.upgrade() else { return };
        if !confirm_discard(&s) {
            return;
        }
        open_project(&ui, &s, Path::new(path.as_str()));
        refresh_all(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_save(move || {
        let Some(ui) = weak.upgrade() else { return };
        save_project(&s);
        refresh_map_tab_title(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_export_wad(move || {
        let Some(ui) = weak.upgrade() else { return };
        if s.borrow().job_busy {
            log::warn!("export already running");
            return;
        }
        let default_name = format!("{}.wad", s.borrow().app.map_name);
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(default_name)
            .add_filter("WAD", &["wad"])
            .save_file()
        else {
            return;
        };
        jobs::start_export(&ui, &s, path);
    });

    wire_png(ui, shared);

    let s = shared.clone();
    ui.on_menu_quit(move || {
        if !confirm_discard(&s) {
            return;
        }
        save_prefs_now(&mut s.borrow_mut());
        slint::quit_event_loop().ok();
    });

    let s = shared.clone();
    ui.window().on_close_requested(move || {
        if !confirm_discard(&s) {
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        save_prefs_now(&mut s.borrow_mut());
        slint::CloseRequestResponse::HideWindow
    });
}

fn wire_png(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.on_menu_export_png(move |scale| {
        let Some(ui) = weak.upgrade() else { return };
        let scale = PNG_SCALE_PRESETS
            .iter()
            .copied()
            .find(|p| (p - scale).abs() < f32::EPSILON)
            .unwrap_or(1.0);
        // Drop the borrow before the modal dialog re-enters the event loop.
        let map_name = {
            let state = s.borrow();
            if state.app.map.is_none() {
                return;
            }
            state.app.map_name.clone()
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{map_name}.png"))
            .add_filter("PNG", &["png"])
            .save_file()
        else {
            return;
        };
        jobs::start_png_export(&ui, &s, scale, path);
    });
}
