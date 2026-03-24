mod text;

use game_config::GameMode;
use gameplay::TICRATE;
use gamestate_traits::{ConfigTraits, GameTraits, KeyCode, SubsystemTrait};
use hud_util::{HUD_STRING, HUDString, hud_scale, load_char_patches};
use render_common::DrawBuffer;
use sound_common::MusTrack;
use text::*;
use wad::WadData;
use wad::types::{WadFlat, WadPalette};

pub struct Finale {
    palette: WadPalette,
    screen_width: i32,
    screen_height: i32,
    text: HUDString,
    bg_flat: WadFlat,
    count: i32,
}

impl Finale {
    pub fn new(wad: &WadData) -> Self {
        // initialise
        load_char_patches(wad);
        let palette = wad.lump_iter::<WadPalette>("PLAYPAL").next().unwrap();

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
            count: 0,
        }
    }

    fn draw_pixels(&mut self, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);
        self.screen_width = pixels.size().width();
        self.screen_height = pixels.size().height();

        let pal = &self.palette;
        for tile_x in (0..self.screen_width).step_by(64) {
            for tile_y in (0..self.screen_height).step_by(64) {
                for (y, col) in self.bg_flat.data.chunks(64).enumerate() {
                    for (x, c) in col.iter().enumerate() {
                        let c = pal.0[*c as usize];
                        pixels.set_pixel(tile_x as usize + x, tile_y as usize + y, c);
                    }
                }
            }
        }
        let x_ofs = (pixels.size().width_f32() - 320.0 * sx) / 2.0;
        self.text
            .draw_pixels(x_ofs + 6.0 * sx, 6.0 * sy, &self.palette, pixels);
    }
}

impl SubsystemTrait for Finale {
    fn init<T: GameTraits + ConfigTraits>(&mut self, game: &T) {
        let mut name = "FLOOR4_8";
        self.count = 20 * TICRATE;

        if game.get_mode() != GameMode::Commercial {
            game.change_music(MusTrack::Victor);
            match game.level_end_info().episode + 1 {
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
            match game.level_end_info().last {
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

    fn responder<T: GameTraits + ConfigTraits>(&mut self, sc: KeyCode, _game: &mut T) -> bool {
        if sc == KeyCode::Return || sc == KeyCode::Space {
            if !self.text.is_at_end() {
                self.text.set_draw_all();
            } else {
                self.count = 0;
            }
            return true;
        }
        false
    }

    fn ticker<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) -> bool {
        self.text.inc_current_char();
        self.count -= 1;
        if self.count <= 0
            && game.get_mode() == GameMode::Commercial
            && game.level_end_info().last != 30
        {
            game.finale_done();
        }
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        self.draw_pixels(buffer);
    }
}
