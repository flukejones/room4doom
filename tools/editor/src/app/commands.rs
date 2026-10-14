//! Application commands run outside GPUI entity borrows; the shell applies their effects.

use editor_core::EditorMap;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::app::project::{OpenWad, WadLoad};
use crate::app::session::{Damage, EditorSession};
use crate::app::{jobs, project};
use crate::png_export::PNG_SCALE_PRESETS;

pub(crate) enum AppCommand {
    NewProject,
    CloseProject,
    OpenWad,
    OpenProject,
    ImportProject,
    OpenRecent(String),
    Save,
    ExportWad,
    ExportPng(f32),
    Quit,
    LoadMap { map: Box<EditorMap>, name: String },
    LoadWad(PathBuf),
    ChooseMap(String),
    NewMap(String),
    BuildBsp { interval_ms: i32, keep_all: bool },
    Launch,
}

impl AppCommand {
    pub(crate) fn belongs_to_document(&self) -> bool {
        matches!(
            self,
            Self::LoadMap { .. }
                | Self::LoadWad(_)
                | Self::ChooseMap(_)
                | Self::NewMap(_)
                | Self::BuildBsp { .. }
                | Self::Launch
        )
    }
}

pub(crate) struct AppEffect {
    pub(crate) damage: Damage,
    pub(crate) choose_map: bool,
    pub(crate) quit: bool,
    pub(crate) status: Option<String>,
}

impl Default for AppEffect {
    fn default() -> Self {
        Self {
            damage: Damage::None,
            choose_map: false,
            quit: false,
            status: None,
        }
    }
}

pub(crate) fn run(shared: &Rc<RefCell<EditorSession>>, command: AppCommand) -> AppEffect {
    match command {
        AppCommand::NewProject => {
            project::new_project(shared);
            changed(Damage::Restyle)
        }
        AppCommand::CloseProject => {
            project::close_project(shared);
            changed(Damage::Restyle)
        }
        AppCommand::OpenWad => open_wad(shared),
        AppCommand::OpenProject => choose_project(shared),
        AppCommand::ImportProject => import_project(shared),
        AppCommand::OpenRecent(path) => {
            if !project::confirm_discard(shared) {
                return AppEffect::default();
            }
            project::open_project(shared, Path::new(&path));
            changed(Damage::Restyle)
        }
        AppCommand::Save => {
            project::save_project(shared);
            AppEffect::default()
        }
        AppCommand::ExportWad => export_wad(shared),
        AppCommand::ExportPng(scale) => export_png(shared, scale),
        AppCommand::Quit => {
            if !project::confirm_discard(shared) {
                return AppEffect::default();
            }
            project::save_prefs_now(&mut shared.borrow_mut());
            AppEffect {
                quit: true,
                ..AppEffect::default()
            }
        }
        AppCommand::LoadMap {
            map,
            name,
        } => {
            if project::load_into(shared, *map, &name) {
                changed(Damage::Edited)
            } else {
                AppEffect::default()
            }
        }
        AppCommand::LoadWad(path) => {
            if !project::confirm_discard(shared) {
                return AppEffect::default();
            }
            if matches!(project::load_wad_file(shared, &path), WadLoad::Skipped) {
                AppEffect::default()
            } else {
                changed(Damage::Restyle)
            }
        }
        AppCommand::ChooseMap(name) => {
            if project::open_wad_map(shared, &name) {
                changed(Damage::Edited)
            } else {
                AppEffect::default()
            }
        }
        AppCommand::NewMap(name) => {
            if !name.is_empty() && project::new_map(shared, &name) {
                changed(Damage::Edited)
            } else {
                AppEffect::default()
            }
        }
        AppCommand::BuildBsp {
            interval_ms,
            keep_all,
        } => {
            {
                let mut state = shared.borrow_mut();
                state.anim_interval_ms = u64::try_from(interval_ms).unwrap_or(1);
                state.anim_keep_all = keep_all;
            }
            status(jobs::start_build(shared), "building BSP…")
        }
        AppCommand::Launch => status(jobs::start_launch(shared), "launching…"),
    }
}

fn choose_project(shared: &Rc<RefCell<EditorSession>>) -> AppEffect {
    let identity = shared.borrow().identity();
    if !project::confirm_discard(shared) {
        return AppEffect::default();
    }
    if let Some(dir) = rfd::FileDialog::new()
        .set_title("Open project folder")
        .pick_folder()
    {
        if shared.borrow().identity() != identity {
            return AppEffect::default();
        }
        project::open_project(shared, &dir);
        return changed(Damage::Restyle);
    }
    AppEffect::default()
}

fn import_project(shared: &Rc<RefCell<EditorSession>>) -> AppEffect {
    let identity = shared.borrow().identity();
    if !project::confirm_discard(shared) {
        return AppEffect::default();
    }
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("DoomEd project", &["dpr"])
        .pick_file()
    {
        if shared.borrow().identity() != identity {
            return AppEffect::default();
        }
        project::import_dpr(shared, &path);
        return changed(Damage::Restyle);
    }
    AppEffect::default()
}

fn changed(damage: Damage) -> AppEffect {
    AppEffect {
        damage,
        ..AppEffect::default()
    }
}
fn status(started: bool, status: &str) -> AppEffect {
    AppEffect {
        status: started.then(|| status.to_owned()),
        ..AppEffect::default()
    }
}

fn open_wad(shared: &Rc<RefCell<EditorSession>>) -> AppEffect {
    let identity = shared.borrow().identity();
    if shared.borrow().iwad.is_none() {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("WAD", &["wad", "WAD"])
            .pick_file()
        else {
            return AppEffect::default();
        };
        if shared.borrow().identity() != identity {
            return AppEffect::default();
        }
        if !matches!(project::load_wad_file(shared, &path), WadLoad::Iwad) {
            return AppEffect::default();
        }
    }
    match project::open_wad(shared, None) {
        OpenWad::None => AppEffect::default(),
        OpenWad::MapLoaded => changed(Damage::Edited),
        OpenWad::ChooseMap => AppEffect {
            choose_map: true,
            ..AppEffect::default()
        },
    }
}

fn export_wad(shared: &Rc<RefCell<EditorSession>>) -> AppEffect {
    let identity = shared.borrow().identity();
    if shared.borrow().job_busy {
        log::warn!("export already running");
        return AppEffect::default();
    }
    let default_name = format!("{}.wad", shared.borrow().map_editor.map_name);
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(default_name)
        .add_filter("WAD", &["wad"])
        .save_file()
    else {
        return AppEffect::default();
    };
    if shared.borrow().identity() != identity {
        return AppEffect::default();
    }
    status(jobs::start_export(shared, path), "exporting…")
}

fn export_png(shared: &Rc<RefCell<EditorSession>>, scale: f32) -> AppEffect {
    let identity = shared.borrow().identity();
    let scale = PNG_SCALE_PRESETS
        .iter()
        .copied()
        .find(|p| (p - scale).abs() < f32::EPSILON)
        .unwrap_or(1.0);
    let map_name = {
        let state = shared.borrow();
        if state.map_editor.map.is_none() {
            return AppEffect::default();
        }
        state.map_editor.map_name.clone()
    };
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{map_name}.png"))
        .add_filter("PNG", &["png"])
        .save_file()
    else {
        return AppEffect::default();
    };
    if shared.borrow().identity() != identity {
        return AppEffect::default();
    }
    status(
        jobs::start_png_export(shared, scale, path),
        "exporting png…",
    )
}
