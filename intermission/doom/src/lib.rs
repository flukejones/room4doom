use crate::defs::{MAP_POINTS, SHOW_NEXT_LOC_DELAY, TICRATE};
use game_traits::{
    GameMode, GameTraits, MachinationTrait, PixelBuf, Scancode, WBPlayerStruct, WBStartStruct,
};
use log::info;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

mod defs;

const EP4_BG: &str = "INTERPIC";
const COMMERCIAL_BG: &str = "INTERPIC";
const TITLE_Y: i32 = 2;

pub struct Intermission {
    palette: WadPalette,
    bg_patches: Vec<WadPatch>,
    yah_patches: Vec<WadPatch>,
    /// 0 or 1 (left/right). Splat is 2
    yah_idx: usize,
    level_names: Vec<Vec<WadPatch>>,
    current_bg: usize,
    next_level: bool,
    mode: GameMode,
    // info updated by ticker
    player_info: WBPlayerStruct,
    level_info: WBStartStruct,

    pointer_on: bool,
    count: i32,
    enter: WadPatch,
}

impl Intermission {
    pub fn new(mode: GameMode, wad: &WadData) -> Self {
        let palette = wad.playpal_iter().next().unwrap();

        let mut level_names = Vec::new();
        let mut bg_patches = Vec::new();
        let mut yah_patches = Vec::new();
        if mode == GameMode::Commercial {
            let lump = wad.get_lump(COMMERCIAL_BG).unwrap();
            bg_patches.push(WadPatch::from_lump(lump));
        } else {
            for e in 0..3 {
                let lump = wad.get_lump(&format!("WIMAP{e}")).unwrap();
                bg_patches.push(WadPatch::from_lump(lump));

                let mut names_patches = Vec::new();
                for m in 0..9 {
                    let name = format!("WILV{e}{m}");
                    let lump = wad.get_lump(&name).unwrap();
                    names_patches.push(WadPatch::from_lump(lump));
                }
                level_names.push(names_patches);
            }

            let lump = wad.get_lump("WIURH0").unwrap();
            yah_patches.push(WadPatch::from_lump(lump));
            let lump = wad.get_lump("WIURH1").unwrap();
            yah_patches.push(WadPatch::from_lump(lump));
            let lump = wad.get_lump("WISPLAT").unwrap();
            yah_patches.push(WadPatch::from_lump(lump));
        }

        if mode == GameMode::Retail {
            let lump = wad.get_lump(EP4_BG).unwrap();
            bg_patches.push(WadPatch::from_lump(lump));

            let mut names_patches = Vec::new();
            for m in 0..9 {
                let name = format!("WILV3{m}");
                let lump = wad.get_lump(&name).unwrap();
                names_patches.push(WadPatch::from_lump(lump));
            }
            level_names.push(names_patches);
        }

        let lump = wad.get_lump("WIENTER").unwrap();
        let enter = WadPatch::from_lump(lump);

        Self {
            palette,
            bg_patches,
            level_names,
            yah_patches,
            yah_idx: 0,
            current_bg: 0,
            next_level: false,
            mode,
            player_info: WBPlayerStruct::default(),
            level_info: WBStartStruct::default(),
            pointer_on: true,
            count: SHOW_NEXT_LOC_DELAY * TICRATE,
            enter,
        }
    }

    fn init_no_state(&mut self) {
        self.count = 10;
    }

    fn update_show_next_loc(&mut self) {
        self.count -= 1;
        if self.count <= 0 {
            self.init_no_state();
        } else {
            self.pointer_on = (self.count & 31) < 20;
        }
    }

    fn draw_on_lnode(&self, lv: usize, patch: &WadPatch, buffer: &mut PixelBuf) {
        let ep = self.level_info.epsd as usize;
        let point = MAP_POINTS[ep][lv];

        let x = point.0 - patch.left_offset as i32;
        let y = point.1 - patch.top_offset as i32;

        self.draw_patch(patch, x, y, buffer);
    }

    fn draw_enter_level(&self, buffer: &mut PixelBuf) {
        let mut y = TITLE_Y;
        self.draw_patch(&self.enter, 160 - self.enter.width as i32 / 2, y, buffer);
        y += (5 * self.enter.height as i32) / 4;
        let ep = self.level_info.epsd as usize;
        let patch = &self.level_names[ep][self.level_info.next as usize];
        self.draw_patch(patch, 160 - patch.width as i32 / 2, y, buffer);
    }

    fn draw_next_loc(&self, buffer: &mut PixelBuf) {
        // Background
        self.draw_patch(&self.bg_patches[self.current_bg], 0, 0, buffer);

        if self.mode != GameMode::Commercial {
            if self.level_info.epsd > 2 {
                return;
            }
            let last = if self.level_info.last == 8 {
                self.level_info.next - 1
            } else {
                self.level_info.next
            };

            for i in 0..last {
                self.draw_on_lnode(i as usize, &self.yah_patches[2], buffer);
            }

            if self.level_info.didsecret {
                self.draw_on_lnode(8, &self.yah_patches[2], buffer);
            }

            if self.pointer_on {
                let next_level = self.level_info.next as usize;
                self.draw_on_lnode(next_level, &self.yah_patches[self.yah_idx], buffer);
            }
        }

        if self.mode != GameMode::Commercial || self.level_info.next != 30 {
            self.draw_enter_level(buffer);
        }
    }
}

impl MachinationTrait for Intermission {
    fn responder(&mut self, sc: Scancode, _game: &mut impl GameTraits) -> bool {
        if sc == Scancode::Return || sc == Scancode::Space {
            self.next_level = true;
            return true;
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

        self.current_bg = level.epsd as usize;
        self.player_info = player.clone();
        self.level_info = level.clone();
        self.update_show_next_loc();
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut PixelBuf) {
        // TODO: stats and next are two different screens.
        self.draw_next_loc(buffer);
    }
}
