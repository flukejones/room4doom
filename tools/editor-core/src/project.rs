//! DoomEd project files: the `.dpr` descriptor plus its `.dsp` files. `.dpr` grammar ("Doom Project version 1"): header line, then `wadfile:`/`mapwads:`/`BSPprogram:`/`BSPhost:`/`nummaps:` path/value fields (values run to end of line, may contain spaces), N map-name lines (each ≤8 chars), then `numtextures:` (legacy inline texture records, always written as 0 — texture data lives in `texture{N}.dsp` files). `mapwads` is forced to the project directory on load; `BSPprogram`/`BSPhost` are preserved for round-trip only (BSP building is in-process). DSP files in the project directory: `things.dsp`, `sectorspecials.dsp`, `linespecials.dsp`, `animated.dsp`, and one `texture{N}.dsp` per WAD source; any absent load as empty lists. Each map is stored as `{NAME}.dwd` beside the `.dpr`.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

use rbsp::wad_io::NodesFormat;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use wad::WadData;

use crate::dsp::{
    AnimDef, DspError, PatchPlacement, SpecialDef, TextureDef, ThingDef, parse_animated_dsp,
    parse_specials_dsp, parse_textures_dsp, parse_things_dsp,
};
use crate::map_ron::MAP_RON_EXT;
use crate::name8::Name8;
use crate::texture_group::TextureGroup;

/// DSP file names within a project directory.
const THINGS_DSP: &str = "things.dsp";
const SECTOR_SPECIALS_DSP: &str = "sectorspecials.dsp";
const LINE_SPECIALS_DSP: &str = "linespecials.dsp";
const ANIMATED_DSP: &str = "animated.dsp";
/// Extension of imported patch lumps stored beside the `.dpr`, one per patch.
const IMPORTED_PATCH_EXT: &str = "lmp";
/// Native project manifest file name within a project directory.
const PROJECT_RON: &str = "project.ron";
/// Subdirectory holding native `.ron` maps within a project directory.
const MAPS_DIR: &str = "maps";
/// Native data files (RON), one per concern, beside `project.ron`.
const THINGS_RON: &str = "things.ron";
const SECTOR_SPECIALS_RON: &str = "sector_specials.ron";
const LINE_SPECIALS_RON: &str = "line_specials.ron";
const ANIMATIONS_RON: &str = "animations.ron";
const TEXTURES_RON: &str = "textures.ron";
/// Maximum chars in a map name (`char[9]` in DoomEd).
const MAP_NAME_MAX_LEN: usize = 8;
const DPR_VERSION: i32 = 1;

/// Failure while loading or saving a project.
#[derive(Debug)]
pub enum ProjectError {
    Io(io::Error),
    BadHeader {
        found: String,
    },
    UnsupportedVersion {
        version: i32,
    },
    MissingField {
        field: &'static str,
    },
    BadMapName {
        name: String,
    },
    DspFile {
        file: String,
        error: DspError,
    },
    RonSerialize(ron::Error),
    RonParse(ron::error::SpannedError),
    /// Tried to save an unsaved draft (no directory). Materialise it first.
    NoDir,
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "project io error: {e}"),
            Self::BadHeader {
                found,
            } => {
                write!(f, "expected `Doom Project version` header, found {found:?}")
            }
            Self::UnsupportedVersion {
                version,
            } => {
                write!(
                    f,
                    "unsupported project version {version}, expected {DPR_VERSION}"
                )
            }
            Self::MissingField {
                field,
            } => write!(f, "missing project field {field:?}"),
            Self::BadMapName {
                name,
            } => write!(f, "map name {name:?} exceeds 8 characters"),
            Self::DspFile {
                file,
                error,
            } => write!(f, "{file}: {error}"),
            Self::RonSerialize(e) => write!(f, "project data serialize error: {e}"),
            Self::RonParse(e) => write!(f, "project data parse error: {e}"),
            Self::NoDir => write!(f, "cannot save an unsaved draft project"),
        }
    }
}

impl Error for ProjectError {}

impl From<io::Error> for ProjectError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// A project-local patch lump imported from a PNG, stored beside the `.dpr` as `{name}.lmp` and emitted into the PWAD (and PNAMES) on export.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPatch {
    /// 8-char uppercase name used in `TextureDef` patch placements.
    pub name: Name8,
    /// Raw bytes in Doom picture (patch) format.
    pub lump: Vec<u8>,
}

/// Serialize a [`NodesFormat`] as its canonical lowercase token.
fn serialize_nodes_format<S: Serializer>(f: &NodesFormat, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(f.as_str())
}

/// Deserialize a [`NodesFormat`] from its canonical lowercase token.
fn deserialize_nodes_format<'de, D: Deserializer<'de>>(d: D) -> Result<NodesFormat, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// How textures resolve across loaded WADs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextureMode {
    /// Boom/ZDoom: merge every loaded WAD's `TEXTUREx` by name, later WAD/lump wins, earlier names still resolve. What modern PWADs (e.g. sunder) expect.
    #[default]
    Custom,
    /// Vanilla DOOM.EXE: a map renders against one WAD's `TEXTUREx` only (the per-map target), with no cross-WAD fallback.
    Vanilla,
}

/// Per-project preferences that override the editor's "Preferred Defaults" while a project is open. Persisted in `project.ron`'s `settings` field.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectPreferences {
    /// IWAD this project edits against.
    pub iwad: PathBuf,
    /// Node lumps maps export with.
    #[serde(
        serialize_with = "serialize_nodes_format",
        deserialize_with = "deserialize_nodes_format",
        default
    )]
    pub nodes_format: NodesFormat,
    /// Doom thing-type number the LAUNCH tool temporarily moves (player start).
    pub launch_type: i32,
    /// Map name to reopen when the project is loaded.
    pub last_map: Option<String>,
    /// PWADs loaded alongside the IWAD, restored when the project reopens.
    #[serde(default)]
    pub pwads: Vec<PathBuf>,
    /// How textures resolve across loaded WADs (default merge / Boom).
    #[serde(default)]
    pub texture_mode: TextureMode,
}

/// The native `project.ron` manifest: settings plus the small project header. All bulk data (things, specials, animations, textures) lives in its own `.ron` file so a large texture set never bloats the manifest, and `imported_patches` stay as `.lmp` files (rescanned on load). The project's `dir` is its location and is not stored.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ProjectManifest {
    settings: ProjectPreferences,
    #[serde(default)]
    bsp_program: String,
    #[serde(default)]
    bsp_host: String,
    maps: Vec<String>,
}

/// An open DoomEd-style project: the `.dpr` fields plus all DSP-file data.
#[derive(Debug, PartialEq)]
pub struct Project {
    /// Project directory, or `None` for an unsaved in-memory draft (opened from a WAD/map/`.dpr` before the first Save). `materialise_at` sets it.
    pub dir: Option<PathBuf>,
    /// The game IWAD this project edits against.
    pub wadfile: PathBuf,
    /// Preserved for `.dpr` round-trip; BSP building is in-process.
    pub bsp_program: String,
    pub bsp_host: String,
    /// Map names, each ≤ [`MAP_NAME_MAX_LEN`] chars; one `.dwd` per entry.
    pub maps: Vec<String>,
    pub things: Vec<ThingDef>,
    pub sector_specials: Vec<SpecialDef>,
    pub line_specials: Vec<SpecialDef>,
    /// Flat/texture animation sequences, mirroring `animated.dsp`; exported as a Boom ANIMATED lump.
    pub animations: Vec<AnimDef>,
    /// Patches imported from PNGs, each also on disk as `{name}.lmp`.
    pub imported_patches: Vec<ImportedPatch>,
    /// Texture groups (per source WAD + lump), provenance-tagged; persisted to `textures.ron`. Only `edited` groups are re-emitted to the output WAD.
    pub textures: Vec<TextureGroup>,
    /// Project-level settings; defaulted when importing a legacy `.dpr`.
    pub settings: ProjectPreferences,
}

/// The `.dpr` descriptor's parsed fields, before any DSP-file loading.
struct DprFields {
    wadfile: PathBuf,
    bsp_program: String,
    bsp_host: String,
    maps: Vec<String>,
    inline_textures: Vec<TextureDef>,
}

impl Project {
    /// Import a legacy DoomEd `.dpr` and every DSP file beside it. Read-only: the editor's own format ([`Project::save`]/[`Project::load`]) is native.
    pub fn load_dpr(dpr_path: &Path) -> Result<Self, ProjectError> {
        let text = fs::read_to_string(dpr_path)?;
        let fields = parse_dpr_text(&text)?;
        let dir = dpr_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let things = load_dsp(&dir, THINGS_DSP, parse_things_dsp)?;
        let sector_specials = load_dsp(&dir, SECTOR_SPECIALS_DSP, parse_specials_dsp)?;
        let line_specials = load_dsp(&dir, LINE_SPECIALS_DSP, parse_specials_dsp)?;
        let animations = load_dsp(&dir, ANIMATED_DSP, parse_animated_dsp)?;
        let imported_patches = load_imported_patches(&dir)?;

        let mut textures = Vec::new();
        if !fields.inline_textures.is_empty() {
            textures.push(defs_to_edited_group(
                &wad_basename(&fields.wadfile),
                1,
                fields.inline_textures,
            ));
        }
        let mut index = textures.len() + 1;
        loop {
            let file = texture_dsp_name(index);
            let path = dir.join(&file);
            if !path.exists() {
                break;
            }
            let text = fs::read_to_string(&path)?;
            let defs = parse_textures_dsp(&text).map_err(|error| ProjectError::DspFile {
                file,
                error,
            })?;
            textures.push(defs_to_edited_group(
                &wad_basename(&fields.wadfile),
                index,
                defs,
            ));
            index += 1;
        }

        // Legacy `.dpr` carries no settings block; seed the IWAD from the descriptor's wadfile so an imported project keeps editing the same WAD. Imported as a draft (`dir: None`): the first Save writes a *native* project elsewhere, never back into the DoomEd directory.
        let settings = ProjectPreferences {
            iwad: fields.wadfile.clone(),
            ..ProjectPreferences::default()
        };
        Ok(Self {
            dir: None,
            wadfile: fields.wadfile,
            bsp_program: fields.bsp_program,
            bsp_host: fields.bsp_host,
            maps: fields.maps,
            things,
            sector_specials,
            line_specials,
            animations,
            imported_patches,
            textures,
            settings,
        })
    }

    /// The project directory, or `None` for an unsaved draft.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// True for an unsaved in-memory draft (no directory yet).
    pub fn is_draft(&self) -> bool {
        self.dir.is_none()
    }

    /// An in-memory draft project against an IWAD (no directory yet): imports the IWAD's composite texture definitions as texture set 1. The first [`materialise_at`](Self::materialise_at) writes it to disk.
    pub fn draft(iwad: &Path, wad: &WadData) -> Self {
        Self {
            dir: None,
            wadfile: iwad.to_path_buf(),
            bsp_program: String::new(),
            bsp_host: String::new(),
            maps: Vec::new(),
            things: Vec::new(),
            sector_specials: Vec::new(),
            line_specials: Vec::new(),
            animations: Vec::new(),
            imported_patches: Vec::new(),
            textures: import_wad_texture_groups(&wad_basename(iwad), wad),
            settings: ProjectPreferences {
                iwad: iwad.to_path_buf(),
                ..ProjectPreferences::default()
            },
        }
    }

    /// Create a fresh project on disk against an IWAD (a [`draft`](Self::draft) materialised at `dir`).
    pub fn create(dir: &Path, iwad: &Path, wad: &WadData) -> Result<Self, ProjectError> {
        let mut project = Self::draft(iwad, wad);
        project.materialise_at(dir)?;
        Ok(project)
    }

    /// Give a draft a directory and write it. No-op location change if already saved (re-points and rewrites).
    pub fn materialise_at(&mut self, dir: &Path) -> Result<(), ProjectError> {
        self.dir = Some(dir.to_path_buf());
        self.save()
    }

    /// Register a PWAD alongside the IWAD, deduplicated by path. Returns `true` if added, `false` if it was already the IWAD or an existing PWAD. The single IWAD ([`wadfile`](Self::wadfile)) is set at construction and never changed here — only PWADs accumulate.
    pub fn add_pwad(&mut self, path: &Path) -> bool {
        if path == self.wadfile || self.settings.pwads.iter().any(|p| p == path) {
            return false;
        }
        self.settings.pwads.push(path.to_path_buf());
        true
    }

    /// Where a map's native `.ron` lives: `{dir}/maps/{NAME}.ron`. `None` for a draft.
    pub fn map_ron_path(&self, map_name: &str) -> Option<PathBuf> {
        Some(
            self.dir
                .as_ref()?
                .join(MAPS_DIR)
                .join(format!("{map_name}.{MAP_RON_EXT}")),
        )
    }

    /// Write the project: the small `project.ron` manifest, one RON data file per data concern (things, specials, animations, textures), and `.lmp` patch data files. Map geometry is written separately as `maps/{NAME}.ron`. Errors with [`ProjectError::NoDir`] on a draft (call `materialise_at` first).
    pub fn save(&self) -> Result<(), ProjectError> {
        let dir = self.dir.as_ref().ok_or(ProjectError::NoDir)?;
        fs::create_dir_all(dir)?;
        fs::create_dir_all(dir.join(MAPS_DIR))?;
        for name in &self.maps {
            check_map_name(name)?;
        }
        let manifest = ProjectManifest {
            settings: self.settings.clone(),
            bsp_program: self.bsp_program.clone(),
            bsp_host: self.bsp_host.clone(),
            maps: self.maps.clone(),
        };
        let text = ron::ser::to_string_pretty(&manifest, ron::ser::PrettyConfig::default())
            .map_err(ProjectError::RonSerialize)?;
        write_atomic(&dir.join(PROJECT_RON), text.as_bytes())?;

        save_ron_data(&dir.join(THINGS_RON), &self.things)?;
        save_ron_data(&dir.join(SECTOR_SPECIALS_RON), &self.sector_specials)?;
        save_ron_data(&dir.join(LINE_SPECIALS_RON), &self.line_specials)?;
        save_ron_data(&dir.join(ANIMATIONS_RON), &self.animations)?;
        save_ron_data(&dir.join(TEXTURES_RON), &self.textures)?;

        // Binary patch lumps stay as their own files, never inlined in any manifest.
        for patch in &self.imported_patches {
            write_atomic(
                &dir.join(format!("{}.{IMPORTED_PATCH_EXT}", patch.name.as_str())),
                &patch.lump,
            )?;
        }
        Ok(())
    }

    /// Load a project: parse `project.ron`, the RON data files (absent = empty), and rescan `.lmp` patch files.
    pub fn load(dir: &Path) -> Result<Self, ProjectError> {
        let text = fs::read_to_string(dir.join(PROJECT_RON))?;
        let manifest: ProjectManifest = ron::from_str(&text).map_err(ProjectError::RonParse)?;
        Ok(Self {
            dir: Some(dir.to_path_buf()),
            wadfile: manifest.settings.iwad.clone(),
            bsp_program: manifest.bsp_program,
            bsp_host: manifest.bsp_host,
            maps: manifest.maps,
            things: load_ron_data(&dir.join(THINGS_RON))?,
            sector_specials: load_ron_data(&dir.join(SECTOR_SPECIALS_RON))?,
            line_specials: load_ron_data(&dir.join(LINE_SPECIALS_RON))?,
            animations: load_ron_data(&dir.join(ANIMATIONS_RON))?,
            imported_patches: load_imported_patches(dir)?,
            textures: load_ron_data(&dir.join(TEXTURES_RON))?,
            settings: manifest.settings,
        })
    }
}

/// Parse `.dpr` descriptor text (header, path/value fields, map names, inline texture section). Pure parsing: no filesystem access.
fn parse_dpr_text(text: &str) -> Result<DprFields, ProjectError> {
    // The legacy inline texture section runs from a line-initial `numtextures:` to end of file, in the texture-DSP grammar.
    let (fields_text, inline) = match text.split_once("\nnumtextures:") {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    };

    let mut lines = fields_text.lines();
    let header = lines.next().unwrap_or_default();
    let version = header
        .strip_prefix("Doom Project version ")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .ok_or_else(|| ProjectError::BadHeader {
            found: header.to_owned(),
        })?;
    if version != DPR_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            version,
        });
    }

    let mut wadfile = None;
    let mut bsp_program = String::new();
    let mut bsp_host = String::new();
    let mut maps = Vec::new();
    let mut rest_for_maps = 0usize;
    for line in lines {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if rest_for_maps > 0 {
            let name = line.trim();
            check_map_name(name)?;
            maps.push(name.to_owned());
            rest_for_maps -= 1;
        } else if let Some(v) = line.strip_prefix("wadfile: ") {
            wadfile = Some(PathBuf::from(v));
        } else if line.strip_prefix("mapwads: ").is_some() {
            // Forced to the project directory (DoomEd behavior).
        } else if let Some(v) = line.strip_prefix("BSPprogram: ") {
            bsp_program = v.to_owned();
        } else if let Some(v) = line.strip_prefix("BSPhost: ") {
            bsp_host = v.to_owned();
        } else if let Some(v) = line.strip_prefix("nummaps: ") {
            rest_for_maps = v.trim().parse::<i32>().unwrap_or(0).max(0) as usize;
        }
    }

    let wadfile = wadfile.ok_or(ProjectError::MissingField {
        field: "wadfile",
    })?;

    let inline_textures = match inline {
        Some(tail) => parse_textures_dsp(&format!("numtextures:{tail}")).map_err(|error| {
            ProjectError::DspFile {
                file: "inline texture section".to_owned(),
                error,
            }
        })?,
        None => Vec::new(),
    };

    Ok(DprFields {
        wadfile,
        bsp_program,
        bsp_host,
        maps,
        inline_textures,
    })
}

/// Reject a map name longer than [`MAP_NAME_MAX_LEN`].
fn check_map_name(name: &str) -> Result<(), ProjectError> {
    if name.len() > MAP_NAME_MAX_LEN {
        return Err(ProjectError::BadMapName {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// Temp + rename write: a crash mid-write must not destroy the existing file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Write a RON data file, but only when there is data — an empty list leaves no file (and removes a stale one), so the project dir stays tidy.
fn save_ron_data<T: Serialize>(path: &Path, data: &[T]) -> Result<(), ProjectError> {
    if data.is_empty() {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }
    let text = ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default())
        .map_err(ProjectError::RonSerialize)?;
    write_atomic(path, text.as_bytes())?;
    Ok(())
}

/// Read a RON data file; a missing file loads as an empty list (each concern is optional, mirroring the legacy DSP files).
fn load_ron_data<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, ProjectError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    ron::from_str(&text).map_err(ProjectError::RonParse)
}

/// Import one WAD's `TEXTURE<n>` lumps as provenance-tagged groups (one per lump). Call once per source WAD path (`WadData::new(path)`), not the merged blob — a merged WAD only exposes the last same-named lump.
pub fn import_wad_texture_groups(wad_name: &str, wad: &WadData) -> Vec<TextureGroup> {
    let pnames: Vec<String> = wad.pnames_iter().collect();
    let mut groups = Vec::new();
    let mut n = 1;
    loop {
        let lump = format!("TEXTURE{n}");
        if !wad.lump_exists(&lump) {
            break;
        }
        n += 1;
        let defs = texture_defs_from_lump(wad, &pnames, &lump);
        if !defs.is_empty() {
            groups.push(TextureGroup {
                wad_name: wad_name.to_owned(),
                lump: Name8::new(&lump).expect("TEXTUREn fits Name8"),
                defs,
                edited: false,
            });
        }
    }
    groups
}

/// Wrap a parsed `texture{n}.dsp` def list as an edited [`TextureGroup`] tagged `TEXTURE{n}`, with `wad_name` from the project's IWAD basename.
fn defs_to_edited_group(wad_name: &str, index: usize, defs: Vec<TextureDef>) -> TextureGroup {
    TextureGroup {
        wad_name: wad_name.to_owned(),
        lump: Name8::new(&format!("TEXTURE{index}")).expect("TEXTUREn fits Name8"),
        defs,
        edited: true,
    }
}

/// IWAD path → its basename string (for tagging project texture groups).
fn wad_basename(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_owned()
}

/// Decode a single `TEXTURE<n>` lump's records into [`TextureDef`]s, resolving patch indices through `pnames`.
fn texture_defs_from_lump(wad: &WadData, pnames: &[String], lump: &str) -> Vec<TextureDef> {
    let mut defs = Vec::new();
    for tex in wad.texture_iter(lump) {
        let Ok(name) = Name8::from_wad(&tex.name) else {
            continue;
        };
        let mut patches = Vec::with_capacity(tex.patches.len());
        for p in &tex.patches {
            let Some(patch_name) = pnames.get(p.patch_index) else {
                continue;
            };
            let Ok(patch) = Name8::from_wad(patch_name) else {
                continue;
            };
            patches.push(PatchPlacement {
                origin_x: p.origin_x,
                origin_y: p.origin_y,
                patch,
                step_dir: 1,
                colormap: 0,
            });
        }
        defs.push(TextureDef {
            name,
            width: tex.width as i32,
            height: tex.height as i32,
            patches,
        });
    }
    defs
}

fn texture_dsp_name(index: usize) -> String {
    format!("texture{index}.dsp")
}

/// Read every `*.lmp` patch lump in the project directory. The descriptor's directory is read directly (it must exist), so a `read_dir` failure is a real error; individual files that cannot be read or whose stem is not a valid 8-char name are logged and skipped. Results sort by name so load order is deterministic.
fn load_imported_patches(dir: &Path) -> Result<Vec<ImportedPatch>, ProjectError> {
    let mut patches = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some(IMPORTED_PATCH_EXT) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            log::warn!(
                "skipping patch lump with non-UTF-8 name: {}",
                path.display()
            );
            continue;
        };
        let name = match Name8::new(stem) {
            Ok(name) => name,
            Err(e) => {
                log::warn!("skipping patch lump {}: invalid name: {e}", path.display());
                continue;
            }
        };
        match fs::read(&path) {
            Ok(lump) => patches.push(ImportedPatch {
                name,
                lump,
            }),
            Err(e) => log::warn!("skipping unreadable patch lump {}: {e}", path.display()),
        }
    }
    patches.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    Ok(patches)
}

fn load_dsp<T>(
    dir: &Path,
    file: &str,
    parse: impl Fn(&str) -> Result<Vec<T>, DspError>,
) -> Result<Vec<T>, ProjectError> {
    let path = dir.join(file);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path)?;
    parse(&text).map_err(|error| ProjectError::DspFile {
        file: file.to_owned(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    fn temp_project_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("editor_core_project_{tag}"));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).expect("temp dir creates");
        dir
    }

    fn sample_project(dir: PathBuf) -> Project {
        Project {
            dir: Some(dir),
            wadfile: PathBuf::from("/doom/doom1.wad"),
            bsp_program: "/applications/doombsp".to_owned(),
            bsp_host: "localhost".to_owned(),
            maps: vec!["E1M1".to_owned(), "E1M2".to_owned()],
            things: vec![ThingDef {
                name: "Player1".to_owned(),
                angle: 90,
                value: 1,
                option: 7,
                color: [0.2, 0.8, 0.2],
                icon: Name8::new("PLAYA1").expect("valid"),
            }],
            sector_specials: vec![SpecialDef {
                value: 9,
                desc: "Secret".to_owned(),
            }],
            line_specials: vec![SpecialDef {
                value: 1,
                desc: "Door_Raise".to_owned(),
            }],
            animations: vec![AnimDef {
                is_texture: false,
                start: Name8::new("NUKAGE1").expect("valid"),
                end: Name8::new("NUKAGE3").expect("valid"),
                speed: 8,
            }],
            imported_patches: vec![ImportedPatch {
                name: Name8::new("MYPATCH").expect("valid"),
                lump: vec![1, 0, 1, 0, 0, 0, 0, 0],
            }],
            textures: vec![TextureGroup {
                wad_name: "doom1.wad".to_owned(),
                lump: Name8::new("TEXTURE1").expect("valid"),
                edited: true,
                defs: vec![TextureDef {
                    name: Name8::new("STARTAN3").expect("valid"),
                    width: 128,
                    height: 128,
                    patches: vec![PatchPlacement {
                        origin_x: 0,
                        origin_y: 0,
                        patch: Name8::new("SW17_4").expect("valid"),
                        step_dir: 1,
                        colormap: 0,
                    }],
                }],
            }],
            settings: ProjectPreferences {
                iwad: PathBuf::from("/doom/doom1.wad"),
                ..ProjectPreferences::default()
            },
        }
    }

    #[test]
    fn add_pwad_dedups_and_rejects_iwad() {
        let mut project = sample_project(temp_project_dir("pwad"));
        let iwad = project.wadfile.clone();
        let extra = PathBuf::from("/doom/extra.wad");

        assert!(project.add_pwad(&extra), "first add succeeds");
        assert!(!project.add_pwad(&extra), "duplicate is rejected");
        assert!(!project.add_pwad(&iwad), "the IWAD is not a PWAD");
        assert_eq!(project.settings.pwads, vec![extra]);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_project_dir("roundtrip");
        let mut project = sample_project(dir.clone());
        project.settings = ProjectPreferences {
            iwad: PathBuf::from("/doom/doom1.wad"),
            nodes_format: NodesFormat::Both,
            launch_type: 1,
            last_map: Some("E1M2".to_owned()),
            pwads: vec![PathBuf::from("/doom/extra.wad")],
            texture_mode: TextureMode::Vanilla,
        };
        project.save().expect("project saves");

        let loaded = Project::load(&dir).expect("project loads");
        assert_eq!(loaded, project);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn data_lives_in_separate_files_not_the_manifest() {
        let dir = temp_project_dir("datafiles");
        sample_project(dir.clone()).save().expect("saves");

        // Bulk data is in its own file; the manifest holds none of it.
        assert!(dir.join(TEXTURES_RON).exists());
        assert!(dir.join(THINGS_RON).exists());
        assert!(dir.join(SECTOR_SPECIALS_RON).exists());
        assert!(dir.join(LINE_SPECIALS_RON).exists());
        assert!(dir.join(ANIMATIONS_RON).exists());
        let manifest = fs::read_to_string(dir.join(PROJECT_RON)).expect("manifest");
        assert!(
            !manifest.contains("STARTAN3"),
            "textures leaked into manifest"
        );
        assert!(!manifest.contains("Player1"), "things leaked into manifest");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_concern_writes_no_file() {
        let dir = temp_project_dir("empty_datafile");
        let mut project = sample_project(dir.clone());
        project.things.clear();
        project.save().expect("saves");
        assert!(!dir.join(THINGS_RON).exists());
        // A previously-written data file is removed when its data empties.
        assert_eq!(
            Project::load(&dir).expect("loads").things,
            Vec::<ThingDef>::new()
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dpr_import_defaults_settings_and_writes_nothing_native() {
        let dir = temp_project_dir("dpr_import");
        fs::write(
            dir.join("dpr_import.dpr"),
            "Doom Project version 1\n\nwadfile: /doom/doom1.wad\nmapwads: /tmp\nBSPprogram: \nBSPhost: \nnummaps: 0\nnumtextures: 0\n",
        )
        .expect("dpr writes");

        let loaded = Project::load_dpr(&dir.join("dpr_import.dpr")).expect("dpr loads");
        // Import seeds the IWAD from the descriptor but otherwise defaults.
        assert_eq!(loaded.settings.iwad, PathBuf::from("/doom/doom1.wad"));
        assert_eq!(loaded.settings.nodes_format, NodesFormat::default());
        assert_eq!(loaded.settings.last_map, None);
        // Importing a .dpr writes no native manifest.
        assert!(!dir.join(PROJECT_RON).exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_dsp_files_load_as_empty() {
        let dir = temp_project_dir("no_dsp");
        fs::write(
            dir.join("no_dsp.dpr"),
            "Doom Project version 1\n\nwadfile: /doom/doom1.wad\nmapwads: /tmp\nBSPprogram: \nBSPhost: \nnummaps: 0\nnumtextures: 0\n",
        )
        .expect("dpr writes");

        let loaded = Project::load_dpr(&dir.join("no_dsp.dpr")).expect("loads");
        assert!(loaded.things.is_empty());
        assert!(loaded.sector_specials.is_empty());
        assert!(loaded.animations.is_empty());
        assert!(loaded.imported_patches.is_empty());
        assert!(loaded.textures.is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn imported_patch_lmp_with_invalid_name_skipped() {
        let dir = temp_project_dir("bad_lmp");
        fs::write(
            dir.join("bad_lmp.dpr"),
            "Doom Project version 1\n\nwadfile: /doom/doom1.wad\nnummaps: 0\nnumtextures: 0\n",
        )
        .expect("dpr writes");
        fs::write(dir.join("GOODPTCH.lmp"), [0u8; 8]).expect("lmp writes");
        fs::write(dir.join("TOOLONGNAME.lmp"), [0u8; 8]).expect("lmp writes");

        let loaded = Project::load_dpr(&dir.join("bad_lmp.dpr")).expect("loads");
        assert_eq!(loaded.imported_patches.len(), 1);
        assert_eq!(loaded.imported_patches[0].name.as_str(), "GOODPTCH");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dpr_inline_texture_section_imports_as_edited_group() {
        let dir = temp_project_dir("inline_tex");
        fs::write(
            dir.join("inline_tex.dpr"),
            "Doom Project version 1\n\nwadfile: /doom/doom1.wad\nnummaps: 1\nE1M1\nnumtextures: 1\nSTARTAN3 128, 128, 2\n   (0, 0 : SW17_4 ) 1, 0\n   (64, 0 : SW17_5 ) 1, 0\n",
        )
        .expect("dpr writes");

        let loaded = Project::load_dpr(&dir.join("inline_tex.dpr")).expect("loads");
        assert_eq!(loaded.maps, vec!["E1M1".to_owned()]);
        assert_eq!(loaded.textures.len(), 1);
        let group = &loaded.textures[0];
        assert!(group.edited);
        assert_eq!(group.wad_name, "doom1.wad");
        assert_eq!(group.lump.as_str(), "TEXTURE1");
        assert_eq!(group.defs.len(), 1);
        assert_eq!(group.defs[0].name.as_str(), "STARTAN3");
        assert_eq!(group.defs[0].patches.len(), 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn long_map_name_rejected() {
        let dir = temp_project_dir("longmap");
        fs::write(
            dir.join("longmap.dpr"),
            "Doom Project version 1\n\nwadfile: /doom/doom1.wad\nnummaps: 1\nWAYTOOLONG\n",
        )
        .expect("dpr writes");

        let err = Project::load_dpr(&dir.join("longmap.dpr")).expect_err("9+ char map name");
        assert!(matches!(err, ProjectError::BadMapName { .. }), "{err}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_version_rejected() {
        let dir = temp_project_dir("badver");
        fs::write(dir.join("badver.dpr"), "Doom Project version 2\n").expect("writes");
        let err = Project::load_dpr(&dir.join("badver.dpr")).expect_err("version 2");
        assert!(
            matches!(
                err,
                ProjectError::UnsupportedVersion {
                    version: 2
                }
            ),
            "{err}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn create_imports_iwad_textures() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let dir = temp_project_dir("create");
        let project = Project::create(&dir, &test_utils::doom1_wad_path(), &wad).expect("creates");

        assert_eq!(project.textures.len(), 1, "shareware has TEXTURE1 only");
        assert!(!project.textures[0].defs.is_empty());
        assert_eq!(project.textures[0].wad_name, "doom1.wad");
        assert_eq!(project.textures[0].lump.as_str(), "TEXTURE1");
        let startan = project.textures[0]
            .defs
            .iter()
            .find(|t| t.name.as_str() == "STARTAN3")
            .expect("STARTAN3 exists in shareware TEXTURE1");
        assert!(startan.width > 0 && !startan.patches.is_empty());

        let reloaded = Project::load(&dir).expect("reloads");
        assert_eq!(reloaded.textures, project.textures);

        fs::remove_dir_all(&dir).ok();
    }
}
