//! User configuration options.

use crate::{BASE_DIR, CLIOptions};
use dirs::config_dir;
use gameplay::log::{error, info, warn};
use input::config::InputConfig;
use nanoserde::{DeRon, SerRon};
use render_target::shaders::Shaders;
use sound_sdl2::timidity::GusMemSize;
use std::fs::{File, OpenOptions, create_dir};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;

const LOG_TAG: &str = "UserConfig";

fn get_cfg_file() -> PathBuf {
    let mut dir =
        config_dir().unwrap_or_else(|| panic!("{}: Couldn't open user config dir", LOG_TAG));
    dir.push(BASE_DIR);
    if !dir.exists() {
        create_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: Couldn't create {:?}: {}", LOG_TAG, dir, e));
    }
    dir.push("user.toml");
    dir
}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy, DeRon, SerRon)]
pub enum RenderType {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    #[default]
    Software,
    /// Software framebuffer blitted to screen using OpenGL (and can use
    /// shaders)
    SoftOpenGL,
    /// OpenGL
    OpenGL,
    /// Vulkan
    Vulkan,
}

impl FromStr for RenderType {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "software" => Ok(Self::Software),
            "softopengl" => Ok(Self::SoftOpenGL),
            "cgwg" => Ok(Self::OpenGL),
            "basic" => Ok(Self::Vulkan),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Invalid rendering type",
            )),
        }
    }
}

impl Into<render_target::RenderApiType> for RenderType {
    fn into(self) -> render_target::RenderApiType {
        match self {
            RenderType::Software => render_target::RenderApiType::Software,
            RenderType::SoftOpenGL => render_target::RenderApiType::SoftOpenGL,
            RenderType::OpenGL => render_target::RenderApiType::OpenGL,
            RenderType::Vulkan => render_target::RenderApiType::Vulkan,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, DeRon, SerRon)]
pub enum MusicType {
    FluidSynth,
    #[default]
    Timidity,
    OPL2,
    OPL3,
}

impl FromStr for MusicType {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "timidity" => Ok(Self::Timidity),
            "fluidsynth" => Ok(Self::FluidSynth),
            "opl2" => Ok(Self::OPL2),
            "opl3" => Ok(Self::OPL3),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
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
    pub fullscreen: bool,
    pub hi_res: bool,
    pub renderer: RenderType,
    pub shader: Option<Shaders>,
    pub sfx_vol: i32,
    pub mus_vol: i32,
    pub music_type: MusicType,
    pub gus_mem_size: GusMemSize,
    pub input: InputConfig,
}

impl UserConfig {
    /// `load` will attempt to read the config, and panic if errored
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
                    info!(target: LOG_TAG, "Loaded user config file");
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
            fullscreen: true,
            sfx_vol: 80,
            mus_vol: 70,
            ..UserConfig::default()
        };
        info!("Created default user config file");
        // Should be okay to unwrap this as is since it is a Default
        let data = config.serialize_ron();
        file.write_all(data.as_bytes())
            .unwrap_or_else(|_| panic!("Could not write {:?}", get_cfg_file()));
        info!("Saved user config to {:?}", get_cfg_file());
        config
    }

    pub fn write(&self) {
        let mut file = File::create(get_cfg_file()).expect("Couldn't overwrite config");
        let data = self.serialize_ron();
        file.write_all(data.as_bytes())
            .unwrap_or_else(|err| error!("Could not write config: {}", err));
    }

    /// Sync the CLI options and UserOptions with each other
    pub fn sync_cli(&mut self, cli: &mut CLIOptions) {
        info!("Checking CLI options");

        if !cli.iwad.is_empty() && cli.iwad != self.iwad {
            cli.iwad.clone_into(&mut self.iwad);
            info!("IWAD changed to: {}", &cli.iwad);
        } else {
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

        let hi_res = cli.hi_res && !cli.lo_res;
        if hi_res != self.hi_res {
            self.hi_res = hi_res;
        } else {
            cli.hi_res = self.hi_res;
        }

        if let Some(renderer) = cli.rendering {
            if renderer != self.renderer {
                self.renderer = renderer;
            }
        } else {
            cli.rendering = Some(self.renderer);
        }

        if cli.shader.is_some() {
            if cli.shader != self.shader {
                self.shader = cli.shader;
            }
        } else {
            cli.shader = self.shader;
        }

        if let Some(f) = cli.fullscreen {
            if f != self.fullscreen {
                self.fullscreen = f;
            }
        } else {
            cli.fullscreen = Some(self.fullscreen);
        }

        if let Some(f) = cli.music_type {
            if f != self.music_type {
                self.music_type = f;
            }
        } else {
            cli.music_type = Some(self.music_type);
        }
    }
}
