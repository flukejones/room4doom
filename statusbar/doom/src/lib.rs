use game_traits::{
    util::{draw_num, get_num_sprites},
    AmmoType, GameMode, GameTraits, MachinationTrait, PixelBuf, PlayerInfo, Scancode, WeaponType,
    WEAPON_INFO,
};
use log::info;
use std::collections::HashMap;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

const SCREEN_WIDTH: i32 = 320;
const SCREEN_HEIGHT: i32 = 200;

pub struct Statusbar {
    mode: GameMode,
    palette: WadPalette,
    patches: HashMap<&'static str, WadPatch>,
    /// Nums, index is the actual number
    big_nums: [WadPatch; 10],
    big_percent: WadPatch,
    lil_nums: [WadPatch; 10],
    lil_percent: WadPatch,
    grey_nums: [WadPatch; 10],
    yell_nums: [WadPatch; 10],
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
            mode,
            palette,
            patches,
            big_nums: get_num_sprites("STTNUM", 0, wad),
            big_percent: WadPatch::from_lump(wad.get_lump("STTPRCNT").unwrap()),
            lil_nums: get_num_sprites("STCFN0", 48, wad),
            lil_percent: WadPatch::from_lump(wad.get_lump("STCFN037").unwrap()),
            grey_nums: get_num_sprites("STGNUM", 0, wad),
            yell_nums: get_num_sprites("STYSNUM", 0, wad),
            info: PlayerInfo::default(),
        }
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .expect(&format!("{name} not in cache"))
    }

    fn draw_percent(&self, big: bool, p: u32, x: i32, y: i32, buffer: &mut PixelBuf) {
        if big {
            self.draw_patch(&self.big_percent, x, y, buffer);
            draw_num(p, x, y, 0, &self.big_nums, self, buffer);
        } else {
            self.draw_patch(&self.lil_percent, x, y, buffer);
            draw_num(p, x, y, 0, &self.lil_nums, self, buffer);
        }
    }

    fn draw_health(&self, big: bool, buffer: &mut PixelBuf) {
        let nums = if big { &self.big_nums } else { &self.lil_nums };

        let mut y = nums[0].height as i32;
        let mut x = nums[0].width as i32;
        if !big {
            y = y * 2 + 2;
            x *= 5;
        } else {
            x *= 4;
        }

        let h = if self.info.health < 0 {
            0
        } else {
            self.info.health as u32
        };
        self.draw_percent(big, h, x, SCREEN_HEIGHT - 2 - y, buffer)
    }

    fn draw_armour(&self, big: bool, buffer: &mut PixelBuf) {
        let nums = if big { &self.big_nums } else { &self.lil_nums };

        let mut y = nums[0].height as i32;
        let mut x = nums[0].width as i32;
        if !big {
            y = y + 1;
            x *= 5;
        } else {
            x *= 4;
        }

        let h = if self.info.armour < 0 {
            0
        } else {
            self.info.armour as u32
        };
        if big {
            self.draw_percent(big, h, SCREEN_WIDTH - x * 2, SCREEN_HEIGHT - 2 - y, buffer)
        } else {
            self.draw_percent(big, h, x, SCREEN_HEIGHT - 2 - y, buffer)
        }
    }

    fn draw_ammo_big(&self, buffer: &mut PixelBuf) {
        if matches!(self.info.readyweapon, WeaponType::NoChange) {
            return;
        }
        if !(self.mode == GameMode::Commercial) && self.info.readyweapon == WeaponType::SuperShotgun
        {
            return;
        }

        let ammo = WEAPON_INFO[self.info.readyweapon as usize].ammo;
        if ammo == AmmoType::NoAmmo {
            return;
        }

        let height = self.big_nums[0].height as i32;
        let width = self.big_nums[0].width as i32;
        let ammo = self.info.ammo[ammo as usize];
        draw_num(
            ammo,
            SCREEN_WIDTH - width * 2,
            SCREEN_HEIGHT - 2 - height,
            0,
            &self.big_nums,
            self,
            buffer,
        );
    }

    fn draw_weapons(&self, buffer: &mut PixelBuf) {
        let y = self.grey_nums[0].height as i32;
        let x = SCREEN_WIDTH - self.grey_nums[0].width as i32;
        let start_y = SCREEN_HEIGHT - y - 2;

        for (i, owned) in self.info.weaponowned.iter().enumerate() {
            if !(self.mode == GameMode::Commercial) && i == 8 {
                continue;
            }
            let nums = if self.info.readyweapon as usize == i {
                &self.yell_nums
            } else {
                &self.grey_nums
            };
            if *owned {
                draw_num(i as u32, x, start_y - y * i as i32, 0, nums, self, buffer);
            }
        }
    }

    fn draw_face(&self, upper: bool, buffer: &mut PixelBuf) {
        let patch = self.get_patch("STFB1");
        let offset_x = patch.width as i32 / 2;
        let offset_y = patch.height as i32;
        let mut y = 0;
        if !upper {
            y = SCREEN_HEIGHT - offset_y
        };
        self.draw_patch(patch, SCREEN_WIDTH / 2 - offset_x, y, buffer);

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
        if upper {
            y = 1;
        } else {
            y = SCREEN_HEIGHT - offset_y
        };
        self.draw_patch(patch, SCREEN_WIDTH / 2 - offset_x, y, buffer);
    }
}

impl MachinationTrait for Statusbar {
    fn init(&mut self, _game: &impl GameTraits) {}

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
        //self.draw_face(false, buffer);
        self.draw_health(false, buffer);
        self.draw_armour(false, buffer);
        self.draw_ammo_big(buffer);
        self.draw_weapons(buffer);
    }
}
