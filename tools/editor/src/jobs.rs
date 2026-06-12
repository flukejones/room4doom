//! Off-thread worker jobs: WAD export, BSP build (animation-only), play-test launch. Pattern: `start_*` (UI) → `run_*` (worker) → `finish_*` (UI). BSP must not run on the Slint event loop — worker posts [`JobOutcome`] over a channel then pings `ExportController.job-done` via `upgrade_in_event_loop`; channel bridges the `Send` closure ↔ `Rc`-owning UI thread.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::rc::Rc;
use std::thread;

use editor_core::wad_export::{export_map_pwad_with_lumps, export_map_pwad_with_lumps_traced};
use editor_core::{AnimDef, EditorMap, ExportOptions, Name8, TextureDef};
use slint::ComponentHandle as _;
use wad::Lump;
use wad::boom::{AnimatedEntry, encode_animated};

use crate::generated::{EditorWindow, ExportController};
use crate::launch::{self, LaunchPlan};
use crate::prefs::EditorPreferences;
use crate::render::view::snap;
use crate::{SharedState, bsp_anim, png_export, prefs};

pub enum JobOutcome {
    Export {
        path: PathBuf,
        /// BSP trace events (for animation) or failure message.
        result: Result<Vec<rbsp::BuildEvent>, String>,
    },
    /// BSP build with no file written; events only.
    Build {
        result: Result<Vec<rbsp::BuildEvent>, String>,
    },
    Launch {
        result: Result<Child, String>,
    },
    /// PNG encode + write of an already-rendered map image.
    PngExport {
        path: PathBuf,
        result: Result<(), String>,
    },
}

// ── Dispatch ────────────────────────────────────────────────────────────────

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ExportController>().on_job_done(move || {
        let Some(ui) = weak.upgrade() else { return };
        drain(&ui, &s);
    });
}

fn drain(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    loop {
        let outcome = {
            let state = shared.borrow();
            state.job_rx.try_recv()
        };
        let Ok(outcome) = outcome else { break };
        match outcome {
            JobOutcome::Export {
                path,
                result,
            } => finish_export(ui, shared, path, result),
            JobOutcome::Build {
                result,
            } => finish_build(ui, shared, result),
            JobOutcome::Launch {
                result,
            } => finish_launch(ui, shared, result),
            JobOutcome::PngExport {
                path,
                result,
            } => finish_png_export(ui, shared, path, result),
        }
    }
}

// ── Export ──────────────────────────────────────────────────────────────────

pub fn start_export(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, path: PathBuf) {
    let weak = ui.as_weak();
    let (bytes_in, map_name, opts, extra, traced, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("export already running");
            return;
        }
        let Some(map) = &state.app.map else { return };
        let snapshot = match bincode::serialize(map) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("snapshot map for export: {e}");
                return;
            }
        };
        let opts = ExportOptions {
            nodes: state.effective_nodes_format(),
            ..ExportOptions::default()
        };
        let extra = project_lumps(state);
        let traced = state.prefs.bsp_anim != prefs::BspAnimPref::Off;
        state.job_busy = true;
        (
            snapshot,
            state.app.map_name.clone(),
            opts,
            extra,
            traced,
            state.job_tx.clone(),
        )
    };
    set_status(ui, true, "exporting…");

    thread::spawn(move || {
        let outcome = run_export(&bytes_in, &map_name, &opts, extra, traced, &path);
        if tx
            .send(JobOutcome::Export {
                path,
                result: outcome,
            })
            .is_err()
        {
            log::warn!("export result dropped: UI receiver gone");
        }
        if let Err(e) = weak.upgrade_in_event_loop(|ui| {
            ui.global::<ExportController>().invoke_job_done();
        }) {
            log::warn!("export done ping dropped: {e}");
        }
    });
}

fn run_export(
    snapshot: &[u8],
    map_name: &str,
    opts: &ExportOptions,
    extra: Vec<Lump>,
    traced: bool,
    path: &PathBuf,
) -> Result<Vec<rbsp::BuildEvent>, String> {
    let map: EditorMap = bincode::deserialize(snapshot).map_err(|e| e.to_string())?;
    let (bytes, events) = if traced {
        export_map_pwad_with_lumps_traced(&map, map_name, opts, extra).map_err(|e| e.to_string())?
    } else {
        let bytes =
            export_map_pwad_with_lumps(&map, map_name, opts, extra).map_err(|e| e.to_string())?;
        (bytes, Vec::new())
    };
    // Temp + rename: a crash mid-write must not destroy an existing WAD.
    let tmp = path.with_extension("wad.tmp");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(events)
}

fn finish_export(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    path: PathBuf,
    result: Result<Vec<rbsp::BuildEvent>, String>,
) {
    let events = {
        let state = &mut *shared.borrow_mut();
        state.job_busy = false;
        result
    };
    match events {
        Ok(events) => {
            log::info!("exported {}", path.display());
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            set_status(ui, false, &format!("exported {name}"));
            play_build_anim(ui, shared, events);
        }
        Err(e) => {
            log::error!("export: {e}");
            set_status(ui, false, "export failed");
        }
    }
}

// ── PNG export ──────────────────────────────────────────────────────────────

/// GPU render on the event loop (the context lives there); encode + write on a worker.
pub fn start_png_export(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    scale: f32,
    path: PathBuf,
) {
    let weak = ui.as_weak();
    let (rgba, width, height, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("a job is already running");
            return;
        }
        let (rgba, width, height) = match png_export::render_map_rgba(state, scale) {
            Ok(image) => image,
            Err(e) => {
                log::error!("png export: {e}");
                return;
            }
        };
        state.job_busy = true;
        (rgba, width, height, state.job_tx.clone())
    };
    set_status(ui, true, "exporting png…");

    thread::spawn(move || {
        let outcome = png_export::write_png(&path, width, height, &rgba).map_err(|e| e.to_string());
        if tx
            .send(JobOutcome::PngExport {
                path,
                result: outcome,
            })
            .is_err()
        {
            log::warn!("png export result dropped: UI receiver gone");
        }
        if let Err(e) = weak.upgrade_in_event_loop(|ui| {
            ui.global::<ExportController>().invoke_job_done();
        }) {
            log::warn!("png export done ping dropped: {e}");
        }
    });
}

fn finish_png_export(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    path: PathBuf,
    result: Result<(), String>,
) {
    shared.borrow_mut().job_busy = false;
    match result {
        Ok(()) => {
            log::info!("wrote {}", path.display());
            set_status(ui, false, "png exported");
        }
        Err(e) => {
            log::error!("png export: {e}");
            set_status(ui, false, "png export failed");
        }
    }
}

// ── Build (animation only, no file) ─────────────────────────────────────────

pub fn start_build(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let (bytes_in, map_name, opts, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("a job is already running");
            return;
        }
        let Some(map) = &state.app.map else { return };
        let snapshot = match bincode::serialize(map) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("snapshot map for build: {e}");
                return;
            }
        };
        let opts = ExportOptions {
            nodes: state.effective_nodes_format(),
            ..ExportOptions::default()
        };
        state.job_busy = true;
        (
            snapshot,
            state.app.map_name.clone(),
            opts,
            state.job_tx.clone(),
        )
    };
    set_status(ui, true, "building BSP…");

    thread::spawn(move || {
        let result = run_build(&bytes_in, &map_name, &opts);
        if tx
            .send(JobOutcome::Build {
                result,
            })
            .is_err()
        {
            log::warn!("build result dropped: UI receiver gone");
        }
        if let Err(e) = weak.upgrade_in_event_loop(|ui| {
            ui.global::<ExportController>().invoke_job_done();
        }) {
            log::warn!("build done ping dropped: {e}");
        }
    });
}

fn run_build(
    snapshot: &[u8],
    map_name: &str,
    opts: &ExportOptions,
) -> Result<Vec<rbsp::BuildEvent>, String> {
    let map: EditorMap = bincode::deserialize(snapshot).map_err(|e| e.to_string())?;
    let (_bytes, events) = export_map_pwad_with_lumps_traced(&map, map_name, opts, Vec::new())
        .map_err(|e| e.to_string())?;
    Ok(events)
}

fn finish_build(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    result: Result<Vec<rbsp::BuildEvent>, String>,
) {
    let events = {
        let state = &mut *shared.borrow_mut();
        state.job_busy = false;
        result
    };
    match events {
        Ok(events) => {
            set_status(ui, false, "BSP built");
            play_build_anim(ui, shared, events);
        }
        Err(e) => {
            log::error!("build: {e}");
            set_status(ui, false, "build failed");
        }
    }
}

// ── Launch (play-test) ──────────────────────────────────────────────────────

pub fn start_launch(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let (snapshot, plan, prefs, iwad, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("a job is already running");
            return;
        }
        if let Some(child) = &mut state.launched
            && child.try_wait().ok().flatten().is_none()
        {
            log::warn!("previous launch still running");
            return;
        }
        let Some(iwad) = state.iwad.clone() else {
            log::warn!("no IWAD known - cannot launch");
            return;
        };
        let Some(map) = &state.app.map else { return };
        let click = state.app.cursor_world;
        let snapped = [
            snap(click[0], state.app.grid) as i32,
            snap(click[1], state.app.grid) as i32,
        ];
        let launch_type = state.effective_launch_type();
        let nodes = state.effective_nodes_format();
        let plan = match launch::plan_launch(map, &state.app.map_name, launch_type, nodes, snapped)
        {
            Ok(plan) => plan,
            Err(e) => {
                log::error!("launch: {e}");
                return;
            }
        };
        let snapshot = match bincode::serialize(map) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("snapshot map for launch: {e}");
                return;
            }
        };
        state.job_busy = true;
        (
            snapshot,
            plan,
            state.prefs.clone(),
            iwad,
            state.job_tx.clone(),
        )
    };
    set_status(ui, true, "launching…");

    thread::spawn(move || {
        let result = run_launch(&snapshot, &plan, &prefs, &iwad);
        if tx
            .send(JobOutcome::Launch {
                result,
            })
            .is_err()
        {
            log::warn!("launch result dropped: UI receiver gone");
        }
        if let Err(e) = weak.upgrade_in_event_loop(|ui| {
            ui.global::<ExportController>().invoke_job_done();
        }) {
            log::warn!("launch done ping dropped: {e}");
        }
    });
}

fn run_launch(
    snapshot: &[u8],
    plan: &LaunchPlan,
    prefs: &EditorPreferences,
    iwad: &Path,
) -> Result<Child, String> {
    let mut map: EditorMap = bincode::deserialize(snapshot).map_err(|e| e.to_string())?;
    let wad_path = launch::export_launch_wad(&mut map, plan).map_err(|e| e.to_string())?;
    launch::spawn_engine(prefs, iwad, &wad_path, plan.slot).map_err(|e| e.to_string())
}

fn finish_launch(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    result: Result<Child, String>,
) {
    match result {
        Ok(child) => {
            let pid = child.id();
            {
                let state = &mut *shared.borrow_mut();
                state.job_busy = false;
                state.launched = Some(child);
            }
            log::info!("launched (pid {pid})");
            set_status(ui, false, "launched");
        }
        Err(e) => {
            shared.borrow_mut().job_busy = false;
            log::error!("launch: {e}");
            set_status(ui, false, "launch failed");
        }
    }
}

// ── Shared helpers ──────────────────────────────────────────────────────────

fn play_build_anim(
    ui: &EditorWindow,
    shared: &Rc<RefCell<SharedState>>,
    events: Vec<rbsp::BuildEvent>,
) {
    let (mode, interval_ms, keep_all) = {
        let state = shared.borrow();
        (
            state.prefs.bsp_anim,
            state.anim_interval_ms,
            state.anim_keep_all,
        )
    };
    bsp_anim::start(ui, shared, events, mode, interval_ms, keep_all);
}

/// Lumps after the map: imported patches, TEXTURE1/2+PNAMES (if edited), ANIMATED (if defined).
fn project_lumps(state: &SharedState) -> Vec<Lump> {
    let Some(project) = &state.project else {
        return Vec::new();
    };
    let mut lumps = Vec::new();
    for patch in &project.imported_patches {
        lumps.push(Lump {
            name: patch.name.as_str().to_owned(),
            data: patch.lump.clone(),
        });
    }
    let edited: Vec<Vec<TextureDef>> = project
        .textures
        .iter()
        .filter(|g| g.edited && !g.defs.is_empty())
        .map(|g| g.defs.clone())
        .collect();
    if !edited.is_empty() {
        let extra: Vec<Name8> = project.imported_patches.iter().map(|p| p.name).collect();
        match editor_core::encode_texture_lumps(&edited, &extra) {
            Ok((texture_lumps, pnames)) => {
                lumps.extend(texture_lumps);
                lumps.push(pnames);
            }
            Err(e) => log::error!("texture lumps: {e}"),
        }
    }
    if !project.animations.is_empty() {
        lumps.push(animated_lump(&project.animations));
    }
    lumps
}

fn animated_lump(animations: &[AnimDef]) -> Lump {
    let entries: Vec<AnimatedEntry> = animations
        .iter()
        .map(|a| AnimatedEntry {
            is_texture: a.is_texture,
            end_name: a.end.as_str().to_owned(),
            start_name: a.start.as_str().to_owned(),
            speed: a.speed.max(0) as u32,
        })
        .collect();
    Lump {
        name: "ANIMATED".to_owned(),
        data: encode_animated(&entries),
    }
}

fn set_status(ui: &EditorWindow, busy: bool, text: &str) {
    let ctl = ui.global::<ExportController>();
    ctl.set_busy(busy);
    ctl.set_status(text.into());
}
