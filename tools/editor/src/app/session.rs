//! Application domain session: open resources, documents, workers and renderer caches.

use async_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::process::Child;

use editor_core::{NodesFormat, Project};
use wad::WadData;
use wad::types::GameMode;

use crate::assets::{EditorAssets, MissingResource};
use crate::defaults::{DEFAULT_THINGS, ThingType, thing_palette};
use crate::gfx;

use crate::app::jobs::JobOutcome;
use crate::app::preferences::EditorPreferences;
use crate::app::project::path_basename;
use crate::bsp_anim::{BspAnim, DEFAULT_INTERVAL_MS};
use crate::gfx::GfxCache;
use crate::level_editor::LevelEditorState;
pub use crate::level_editor::state::Damage;
use crate::level_editor::state::MapClipboard;
use crate::render::cache::MapRender;
pub use crate::render::input::SelItem;
use crate::render::sprites::ThingSpriteCache;
use crate::render::wgpu::WgpuContext;
use crate::undo::UndoStack;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionIdentity {
    document: u64,
    resources: u64,
}

/// Editing state owned on the GPUI foreground thread.
pub(crate) struct EditorSession {
    pub(crate) map_editor: LevelEditorState,
    pub(crate) map_render: MapRender,
    /// Resource replacement identity; changes even when a project reloads from the same path.
    pub(crate) resource_generation: u64,
    /// Identity of the open map lifetime, even when its name and arena keys repeat.
    pub(crate) document_generation: u64,
    /// Map markers from the open WAD, for the picker popup.
    pub(crate) wad_maps: Vec<String>,
    pub(crate) iwad: Option<PathBuf>,
    /// Live PWAD list; `ensure_wad` appends to the WAD, persisted to project on Save.
    pub(crate) pwads: Vec<PathBuf>,
    pub(crate) wad_data: Option<WadData>,
    /// Palette, textures, patches, animations. Built from IWAD + project on first use.
    pub(crate) assets: Option<EditorAssets>,
    /// Working texture, animation or patch edits not yet persisted to the project.
    pub(crate) assets_dirty: bool,
    /// Thumbnail cache over `assets`.
    pub(crate) gfx: Option<GfxCache>,
    pub(crate) prefs: EditorPreferences,
    /// Play-test process; reaped before next launch.
    pub(crate) launched: Option<Child>,
    pub(crate) project: Option<Project>,
    /// Lazily populated per distinct thing kind.
    pub(crate) thing_sprites: Option<ThingSpriteCache>,
    /// Cumulative pinch scale at last update; divide into new value for step delta.
    pub(crate) pinch_scale: f32,
    /// Worker outcomes awaited by the GPUI foreground task.
    pub(crate) job_tx: Sender<JobOutcome>,
    pub(crate) job_rx: Receiver<JobOutcome>,
    pub(crate) job_busy: bool,
    pub(crate) bsp_anim: Option<BspAnim>,
    pub(crate) anim_interval_ms: u64,
    pub(crate) anim_keep_all: bool,
    /// Dedicated canvas device/queue; shared native textures are presented by GPUI.
    pub(crate) wgpu: WgpuContext,
    /// Map resources missing from the loaded WAD set. Drives resources panel + magenta render.
    pub(crate) missing_resources: Vec<MissingResource>,
}

impl EditorSession {
    pub(crate) fn refresh_missing_resources(&mut self) {
        self.missing_resources = match (&self.assets, &self.map_editor.map, &self.wad_data) {
            (Some(assets), Some(map), Some(wad)) => assets.missing_resources(map, wad),
            _ => Vec::new(),
        };
    }

    pub(crate) fn identity(&self) -> SessionIdentity {
        SessionIdentity {
            document: self.document_generation,
            resources: self.resource_generation,
        }
    }

    pub(crate) fn new(
        app: LevelEditorState,
        iwad: Option<PathBuf>,
        prefs: EditorPreferences,
    ) -> Self {
        let (job_tx, job_rx) = async_channel::unbounded();
        Self {
            map_editor: app,
            map_render: MapRender::default(),
            resource_generation: 0,
            document_generation: 0,
            wad_maps: Vec::new(),
            iwad,
            pwads: Vec::new(),
            wad_data: None,
            assets: None,
            assets_dirty: false,
            gfx: None,
            prefs,
            launched: None,
            project: None,
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
        }
    }

    /// Replace map-scoped domain and renderer state; native views observe the document generation.
    pub(crate) fn reset_map(&mut self) {
        self.document_generation = self
            .document_generation
            .checked_add(1)
            .expect("document generation exhausted");
        self.map_editor.undo = UndoStack::new();
        self.map_editor.clipboard = MapClipboard::default();
        self.map_render = MapRender::default();
        self.bsp_anim = None;
        self.wgpu.clear_map();
        self.missing_resources.clear();
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
        self.refresh_missing_resources();
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
        self.resource_generation = self
            .resource_generation
            .checked_add(1)
            .expect("resource generation exhausted");
        self.wad_data = None;
        self.assets = None;
        self.assets_dirty = false;
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

    fn state_with(prefs: EditorPreferences) -> EditorSession {
        EditorSession::new(LevelEditorState::new(), None, prefs)
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
