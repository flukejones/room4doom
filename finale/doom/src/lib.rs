mod text;

use crate::text::*;
use gamestate_traits::{GameMode, GameTraits, MachinationTrait, MusTrack, PixelBuf, Scancode};
use hud_util::{load_char_patches, HUDString, HUD_STRING};
use wad::{
    lumps::{WadFlat, WadPalette},
    WadData,
};

pub struct Finale {
    palette: WadPalette,
    screen_width: i32,
    screen_height: i32,
    text: HUDString,
    bg_flat: WadFlat,
}

impl Finale {
    pub fn new(wad: &WadData) -> Self {
        // initialise
        unsafe { load_char_patches(wad) };
        let palette = wad.playpal_iter().next().unwrap();

        let lump = wad.get_lump("FLOOR4_8").unwrap();
        let bg_flat = WadFlat {
            name: "FLOOR4_8".to_string(),
            data: lump.data.clone(),
        };

        Self {
            palette,
            screen_width: 0,
            screen_height: 0,
            text: HUD_STRING,
            bg_flat,
        }
    }
}

impl MachinationTrait for Finale {
    fn init(&mut self, game: &impl GameTraits) {
        let mut name = "FLOOR4_8";

        if game.get_mode() != GameMode::Commercial {
            game.change_music(MusTrack::Victor);
            match game.level_end_info().epsd + 1 {
                1 => {
                    name = "FLOOR4_8";
                    self.text.replace(E1TEXT.to_ascii_uppercase());
                }
                2 => {
                    name = "SFLR6_1";
                    self.text.replace(E2TEXT.to_ascii_uppercase());
                }
                3 => {
                    name = "MFLR8_4";
                    self.text.replace(E3TEXT.to_ascii_uppercase());
                }
                4 => {
                    name = "MFLR8_3";
                    self.text.replace(E4TEXT.to_ascii_uppercase());
                }
                _ => {}
            }
        } else {
            game.change_music(MusTrack::Read_M);
            match game.level_end_info().map {
                6 => {
                    name = "SLIME16";
                    self.text.replace(C1TEXT.to_ascii_uppercase());
                }
                11 => {
                    name = "RROCK14";
                    self.text.replace(C2TEXT.to_ascii_uppercase());
                }
                20 => {
                    name = "RROCK07";
                    self.text.replace(C3TEXT.to_ascii_uppercase());
                }
                30 => {
                    name = "RROCK17";
                    self.text.replace(C4TEXT.to_ascii_uppercase());
                }
                15 => {
                    name = "RROCK13";
                    self.text.replace(C5TEXT.to_ascii_uppercase());
                }
                31 => {
                    name = "RROCK19";
                    self.text.replace(C6TEXT.to_ascii_uppercase());
                }
                _ => {}
            }
        };

        let lump = game.get_wad_data().get_lump(name).unwrap();
        self.bg_flat = WadFlat {
            name: name.to_string(),
            data: lump.data.clone(),
        };
    }

    fn responder(&mut self, _sc: Scancode, _game: &mut impl GameTraits) -> bool {
        false
    }

    fn ticker(&mut self, _game: &mut impl GameTraits) -> bool {
        self.text.inc_current_char();
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut PixelBuf) {
        self.screen_width = buffer.width() as i32;
        self.screen_height = buffer.height() as i32;

        // TODO: Need to draw a flat
        let pal = &self.palette;
        for sx in (0..self.screen_width).step_by(64) {
            for sy in (0..self.screen_height).step_by(64) {
                for (y, col) in self.bg_flat.data.chunks(64).enumerate() {
                    for (x, c) in col.iter().enumerate() {
                        let c = &pal.0[*c as usize];
                        buffer.set_pixel(sx as usize + x, sy as usize + y, c.r, c.g, c.b, 255);
                    }
                }
            }
        }
        self.text.draw(4, 4, self, buffer);
    }
}
