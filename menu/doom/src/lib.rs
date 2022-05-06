use menu_traits::{MenuDraw, MenuFunctions, MenuResponder, MenuTicker, PixelBuf, Scancode};

pub struct MenuDoom {
    pause: bool,
}

impl MenuDoom {
    pub fn new() -> Self {
        Self { pause: false }
    }
}

impl MenuResponder for MenuDoom {
    fn responder(&mut self, sc: Scancode, game: &mut impl MenuFunctions) -> bool {
        let mut res = false;
        if sc == Scancode::Escape {
            res = true;
            game.quit_game();
        }
        if sc == Scancode::P {
            res = true;
            self.pause = !self.pause;
            game.pause_game(self.pause);
        }
        res
    }
}

impl MenuTicker for MenuDoom {
    fn ticker(&mut self) {}
}

impl MenuDraw for MenuDoom {
    fn render_menu(&mut self, buffer: &mut PixelBuf) {
        if self.pause {
            // for y in 0..200 {
            //     for x in 0..320 {
            //         buffer.set_pixel(x, y, 255, 255, 255, 255);
            //     }
            // }
        }
    }
}
