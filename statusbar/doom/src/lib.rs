use game_traits::{GameMode, GameTraits, MachinationTrait, PixelBuf, PlayerInfo, Scancode};
use log::info;
use std::collections::HashMap;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

pub struct Statusbar {
    palette: WadPalette,
    patches: HashMap<&'static str, WadPatch>,
    info: PlayerInfo,
}

impl Statusbar {
    pub fn new(mode: GameMode, wad: &WadData) -> Self {
        let palette = wad.playpal_iter().next().unwrap();

        let mut patches = HashMap::new();
        for p in ["STFST01", "STFST11", "STFST21", "STFST31", "STFST41"] {
            let lump = wad.get_lump(p).unwrap();
            patches.insert(p, WadPatch::from_lump(lump));
        }

        let lump = wad.get_lump("STFB1").unwrap();
        patches.insert("STFB1", WadPatch::from_lump(lump));

        Self {
            palette,
            patches,
            info: PlayerInfo::default(),
        }
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .expect(&format!("{name} not in cache"))
    }
}

impl MachinationTrait for Statusbar {
    fn responder(&mut self, _sc: Scancode, _game: &mut impl GameTraits) -> bool {
        false
    }

    fn ticker(&mut self, game: &mut impl GameTraits) -> bool {
        self.info = game.player_info();
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut PixelBuf) {
        let patch = self.get_patch("STFB1");
        let offset_x = patch.width as i32 / 2;
        let offset_y = patch.height as i32;
        self.draw_patch(patch, 160 - offset_x, 200 - offset_y, buffer);

        let patch = if self.info.health < 20 {
            self.get_patch("STFST41")
        } else if self.info.health < 40 {
            self.get_patch("STFST31")
        } else if self.info.health < 60 {
            self.get_patch("STFST21")
        } else if self.info.health < 80 {
            self.get_patch("STFST11")
        } else {
            self.get_patch("STFST01")
        };

        let offset_x = patch.width as i32 / 2;
        let offset_y = patch.height as i32;
        self.draw_patch(patch, 160 - offset_x, 200 - offset_y, buffer);
    }
}
