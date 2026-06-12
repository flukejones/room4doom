//! Shared editing state: selection, drag, overlays, [`SharedState`].

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc::{self, Receiver, Sender};

use editor_core::{
    EditorMap, LineKey, NodesFormat, PatchPlacement, Project, Sector, SectorKey, ThingKey, VertKey,
};
use wad::WadData;
use wad::types::GameMode;

use crate::assets::texture::TextureHistory;
use crate::assets::{AssetGen, EditorAssets, MissingResource};
use crate::defaults::{DEFAULT_THINGS, ThingType, thing_palette};
use crate::gfx;

pub use crate::boundary::SkillFilter;
use crate::boundary::{DrawShape, SelectMode, Tool};
use crate::bsp_anim::{BspAnim, DEFAULT_INTERVAL_MS};
use crate::gfx::GfxCache;
use crate::jobs::JobOutcome;
use crate::level_editor::remap::{RemapKind, RemapPair};
use crate::level_editor::{LevelEditorState, ThingTemplate};
use crate::light_anim::SectorLight;
use crate::prefs::EditorPreferences;
use crate::project::path_basename;
use crate::render::atlas::AtlasMaps;
use crate::render::frame3d::SurfaceSlots;
pub use crate::render::input::{Overlay, SectorFill, SelItem, Selection};
use crate::render::sprites::ThingSpriteCache;
use crate::render::stop_light_timer;
use crate::render::triangulate::SectorTris;
use crate::render::wgpu::WgpuContext;
use crate::undo::UndoStack;
use crate::views::view_audit::AuditTarget;
use crate::views::view_sector_edit::SectorEditDraft;
use crate::views::view_tex_browser::TexBrowseTarget;
use crate::views::view_tex_edit::TexDrag;
use crate::views::view_wall_edit::WallEditDraft;

/// Fingerprint of a selected-key set (len, first, last, order-independent xor).
pub(crate) type SelKey = (usize, u32, u32, u32);
/// Panel-sync key: matching the last push means the panels are still accurate.
pub(crate) type SyncKey = (SelKey, SelKey, Option<SectorKey>, Tool, ThingTemplate);

/// Map clipboard: `fragment` = self-contained geometry, `anchor` = min-corner for paste offset, `sectors` = copied sector records applied on paste.
#[derive(Debug, Default, PartialEq)]
pub struct MapClipboard {
    pub anchor: [f32; 2],
    pub fragment: EditorMap,
    pub sectors: Vec<Sector>,
}

impl MapClipboard {
    pub fn is_empty(&self) -> bool {
        self.fragment.lines.is_empty() && self.fragment.things.is_empty() && self.sectors.is_empty()
    }
}

#[derive(Debug, PartialEq, Default)]
pub enum DragState {
    #[default]
    None,
    /// Moving selection. Positions captured at drag start; `snap(original + delta)` per element.
    MoveSel {
        start_world: [f32; 2],
        verts: Vec<(VertKey, [f32; 2])>,
        things: Vec<(ThingKey, [i32; 2])>,
        /// Endpoint pairs of lines touching a moved vertex, captured at drag start (topology is stable mid-drag).
        lines: Vec<[VertKey; 2]>,
    },
    /// Rubber-band selection in world coordinates, restricted to a select mode.
    Rubber { start: [f32; 2], mode: SelectMode },
    /// Rotating/scaling the selection about `pivot` via a handle drag; positions captured at drag start, map untouched until release.
    Transform {
        pivot: [f32; 2],
        mode: TransformMode,
        verts: Vec<(VertKey, [f32; 2])>,
        /// Endpoint pairs of lines touching a captured vertex (preview + mirror flip).
        lines: Vec<[VertKey; 2]>,
    },
}

/// Which transform handle is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformMode {
    /// Free rotation; `start_angle` is pivot→cursor at drag start.
    Rotate { start_angle: f32 },
    /// Corner scale about the opposite corner; `start` is the dragged corner at drag start.
    Scale { anchor: [f32; 2], start: [f32; 2] },
}

/// In-progress line chain. Points are overlay-only until chain finishes.
#[derive(Debug, Clone, PartialEq)]
pub struct PolyChain {
    /// Snapped points placed so far, in order.
    pub points: Vec<[f32; 2]>,
    /// Line keys present at chain start; lines not in this set were drawn by the chain.
    pub base: HashSet<LineKey>,
}

/// Two-click shape draw (rect/triangle/N-gon): click 1 anchors, click 2 commits.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ShapeDraw {
    #[default]
    None,
    Anchored {
        shape: DrawShape,
        /// Corner (Rect) or centre (Triangle/N-gon).
        anchor: [f32; 2],
    },
}

/// What a handled input event invalidated, driving renderer work on the wgpu canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Damage {
    None,
    /// Camera moved; grid regenerated, mesh reused.
    View,
    /// Grid rebuilt; geometry unchanged (theme/spacing/skill filter).
    Repaint,
    /// Transient overlay only (rubber band, draw, move preview).
    Overlay,
    /// The map or selection changed; the reconciler diffs against the last-synced snapshot and patches exactly the affected GPU slots/spans — every edit (value, topology, undo/redo, paste, delete) flows through here.
    Edited,
    /// Colours baked into GPU instances went stale (theme/gradient change): full resync.
    Restyle,
}

impl Damage {
    /// Merge two damages. `None` = identity; `Restyle` > `Edited` dominate; view mixes → `Repaint`.
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, d) | (d, Self::None) => d,
            (Self::Restyle, _) | (_, Self::Restyle) => Self::Restyle,
            (Self::Edited, _) | (_, Self::Edited) => Self::Edited,
            _ => Self::Repaint,
        }
    }
}

/// Render caches for the open map; replaced wholesale on map load.
#[derive(Default)]
pub(crate) struct MapRender {
    /// Swept sector fill triangles, keyed by sector; dirty sectors re-triangulate in place.
    pub(crate) sector_tris: SectorTris,
    /// Lowest bordering floor Z per vertex slot, for wireframe mode.
    pub(crate) vertex_floor_z: Vec<f32>,
    /// Atlas lookup maps (flat tiles + thing sprite slots).
    pub(crate) atlas_maps: AtlasMaps,
    /// Content key of the last-packed atlas. Unchanged → skip RGBA pack + GPU upload.
    pub(crate) atlas_key: Option<u64>,
    /// Per-sector light effects; feeds GPU brightness buffer each tic.
    pub(crate) light_anim: Vec<SectorLight>,
    /// Set when a reconcile touched light-effect inputs (sectors/lines); consumed by the light-set refresh.
    pub(crate) light_set_dirty: bool,
    /// Last-synced panel key; unchanged → skip panel sync.
    pub(crate) panels_key: Option<SyncKey>,
    pub(crate) hovered_line: Option<LineKey>,
    pub(crate) hovered_sector: Option<SectorKey>,
    /// The map as last pushed to the GPU; the reconciler diffs against it. `None` = nothing synced yet → the one full build (map load).
    pub(crate) last_synced: Option<EditorMap>,
    /// Selection as last pushed (drives instance-colour and sector-tint patches).
    pub(crate) last_selection: Selection,
    /// Highlighted sectors as last pushed (floor-tint diff).
    pub(crate) last_highlighted: Vec<SectorKey>,
    /// Grid-plane Z as last pushed; a move re-emits the instance layers riding it.
    pub(crate) last_grid_z: f32,
    /// Fill mode as last pushed; a switch re-emits instances and the wire layer.
    pub(crate) last_fill: SectorFill,
    /// Pixel ratio the instance layers were last built with; a DPI change re-emits them.
    pub(crate) last_pixel_ratio: f32,
    /// Asset generation the atlases were last packed against (texture-editor edits move it).
    pub(crate) last_asset_gen: Option<AssetGen>,
    /// Surface3d span table: per-sector fills + per-line walls over the CPU mirror.
    pub(crate) surface_slots: SurfaceSlots,
}

/// Texture-editor transient state (history, clipboard, drag, zoom); working asset set lives in [`EditorAssets`].
pub(crate) struct TexEditState {
    pub(crate) history: TextureHistory,
    pub(crate) clipboard: Vec<PatchPlacement>,
    pub(crate) drag: Option<TexDrag>,
    /// Texel-to-logical-px scale.
    pub(crate) zoom: f32,
}

impl Default for TexEditState {
    fn default() -> Self {
        Self {
            history: TextureHistory::default(),
            clipboard: Vec::new(),
            drag: None,
            zoom: 1.0,
        }
    }
}

/// All state shared by UI callbacks. Owned by the Slint event-loop thread as `Rc<RefCell<_>>`.
pub(crate) struct SharedState {
    pub(crate) app: LevelEditorState,
    pub(crate) map_render: MapRender,
    pub(crate) texedit: TexEditState,
    /// Map markers from the open WAD, for the picker popup.
    pub(crate) wad_maps: Vec<String>,
    pub(crate) iwad: Option<PathBuf>,
    /// Live PWAD list; `ensure_wad` appends to the WAD, persisted to project on Save.
    pub(crate) pwads: Vec<PathBuf>,
    pub(crate) wad_data: Option<WadData>,
    /// Palette, textures, patches, animations. Built from IWAD + project on first use.
    pub(crate) assets: Option<EditorAssets>,
    /// Thumbnail cache over `assets`.
    pub(crate) gfx: Option<GfxCache>,
    pub(crate) tex_browse_target: Option<TexBrowseTarget>,
    pub(crate) prefs: EditorPreferences,
    /// Play-test process; reaped before next launch.
    pub(crate) launched: Option<Child>,
    pub(crate) remap_kind: RemapKind,
    pub(crate) remap_pairs: Vec<RemapPair>,
    pub(crate) project: Option<Project>,
    /// Double-clicked line edit; committed on Apply.
    pub(crate) wall_edit: Option<WallEditDraft>,
    pub(crate) sector_edit: Option<SectorEditDraft>,
    /// Lazily populated per distinct thing kind.
    pub(crate) thing_sprites: Option<ThingSpriteCache>,
    /// Cumulative pinch scale at last update; divide into new value for step delta.
    pub(crate) pinch_scale: f32,
    /// Worker → UI channel. `upgrade_in_event_loop` cannot carry `Rc`; channel bridges that.
    pub(crate) job_tx: Sender<JobOutcome>,
    pub(crate) job_rx: Receiver<JobOutcome>,
    pub(crate) job_busy: bool,
    pub(crate) bsp_anim: Option<BspAnim>,
    pub(crate) anim_interval_ms: u64,
    pub(crate) anim_keep_all: bool,
    /// wgpu device/queue from Slint's renderer. Empty until first frame.
    pub(crate) wgpu: WgpuContext,
    /// Map resources missing from the loaded WAD set. Drives resources panel + magenta render.
    pub(crate) missing_resources: Vec<MissingResource>,
    /// Audit rows' map elements, index-parallel to the panel's string list.
    pub(crate) audit_targets: Vec<AuditTarget>,
}

impl SharedState {
    pub(crate) fn new(
        app: LevelEditorState,
        iwad: Option<PathBuf>,
        prefs: EditorPreferences,
    ) -> Self {
        let (job_tx, job_rx) = mpsc::channel();
        Self {
            app,
            map_render: MapRender::default(),
            texedit: TexEditState::default(),
            wad_maps: Vec::new(),
            iwad,
            pwads: Vec::new(),
            wad_data: None,
            assets: None,
            gfx: None,
            tex_browse_target: None,
            prefs,
            launched: None,
            remap_kind: RemapKind::Thing,
            remap_pairs: Vec::new(),
            project: None,
            wall_edit: None,
            sector_edit: None,
            thing_sprites: None,
            pinch_scale: 1.0,
            job_tx,
            job_rx,
            job_busy: false,
            bsp_anim: None,
            anim_interval_ms: DEFAULT_INTERVAL_MS,
            anim_keep_all: true,
            wgpu: WgpuContext::default(),
            missing_resources: Vec::new(),
            audit_targets: Vec::new(),
        }
    }

    /// Clear map-scoped state: undo, caches, edit drafts, light timer; callers with a UI also run `close_map_popups`.
    pub(crate) fn reset_map(&mut self) {
        self.app.undo = UndoStack::new();
        self.app.clipboard = MapClipboard::default();
        self.map_render = MapRender::default();
        self.wgpu.clear_map();
        self.missing_resources.clear();
        self.wall_edit = None;
        self.sector_edit = None;
        stop_light_timer();
    }

    /// Open (or reuse) the IWAD + PWADs; PWADs come from `self.pwads` (CLI/draft PWADs only reach project settings on Save, so project settings are not read here).
    pub(crate) fn ensure_wad(&mut self) -> bool {
        if self.wad_data.is_some() {
            return true;
        }
        let Some(iwad) = &self.iwad else {
            return false;
        };
        let mut wad = match WadData::try_new(iwad) {
            Ok(wad) => wad,
            Err(e) => {
                log::error!("open IWAD {}: {e}", iwad.display());
                return false;
            }
        };
        for pwad in &self.pwads {
            if let Err(e) = wad.try_add_file(pwad) {
                log::warn!("skipping unreadable PWAD {}: {e}", pwad.display());
            }
        }
        self.wad_data = Some(wad);
        true
    }

    pub(crate) fn ensure_assets(&mut self) -> bool {
        if self.assets.is_some() {
            return true;
        }
        if !self.ensure_wad() {
            return false;
        }
        let mut paths = Vec::new();
        if let Some(iwad) = &self.iwad {
            paths.push(iwad.clone());
        }
        paths.extend(self.pwads.iter().cloned());
        let wad = self.wad_data.as_ref().expect("ensured above");
        self.assets = Some(EditorAssets::load(&paths, wad, self.project.as_ref()));
        true
    }

    pub(crate) fn ensure_gfx(&mut self) -> bool {
        if self.gfx.is_some() {
            return true;
        }
        if !self.ensure_assets() {
            return false;
        }
        let assets = self.assets.as_ref().expect("ensured above");
        self.gfx = Some(GfxCache::new(assets));
        true
    }

    /// Drop WAD/asset/thumbnail/sprite caches; rebuild on next `ensure_*`.
    pub(crate) fn invalidate_wad_caches(&mut self) {
        self.wad_data = None;
        self.assets = None;
        self.gfx = None;
        self.thing_sprites = None;
    }

    pub(crate) fn set_iwad(&mut self, path: PathBuf) {
        self.iwad = Some(path);
        self.invalidate_wad_caches();
    }

    /// Append a PWAD and rebuild caches; draft re-derives textures, saved project keeps authored textures and gains the PWAD in settings. Returns `false` if path is the IWAD or already registered.
    pub(crate) fn add_pwad(&mut self, path: PathBuf) -> bool {
        if self.iwad.as_ref() == Some(&path) || self.pwads.iter().any(|p| p == &path) {
            return false;
        }
        let was_draft = self.project.as_ref().is_some_and(Project::is_draft);
        if let Some(project) = self.project.as_mut().filter(|p| !p.is_draft()) {
            project.add_pwad(&path);
        }
        self.pwads.push(path);
        self.invalidate_wad_caches();
        if was_draft {
            self.ensure_wad();
            let wad = self.wad_data.as_ref().expect("ensure_wad ran");
            let iwad = self.iwad.clone().expect("draft implies an IWAD");
            self.project = Some(Project::draft(&iwad, wad));
        }
        true
    }

    /// Thing types filtered for the loaded IWAD's game/sprite set; uses the cached WAD (no re-parse per call).
    pub(crate) fn thing_types(&mut self) -> Vec<&'static ThingType> {
        if !self.ensure_wad() {
            return DEFAULT_THINGS.iter().collect();
        }
        let wad = self.wad_data.as_ref().expect("ensured above");
        let doom2 = wad.game_mode().0 == GameMode::Commercial;
        thing_palette(doom2, |prefix| gfx::sprite_present(wad, prefix))
            .into_iter()
            .map(|i| &DEFAULT_THINGS[i])
            .collect()
    }

    /// Export node format: project setting, else prefs default.
    pub(crate) fn effective_nodes_format(&self) -> NodesFormat {
        match &self.project {
            Some(p) => p.settings.nodes_format,
            None => self.prefs.nodes_format,
        }
    }

    /// Launch-tool thing type: project setting, else prefs default.
    pub(crate) fn effective_launch_type(&self) -> i32 {
        match &self.project {
            Some(p) => p.settings.launch_type,
            None => self.prefs.launch_type,
        }
    }

    /// Directory basename of the open project; `None` if draft or no project.
    pub(crate) fn project_name(&self) -> Option<&str> {
        self.project.as_ref()?.dir()?.file_name()?.to_str()
    }

    /// Texture WAD basename: last PWAD, else IWAD.
    pub(crate) fn texture_wad(&self) -> String {
        self.pwads
            .last()
            .and_then(|p| path_basename(p))
            .or_else(|| self.iwad.as_deref().and_then(path_basename))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use editor_core::{NodesFormat, ProjectPreferences};

    use super::*;

    fn state_with(prefs: EditorPreferences) -> SharedState {
        SharedState::new(LevelEditorState::new(), None, prefs)
    }

    fn project_with(settings: ProjectPreferences) -> Project {
        Project {
            dir: Some(PathBuf::from("/tmp/proj")),
            wadfile: PathBuf::new(),
            bsp_program: String::new(),
            bsp_host: String::new(),
            maps: Vec::new(),
            things: Vec::new(),
            sector_specials: Vec::new(),
            line_specials: Vec::new(),
            animations: Vec::new(),
            imported_patches: Vec::new(),
            textures: Vec::new(),
            settings,
        }
    }

    #[test]
    fn effective_settings_fall_back_to_prefs_without_project() {
        let prefs = EditorPreferences {
            launch_type: 11,
            nodes_format: NodesFormat::Classic,
            ..EditorPreferences::default()
        };
        let state = state_with(prefs);
        assert_eq!(state.effective_launch_type(), 11);
        assert_eq!(state.effective_nodes_format(), NodesFormat::Classic);
    }

    #[test]
    fn effective_settings_prefer_open_project() {
        let prefs = EditorPreferences {
            launch_type: 11,
            nodes_format: NodesFormat::Classic,
            ..EditorPreferences::default()
        };
        let mut state = state_with(prefs);
        state.project = Some(project_with(ProjectPreferences {
            nodes_format: NodesFormat::Both,
            launch_type: 1,
            ..ProjectPreferences::default()
        }));
        assert_eq!(state.effective_launch_type(), 1);
        assert_eq!(state.effective_nodes_format(), NodesFormat::Both);
    }
}
