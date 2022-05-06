use gameplay::Skill;
pub use render_traits::PixelBuf;
pub use sdl2::keyboard::Scancode;

/// To be implemented by the Game
pub trait MenuFunctions {
    fn defered_init_new(&mut self, skill: Skill, episode: i32, map: i32);

    fn load_game(&mut self, name: String);

    fn save_game(&mut self, name: String, slot: usize);

    fn pause_game(&mut self, pause: bool);

    fn quit_game(&mut self);
}

/// To be implemented by the Menu
pub trait MenuDraw {
    /// Draw game-exe menus on top of the `PixelBuf`.
    fn render_menu(&mut self, buffer: &mut PixelBuf);
}

/// To be implemented by the Menu
pub trait MenuTicker {
    fn ticker(&mut self, game: &mut impl MenuFunctions);
}

/// To be implemented by the Menu
pub trait MenuResponder {
    // Return true if the responder took the event
    fn responder(&mut self, sc: Scancode, game: &mut impl MenuFunctions) -> bool;
}
