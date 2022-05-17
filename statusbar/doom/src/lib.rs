//! A custom statusbar to show the players status during gameplay.
//!
//! Displays things like ammo count, weapons owned, key/skulls owned, health and so on.

use gamestate_traits::{
    util::{draw_num, get_num_sprites, get_st_key_sprites},
    AmmoType, GameMode, GameTraits, MachinationTrait, PixelBuf, PlayerStatus, Scancode, WeaponType,
    WEAPON_INFO,
};
use std::collections::HashMap;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

pub struct Statusbar {
    screen_width: i32,
    screen_height: i32,
    mode: GameMode,
    palette: WadPalette,
    patches: HashMap<&'static str, WadPatch>,
    /// Nums, index is the actual number
    big_nums: [WadPatch; 10],
    lil_nums: [WadPatch; 10],
    grey_nums: [WadPatch; 10],
    yell_nums: [WadPatch; 10],
    /// Keys: blue yellow red. Skulls: blue yellow red
    keys: [WadPatch; 6],
    status: PlayerStatus,
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
            screen_width: 0,
            screen_height: 0,
            mode,
            palette,
            patches,
            big_nums: get_num_sprites("STTNUM", 0, wad),
            lil_nums: get_num_sprites("STCFN0", 48, wad),
            grey_nums: get_num_sprites("STGNUM", 0, wad),
            yell_nums: get_num_sprites("STYSNUM", 0, wad),
            keys: get_st_key_sprites(wad),
            status: PlayerStatus::default(),
        }
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .expect(&format!("{name} not in cache"))
    }

    fn draw_health(&self, big: bool, face: bool, buffer: &mut PixelBuf) {
        let nums = if big { &self.big_nums } else { &self.lil_nums };

        let mut y = nums[0].height as i32;
        let mut x = nums[0].width as i32;
        if !big {
            y = y * 2 + 2;
            x *= 5;
        } else {
            y = y + self.lil_nums[0].height as i32 + 1;
            x *= 4;
        }
        if face {
            x += self.get_patch("STFST41").width as i32 + 1;
        }

        let h = if self.status.health < 0 {
            0
        } else {
            self.status.health as u32
        };

        if h < 100 {
            x -= nums[0].width as i32;
        }
        if h < 10 {
            x -= nums[0].width as i32;
        }
        draw_num(h, x, self.screen_height - 2 - y, 0, nums, self, buffer);
    }

    fn draw_armour(&self, face: bool, buffer: &mut PixelBuf) {
        if self.status.armorpoints <= 0 {
            return;
        }

        let nums = &self.lil_nums;

        let mut y = nums[0].height as i32;
        let mut x = nums[0].width as i32;
        y = y + 1;
        x *= 5;
        if face {
            x += self.get_patch("STFST41").width as i32 + 1;
        }

        let h = self.status.armorpoints as u32;
        if h < 100 {
            x -= nums[0].width as i32;
        }
        if h < 10 {
            x -= nums[0].width as i32;
        }
        draw_num(h, x, self.screen_height - 2 - y, 0, nums, self, buffer);
    }

    fn draw_ammo_big(&self, buffer: &mut PixelBuf) {
        if matches!(self.status.readyweapon, WeaponType::NoChange) {
            return;
        }
        if !(self.mode == GameMode::Commercial)
            && self.status.readyweapon == WeaponType::SuperShotgun
        {
            return;
        }

        let ammo = WEAPON_INFO[self.status.readyweapon as usize].ammo;
        if ammo == AmmoType::NoAmmo {
            return;
        }

        let height = self.big_nums[0].height as i32;
        let start_x = self.big_nums[0].width as i32 + self.keys[0].width as i32 + 2;
        let ammo = self.status.ammo[ammo as usize];
        draw_num(
            ammo,
            self.screen_width - start_x,
            self.screen_height - 2 - height - self.grey_nums[0].height as i32,
            0,
            &self.big_nums,
            self,
            buffer,
        );
    }

    fn draw_keys(&self, buffer: &mut PixelBuf) {
        let height = self.keys[3].height as i32;
        let width = self.keys[0].width as i32;

        let skull_x = self.screen_width - width as i32 - 4;
        let mut x = skull_x - width - 2;
        let start_y = self.screen_height - height - 2;

        for (mut i, owned) in self.status.cards.iter().enumerate() {
            if !*owned {
                continue;
            }

            let height = self.keys[3].height as i32;
            let patch = &self.keys[i];
            let mut pad = 0;
            if i > 2 {
                i = i - 3;
                x = skull_x;
            } else {
                pad = -3;
            }
            self.draw_patch(
                patch,
                x,
                start_y - pad - height * i as i32 - i as i32,
                buffer,
            );
        }
    }

    fn draw_weapons(&self, buffer: &mut PixelBuf) {
        let y = self.grey_nums[0].height as i32;
        let x = self.grey_nums[0].width as i32;
        let mult = if self.mode == GameMode::Commercial {
            10
        } else {
            9
        };
        let start_x = self.screen_width
            - self.grey_nums[0].width as i32 * mult // align with big ammo
            - self.big_nums[0].width as i32
            - self.keys[0].width as i32 - 2;
        let start_y = self.screen_height - y - 2;

        for (i, owned) in self.status.weaponowned.iter().enumerate() {
            if !(self.mode == GameMode::Commercial) && i == 8 || !*owned {
                continue;
            }
            let nums = if self.status.readyweapon as usize == i {
                &self.yell_nums
            } else {
                &self.grey_nums
            };
            draw_num(
                i as u32 + 1,
                start_x + x * i as i32 + i as i32,
                start_y,
                0,
                nums,
                self,
                buffer,
            );
        }
    }

    fn draw_face(&self, mut big: bool, upper: bool, buffer: &mut PixelBuf) {
        if upper {
            big = true;
        }
        let nums = if big { &self.big_nums } else { &self.lil_nums };
        let mut x = nums[0].width as i32;

        let mut y;
        if big && !upper {
            let patch = self.get_patch("STFB1");
            y = if upper {
                0
            } else {
                self.screen_height - patch.height as i32
            };
            x = self.screen_width / 2 - patch.width as i32 / 2;
            self.draw_patch(patch, x, y, buffer);
        };

        let patch = if self.status.health < 20 {
            self.get_patch("STFST41")
        } else if self.status.health < 40 {
            self.get_patch("STFST31")
        } else if self.status.health < 60 {
            self.get_patch("STFST21")
        } else if self.status.health < 80 {
            self.get_patch("STFST11")
        } else {
            self.get_patch("STFST01")
        };

        let offset_x = patch.width as i32 / 2;
        let offset_y = patch.height as i32;
        if upper || big {
            x = self.screen_width / 2 - patch.width as i32 / 2;
            y = if upper {
                1
            } else {
                self.screen_height - patch.height as i32
            };
        } else {
            x = offset_x;
            y = self.screen_height - offset_y
        };
        self.draw_patch(patch, x, y, buffer);
    }
}

impl MachinationTrait for Statusbar {
    fn init(&mut self, _game: &impl GameTraits) {}

    fn responder(&mut self, _sc: Scancode, _game: &mut impl GameTraits) -> bool {
        false
    }

    fn ticker(&mut self, game: &mut impl GameTraits) -> bool {
        self.status = game.player_status();
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut PixelBuf) {
        self.screen_width = buffer.width() as i32;
        self.screen_height = buffer.height() as i32;

        let face = true;
        if face {
            self.draw_face(false, false, buffer);
        }
        self.draw_health(true, face, buffer);
        self.draw_armour(face, buffer);
        self.draw_ammo_big(buffer);
        self.draw_weapons(buffer);
        self.draw_keys(buffer);
    }
}
