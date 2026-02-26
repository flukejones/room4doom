//! A custom statusbar to show the players status during gameplay.
//!
//! Displays things like ammo count, weapons owned, key/skulls owned, health and
//! so on.

use faces::DoomguyFace;
use gamestate_traits::util::{get_num_sprites, get_st_key_sprites};
use gamestate_traits::{
    AmmoType, DrawBuffer, GameMode, GameTraits, PlayerStatus, Scancode, SubsystemTrait,
    WEAPON_INFO, WeaponType,
};
use hud_util::{draw_num, draw_patch, hud_scale};
use std::collections::HashMap;
use wad::WadData;
use wad::types::{WadPalette, WadPatch};

mod faces;

pub struct Statusbar {
    screen_width: f32,
    screen_height: f32,
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
            screen_width: 0.0,
            screen_height: 0.0,
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

    fn draw_health_pixels(&self, big: bool, face: bool, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);

        let nums = if big { &self.big_nums } else { &self.lil_nums };

        let mut y = nums[0].height as f32 * sy;
        let mut x = nums[0].width as f32 * sx;
        if !big {
            y = y * 2.0 + 2.0;
            x *= 5.0;
        } else {
            y = y + self.lil_nums[0].height as f32 * sy + 1.0;
            x *= 4.0;
        }
        if face {
            x += self.faces.get_face().width as f32 * sx + 1.0;
        }

        let h = if self.status.health < 0 {
            0
        } else {
            self.status.health as u32
        };

        if h < 100 {
            x -= nums[0].width as f32;
        }
        if h < 10 {
            x -= nums[0].width as f32;
        }
        draw_num(
            h,
            x,
            self.screen_height - 2.0 - y,
            0,
            nums,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_armour_pixels(&self, face: bool, pixels: &mut impl DrawBuffer) {
        if self.status.armorpoints <= 0 {
            return;
        }
        let (sx, sy) = hud_scale(pixels);

        let nums = &self.lil_nums;

        let mut y = nums[0].height as f32 * sy;
        let mut x = nums[0].width as f32 * sx;
        y += 1.0;
        x *= 5.0;
        if face {
            x += self.faces.get_face().width as f32 * sx + 1.0;
        }

        let h = self.status.armorpoints as u32;
        if h < 100 {
            x -= nums[0].width as f32;
        }
        if h < 10 {
            x -= nums[0].width as f32;
        }
        draw_num(
            h,
            x,
            self.screen_height - 2.0 - y,
            0,
            nums,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_ammo_big_pixels(&self, pixels: &mut impl DrawBuffer) {
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
        let (sx, sy) = hud_scale(pixels);

        let height = self.big_nums[0].height as f32 * sy;
        let start_x = self.big_nums[0].width as f32 * sx + self.keys[0].width as f32 * sx + 2.0;
        let ammo = self.status.ammo[ammo as usize];
        draw_num(
            ammo,
            self.screen_width - start_x,
            self.screen_height - 2.0 - height - self.grey_nums[0].height as f32 * sy,
            0,
            &self.big_nums,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_keys_pixels(&self, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);
        let height = self.keys[3].height as f32 * sy;
        let width = self.keys[0].width as f32 * sx;

        let skull_x = self.screen_width - width - 4.0;
        let mut x = skull_x - width - 2.0;
        let start_y = self.screen_height - height - 2.0;

        for (mut i, owned) in self.status.cards.iter().enumerate() {
            if !*owned {
                continue;
            }

            let height = self.keys[3].height as f32 * sy;
            let patch = &self.keys[i];
            let mut pad = 0.0;
            if i > 2 {
                i -= 3;
                x = skull_x;
            } else {
                pad = -3.0;
            }
            draw_patch(
                patch,
                x,
                start_y - pad - height * i as f32 - i as f32,
                sx,
                sy,
                &self.palette,
                pixels,
            );
        }
    }

    fn draw_weapons_pixels(&self, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);
        let y = self.grey_nums[0].height as f32 * sy;
        let x = self.grey_nums[0].width as f32 * sx;
        let mult: f32 = if self.mode == GameMode::Commercial {
            10.0
        } else {
            9.0
        };
        let start_x = self.screen_width
            - self.grey_nums[0].width as f32 * sx * mult
            - self.big_nums[0].width as f32 * sx
            - self.keys[0].width as f32 * sx
            - 2.0;
        let start_y = self.screen_height - y - 2.0;

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
                start_x + x * i as f32 + i as f32,
                start_y,
                0,
                nums,
                sx,
                sy,
                &self.palette,
                pixels,
            );
        }
    }

    fn draw_face_pixels(&self, mut big: bool, upper: bool, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);
        if upper {
            big = true;
        }

        let mut x: f32;
        let mut y: f32;
        if big && !upper {
            let patch = self.get_patch("STFB1");
            y = self.screen_height - patch.height as f32 * sy;
            x = self.screen_width / 2.0 - patch.width as f32 * sx / 2.0;
            draw_patch(patch, x, y, sx, sy, &self.palette, pixels);
        };

        let patch = self.faces.get_face();

        let offset_x = patch.width as f32 * sx / 2.0;
        let offset_y = patch.height as f32 * sy;
        if upper || big {
            x = self.screen_width / 2.0 - patch.width as f32 * sx / 2.0;
            y = if upper {
                1.0
            } else {
                self.screen_height - patch.height as f32 * sy
            };
        } else {
            x = offset_x;
            y = self.screen_height - offset_y;
        };
        draw_patch(patch, x, y, sx, sy, &self.palette, pixels);
    }
}

impl SubsystemTrait for Statusbar {
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

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        self.screen_width = buffer.size().width_f32();
        self.screen_height = buffer.size().height_f32();

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
