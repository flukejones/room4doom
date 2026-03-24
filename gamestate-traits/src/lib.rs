//! Exposes an API of sorts that allows crates for things like statusbar and
//! intermission screens to get certain information they require or cause a
//! gamestate change.

pub mod keys;

pub use keys::{KeyCode, MouseBtn};

use game_config::{GameMode, Skill};
use gameplay::{MAXPLAYERS, PlayerStatus, WorldEndPlayerInfo};
use render_common::DrawBuffer;
use sound_common::{MusTrack, SfxName};
use wad::WadData;
use wad::types::WadPalette;

/// The current state of the game-exe: whether we are playing, gazing at the
/// intermission screen, the game-exe final animation, or a demo.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameState {
    ForceWipe = -1,
    Level,
    Intermission,
    Finale,
    DemoScreen,
}

/// parms for world level / intermission
#[derive(Default, Clone)]
pub struct WorldInfo {
    pub episode: usize,
    pub map: usize,
    pub didsecret: bool,
    pub last: usize,
    pub next: usize,
    pub maxkills: i32,
    pub maxitems: i32,
    pub maxsecret: i32,
    pub maxfrags: i32,
    pub partime: i32,
    pub pnum: usize,
    pub plyr: [WorldEndPlayerInfo; MAXPLAYERS],
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(usize)]
pub enum ConfigKey {
    SfxVolume,
    MusVolume,
    MusicType,
    WindowMode,
    VSync,
    Renderer,
    HiRes,
    FrameInterpolation,
    Voxels,
    CrtGamma,
    ShowFps,
    MenuDim,
    HudSize,
    HudWidth,
    HudMsgMode,
    HudMsgTime,
    HealthVignette,
    MouseSensitivity,
    InvertY,
    KeyCount,
}

pub trait GameTraits {
    fn defered_init_new(&mut self, skill: Skill, episode: usize, map: usize);
    fn get_mode(&self) -> GameMode;
    fn game_state(&self) -> GameState;
    fn read_save_descriptions(&self) -> Vec<Option<String>>;
    fn load_game(&mut self, name: String);
    fn save_game(&mut self, name: String, description: String);
    fn toggle_pause_game(&mut self);
    fn quit_game(&mut self);
    fn start_sound(&mut self, sfx: SfxName);
    fn change_music(&self, music: MusTrack);
    fn change_music_by_lump(&self, lump_name: &str);
    fn level_done(&mut self);
    fn finale_done(&mut self);
    fn start_title(&mut self);
    fn level_end_info(&self) -> &WorldInfo;
    fn player_end_info(&self) -> &WorldEndPlayerInfo;
    fn player_status(&self) -> PlayerStatus;
    fn player_msg_take(&mut self) -> Option<String>;
    fn get_wad_data(&self) -> &WadData;
}

pub trait ConfigTraits {
    fn config_value(&self, key: ConfigKey) -> i32;
    fn set_config_value(&mut self, key: ConfigKey, val: i32);
    fn mark_config_changed(&mut self);
    fn is_config_dirty(&self) -> bool;
    fn clear_config_dirty(&mut self);
    fn config_snapshot(&self) -> [i32; ConfigKey::KeyCount as usize];
}

pub trait SubsystemTrait {
    fn init<T: GameTraits + ConfigTraits>(&mut self, game: &T);
    fn responder<T: GameTraits + ConfigTraits>(&mut self, sc: KeyCode, game: &mut T) -> bool;
    fn ticker<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) -> bool;
    fn get_palette(&self) -> &WadPalette;
    fn draw(&mut self, buffer: &mut impl DrawBuffer);
}
