//! A custom statusbar to show the players status during gameplay.
//!
//! Displays things like ammo count, weapons owned, key/skulls owned, health and
//! so on.

use faces::DoomguyFace;
use gamestate_traits::util::{draw_num_pixels, get_num_sprites, get_st_key_sprites};
use gamestate_traits::{
    AmmoType, GameMode, GameTraits, MachinationTrait, PixelBuffer, PlayerStatus, Scancode,
    WeaponType, WEAPON_INFO,
};
use std::collections::HashMap;
use wad::types::{WadPalette, WadPatch};
use wad::WadData;

mod faces;

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
    faces: DoomguyFace,
}

impl Statusbar {
    pub fn new(mode: GameMode, wad: &WadData) -> Self {
        let palette = wad.playpal_iter().next().unwrap();

        let mut patches = HashMap::new();

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
            faces: DoomguyFace::new(wad),
        }
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .unwrap_or_else(|| panic!("{name} not in cache"))
    }

    fn draw_health_pixels(&self, big: bool, face: bool, pixels: &mut dyn PixelBuffer) {
        let f = pixels.size().height() / 200;

        let nums = if big { &self.big_nums } else { &self.lil_nums };

        let mut y = nums[0].height as i32 * f;
        let mut x = nums[0].width as i32 * f;
        if !big {
            y = y * 2 + 2;
            x *= 5;
        } else {
            y = y + self.lil_nums[0].height as i32 * f + 1;
            x *= 4;
        }
        if face {
            x += self.faces.get_face().width as i32 * f + 1;
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
        draw_num_pixels(h, x, self.screen_height - 2 - y, 0, nums, self, pixels);
    }

    fn draw_armour_pixels(&self, face: bool, pixels: &mut dyn PixelBuffer) {
        if self.status.armorpoints <= 0 {
            return;
        }
        let f = pixels.size().height() / 200;

        let nums = &self.lil_nums;

        let mut y = nums[0].height as i32 * f;
        let mut x = nums[0].width as i32 * f;
        y += 1;
        x *= 5;
        if face {
            x += self.faces.get_face().width as i32 * f + 1;
        }

        let h = self.status.armorpoints as u32;
        if h < 100 {
            x -= nums[0].width as i32;
        }
        if h < 10 {
            x -= nums[0].width as i32;
        }
        draw_num_pixels(h, x, self.screen_height - 2 - y, 0, nums, self, pixels);
    }

    fn draw_ammo_big_pixels(&self, pixels: &mut dyn PixelBuffer) {
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
        let f = pixels.size().height() / 200;

        let height = self.big_nums[0].height as i32 * f;
        let start_x = self.big_nums[0].width as i32 * f + self.keys[0].width as i32 * f + 2;
        let ammo = self.status.ammo[ammo as usize];
        draw_num_pixels(
            ammo,
            self.screen_width - start_x,
            self.screen_height - 2 - height - self.grey_nums[0].height as i32 * f,
            0,
            &self.big_nums,
            self,
            pixels,
        );
    }

    fn draw_keys_pixels(&self, pixels: &mut dyn PixelBuffer) {
        let f = pixels.size().height() / 200;
        let height = self.keys[3].height as i32 * f;
        let width = self.keys[0].width as i32 * f;

        let skull_x = self.screen_width - width - 4;
        let mut x = skull_x - width - 2;
        let start_y = self.screen_height - height - 2;

        for (mut i, owned) in self.status.cards.iter().enumerate() {
            if !*owned {
                continue;
            }

            let height = self.keys[3].height as i32 * f;
            let patch = &self.keys[i];
            let mut pad = 0;
            if i > 2 {
                i -= 3;
                x = skull_x;
            } else {
                pad = -3;
            }
            self.draw_patch_pixels(
                patch,
                x,
                start_y - pad - height * i as i32 - i as i32,
                pixels,
            );
        }
    }

    fn draw_weapons_pixels(&self, pixels: &mut dyn PixelBuffer) {
        let f = pixels.size().height() / 200;
        let y = self.grey_nums[0].height as i32 * f;
        let x = self.grey_nums[0].width as i32 * f;
        let mult = if self.mode == GameMode::Commercial {
            10
        } else {
            9
        };
        let start_x = self.screen_width
            - self.grey_nums[0].width as i32 * f * mult // align with big ammo
            - self.big_nums[0].width as i32 * f
            - self.keys[0].width as i32 * f - 2;
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
            draw_num_pixels(
                i as u32 + 1,
                start_x + x * i as i32 + i as i32,
                start_y,
                0,
                nums,
                self,
                pixels,
            );
        }
    }

    fn draw_face_pixels(&self, mut big: bool, upper: bool, pixels: &mut dyn PixelBuffer) {
        let f = pixels.size().height() / 200;
        if upper {
            big = true;
        }

        let mut x;
        let mut y;
        if big && !upper {
            let patch = self.get_patch("STFB1");
            y = if upper {
                0
            } else {
                self.screen_height - patch.height as i32
            };
            x = self.screen_width / 2 - patch.width as i32 / 2;
            self.draw_patch_pixels(patch, x, y, pixels);
        };

        let patch = self.faces.get_face();

        let offset_x = (patch.width as i32 * f) / 2;
        let offset_y = patch.height as i32 * f;
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
        self.draw_patch_pixels(patch, x, y, pixels);
    }
}

impl MachinationTrait for Statusbar {
    fn init(&mut self, _game: &impl GameTraits) {}

    fn responder(&mut self, _sc: Scancode, _game: &mut impl GameTraits) -> bool {
        false
    }

    fn ticker(&mut self, game: &mut impl GameTraits) -> bool {
        self.status = game.player_status();
        self.faces.tick(&self.status);
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut dyn PixelBuffer) {
        self.screen_width = buffer.size().width();
        self.screen_height = buffer.size().height();

        let face = true;
        if face {
            self.draw_face_pixels(false, false, buffer);
        }
        self.draw_health_pixels(true, face, buffer);
        self.draw_armour_pixels(face, buffer);
        self.draw_ammo_big_pixels(buffer);
        self.draw_weapons_pixels(buffer);
        self.draw_keys_pixels(buffer);
    }
}
