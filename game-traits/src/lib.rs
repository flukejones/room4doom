pub use gameplay::{AmmoType, Card, GameMode, Skill, WBPlayerStruct, WBStartStruct, WeaponType};
pub use render_traits::PixelBuf;
pub use sdl2::keyboard::Scancode;
use sound_traits::{MusEnum, SfxNum};
use wad::lumps::{WadPalette, WadPatch};

/// The current state of the game-exe: whether we are playing, gazing at the intermission screen,
/// the game-exe final animation, or a demo.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameState {
    ForceWipe = -1,
    Level,
    Intermission,
    Finale,
    Demo,
}

#[derive(Debug, Default, Clone)]
pub struct PlayerInfo {
    pub attackdown: bool,
    pub readyweapon: WeaponType,
    pub health: i32,
    pub armour: i32,
    pub armour_type: i32,
    pub cards: [bool; Card::NumCards as usize],
    pub weaponowned: [bool; WeaponType::NumWeapons as usize],
    pub ammo: [u32; AmmoType::NumAmmo as usize],
    pub maxammo: [u32; AmmoType::NumAmmo as usize],
}

/// Universal game traits. To be implemented by the Game
pub trait GameTraits {
    fn defered_init_new(&mut self, skill: Skill, episode: i32, map: i32);

    fn get_mode(&mut self) -> GameMode;

    fn load_game(&mut self, name: String);

    fn save_game(&mut self, name: String, slot: usize);

    fn toggle_pause_game(&mut self);

    fn quit_game(&mut self);

    fn start_sound(&mut self, sfx: SfxNum);

    fn change_music(&mut self, music: MusEnum);

    fn set_game_state(&mut self, state: GameState);

    fn get_game_state(&mut self);

    fn world_done(&mut self);

    fn level_end_info(&self) -> &WBStartStruct;

    fn player_end_info(&self) -> &WBPlayerStruct;

    fn player_info(&self) -> PlayerInfo;

    // TODO: get and set settings Struct
}

/// To be implemented by machination type things (HUD, Map, Statusbar)
pub trait MachinationTrait {
    /// Return true if the responder took the event
    fn responder(&mut self, sc: Scancode, game: &mut impl GameTraits) -> bool;

    /// Responds to changes in the game or affects game.
    fn ticker(&mut self, game: &mut impl GameTraits) -> bool;

    fn get_palette(&self) -> &WadPalette;

    /// Draw this Machination to the `PixelBuf`.
    fn draw(&mut self, buffer: &mut PixelBuf);

    /// Free method, requires `get_palette()` to be implemented
    fn draw_patch(&self, patch: &WadPatch, x: i32, y: i32, pixels: &mut PixelBuf) {
        let mut xtmp = 0;
        for c in patch.columns.iter() {
            for (ytmp, p) in c.pixels.iter().enumerate() {
                let colour = self.get_palette().0[*p];

                pixels.set_pixel(
                    (x + xtmp as i32) as usize, // - (image.left_offset as i32),
                    (y + ytmp as i32 + c.y_offset as i32) as usize, // - image.top_offset as i32 - 30,
                    colour.r,
                    colour.g,
                    colour.b,
                    255,
                );
            }
            if c.y_offset == 255 {
                xtmp += 1;
            }
        }
    }
}
