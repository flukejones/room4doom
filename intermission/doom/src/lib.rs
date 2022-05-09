use game_traits::{GameMode, GameTraits, MachinationTrait, PixelBuf, Scancode};
use log::info;
use std::collections::HashMap;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

const EP1_BG: &str = "WIMAP0";
const EP2_BG: &str = "WIMAP1";
const EP3_BG: &str = "WIMAP2";
const EP4_BG: &str = "INTERPIC";
const COMMERCIAL_BG: &str = "INTERPIC";

pub struct Intermission {
    palette: WadPalette,
    patches: HashMap<&'static str, WadPatch>,
    bg: &'static str,
    next_level: bool,
}

impl Intermission {
    pub fn new(mode: GameMode, wad: &WadData) -> Self {
        let palette = wad.playpal_iter().next().unwrap();

        let bg;
        let mut patches = HashMap::new();
        if mode == GameMode::Commercial {
            let lump = wad.get_lump(COMMERCIAL_BG).unwrap();
            patches.insert(COMMERCIAL_BG, WadPatch::from_lump(lump));
            bg = COMMERCIAL_BG;
        } else {
            let lump = wad.get_lump(EP1_BG).unwrap();
            patches.insert(EP1_BG, WadPatch::from_lump(lump));
            let lump = wad.get_lump(EP2_BG).unwrap();
            patches.insert(EP2_BG, WadPatch::from_lump(lump));
            let lump = wad.get_lump(EP3_BG).unwrap();
            patches.insert(EP3_BG, WadPatch::from_lump(lump));
            bg = EP1_BG;
        }
        if mode == GameMode::Retail {
            let lump = wad.get_lump(EP4_BG).unwrap();
            patches.insert(EP4_BG, WadPatch::from_lump(lump));
        }

        Self {
            palette,
            patches,
            bg,
            next_level: false,
        }
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .expect(&format!("{name} not in cache"))
    }
}

impl MachinationTrait for Intermission {
    fn responder(&mut self, sc: Scancode, _game: &mut impl GameTraits) -> bool {
        if sc == Scancode::Return {
            self.next_level = true;
        }
        false
    }

    fn ticker(&mut self, game: &mut impl GameTraits) -> bool {
        let player = game.player_end_info();
        let level = game.level_end_info();
        if self.next_level {
            info!("Player: Total Items: {}/{}", player.sitems, level.maxitems);
            info!("Player: Total Kills: {}/{}", player.skills, level.maxkills);
            info!(
                "Player: Total Secrets: {}/{}",
                player.ssecret, level.maxsecret
            );
            info!("Player: Level Time: {}", player.stime);

            game.world_done();
            self.next_level = false;
            return true;
        }

        match level.epsd {
            0 => self.bg = EP1_BG,
            1 => self.bg = EP2_BG,
            2 => self.bg = EP3_BG,
            _ => {
                self.bg = COMMERCIAL_BG;
            }
        }
        if game.get_mode() == GameMode::Commercial {
            self.bg = COMMERCIAL_BG;
        }

        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut PixelBuf) {
        self.draw_patch(self.get_patch(self.bg), 0, 0, buffer);
    }
}
