//! A single trait to enable generic rendering impl

use gameplay::{Level, Player};
use sdl2::{render::Canvas, surface::Surface};

pub trait Renderer {
    /// This function is responsible for drawing the full player view to the SDL2
    /// `Surface`.
    ///
    /// Doom function name `R_RenderPlayerView`
    fn render_player_view(&mut self, player: &Player, level: &Level, canvas: &mut Canvas<Surface>);
}
