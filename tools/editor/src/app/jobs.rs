//! Background export, BSP and launch jobs; GPUI awaits outcomes on the UI thread.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::rc::Rc;
use std::thread;

use editor_core::wad_export::{export_map_pwad_with_lumps, export_map_pwad_with_lumps_traced};
use editor_core::{AnimDef, EditorMap, ExportOptions, Name8, TextureDef};
use wad::Lump;
use wad::boom::{AnimatedEntry, encode_animated};

use crate::app::preferences as prefs;
use crate::app::preferences::EditorPreferences;
use crate::app::project;
use crate::launch::{self, LaunchPlan};
use crate::render::view::snap;
use crate::{EditorSession, png_export};

pub enum JobOutcome {
    Export {
        document: u64,
        path: PathBuf,
        /// BSP trace events (for animation) or failure message.
        result: Result<Vec<rbsp::BuildEvent>, String>,
    },
    /// BSP build with no file written; events only.
    Build {
        document: u64,
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

pub(crate) struct JobEffect {
    pub(crate) status: String,
    pub(crate) build_events: Option<Vec<rbsp::BuildEvent>>,
}

pub(crate) fn finish(shared: &Rc<RefCell<EditorSession>>, outcome: JobOutcome) -> JobEffect {
    shared.borrow_mut().job_busy = false;
    let (document, result, success, failure) = match outcome {
        JobOutcome::Export {
            document,
            path,
            result,
        } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (
                document,
                result,
                format!("exported {name}"),
                "export failed",
            )
        }
        JobOutcome::Build {
            document,
            result,
        } => (document, result, "BSP built".into(), "build failed"),
        JobOutcome::Launch {
            result,
        } => {
            return match result {
                Ok(child) => {
                    log::info!("launched (pid {})", child.id());
                    shared.borrow_mut().launched = Some(child);
                    JobEffect {
                        status: "launched".into(),
                        build_events: None,
                    }
                }
                Err(error) => {
                    log::error!("launch: {error}");
                    JobEffect {
                        status: "launch failed".into(),
                        build_events: None,
                    }
                }
            };
        }
        JobOutcome::PngExport {
            path,
            result,
        } => {
            return match result {
                Ok(()) => {
                    log::info!("wrote {}", path.display());
                    JobEffect {
                        status: "png exported".into(),
                        build_events: None,
                    }
                }
                Err(error) => {
                    log::error!("PNG export: {error}");
                    JobEffect {
                        status: "png export failed".into(),
                        build_events: None,
                    }
                }
            };
        }
    };
    match result {
        Ok(events) => JobEffect {
            status: success,
            build_events: (shared.borrow().document_generation == document).then_some(events),
        },
        Err(error) => {
            log::error!("{failure}: {error}");
            JobEffect {
                status: failure.into(),
                build_events: None,
            }
        }
    }
}

// ── Export ──────────────────────────────────────────────────────────────────

pub fn start_export(shared: &Rc<RefCell<EditorSession>>, path: PathBuf) -> bool {
    let (bytes_in, map_name, opts, extra, traced, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("export already running");
            return false;
        }
        let Some(map) = &state.map_editor.map else {
            return false;
        };
        let snapshot = match bincode::serialize(map) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("snapshot map for export: {e}");
                return false;
            }
        };
        let opts = ExportOptions {
            nodes: state.effective_nodes_format(),
            ..ExportOptions::default()
        };
        project::sync_project_assets(state);
        let extra = project_lumps(state);
        let traced = state.prefs.bsp_anim != prefs::BspAnimPref::Off;
        state.job_busy = true;
        (
            snapshot,
            state.map_editor.map_name.clone(),
            opts,
            extra,
            traced,
            state.job_tx.clone(),
        )
    };

    let document = shared.borrow().document_generation;
    thread::spawn(move || {
        let outcome = run_export(&bytes_in, &map_name, &opts, extra, traced, &path);
        if tx
            .send_blocking(JobOutcome::Export {
                document,
                path,
                result: outcome,
            })
            .is_err()
        {
            log::warn!("export result dropped: UI receiver gone");
        }
    });
    true
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

// ── PNG export ──────────────────────────────────────────────────────────────

/// GPU render on the event loop (the context lives there); encode + write on a worker.
pub fn start_png_export(shared: &Rc<RefCell<EditorSession>>, scale: f32, path: PathBuf) -> bool {
    let (rgba, width, height, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("a job is already running");
            return false;
        }
        let (rgba, width, height) = match png_export::render_map_rgba(state, scale) {
            Ok(image) => image,
            Err(e) => {
                log::error!("png export: {e}");
                return false;
            }
        };
        state.job_busy = true;
        (rgba, width, height, state.job_tx.clone())
    };

    thread::spawn(move || {
        let outcome = png_export::write_png(&path, width, height, &rgba).map_err(|e| e.to_string());
        if tx
            .send_blocking(JobOutcome::PngExport {
                path,
                result: outcome,
            })
            .is_err()
        {
            log::warn!("png export result dropped: UI receiver gone");
        }
    });
    true
}

// ── Build (animation only, no file) ─────────────────────────────────────────

pub fn start_build(shared: &Rc<RefCell<EditorSession>>) -> bool {
    let (bytes_in, map_name, opts, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("a job is already running");
            return false;
        }
        let Some(map) = &state.map_editor.map else {
            return false;
        };
        let snapshot = match bincode::serialize(map) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("snapshot map for build: {e}");
                return false;
            }
        };
        let opts = ExportOptions {
            nodes: state.effective_nodes_format(),
            ..ExportOptions::default()
        };
        state.job_busy = true;
        (
            snapshot,
            state.map_editor.map_name.clone(),
            opts,
            state.job_tx.clone(),
        )
    };

    let document = shared.borrow().document_generation;
    thread::spawn(move || {
        let result = run_build(&bytes_in, &map_name, &opts);
        if tx
            .send_blocking(JobOutcome::Build {
                document,
                result,
            })
            .is_err()
        {
            log::warn!("build result dropped: UI receiver gone");
        }
    });
    true
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

// ── Launch (play-test) ──────────────────────────────────────────────────────

#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
pub fn start_launch(shared: &Rc<RefCell<EditorSession>>) -> bool {
    let (snapshot, plan, prefs, iwad, tx) = {
        let state = &mut *shared.borrow_mut();
        if state.job_busy {
            log::warn!("a job is already running");
            return false;
        }
        if let Some(child) = &mut state.launched
            && child.try_wait().ok().flatten().is_none()
        {
            log::warn!("previous launch still running");
            return false;
        }
        let Some(iwad) = state.iwad.clone() else {
            log::warn!("no IWAD known - cannot launch");
            return false;
        };
        let Some(map) = &state.map_editor.map else {
            return false;
        };
        let click = state.map_editor.cursor_world;
        let snapped = [
            snap(click[0], state.map_editor.grid) as i32,
            snap(click[1], state.map_editor.grid) as i32,
        ];
        let launch_type = state.effective_launch_type();
        let nodes = state.effective_nodes_format();
        let plan =
            match launch::plan_launch(map, &state.map_editor.map_name, launch_type, nodes, snapped)
            {
                Ok(plan) => plan,
                Err(e) => {
                    log::error!("launch: {e}");
                    return false;
                }
            };
        let snapshot = match bincode::serialize(map) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("snapshot map for launch: {e}");
                return false;
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

    thread::spawn(move || {
        let result = run_launch(&snapshot, &plan, &prefs, &iwad);
        if tx
            .send_blocking(JobOutcome::Launch {
                result,
            })
            .is_err()
        {
            log::warn!("launch result dropped: UI receiver gone");
        }
    });
    true
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

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Lumps after the map: imported patches, TEXTURE1/2+PNAMES (if edited), ANIMATED (if defined).
fn project_lumps(state: &EditorSession) -> Vec<Lump> {
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
