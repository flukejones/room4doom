use doom_lib::{Level, Player};
use sdl2::{render::Canvas, surface::Surface};

pub mod software;

pub trait Renderer {
    /// This function is responsible for drawing the full player view to the SDL2
    /// `Surface`.
    ///
    /// Doom function name `R_RenderPlayerView`
    fn render_player_view(&mut self, player: &Player, level: &Level, canvas: &mut Canvas<Surface>);
}
