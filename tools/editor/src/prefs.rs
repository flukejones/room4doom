//! Editor preferences, persisted as `{config_dir}/room4doom/editor.ron`.

use std::path::PathBuf;
use std::{fs, io};

use rbsp::wad_io::NodesFormat;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::theme;

pub(crate) const CONFIG_APP_DIR: &str = "room4doom";
pub(crate) const EDITOR_CONFIG_FILE: &str = "editor.ron";
pub(crate) const RECENT_PROJECTS_MAX: usize = 8;

fn serialize_nodes_format<S: Serializer>(f: &NodesFormat, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(f.as_str())
}

fn deserialize_nodes_format<'de, D: Deserializer<'de>>(d: D) -> Result<NodesFormat, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// BSP build visualisation mode after export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BspAnimPref {
    Off,
    /// All events at once, held briefly then cleared.
    Instant,
    /// Events replayed over a fixed duration.
    #[default]
    Timed,
}

/// Map toolbar position within the Map tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ToolbarPositionPref {
    #[default]
    Top,
    Left,
}

/// Texture preview card display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PreviewMode {
    /// Follows cursor, shown after hover delay.
    #[default]
    HoverDelayed,
    /// Follows cursor, shown only after click-selection.
    OnClick,
    /// Pinned to map bottom-left, updated instantly on hover.
    PinnedCorner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Auto,
    Light,
    Dark,
}

/// `colorous` gradient for sector colour-fill mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SectorGradient {
    #[default]
    Plasma,
    Viridis,
    Inferno,
    Magma,
    Turbo,
    Cividis,
    Cool,
    Warm,
    Rainbow,
    Sinebow,
}

impl SectorGradient {
    pub const ALL: [Self; 10] = [
        Self::Plasma,
        Self::Viridis,
        Self::Inferno,
        Self::Magma,
        Self::Turbo,
        Self::Cividis,
        Self::Cool,
        Self::Warm,
        Self::Rainbow,
        Self::Sinebow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Plasma => "Plasma",
            Self::Viridis => "Viridis",
            Self::Inferno => "Inferno",
            Self::Magma => "Magma",
            Self::Turbo => "Turbo",
            Self::Cividis => "Cividis",
            Self::Cool => "Cool",
            Self::Warm => "Warm",
            Self::Rainbow => "Rainbow",
            Self::Sinebow => "Sinebow",
        }
    }

    pub fn gradient(self) -> colorous::Gradient {
        match self {
            Self::Plasma => colorous::PLASMA,
            Self::Viridis => colorous::VIRIDIS,
            Self::Inferno => colorous::INFERNO,
            Self::Magma => colorous::MAGMA,
            Self::Turbo => colorous::TURBO,
            Self::Cividis => colorous::CIVIDIS,
            Self::Cool => colorous::COOL,
            Self::Warm => colorous::WARM,
            Self::Rainbow => colorous::RAINBOW,
            Self::Sinebow => colorous::SINEBOW,
        }
    }
}

/// MDI popup geometry (logical px). Zero size → design default (never resized).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct WindowGeom {
    pub off_x: f32,
    pub off_y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupWindow {
    MapList,
    Browser,
    Remap,
    Prefs,
    WallEdit,
    SectorEdit,
    ProjectSettings,
    BuildBsp,
    NewMap,
    Audit,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PopupWindows {
    pub map_list: WindowGeom,
    pub browser: WindowGeom,
    pub remap: WindowGeom,
    pub prefs: WindowGeom,
    pub wall_edit: WindowGeom,
    pub sector_edit: WindowGeom,
    #[serde(default)]
    pub project_settings: WindowGeom,
    #[serde(default)]
    pub build_bsp: WindowGeom,
    #[serde(default)]
    pub new_map: WindowGeom,
    #[serde(default)]
    pub audit: WindowGeom,
}

impl PopupWindows {
    pub fn get(&self, which: PopupWindow) -> WindowGeom {
        match which {
            PopupWindow::MapList => self.map_list,
            PopupWindow::Browser => self.browser,
            PopupWindow::Remap => self.remap,
            PopupWindow::Prefs => self.prefs,
            PopupWindow::WallEdit => self.wall_edit,
            PopupWindow::SectorEdit => self.sector_edit,
            PopupWindow::ProjectSettings => self.project_settings,
            PopupWindow::BuildBsp => self.build_bsp,
            PopupWindow::NewMap => self.new_map,
            PopupWindow::Audit => self.audit,
        }
    }

    pub fn set(&mut self, which: PopupWindow, geom: WindowGeom) {
        match which {
            PopupWindow::MapList => self.map_list = geom,
            PopupWindow::Browser => self.browser = geom,
            PopupWindow::Remap => self.remap = geom,
            PopupWindow::Prefs => self.prefs = geom,
            PopupWindow::WallEdit => self.wall_edit = geom,
            PopupWindow::SectorEdit => self.sector_edit = geom,
            PopupWindow::ProjectSettings => self.project_settings = geom,
            PopupWindow::BuildBsp => self.build_bsp = geom,
            PopupWindow::NewMap => self.new_map = geom,
            PopupWindow::Audit => self.audit = geom,
        }
    }
}

/// Persisted editor settings. `#[serde(default)]` tolerates missing keys in old configs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorPreferences {
    pub launch_type: i32,
    pub engine_path: String,
    pub iwad: PathBuf,
    #[serde(
        serialize_with = "serialize_nodes_format",
        deserialize_with = "deserialize_nodes_format"
    )]
    pub nodes_format: NodesFormat,
    pub grid: i32,
    pub snap: bool,
    pub snap_to_vertex: bool,
    pub snap_to_line: bool,
    pub angle_snap: bool,
    /// Aspect-preserving wall preview pane bounds (logical px).
    pub wall_preview_min_w: f32,
    pub wall_preview_min_h: f32,
    pub wall_preview_max_w: f32,
    pub wall_preview_max_h: f32,
    pub bsp_anim: BspAnimPref,
    pub light_anim: bool,
    pub toolbar_position: ToolbarPositionPref,
    pub preview_mode: PreviewMode,
    /// Delay before follow-cursor preview appears in `HoverDelayed` mode.
    pub preview_hover_delay_ms: f32,
    pub theme_mode: ThemeMode,
    pub sector_gradient: SectorGradient,
    pub light_theme: String,
    pub dark_theme: String,
    /// Most-recent first, capped at [`RECENT_PROJECTS_MAX`].
    pub recent_projects: Vec<String>,
    pub popup_windows: PopupWindows,
    pub browser_tree_w: f32,
    pub browser_lump_w: f32,
    /// macOS vibrancy opacity: 0 = full glass, 1 = opaque. No effect elsewhere.
    pub window_glass_alpha: f32,
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            launch_type: 1,
            engine_path: "room4doom".to_owned(),
            iwad: PathBuf::new(),
            nodes_format: NodesFormat::Room4Doom,
            grid: 8,
            snap: true,
            snap_to_vertex: false,
            snap_to_line: false,
            angle_snap: false,
            wall_preview_min_w: 112.5,
            wall_preview_min_h: 96.0,
            wall_preview_max_w: 150.0,
            wall_preview_max_h: 127.5,
            bsp_anim: BspAnimPref::default(),
            light_anim: true,
            toolbar_position: ToolbarPositionPref::default(),
            preview_mode: PreviewMode::default(),
            preview_hover_delay_ms: 400.0,
            theme_mode: ThemeMode::default(),
            sector_gradient: SectorGradient::default(),
            light_theme: theme::DEFAULT_LIGHT_THEME.to_owned(),
            dark_theme: theme::DEFAULT_DARK_THEME.to_owned(),
            recent_projects: Vec::new(),
            popup_windows: PopupWindows::default(),
            browser_tree_w: 240.0,
            browser_lump_w: 200.0,
            window_glass_alpha: 0.0,
        }
    }
}

/// Returns true when the active appearance is dark.
pub fn resolve_dark(prefs: &EditorPreferences, os_dark: bool) -> bool {
    match prefs.theme_mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::Auto => os_dark,
    }
}

/// Pushes `path` to front of recent list, deduplicating and capping at [`RECENT_PROJECTS_MAX`].
pub fn push_recent_project(prefs: &mut EditorPreferences, path: &str) {
    prefs.recent_projects.retain(|p| p != path);
    prefs.recent_projects.insert(0, path.to_owned());
    prefs.recent_projects.truncate(RECENT_PROJECTS_MAX);
}

fn prefs_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join(CONFIG_APP_DIR)
            .join(EDITOR_CONFIG_FILE),
    )
}

/// Loads preferences; missing → defaults, corrupt → defaults + warn + rename old file.
pub fn load_prefs() -> EditorPreferences {
    let Some(path) = prefs_path() else {
        return EditorPreferences::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return EditorPreferences::default();
    };
    match ron::from_str(&text) {
        Ok(prefs) => prefs,
        Err(e) => {
            log::warn!("{}: unreadable prefs ({e}), using defaults", path.display());
            let old = path.with_extension("ron-old");
            if let Err(e) = fs::rename(&path, &old) {
                log::warn!("renaming bad prefs to {}: {e}", old.display());
            }
            EditorPreferences::default()
        }
    }
}

pub fn save_prefs(prefs: &EditorPreferences) -> io::Result<()> {
    let Some(path) = prefs_path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = ron::ser::to_string_pretty(prefs, ron::ser::PrettyConfig::default())
        .map_err(io::Error::other)?;
    // Temp + rename: a crash mid-write must not corrupt the prefs file.
    let tmp = path.with_extension("ron.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ron_round_trip() {
        let mut prefs = EditorPreferences {
            launch_type: 11,
            nodes_format: NodesFormat::Classic,
            ..Default::default()
        };
        prefs.popup_windows.set(
            PopupWindow::Browser,
            WindowGeom {
                off_x: -40.0,
                off_y: 12.5,
                w: 720.0,
                h: 640.0,
            },
        );
        let text = ron::ser::to_string_pretty(&prefs, ron::ser::PrettyConfig::default())
            .expect("serialises");
        let back: EditorPreferences = ron::from_str(&text).expect("own output parses");
        assert_eq!(back, prefs);
    }

    #[test]
    fn old_config_without_popup_windows_loads_default() {
        let prefs = EditorPreferences::default();
        let text = ron::ser::to_string_pretty(&prefs, ron::ser::PrettyConfig::default())
            .expect("serialises");
        let key = "popup_windows:";
        let start = text.find(key).expect("default RON has popup_windows");
        let open = text[start..].find('(').expect("nested struct opens") + start;
        let mut depth = 0;
        let mut end = open;
        for (i, ch) in text[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        let mut stripped = text[..start].to_owned();
        stripped.push_str(text[end..].trim_start_matches([',', ' ']));
        let back: EditorPreferences =
            ron::from_str(&stripped).expect("config without popup_windows still parses");
        assert_eq!(back.popup_windows, PopupWindows::default());
        assert_eq!(back, prefs);
    }

    #[test]
    fn resolve_dark_follows_mode_then_os() {
        assert!(!resolve_dark(&EditorPreferences::default(), false));
        assert!(resolve_dark(&EditorPreferences::default(), true));
        let light = EditorPreferences {
            theme_mode: ThemeMode::Light,
            ..Default::default()
        };
        assert!(!resolve_dark(&light, true));
        let dark = EditorPreferences {
            theme_mode: ThemeMode::Dark,
            ..Default::default()
        };
        assert!(resolve_dark(&dark, false));
    }

    #[test]
    fn recent_projects_dedup_front_and_cap() {
        let mut prefs = EditorPreferences::default();
        for i in 0..(RECENT_PROJECTS_MAX + 3) {
            push_recent_project(&mut prefs, &format!("/p{i}.dpr"));
        }
        assert_eq!(prefs.recent_projects.len(), RECENT_PROJECTS_MAX);
        assert_eq!(prefs.recent_projects[0], "/p10.dpr", "most recent first");
        push_recent_project(&mut prefs, "/p5.dpr");
        assert_eq!(prefs.recent_projects[0], "/p5.dpr");
        assert_eq!(
            prefs
                .recent_projects
                .iter()
                .filter(|p| *p == "/p5.dpr")
                .count(),
            1,
            "no duplicates"
        );
    }
}
