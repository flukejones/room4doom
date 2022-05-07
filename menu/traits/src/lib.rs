pub use gameplay::{GameMode, Skill};
pub use render_traits::PixelBuf;
pub use sdl2::keyboard::Scancode;
use sound_traits::{MusEnum, SfxNum, SoundAction};

/// To be implemented by the Game
pub trait MenuFunctions {
    fn defered_init_new(&mut self, skill: Skill, episode: i32, map: i32);

    fn get_mode(&mut self) -> GameMode;

    fn load_game(&mut self, name: String);

    fn save_game(&mut self, name: String, slot: usize);

    fn toggle_pause_game(&mut self);

    fn quit_game(&mut self);

    fn start_sound(&mut self, sfx: SfxNum);
}

/// To be implemented by the Menu
pub trait MenuDraw {
    /// Draw game-exe menus on top of the `PixelBuf`.
    fn render_menu(&mut self, buffer: &mut PixelBuf);
}

/// To be implemented by the Menu
pub trait MenuTicker {
    fn ticker(&mut self, game: &mut impl MenuFunctions) -> bool;
}

/// To be implemented by the Menu
pub trait MenuResponder {
    // Return true if the responder took the event
    fn responder(&mut self, sc: Scancode, game: &mut impl MenuFunctions) -> bool;
}
