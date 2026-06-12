//! Project/WAD lifecycle: open, load, save, close.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use editor_core::{EditorMap, Project, import_wad_map, load_map_ron, save_map_ron};
use rbsp::wad_io::find_maps;
use slint::ComponentHandle as _;
use wad::WadData;

use crate::assets::MissingResource;
use crate::generated::{
    CanvasController, EditorWindow, MapsController, ProjectBrowserController, RecentController,
    ResourceEntry, ResourcesController, SectorEditController, WallEditController, WallPreview,
};
use crate::level_editor::LevelEditorState;
use crate::prefs::PopupWindow;
use crate::state::SharedState;
use crate::views::model;
use crate::views::view_canvas::after_edit;
use crate::views::view_panels as panels;
use crate::views::view_window::restore as restore_geom;
use crate::{Options, prefs};

/// Full refresh after any project/WAD change. Browser populate is idempotent.
pub(crate) fn refresh_all(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    panels::set_special_models(ui, shared);
    panels::sync(ui, shared);
    refresh_recent(ui, shared);
    refresh_browser(ui);
    refresh_map_tab_title(ui, shared);
}

/// Force browser repopulate (otherwise only refreshes on tab-show).
pub(crate) fn refresh_browser(ui: &EditorWindow) {
    ui.global::<ProjectBrowserController>().invoke_populate();
}

pub(crate) fn refresh_recent(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let entries: Vec<slint::SharedString> = shared
        .borrow()
        .prefs
        .recent_projects
        .iter()
        .map(slint::SharedString::from)
        .collect();
    ui.global::<RecentController>().set_entries(model(entries));
}

/// Recompute missing-resources and push to panel; call after atlas refresh so the texture index reflects this map's WAD.
pub(crate) fn refresh_resources(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let (entries, required_wads): (Vec<ResourceEntry>, String) = {
        let mut state = shared.borrow_mut();
        state.missing_resources = validate_map_resources(&state);
        let entries = state
            .missing_resources
            .iter()
            .map(|m| ResourceEntry {
                name: m.name.as_str().into(),
                kind: m.kind.label().into(),
            })
            .collect();
        let required = state
            .app
            .map
            .as_ref()
            .map(|m| m.required_wads.join(", "))
            .unwrap_or_default();
        (entries, required)
    };
    let ctl = ui.global::<ResourcesController>();
    ctl.set_count(entries.len() as i32);
    ctl.set_active(!entries.is_empty());
    ctl.set_required_wads(required_wads.into());
    ctl.set_entries(model(entries));
}

pub(crate) fn open_project(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, dir: &Path) {
    let project = match Project::load(dir) {
        Ok(project) => project,
        Err(e) => {
            log::error!("project {}: {e}", dir.display());
            return;
        }
    };
    let last_map = project.settings.last_map.clone();
    adopt_project(shared, project, Some(&dir.display().to_string()));
    if let Some(name) = last_map {
        open_project_map(ui, shared, &name);
    }
}

/// Import a legacy DoomEd `.dpr` (not added to recent list).
pub(crate) fn import_dpr(shared: &Rc<RefCell<SharedState>>, path: &Path) {
    match Project::load_dpr(path) {
        Ok(project) => adopt_project(shared, project, None),
        Err(e) => log::error!("import {}: {e}", path.display()),
    }
}

fn adopt_project(shared: &Rc<RefCell<SharedState>>, project: Project, recent: Option<&str>) {
    let state = &mut *shared.borrow_mut();
    for thing in &project.things {
        let to_byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0) as u8;
        state.app.thing_colors.insert(
            thing.value,
            [
                to_byte(thing.color[0]),
                to_byte(thing.color[1]),
                to_byte(thing.color[2]),
                0xff,
            ],
        );
    }
    if state.iwad.is_none() {
        state.set_iwad(project.settings.iwad.clone());
    }
    state.pwads = project.settings.pwads.clone();
    match project.dir() {
        Some(dir) => log::info!("opened project {}", dir.display()),
        None => log::info!("opened draft project"),
    }
    state.project = Some(project);
    state.texedit.history.clear();
    state.invalidate_wad_caches();
    if let Some(recent) = recent {
        prefs::push_recent_project(&mut state.prefs, recent);
    }
    if let Err(e) = prefs::save_prefs(&state.prefs) {
        log::warn!("saving prefs: {e}");
    }
}

/// Create in-memory draft project from open IWAD if none exists. Requires `ensure_wad`.
fn ensure_draft_project(state: &mut SharedState) {
    if state.project.is_some() {
        return;
    }
    let Some(iwad) = state.iwad.clone() else {
        return;
    };
    let wad = state.wad_data.as_ref().expect("ensure_wad ran");
    state.project = Some(Project::draft(&iwad, wad));
}

fn open_project_map(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, name: &str) {
    let path = {
        let state = shared.borrow();
        let Some(project) = &state.project else {
            return;
        };
        let Some(path) = project.map_ron_path(name) else {
            return;
        };
        path
    };
    match load_map_ron(&path) {
        Ok(map) => {
            load_into(ui, shared, map, name);
        }
        Err(e) => log::error!("open map {}: {e}", path.display()),
    }
}

pub(crate) fn open_from_options(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    options: &Options,
) {
    if options.iwad.is_some() {
        for pwad in &options.pwad {
            if matches!(load_wad_file(shared, pwad), WadLoad::Skipped) {
                log::warn!("ignored --pwad {}", pwad.display());
            }
        }
        open_wad(ui, shared, options.map.as_deref());
        refresh_all(ui, shared);
    }
}

/// Open the WAD: requested/sole map loads directly, else pops map picker; no project → creates a draft (materialised on first Save).
pub(crate) fn open_wad(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    requested: Option<&str>,
) {
    let maps = {
        let state = &mut *shared.borrow_mut();
        if !state.ensure_wad() {
            return;
        }
        ensure_draft_project(state);
        find_maps(state.wad_data.as_ref().expect("ensured above"))
    };
    match requested {
        Some(name) => open_wad_map(ui, shared, name),
        None if maps.len() == 1 => open_wad_map(ui, shared, &maps[0]),
        None => {
            shared.borrow_mut().wad_maps = maps.clone();
            let names: Vec<slint::SharedString> =
                maps.iter().map(slint::SharedString::from).collect();
            let maps_ctl = ui.global::<MapsController>();
            maps_ctl.set_maps(model(names));
            restore_geom(ui, shared, PopupWindow::MapList);
            maps_ctl.set_list_visible(true);
        }
    }
}

/// Outcome of routing a WAD file through [`load_wad_file`].
pub(crate) enum WadLoad {
    /// File became the project IWAD (draft created if needed).
    Iwad,
    /// File registered as a PWAD.
    Pwad,
    /// No change: already loaded or rule rejected it (user notified).
    Skipped,
}

fn notify(title: &str, message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Classify WAD by header: IWAD sets project base (one only); PWAD appends (requires IWAD).
pub(crate) fn load_wad_file(shared: &Rc<RefCell<SharedState>>, path: &Path) -> WadLoad {
    let is_iwad = WadData::file_wad_type(path).as_deref() == Some("IWAD");
    // A timer tic firing during the modal borrows SharedState; never hold it across notify().
    let has_iwad = shared.borrow().iwad.is_some();

    if is_iwad {
        if has_iwad {
            notify(
                "IWAD already loaded",
                "An IWAD is already loaded for this project. Close the project to \
                 use a different IWAD.",
            );
            return WadLoad::Skipped;
        }
        let state = &mut *shared.borrow_mut();
        state.set_iwad(path.to_path_buf());
        if state.project.is_none() {
            state.project = Some(Project::draft(path, &WadData::new(path)));
        }
        return WadLoad::Iwad;
    }

    if !has_iwad {
        notify(
            "Load an IWAD first",
            "A PWAD adds to an IWAD. Open an IWAD before loading PWADs.",
        );
        return WadLoad::Skipped;
    }
    if !shared.borrow_mut().add_pwad(path.to_path_buf()) {
        return WadLoad::Skipped;
    }
    WadLoad::Pwad
}

pub(crate) fn open_wad_map(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, name: &str) {
    let imported = {
        let state = &mut *shared.borrow_mut();
        if !state.ensure_wad() {
            return;
        }
        import_wad_map(state.wad_data.as_ref().expect("ensured above"), name)
    };
    match imported {
        Ok(map) => {
            load_into(ui, shared, map, name);
        }
        Err(e) => log::error!("import {name}: {e}"),
    }
}

/// Replace open map. Returns `false` if unsaved-changes guard cancelled.
pub(crate) fn load_into(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    map: EditorMap,
    name: &str,
) -> bool {
    if !confirm_discard(shared) {
        return false;
    }
    let damage = {
        let mut s = shared.borrow_mut();
        s.reset_map();
        s.app.load_map(map, name)
    };
    close_map_popups(ui);
    ui.global::<CanvasController>()
        .set_wall_preview(WallPreview::default());
    ui.set_map_tab_title(slint::format!("Map - {name}"));
    after_edit(ui, shared, damage);
    // validate after after_edit so atlas is refreshed and texture index is current
    refresh_resources(ui, shared);
    true
}

/// `true` when safe to replace map. Dirty → Save/Discard/Cancel dialog.
pub(crate) fn confirm_discard(shared: &Rc<RefCell<SharedState>>) -> bool {
    if !shared.borrow().app.dirty {
        return true;
    }
    match rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description("The open map has unsaved changes. Save them?")
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show()
    {
        rfd::MessageDialogResult::Yes => save_project(shared),
        rfd::MessageDialogResult::No => true,
        _ => false,
    }
}

/// Save manifest + map RON. Prompts for folder if no native project. Returns `false` on cancel/error.
pub(crate) fn save_project(shared: &Rc<RefCell<SharedState>>) -> bool {
    let needs_folder = {
        let state = shared.borrow();
        if state.app.map.is_none() {
            return false;
        }
        state.project.as_ref().is_none_or(Project::is_draft)
    };
    if needs_folder {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Choose a project folder")
            .pick_folder()
        else {
            return false;
        };
        let mut state = shared.borrow_mut();
        if state.project.is_none() {
            if !state.ensure_wad() {
                log::error!("create project: no IWAD loaded");
                return false;
            }
            ensure_draft_project(&mut state);
        }
        let Some(project) = state.project.as_mut() else {
            log::error!("create project: no IWAD loaded");
            return false;
        };
        if let Err(e) = project.materialise_at(&dir) {
            log::error!("create project {}: {e}", dir.display());
            return false;
        }
        prefs::push_recent_project(&mut state.prefs, &dir.display().to_string());
    }
    write_project_files(&mut shared.borrow_mut())
}

pub(crate) fn refresh_map_tab_title(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let state = shared.borrow();
    let name = state.project_name().unwrap_or(&state.app.map_name);
    ui.set_map_tab_title(slint::format!("Map - {name}"));
}

pub(crate) fn new_project(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !confirm_discard(shared) {
        return;
    }
    let default_iwad = shared.borrow().prefs.iwad.clone();
    let mut iwad_dialog = rfd::FileDialog::new()
        .set_title("Choose the IWAD for the new project")
        .add_filter("WAD", &["wad", "WAD"]);
    if let Some(parent) = default_iwad.parent()
        && !default_iwad.as_os_str().is_empty()
    {
        iwad_dialog = iwad_dialog.set_directory(parent);
    }
    let Some(iwad) = iwad_dialog.pick_file() else {
        return;
    };
    let Some(dir) = rfd::FileDialog::new()
        .set_title("Choose a folder for the new project")
        .pick_folder()
    else {
        return;
    };
    let project = match Project::create(&dir, &iwad, &WadData::new(&iwad)) {
        Ok(project) => project,
        Err(e) => {
            log::error!("create project {}: {e}", dir.display());
            return;
        }
    };
    shared.borrow_mut().set_iwad(iwad);
    adopt_project(shared, project, Some(&dir.display().to_string()));
    refresh_map_tab_title(ui, shared);
}

pub(crate) fn new_map(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, name: &str) {
    load_into(ui, shared, EditorMap::default(), name);
}

/// Close project: reset to cold-start (no project, map, IWAD).
pub(crate) fn close_project(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    if !confirm_discard(shared) {
        return;
    }
    {
        let state = &mut *shared.borrow_mut();
        state.project = None;
        state.pwads.clear();
        state.iwad = None;
        state.invalidate_wad_caches();
        state.app = LevelEditorState::new();
        state.reset_map();
    }
    close_map_popups(ui);
    refresh_all(ui, shared);
}

/// Close the wall/sector edit popups: their drafts index the departing map.
fn close_map_popups(ui: &EditorWindow) {
    ui.global::<WallEditController>()
        .set_wall_edit_visible(false);
    ui.global::<SectorEditController>()
        .set_sector_edit_visible(false);
}

/// Write manifest + map RON. Returns `false` on error (leaves `dirty` set).
fn write_project_files(state: &mut SharedState) -> bool {
    let map_name = state.app.map_name.clone();
    let pwads = state.pwads.clone();
    let required_wads = loaded_wad_basenames(state);
    if let Some(map) = state.app.map.as_mut() {
        map.required_wads = required_wads;
    }
    let Some(project) = state.project.as_mut() else {
        return false;
    };
    if !map_name.is_empty() && !project.maps.contains(&map_name) {
        project.maps.push(map_name.clone());
    }
    if !map_name.is_empty() {
        project.settings.last_map = Some(map_name.clone());
    }
    project.settings.pwads = pwads;
    if let Err(e) = project.save() {
        log::error!("save project: {e}");
        return false;
    }
    if let Some(map) = &state.app.map
        && !map_name.is_empty()
        && let Some(path) = state
            .project
            .as_ref()
            .expect("project set above")
            .map_ron_path(&map_name)
    {
        match save_map_ron(&path, map) {
            Ok(()) => log::info!("saved {}", path.display()),
            Err(e) => {
                log::error!("save map {}: {e}", path.display());
                return false;
            }
        }
    }
    state.app.dirty = false;
    true
}

/// A path's file name as an owned `String`, if valid UTF-8.
pub(crate) fn path_basename(p: &Path) -> Option<String> {
    p.file_name().and_then(|s| s.to_str()).map(str::to_owned)
}

/// IWAD basename + PWAD basenames in load order; stored as `required_wads` on save.
fn loaded_wad_basenames(state: &SharedState) -> Vec<String> {
    state
        .iwad
        .as_deref()
        .and_then(path_basename)
        .into_iter()
        .chain(state.pwads.iter().filter_map(|p| path_basename(p)))
        .collect()
}

/// Resources the map references that the loaded WAD set does not provide.
fn validate_map_resources(state: &SharedState) -> Vec<MissingResource> {
    let (Some(assets), Some(map), Some(wad)) = (
        state.assets.as_ref(),
        state.app.map.as_ref(),
        state.wad_data.as_ref(),
    ) else {
        return Vec::new();
    };
    assets.missing_resources(map, wad)
}

pub(crate) fn save_prefs_now(state: &mut SharedState) {
    state.prefs.grid = state.app.grid;
    state.prefs.snap = state.app.snap;
    state.prefs.snap_to_vertex = state.app.snap_to_vertex;
    state.prefs.snap_to_line = state.app.snap_to_line;
    state.prefs.angle_snap = state.app.angle_snap;
    if let Some(iwad) = &state.iwad {
        state.prefs.iwad = iwad.clone();
    }
    if let Err(e) = prefs::save_prefs(&state.prefs) {
        log::warn!("saving prefs: {e}");
    }
}
