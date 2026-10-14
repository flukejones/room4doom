//! Project/WAD lifecycle: open, load, save, close.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use editor_core::{EditorMap, Project, import_wad_map, load_map_ron, save_map_ron};
use rbsp::wad_io::find_maps;
use wad::WadData;

use crate::Options;
use crate::app::preferences as prefs;
use crate::app::session::EditorSession;
use crate::level_editor::LevelEditorState;

/// Refresh unresolved resource data after the working set changes.
pub(crate) fn refresh_resources(state: &mut EditorSession) {
    state.refresh_missing_resources();
}

#[derive(Clone, Copy, Default)]
pub(crate) enum OpenWad {
    #[default]
    None,
    MapLoaded,
    ChooseMap,
}

pub(crate) fn open_project(shared: &Rc<RefCell<EditorSession>>, dir: &Path) {
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
        open_project_map(shared, &name);
    }
}

/// Import a legacy DoomEd `.dpr` (not added to recent list).
pub(crate) fn import_dpr(shared: &Rc<RefCell<EditorSession>>, path: &Path) {
    match Project::load_dpr(path) {
        Ok(project) => adopt_project(shared, project, None),
        Err(e) => log::error!("import {}: {e}", path.display()),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
fn adopt_project(shared: &Rc<RefCell<EditorSession>>, project: Project, recent: Option<&str>) {
    let state = &mut *shared.borrow_mut();
    for thing in &project.things {
        let to_byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0) as u8;
        state.map_editor.thing_colors.insert(
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
    state.map_editor.clear_map();
    state.reset_map();
    state.invalidate_wad_caches();
    if let Some(recent) = recent {
        prefs::push_recent_project(&mut state.prefs, recent);
    }
    if let Err(e) = prefs::save_prefs(&state.prefs) {
        log::warn!("saving prefs: {e}");
    }
}

/// Create in-memory draft project from open IWAD if none exists. Requires `ensure_wad`.
fn ensure_draft_project(state: &mut EditorSession) {
    if state.project.is_some() {
        return;
    }
    let Some(iwad) = state.iwad.clone() else {
        return;
    };
    let wad = state.wad_data.as_ref().expect("ensure_wad ran");
    state.project = Some(Project::draft(&iwad, wad));
}

fn open_project_map(shared: &Rc<RefCell<EditorSession>>, name: &str) {
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
            load_into(shared, map, name);
        }
        Err(e) => log::error!("open map {}: {e}", path.display()),
    }
}

pub(crate) fn open_from_options(shared: &Rc<RefCell<EditorSession>>, options: &Options) -> OpenWad {
    if options.iwad.is_none() {
        return OpenWad::None;
    }
    for pwad in &options.pwad {
        if matches!(load_wad_file(shared, pwad), WadLoad::Skipped) {
            log::warn!("ignored --pwad {}", pwad.display());
        }
    }
    open_wad(shared, options.map.as_deref())
}

/// Load a requested map, or return a request for the native map chooser.
pub(crate) fn open_wad(shared: &Rc<RefCell<EditorSession>>, requested: Option<&str>) -> OpenWad {
    let maps = {
        let state = &mut *shared.borrow_mut();
        if !state.ensure_wad() {
            return OpenWad::None;
        }
        ensure_draft_project(state);
        find_maps(state.wad_data.as_ref().expect("ensured above"))
    };
    match requested.or_else(|| (maps.len() == 1).then(|| maps[0].as_str())) {
        Some(name) if open_wad_map(shared, name) => OpenWad::MapLoaded,
        Some(_) => OpenWad::None,
        None => {
            shared.borrow_mut().wad_maps = maps;
            OpenWad::ChooseMap
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
pub(crate) fn load_wad_file(shared: &Rc<RefCell<EditorSession>>, path: &Path) -> WadLoad {
    let is_iwad = WadData::file_wad_type(path).as_deref() == Some("IWAD");
    // A timer tic firing during the modal borrows EditorSession; never hold it across notify().
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

pub(crate) fn open_wad_map(shared: &Rc<RefCell<EditorSession>>, name: &str) -> bool {
    let imported = {
        let state = &mut *shared.borrow_mut();
        if !state.ensure_wad() {
            return false;
        }
        import_wad_map(state.wad_data.as_ref().expect("ensured above"), name)
    };
    match imported {
        Ok(map) => load_into(shared, map, name),
        Err(e) => {
            log::error!("import {name}: {e}");
            false
        }
    }
}

/// Replace open map. Returns `false` if unsaved-changes guard cancelled.
pub(crate) fn load_into(shared: &Rc<RefCell<EditorSession>>, map: EditorMap, name: &str) -> bool {
    let map_dirty = shared.borrow().map_editor.dirty;
    if map_dirty && !prompt_discard(shared) {
        return false;
    }
    {
        let mut state = shared.borrow_mut();
        state.reset_map();
        state.map_editor.load_map(map, name);
        refresh_resources(&mut state);
    }
    true
}

/// `true` when safe to replace the document or resources. Prompts for unsaved map or asset edits.
pub(crate) fn confirm_discard(shared: &Rc<RefCell<EditorSession>>) -> bool {
    let dirty = {
        let state = shared.borrow();
        state.map_editor.dirty || state.assets_dirty
    };
    if !dirty {
        return true;
    }
    prompt_discard(shared)
}

fn prompt_discard(shared: &Rc<RefCell<EditorSession>>) -> bool {
    let identity = shared.borrow().identity();
    let choice = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description("The project has unsaved changes. Save them?")
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show();
    if shared.borrow().identity() != identity {
        return false;
    }
    match choice {
        rfd::MessageDialogResult::Yes => save_project(shared),
        rfd::MessageDialogResult::No => true,
        _ => false,
    }
}

/// Save working assets and the optional map. Prompts for a folder for an unsaved project.
pub(crate) fn save_project(shared: &Rc<RefCell<EditorSession>>) -> bool {
    let identity = shared.borrow().identity();
    let needs_folder = {
        let mut state = shared.borrow_mut();
        if state.project.is_none() {
            if !state.ensure_wad() {
                return false;
            }
            ensure_draft_project(&mut state);
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
        if shared.borrow().identity() != identity {
            return false;
        }
        let mut state = shared.borrow_mut();
        sync_project_assets(&mut state);
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

pub(crate) fn new_project(shared: &Rc<RefCell<EditorSession>>) {
    if !confirm_discard(shared) {
        return;
    }
    let identity = shared.borrow().identity();
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
    if shared.borrow().identity() != identity {
        return;
    }
    let Some(dir) = rfd::FileDialog::new()
        .set_title("Choose a folder for the new project")
        .pick_folder()
    else {
        return;
    };
    if shared.borrow().identity() != identity {
        return;
    }
    let project = match Project::create(&dir, &iwad, &WadData::new(&iwad)) {
        Ok(project) => project,
        Err(e) => {
            log::error!("create project {}: {e}", dir.display());
            return;
        }
    };
    shared.borrow_mut().set_iwad(iwad);
    adopt_project(shared, project, Some(&dir.display().to_string()));
}

pub(crate) fn new_map(shared: &Rc<RefCell<EditorSession>>, name: &str) -> bool {
    load_into(shared, EditorMap::default(), name)
}

/// Close project: reset to cold-start (no project, map, IWAD).
pub(crate) fn close_project(shared: &Rc<RefCell<EditorSession>>) {
    if !confirm_discard(shared) {
        return;
    }
    {
        let state = &mut *shared.borrow_mut();
        state.project = None;
        state.pwads.clear();
        state.iwad = None;
        state.invalidate_wad_caches();
        state.map_editor = LevelEditorState::new();
        state.reset_map();
    }
}

pub(crate) fn sync_project_assets(state: &mut EditorSession) {
    let EditorSession {
        assets,
        project,
        ..
    } = state;
    if let (Some(assets), Some(project)) = (assets, project) {
        assets.write_into(project);
    }
}

/// Write working assets and the optional map. Failed writes leave `dirty` set.
fn write_project_files(state: &mut EditorSession) -> bool {
    sync_project_assets(state);
    let map_name = state.map_editor.map_name.clone();
    let pwads = state.pwads.clone();
    let required_wads = loaded_wad_basenames(state);
    if let Some(map) = state.map_editor.map.as_mut() {
        map.required_wads = required_wads;
    }
    let Some(project) = state.project.as_mut() else {
        return false;
    };
    if state.map_editor.map.is_some() && !map_name.is_empty() && !project.maps.contains(&map_name) {
        project.maps.push(map_name.clone());
    }
    if state.map_editor.map.is_some() && !map_name.is_empty() {
        project.settings.last_map = Some(map_name.clone());
    }
    project.settings.pwads = pwads;
    if let Err(e) = project.save() {
        log::error!("save project: {e}");
        return false;
    }
    if let Some(map) = &state.map_editor.map
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
    state.map_editor.dirty = false;
    state.assets_dirty = false;
    true
}

/// A path's file name as an owned `String`, if valid UTF-8.
pub(crate) fn path_basename(p: &Path) -> Option<String> {
    p.file_name().and_then(|s| s.to_str()).map(str::to_owned)
}

/// IWAD basename + PWAD basenames in load order; stored as `required_wads` on save.
fn loaded_wad_basenames(state: &EditorSession) -> Vec<String> {
    state
        .iwad
        .as_deref()
        .and_then(path_basename)
        .into_iter()
        .chain(state.pwads.iter().filter_map(|p| path_basename(p)))
        .collect()
}

pub(crate) fn save_prefs_now(state: &mut EditorSession) {
    state.prefs.grid = state.map_editor.grid;
    state.prefs.snap = state.map_editor.snap;
    state.prefs.snap_to_vertex = state.map_editor.snap_to_vertex;
    state.prefs.snap_to_line = state.map_editor.snap_to_line;
    state.prefs.angle_snap = state.map_editor.angle_snap;
    if let Some(iwad) = &state.iwad {
        state.prefs.iwad = iwad.clone();
    }
    if let Err(e) = prefs::save_prefs(&state.prefs) {
        log::warn!("saving prefs: {e}");
    }
}

#[cfg(test)]
mod tests {
    use editor_core::{AnimDef, ImportedPatch, Name8};

    use super::*;
    use crate::{app::preferences::EditorPreferences, assets::encode_patch};

    #[test]
    fn save_assets_without_open_map_round_trips_working_edits() {
        let iwad = test_utils::doom1_wad_path();
        let dir = std::env::temp_dir().join(format!(
            "room4doom-editor-asset-save-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let mut state = EditorSession::new(
            LevelEditorState::new(),
            Some(iwad.clone()),
            EditorPreferences::default(),
        );
        assert!(state.ensure_assets());
        let mut project = Project::draft(&iwad, state.wad_data.as_ref().unwrap());
        project.dir = Some(dir.clone());
        state.project = Some(project);
        let assets = state.assets.as_mut().unwrap();
        let texture = assets.textures()[0].name;
        assets.texture_mut(0).unwrap().width = 73;
        let animation = AnimDef {
            is_texture: true,
            start: texture,
            end: texture,
            speed: 13,
        };
        let mut animations = assets.animations().to_vec();
        animations.push(animation);
        assets.replace_animations(animations);
        let patch_name = Name8::new("GPTEST").unwrap();
        let patch_lump = encode_patch(1, 1, &[27]);
        assets
            .import_patch(ImportedPatch {
                name: patch_name,
                lump: patch_lump.clone(),
            })
            .unwrap();
        let shared = Rc::new(RefCell::new(state));

        assert!(save_project(&shared));
        let saved = Project::load(&dir).unwrap();
        assert!(saved.maps.is_empty());
        let saved_texture = saved
            .textures
            .iter()
            .flat_map(|group| &group.defs)
            .find(|def| def.name == texture)
            .unwrap();
        assert_eq!(saved_texture.width, 73);
        assert!(saved.animations.contains(&animation));
        assert_eq!(
            saved
                .imported_patches
                .iter()
                .find(|patch| patch.name == patch_name)
                .unwrap()
                .lump,
            patch_lump
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
