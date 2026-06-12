//! LAUNCH tool: play-test the open map from a clicked spot.
//! Moves the launch thing on a map copy, exports a temp PWAD, spawns the engine detached.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use editor_core::EditorMap;
use editor_core::wad_export::{ExportError, ExportOptions, export_map_pwad};
use rbsp::wad_io::NodesFormat;

use crate::prefs::EditorPreferences;

const LAUNCH_WAD: &str = "redoomed_launch.wad";
const LAUNCH_SKILL: &str = "3";

/// Episode/map numbers extracted from a map marker name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSlot {
    Episodic { e: u32, m: u32 },
    Commercial { m: u32 },
}

/// Parses `E#M#` or `MAP##`; anything else returns `None`.
pub fn parse_map_name(name: &str) -> Option<MapSlot> {
    let b = name.as_bytes();
    if b.len() == 4 && b[0] == b'E' && b[2] == b'M' {
        let e = (b[1] as char).to_digit(10)?;
        let m = (b[3] as char).to_digit(10)?;
        return Some(MapSlot::Episodic {
            e,
            m,
        });
    }
    if b.len() == 5 && name.starts_with("MAP") {
        let m: u32 = name[3..].parse().ok()?;
        return Some(MapSlot::Commercial {
            m,
        });
    }
    None
}

#[derive(Debug)]
pub enum LaunchError {
    NoLaunchThing {
        kind: i32,
    },
    BadMapName {
        name: String,
    },
    Export(ExportError),
    /// Engine binary not found or not executable.
    EngineNotFound {
        engine: String,
        source: std::io::Error,
    },
    Io(std::io::Error),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLaunchThing {
                kind,
            } => {
                write!(f, "no thing of launch type {kind} on the map")
            }
            Self::BadMapName {
                name,
            } => {
                write!(f, "map name {name:?} is not E#M# or MAP##")
            }
            Self::Export(e) => write!(f, "{e}"),
            Self::EngineNotFound {
                engine,
                source,
            } => {
                write!(
                    f,
                    "cannot start engine {engine:?}: {source} \
                     (set the engine path in Preferences, or put it on PATH)"
                )
            }
            Self::Io(e) => write!(f, "launch io error: {e}"),
        }
    }
}

impl std::error::Error for LaunchError {}

pub struct LaunchPlan {
    pub slot: MapSlot,
    pub map_name: String,
    pub launch_kind: i32,
    pub snapped_click: [i32; 2],
    pub nodes: ExportOptions,
}

/// Validates the map name and confirms the launch thing exists.
pub fn plan_launch(
    map: &EditorMap,
    map_name: &str,
    launch_type: i32,
    nodes: NodesFormat,
    snapped_click: [i32; 2],
) -> Result<LaunchPlan, LaunchError> {
    let slot = parse_map_name(map_name).ok_or_else(|| LaunchError::BadMapName {
        name: map_name.to_owned(),
    })?;
    if !map.things.iter().any(|t| t.kind == launch_type) {
        return Err(LaunchError::NoLaunchThing {
            kind: launch_type,
        });
    }
    Ok(LaunchPlan {
        slot,
        map_name: map_name.to_owned(),
        launch_kind: launch_type,
        snapped_click,
        nodes: ExportOptions {
            nodes,
            ..ExportOptions::default()
        },
    })
}

/// Moves the launch thing on a map copy, exports a temp PWAD. Editor's map untouched.
pub fn export_launch_wad(map: &mut EditorMap, plan: &LaunchPlan) -> Result<PathBuf, LaunchError> {
    let index = map
        .things
        .iter()
        .position(|t| t.kind == plan.launch_kind)
        .ok_or(LaunchError::NoLaunchThing {
            kind: plan.launch_kind,
        })?;
    map.things[index].x = plan.snapped_click[0];
    map.things[index].y = plan.snapped_click[1];

    let bytes = export_map_pwad(map, &plan.map_name, &plan.nodes).map_err(LaunchError::Export)?;
    let wad_path = std::env::temp_dir().join(LAUNCH_WAD);
    std::fs::write(&wad_path, bytes).map_err(LaunchError::Io)?;
    Ok(wad_path)
}

/// Spawns the engine detached on the exported PWAD.
pub fn spawn_engine(
    prefs: &EditorPreferences,
    iwad: &Path,
    wad_path: &Path,
    slot: MapSlot,
) -> Result<Child, LaunchError> {
    let mut cmd = Command::new(&prefs.engine_path);
    cmd.arg("-i")
        .arg(iwad)
        .arg("-p")
        .arg(wad_path)
        .arg("-s")
        .arg(LAUNCH_SKILL);
    match slot {
        MapSlot::Episodic {
            e,
            m,
        } => {
            cmd.args(["-e", &e.to_string(), "-m", &m.to_string()]);
        }
        MapSlot::Commercial {
            m,
        } => {
            cmd.args(["-m", &m.to_string()]);
        }
    }
    cmd.spawn().map_err(|source| LaunchError::EngineNotFound {
        engine: prefs.engine_path.clone(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_names_parse() {
        assert_eq!(
            parse_map_name("E1M1"),
            Some(MapSlot::Episodic {
                e: 1,
                m: 1
            })
        );
        assert_eq!(
            parse_map_name("E4M9"),
            Some(MapSlot::Episodic {
                e: 4,
                m: 9
            })
        );
        assert_eq!(
            parse_map_name("MAP01"),
            Some(MapSlot::Commercial {
                m: 1
            })
        );
        assert_eq!(
            parse_map_name("MAP32"),
            Some(MapSlot::Commercial {
                m: 32
            })
        );
        assert_eq!(parse_map_name("FOO"), None);
        assert_eq!(parse_map_name("EXMX"), None);
    }
}
