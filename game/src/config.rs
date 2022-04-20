use crate::{
    input::InputConfig,
    log,
    log::{error, warn},
    GameOptions, Shaders, BASE_DIR,
};
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::{
    fs::{create_dir, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

fn get_cfg_file() -> PathBuf {
    let mut dir = config_dir().unwrap_or_else(|| panic!("Couldn't open user config dir"));
    dir.push(BASE_DIR);
    if !dir.exists() {
        create_dir(&dir).unwrap_or_else(|e| panic!("Couldn't create {:?}: {}", dir, e));
    }
    dir.push("user.toml");
    dir
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    pub iwad: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub shader: Option<Shaders>,
    pub input: InputConfig,
}

impl UserConfig {
    /// `load` will attempt to read the config, and panic if the dir is missing
    pub fn load() -> Self {
        if !get_cfg_file().exists() {}

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(get_cfg_file())
            .unwrap_or_else(|_| panic!("The file {:?} is missing", get_cfg_file())); // okay to cause panic here
        let mut buf = String::new();
        if let Ok(read_len) = file.read_to_string(&mut buf) {
            if read_len == 0 {
                return UserConfig::create_default(&mut file);
            } else {
                if let Ok(data) = toml::from_str(&buf) {
                    return data;
                }
                warn!(
                    "Could not deserialise {:?} recreating config",
                    get_cfg_file()
                );
            }
        }
        UserConfig::create_default(&mut file)
    }

    fn create_default(file: &mut File) -> Self {
        // create a default config here
        let mut config = UserConfig::default();
        // Should be okay to unwrap this as is since it is a Default
        let json = toml::to_string(&config).unwrap();
        file.write_all(json.as_bytes())
            .unwrap_or_else(|_| panic!("Could not write {:?}", get_cfg_file()));
        config
    }

    pub fn read(&mut self) {
        let mut file = OpenOptions::new()
            .read(true)
            .open(get_cfg_file())
            .unwrap_or_else(|err| panic!("Error reading {:?}: {}", get_cfg_file(), err));
        let mut buf = String::new();
        if let Ok(l) = file.read_to_string(&mut buf) {
            if l == 0 {
                warn!("File is empty {:?}", get_cfg_file());
            } else {
                let x: UserConfig = toml::from_str(&buf)
                    .unwrap_or_else(|_| panic!("Could not deserialise {:?}", get_cfg_file()));
                *self = x;
            }
        }
    }

    pub fn write(&self) {
        let mut file = File::create(get_cfg_file()).expect("Couldn't overwrite config");
        let json = toml::to_string_pretty(self).expect("Parse config to JSON failed");
        file.write_all(json.as_bytes())
            .unwrap_or_else(|err| error!("Could not write config: {}", err));
    }

    /// Sync the CLI options and UserOptions with each other
    pub fn sync_cli(&mut self, cli: &mut GameOptions) {
        if !cli.iwad.is_empty() && cli.iwad != self.iwad {
            self.iwad = cli.iwad.clone();
        } else {
            cli.iwad = self.iwad.clone();
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

        if cli.shader.is_some() && cli.shader != self.shader {
            self.shader = cli.shader;
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
    }
}
