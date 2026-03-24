//! User configuration options.

use crate::CLIOptions;
use gameplay::dirs::config_dir;
use gamestate_traits::ConfigKey;
use input::config::InputConfig;
use log::{error, info, warn};
use nanoserde::{DeRon, SerRon};

fn pretty_ron(compact: &str) -> String {
    let mut out = String::with_capacity(compact.len() * 2);
    let mut depth: usize = 0;
    let mut in_string = false;
    let indent = "    ";
    for c in compact.chars() {
        if c == '"' {
            in_string = !in_string;
            out.push(c);
            continue;
        }
        if in_string {
            out.push(c);
            continue;
        }
        match c {
            '(' => {
                out.push(c);
                out.push('\n');
                depth += 1;
                for _ in 0..depth {
                    out.push_str(indent);
                }
            }
            ')' => {
                out.push('\n');
                depth = depth.saturating_sub(1);
                for _ in 0..depth {
                    out.push_str(indent);
                }
                out.push(c);
            }
            ',' => {
                out.push(c);
                out.push('\n');
                for _ in 0..depth {
                    out.push_str(indent);
                }
            }
            _ => out.push(c),
        }
    }
    out.push('\n');
    out
}
#[cfg(feature = "sound-sdl2")]
pub use sound_sdl2::timidity::GusMemSize;

/// GUS memory size for Timidity configuration (stub when SDL2 sound is absent).
#[cfg(not(feature = "sound-sdl2"))]
#[derive(Debug, Default, Copy, Clone, DeRon, SerRon)]
pub enum GusMemSize {
    M256Kb,
    M512Kb,
    M768Kb,
    M1024Kb,
    #[default]
    Perfect,
}
use std::fs::{File, OpenOptions, create_dir};
use std::io::{Error as IoError, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const LOG_TAG: &str = "UserConfig";

/// Search for an IWAD in the config directory. Checks for doom2.wad,
/// doom.wad, doom1.wad in that order (case-insensitive).
fn find_iwad() -> String {
    let dir = config_dir();
    let candidates = ["doom2.wad", "doom.wad", "doom1.wad"];
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let files: Vec<String> = entries
            .flatten()
            .filter_map(|e| Some(e.file_name().to_string_lossy().to_string()))
            .collect();
        for candidate in candidates {
            if files.iter().any(|f| f.eq_ignore_ascii_case(candidate)) {
                return dir.join(candidate).to_string_lossy().to_string();
            }
        }
    }
    String::new()
}

/// Find the first `voxel_*.pk3` file in the config directory.
fn find_voxel_pk3() -> String {
    let dir = config_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("voxel_") && name.ends_with(".pk3") {
                return name.to_string();
            }
        }
    }
    String::new()
}

fn get_cfg_file() -> PathBuf {
    let dir = config_dir();
    if !dir.exists() {
        create_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: Couldn't create {:?}: {}", LOG_TAG, dir, e));
    }
    dir.join("user.toml")
}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy, DeRon, SerRon)]
pub enum RenderType {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    Software,
    /// Full 3D software rendering
    #[default]
    Software3D,
}

impl FromStr for RenderType {
    type Err = IoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "software" => Ok(Self::Software),
            "software3d" => Ok(Self::Software3D),
            _ => Err(IoError::new(
                ErrorKind::Unsupported,
                "Invalid rendering type",
            )),
        }
    }
}

impl Into<render_backend::RenderType> for RenderType {
    fn into(self) -> render_backend::RenderType {
        match self {
            RenderType::Software => render_backend::RenderType::Software,
            RenderType::Software3D => render_backend::RenderType::Software3D,
        }
    }
}

/// Window display mode.
#[derive(Debug, Default, Clone, Copy, PartialEq, DeRon, SerRon)]
pub enum WindowMode {
    Windowed,
    /// Borderless desktop fullscreen.
    #[default]
    Borderless,
    /// Exclusive fullscreen with video mode switching.
    Exclusive,
}

impl FromStr for WindowMode {
    type Err = IoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "windowed" => Ok(Self::Windowed),
            "borderless" => Ok(Self::Borderless),
            "exclusive" => Ok(Self::Exclusive),
            _ => Err(IoError::new(
                ErrorKind::Unsupported,
                "Invalid window mode (windowed, borderless, exclusive)",
            )),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, DeRon, SerRon)]
pub enum HudWidth {
    Classic,
    #[default]
    Widescreen,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, DeRon, SerRon)]
pub enum HudMsgMode {
    Off,
    #[default]
    Stack,
    Overwrite,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, DeRon, SerRon)]
pub enum MusicType {
    #[default]
    OPL2,
    OPL3,
    GUS,
}

impl FromStr for MusicType {
    type Err = IoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "opl2" => Ok(Self::OPL2),
            "opl3" => Ok(Self::OPL3),
            "gus" => Ok(Self::GUS),
            _ => Err(IoError::new(
                ErrorKind::Unsupported,
                "Invalid Music type",
            )),
        }
    }
}

#[derive(Debug, Default, Clone, DeRon, SerRon)]
pub struct UserConfig {
    pub iwad: String,
    pub width: u32,
    pub height: u32,
    pub window_mode: WindowMode,
    pub vsync: bool,
    pub refresh_rate: u32,
    pub hi_res: bool,
    pub renderer: RenderType,
    pub sfx_vol: i32,
    pub mus_vol: i32,
    pub music_type: MusicType,
    pub gus_mem_size: GusMemSize,
    pub input: InputConfig,
    pub frame_interpolation: bool,
    pub crt_gamma: bool,
    pub voxels: bool,
    pub voxels_path: String,
    pub show_fps: bool,
    pub menu_dim: bool,
    pub hud_size: i32,
    pub hud_width: HudWidth,
    pub hud_msg_mode: HudMsgMode,
    pub hud_msg_time: i32,
    pub health_vignette: bool,
    pub mouse_sensitivity: i32,
    pub invert_y: bool,
    pub sf2_path: String,
}

impl UserConfig {
    /// Load config from the default path. Missing file returns defaults; parse
    /// errors panic.
    pub fn load() -> Self {
        let path = get_cfg_file();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.clone())
            .unwrap_or_else(|e| panic!("Couldn't open {:?}, {}", path, e));
        let mut buf = String::new();
        if let Ok(read_len) = file.read_to_string(&mut buf) {
            if read_len == 0 {
                return UserConfig::create_default(&mut file);
            } else {
                if let Ok(data) = UserConfig::deserialize_ron(&buf) {
                    info!(target: LOG_TAG, "Loaded user config file: {path:?}");
                    return data;
                }
                warn!("Could not deserialise {:?} recreating config", path);
            }
        }
        UserConfig::create_default(&mut file)
    }

    fn create_default(file: &mut File) -> Self {
        // create a default config here
        let config = UserConfig {
            width: 640,
            height: 480,
            hi_res: true,
            window_mode: WindowMode::Borderless,
            vsync: true,
            sfx_vol: 80,
            mus_vol: 70,
            frame_interpolation: true,
            menu_dim: true,
            hud_width: HudWidth::Classic,
            hud_msg_time: 2,
            health_vignette: true,
            mouse_sensitivity: 5,
            invert_y: false,
            sf2_path: "gm.sf2".to_string(),
            voxels_path: find_voxel_pk3(),
            iwad: find_iwad(),
            ..UserConfig::default()
        };
        info!("Created default user config file");
        // Should be okay to unwrap this as is since it is a Default
        let data = pretty_ron(&config.serialize_ron());
        file.write_all(data.as_bytes())
            .unwrap_or_else(|_| panic!("Could not write {:?}", get_cfg_file()));
        info!("Saved user config to {:?}", get_cfg_file());
        config
    }

    pub fn write(&self) {
        let mut file = File::create(get_cfg_file()).expect("Couldn't overwrite config");
        let data = pretty_ron(&self.serialize_ron());
        file.write_all(data.as_bytes())
            .unwrap_or_else(|err| error!("Could not write config: {}", err));
    }

    pub fn to_config_array(&self) -> [i32; ConfigKey::KeyCount as usize] {
        let mut a = [0i32; ConfigKey::KeyCount as usize];
        a[ConfigKey::SfxVolume as usize] = self.sfx_vol;
        a[ConfigKey::MusVolume as usize] = self.mus_vol;
        a[ConfigKey::MusicType as usize] = match self.music_type {
            MusicType::OPL2 => 0,
            MusicType::OPL3 => 1,
            MusicType::GUS => 2,
        };
        a[ConfigKey::WindowMode as usize] = match self.window_mode {
            WindowMode::Windowed => 0,
            WindowMode::Borderless => 1,
            WindowMode::Exclusive => 2,
        };
        a[ConfigKey::VSync as usize] = self.vsync as i32;
        a[ConfigKey::Renderer as usize] = match self.renderer {
            RenderType::Software => 0,
            RenderType::Software3D => 1,
        };
        a[ConfigKey::HiRes as usize] = self.hi_res as i32;
        a[ConfigKey::FrameInterpolation as usize] = self.frame_interpolation as i32;
        a[ConfigKey::Voxels as usize] = self.voxels as i32;
        a[ConfigKey::CrtGamma as usize] = self.crt_gamma as i32;
        a[ConfigKey::ShowFps as usize] = self.show_fps as i32;
        a[ConfigKey::MenuDim as usize] = self.menu_dim as i32;
        a[ConfigKey::HudSize as usize] = self.hud_size;
        a[ConfigKey::HudWidth as usize] = match self.hud_width {
            HudWidth::Classic => 0,
            HudWidth::Widescreen => 1,
        };
        a[ConfigKey::HudMsgMode as usize] = match self.hud_msg_mode {
            HudMsgMode::Off => 0,
            HudMsgMode::Stack => 1,
            HudMsgMode::Overwrite => 2,
        };
        a[ConfigKey::HudMsgTime as usize] = self.hud_msg_time;
        a[ConfigKey::HealthVignette as usize] = self.health_vignette as i32;
        a[ConfigKey::MouseSensitivity as usize] = self.mouse_sensitivity;
        a[ConfigKey::InvertY as usize] = self.invert_y as i32;
        a
    }

    pub fn apply_config_array(&mut self, vals: &[i32; ConfigKey::KeyCount as usize]) {
        self.sfx_vol = vals[ConfigKey::SfxVolume as usize];
        self.mus_vol = vals[ConfigKey::MusVolume as usize];
        self.music_type = match vals[ConfigKey::MusicType as usize] {
            1 => MusicType::OPL3,
            2 => MusicType::GUS,
            _ => MusicType::OPL2,
        };
        self.window_mode = match vals[ConfigKey::WindowMode as usize] {
            1 => WindowMode::Borderless,
            2 => WindowMode::Exclusive,
            _ => WindowMode::Windowed,
        };
        self.vsync = vals[ConfigKey::VSync as usize] != 0;
        self.renderer = match vals[ConfigKey::Renderer as usize] {
            1 => RenderType::Software3D,
            _ => RenderType::Software,
        };
        self.hi_res = vals[ConfigKey::HiRes as usize] != 0;
        self.frame_interpolation = vals[ConfigKey::FrameInterpolation as usize] != 0;
        self.voxels = vals[ConfigKey::Voxels as usize] != 0;
        self.crt_gamma = vals[ConfigKey::CrtGamma as usize] != 0;
        self.show_fps = vals[ConfigKey::ShowFps as usize] != 0;
        self.menu_dim = vals[ConfigKey::MenuDim as usize] != 0;
        self.hud_size = vals[ConfigKey::HudSize as usize];
        self.hud_width = match vals[ConfigKey::HudWidth as usize] {
            1 => HudWidth::Widescreen,
            _ => HudWidth::Classic,
        };
        self.hud_msg_mode = match vals[ConfigKey::HudMsgMode as usize] {
            0 => HudMsgMode::Off,
            2 => HudMsgMode::Overwrite,
            _ => HudMsgMode::Stack,
        };
        self.hud_msg_time = vals[ConfigKey::HudMsgTime as usize];
        self.health_vignette = vals[ConfigKey::HealthVignette as usize] != 0;
        self.mouse_sensitivity = vals[ConfigKey::MouseSensitivity as usize];
        self.invert_y = vals[ConfigKey::InvertY as usize] != 0;
    }

    /// Sync the CLI options and UserOptions with each other
    pub fn sync_cli(&mut self, cli: &mut CLIOptions) {
        info!("Checking CLI options");

        if !cli.iwad.is_empty() && cli.iwad != self.iwad {
            cli.iwad.clone_into(&mut self.iwad);
            info!("IWAD changed to: {}", &cli.iwad);
        } else {
            if self.iwad.is_empty() {
                self.iwad = find_iwad();
            }
            self.iwad.clone_into(&mut cli.iwad);
        }

        if cli.width != 0 && cli.width != self.width {
            self.width = cli.width;
        } else {
            cli.width = self.width;
        }

        if cli.height != 0 && cli.height != self.height {
            self.height = cli.height;
        } else {
            cli.height = self.height;
        }

        if let Some(h) = cli.hi_res {
            if h != self.hi_res {
                self.hi_res = h;
            }
        } else {
            cli.hi_res = Some(self.hi_res);
        }

        if let Some(renderer) = cli.rendering {
            if renderer != self.renderer {
                self.renderer = renderer;
            }
        } else {
            cli.rendering = Some(self.renderer);
        }

        if let Some(wm) = cli.window_mode {
            if wm != self.window_mode {
                self.window_mode = wm;
            }
        } else {
            cli.window_mode = Some(self.window_mode);
        }

        if let Some(v) = cli.vsync {
            if v != self.vsync {
                self.vsync = v;
            }
        } else {
            cli.vsync = Some(self.vsync);
        }

        if cli.refresh_rate != 0 && cli.refresh_rate != self.refresh_rate {
            self.refresh_rate = cli.refresh_rate;
        } else {
            cli.refresh_rate = self.refresh_rate;
        }

        if let Some(f) = cli.music_type {
            if f != self.music_type {
                self.music_type = f;
            }
        } else {
            cli.music_type = Some(self.music_type);
        }

        if let Some(fi) = cli.frame_interpolation {
            if fi != self.frame_interpolation {
                self.frame_interpolation = fi;
            }
        } else {
            cli.frame_interpolation = Some(self.frame_interpolation);
        }

        if let Some(cg) = cli.crt_gamma {
            if cg != self.crt_gamma {
                self.crt_gamma = cg;
            }
        } else {
            cli.crt_gamma = Some(self.crt_gamma);
        }

        if let Some(ref path) = cli.voxels {
            self.voxels_path = path.clone();
            self.voxels = true;
        } else {
            if self.voxels_path.is_empty() {
                self.voxels_path = find_voxel_pk3();
            }
            if !self.voxels_path.is_empty() && self.voxels {
                let p = Path::new(&self.voxels_path);
                let resolved = if p.is_absolute() {
                    self.voxels_path.clone()
                } else {
                    config_dir()
                        .join(&self.voxels_path)
                        .to_string_lossy()
                        .to_string()
                };
                cli.voxels = Some(resolved);
            }
        }
    }
}
