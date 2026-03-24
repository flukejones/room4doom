//! A custom statusbar to show the players status during gameplay.
//!
//! Two modes:
//! - Fullscreen: minimal HUD overlay on the 3D view (no background)
//! - Bar: classic STBAR background with OG Doom element positions

use faces::DoomguyFace;
use game_config::{GameMode, WeaponType};
use gameplay::{AmmoType, PlayerStatus, WEAPON_INFO};
use gamestate_traits::{ConfigKey, ConfigTraits, GameTraits, KeyCode, SubsystemTrait};
use hud_util::{draw_num, draw_patch, hud_scale, load_key_sprites, load_num_sprites};
use render_common::{DrawBuffer, STBAR_HEIGHT};
use std::collections::HashMap;
use wad::WadData;
use wad::types::{WadPalette, WadPatch};

const FLAT_SIZE: usize = 64;

mod faces;

// --- Fullscreen HUD constants ---

const FACE_X_OFFSET: f32 = -4.0;
const FACE_Y_OFFSET: f32 = 2.0;
const FACE_UPPER_Y: f32 = 1.0;

// --- STBAR constants (OG Doom st_stuff.c) ---
// All positions in 320×200 absolute screen space. Bar top = 168.

const STBAR_WIDTH: usize = 320;

// Ready ammo (big nums, 3 digits)
const ST_AMMOX: f32 = 44.0;
const ST_AMMOY: f32 = 171.0;

// Health (big nums)
const ST_HEALTHX: f32 = 96.0;
const ST_HEALTHY: f32 = 171.0;

// Arms grid (2 rows × 3 cols of grey/yellow nums)
const ST_ARMSX: f32 = 115.0;
const ST_ARMSY: f32 = 172.0;
const ST_ARMSBGX: f32 = 104.0;
const ST_ARMSBGY: f32 = 168.0;
const ST_ARMSXSPACE: f32 = 12.0;
const ST_ARMSYSPACE: f32 = 10.0;

// Face
const ST_FACESX: f32 = 143.0;
const ST_FACESY: f32 = 170.0;

// Armor (big nums)
const ST_ARMORX: f32 = 221.0;
const ST_ARMORY: f32 = 171.0;

// Keys (stacked vertically)
const ST_KEY0X: f32 = 239.0;
const ST_KEY0Y: f32 = 171.0;
const ST_KEY1Y: f32 = 181.0;
const ST_KEY2Y: f32 = 191.0;

// Ammo counts (small nums, 4 rows: bullets, shells, rockets, cells)
const ST_AMMO0X: f32 = 288.0;
const ST_AMMO0Y: f32 = 173.0;
const ST_AMMO1Y: f32 = 179.0;
const ST_AMMO2Y: f32 = 191.0;
const ST_AMMO3Y: f32 = 185.0;

// Max ammo counts
const ST_MAXAMMO0X: f32 = 314.0;
const ST_MAXAMMO0Y: f32 = 173.0;
const ST_MAXAMMO1Y: f32 = 179.0;
const ST_MAXAMMO2Y: f32 = 191.0;
const ST_MAXAMMO3Y: f32 = 185.0;

pub struct Statusbar {
    screen_width: f32,
    screen_height: f32,
    /// Left edge of the centered 320-wide game zone in buffer pixels.
    x_ofs: f32,
    mode: GameMode,
    palette: WadPalette,
    patches: HashMap<&'static str, WadPatch>,
    big_nums: [WadPatch; 10],
    lil_nums: [WadPatch; 10],
    grey_nums: [WadPatch; 10],
    yell_nums: [WadPatch; 10],
    /// Keys: blue yellow red. Skulls: blue yellow red
    keys: [WadPatch; 6],
    status: PlayerStatus,
    faces: DoomguyFace,
    widescreen: bool,
    bar_mode: bool,
    /// STBAR decoded to a flat 320×32 RGBA pixel buffer (native scale).
    stbar_native: Vec<u32>,
    /// FLAT5_4 decoded to 64×64 RGBA for filling bar margins.
    margin_flat: Vec<u32>,
}

impl Statusbar {
    pub fn new(mode: GameMode, wad: &WadData) -> Self {
        let palette = wad.lump_iter::<WadPalette>("PLAYPAL").next().unwrap();

        let mut patches = HashMap::new();

        let lump = wad.get_lump("STFB1").unwrap();
        patches.insert("STFB1", WadPatch::from_lump(lump));

        if let Some(lump) = wad.get_lump("STARMS") {
            patches.insert("STARMS", WadPatch::from_lump(lump));
        }

        let margin_flat = if let Some(lump) = wad.get_lump("FLAT5_4") {
            lump.data
                .iter()
                .map(|&idx| palette.0[idx as usize])
                .collect()
        } else {
            vec![0u32; FLAT_SIZE * FLAT_SIZE]
        };

        let stbar_native = if let Some(lump) = wad.get_lump("STBAR") {
            let stbar_patch = WadPatch::from_lump(lump);
            decode_patch_to_rgba(&stbar_patch, &palette)
        } else {
            vec![0u32; STBAR_WIDTH * STBAR_HEIGHT as usize]
        };

        Self {
            screen_width: 0.0,
            screen_height: 0.0,
            x_ofs: 0.0,
            mode,
            palette,
            patches,
            big_nums: load_num_sprites("STTNUM", 0, wad),
            lil_nums: load_num_sprites("STCFN0", 48, wad),
            grey_nums: load_num_sprites("STGNUM", 0, wad),
            yell_nums: load_num_sprites("STYSNUM", 0, wad),
            keys: load_key_sprites(wad),
            status: PlayerStatus::default(),
            faces: DoomguyFace::new(wad),
            widescreen: false,
            bar_mode: false,
            stbar_native,
            margin_flat,
        }
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .unwrap_or_else(|| panic!("{name} not in cache"))
    }

    // ========================================================================
    // Fullscreen HUD (no background, overlay on 3D view)
    // ========================================================================

    fn draw_fullscreen(&mut self, buffer: &mut impl DrawBuffer) {
        let face = true;
        if face {
            self.draw_face_fullscreen(false, false, buffer);
        }
        self.draw_health_fullscreen(true, face, buffer);
        self.draw_armour_fullscreen(face, buffer);
        self.draw_ammo_big_fullscreen(buffer);
        self.draw_weapons_fullscreen(buffer);
        self.draw_keys_fullscreen(buffer);
    }

    fn draw_health_fullscreen(&self, big: bool, face: bool, pixels: &mut impl DrawBuffer) {
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
            self.x_ofs + x,
            self.screen_height - 2.0 - y,
            0,
            nums,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_armour_fullscreen(&self, face: bool, pixels: &mut impl DrawBuffer) {
        if self.status.armorpoints <= 0 {
            return;
        }
        let (sx, sy) = hud_scale(pixels);
        let nums = &self.lil_nums;

        let y = nums[0].height as f32 * sy + 1.0;
        let mut x = nums[0].width as f32 * sx * 5.0;
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
            self.x_ofs + x,
            self.screen_height - 2.0 - y,
            0,
            nums,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_ammo_big_fullscreen(&self, pixels: &mut impl DrawBuffer) {
        if matches!(self.status.readyweapon, WeaponType::NoChange) {
            return;
        }
        if self.mode != GameMode::Commercial && self.status.readyweapon == WeaponType::SuperShotgun
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

    fn draw_keys_fullscreen(&self, pixels: &mut impl DrawBuffer) {
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

    fn draw_weapons_fullscreen(&self, pixels: &mut impl DrawBuffer) {
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
            if self.mode != GameMode::Commercial && i == 8 || !*owned {
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

    fn draw_face_fullscreen(&self, mut big: bool, upper: bool, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);
        if upper {
            big = true;
        }

        let mut x: f32;
        let mut y: f32;
        if big && !upper {
            let patch = self.get_patch("STFB1");
            y = self.screen_height - patch.height as f32 * sy;
            x = (self.x_ofs + self.screen_width) / 2.0 - patch.width as f32 * sx / 2.0;
            draw_patch(patch, x, y, sx, sy, &self.palette, pixels);
        };

        let patch = self.faces.get_face();
        if upper || big {
            x = (self.x_ofs + self.screen_width) / 2.0 - patch.width as f32 * sx / 2.0;
            y = if upper {
                FACE_UPPER_Y * sy
            } else {
                self.screen_height - patch.height as f32 * sy
            };
        } else {
            x = self.x_ofs + (patch.width as f32 / 2.0 + FACE_X_OFFSET) * sx;
            y = self.screen_height - (patch.height as f32 + FACE_Y_OFFSET) * sy;
        };
        draw_patch(patch, x, y, sx, sy, &self.palette, pixels);
    }

    // ========================================================================
    // STBAR bar mode (classic centered statusbar with background)
    // ========================================================================

    /// Blit the 320px STBAR centered, fill margins with FLAT5_4.
    fn draw_stbar_background(
        &self,
        sx: f32,
        sy: f32,
        bar_y: f32,
        bar_x: f32,
        buffer: &mut impl DrawBuffer,
    ) {
        let bar_h = STBAR_HEIGHT as usize;
        let buf_w = buffer.size().width_usize();
        let buf_h = buffer.size().height_usize();
        let bar_left = bar_x.floor() as i32;
        let bar_right = (bar_x + STBAR_WIDTH as f32 * sx).ceil() as usize;

        // Fill left and right margins with FLAT5_4 (right side rotated 180°)
        if !self.margin_flat.is_empty() {
            for src_y in 0..bar_h {
                let dy0 = (bar_y + src_y as f32 * sy).floor() as i32;
                let dy1 = (bar_y + (src_y + 1) as f32 * sy).floor() as i32;
                for dst_y in dy0..dy1 {
                    if dst_y < 0 || dst_y as usize >= buf_h {
                        continue;
                    }
                    let flat_y = src_y % FLAT_SIZE;

                    // Left margin
                    for dst_x in 0..bar_left.max(0) as usize {
                        let flat_x = dst_x % FLAT_SIZE;
                        let pixel = self.margin_flat[flat_y * FLAT_SIZE + flat_x];
                        if pixel != 0 {
                            buffer.set_pixel(dst_x, dst_y as usize, pixel);
                        }
                    }

                    // Right margin (180° rotation = flip x and y)
                    for dst_x in bar_right..buf_w {
                        let flat_x = (FLAT_SIZE - 1) - ((dst_x - bar_right) % FLAT_SIZE);
                        let flat_y_r = (FLAT_SIZE - 1) - flat_y;
                        let pixel = self.margin_flat[flat_y_r * FLAT_SIZE + flat_x];
                        if pixel != 0 {
                            buffer.set_pixel(dst_x, dst_y as usize, pixel);
                        }
                    }
                }
            }
        }

        // Draw the 320px STBAR
        for src_y in 0..bar_h {
            let dy0 = (bar_y + src_y as f32 * sy).floor() as i32;
            let dy1 = (bar_y + (src_y + 1) as f32 * sy).floor() as i32;
            for dst_y in dy0..dy1 {
                if dst_y < 0 || dst_y as usize >= buf_h {
                    continue;
                }
                for src_x in 0..STBAR_WIDTH {
                    let dx0 = (bar_x + src_x as f32 * sx).floor() as i32;
                    let dx1 = (bar_x + (src_x + 1) as f32 * sx).floor() as i32;
                    let pixel = self.stbar_native[src_y * STBAR_WIDTH + src_x];
                    if pixel == 0 {
                        continue;
                    }
                    for dst_x in dx0..dx1 {
                        if dst_x >= 0 && (dst_x as usize) < buf_w {
                            buffer.set_pixel(dst_x as usize, dst_y as usize, pixel);
                        }
                    }
                }
            }
        }
    }

    fn draw_bar(&mut self, buffer: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(buffer);
        let bar_y = buffer.size().view_height_f32();
        // STBAR is always centered, regardless of widescreen HUD setting
        let x = (buffer.size().width_f32() - 320.0 * sx) / 2.0;

        self.draw_stbar_background(sx, sy, bar_y, x, buffer);

        // ARMS background patch
        if self.mode != GameMode::Commercial {
            if let Some(patch) = self.patches.get("STARMS") {
                draw_patch(
                    patch,
                    x + ST_ARMSBGX * sx,
                    ST_ARMSBGY * sy,
                    sx,
                    sy,
                    &self.palette,
                    buffer,
                );
            }
        }

        // Face
        let face = self.faces.get_face();
        draw_patch(
            face,
            x + ST_FACESX * sx,
            ST_FACESY * sy,
            sx,
            sy,
            &self.palette,
            buffer,
        );

        // Ready ammo
        if !matches!(self.status.readyweapon, WeaponType::NoChange)
            && (self.mode == GameMode::Commercial
                || self.status.readyweapon != WeaponType::SuperShotgun)
        {
            let ammo_type = WEAPON_INFO[self.status.readyweapon as usize].ammo;
            if ammo_type != AmmoType::NoAmmo {
                draw_num(
                    self.status.ammo[ammo_type as usize],
                    x + ST_AMMOX * sx,
                    ST_AMMOY * sy,
                    0,
                    &self.big_nums,
                    sx,
                    sy,
                    &self.palette,
                    buffer,
                );
            }
        }

        // Health
        let h = if self.status.health < 0 {
            0
        } else {
            self.status.health as u32
        };
        draw_num(
            h,
            x + ST_HEALTHX * sx,
            ST_HEALTHY * sy,
            0,
            &self.big_nums,
            sx,
            sy,
            &self.palette,
            buffer,
        );

        // Armor
        let a = if self.status.armorpoints < 0 {
            0
        } else {
            self.status.armorpoints as u32
        };
        draw_num(
            a,
            x + ST_ARMORX * sx,
            ST_ARMORY * sy,
            0,
            &self.big_nums,
            sx,
            sy,
            &self.palette,
            buffer,
        );

        // Arms (2 rows × 3 cols)
        if self.mode != GameMode::Commercial {
            for i in 0..6 {
                if !self.status.weaponowned[i + 1] {
                    continue;
                }
                let nums = if self.status.readyweapon as usize == i + 1 {
                    &self.yell_nums
                } else {
                    &self.grey_nums
                };
                let col = (i % 3) as f32;
                let row = (i / 3) as f32;
                draw_num(
                    i as u32 + 2,
                    x + (ST_ARMSX + col * ST_ARMSXSPACE) * sx,
                    (ST_ARMSY + row * ST_ARMSYSPACE) * sy,
                    0,
                    nums,
                    sx,
                    sy,
                    &self.palette,
                    buffer,
                );
            }
        }

        // Keys (3 slots stacked)
        let key_ys = [ST_KEY0Y, ST_KEY1Y, ST_KEY2Y];
        for (i, owned) in self.status.cards.iter().enumerate() {
            if !*owned {
                continue;
            }
            let row = i % 3;
            draw_patch(
                &self.keys[i],
                x + ST_KEY0X * sx,
                key_ys[row] * sy,
                sx,
                sy,
                &self.palette,
                buffer,
            );
        }

        // Ammo counts (yellow small nums, 4 rows)
        let ammo_ys = [ST_AMMO0Y, ST_AMMO1Y, ST_AMMO3Y, ST_AMMO2Y];
        let max_ys = [ST_MAXAMMO0Y, ST_MAXAMMO1Y, ST_MAXAMMO3Y, ST_MAXAMMO2Y];
        for i in 0..4 {
            draw_num(
                self.status.ammo[i] as u32,
                x + ST_AMMO0X * sx,
                ammo_ys[i] * sy,
                0,
                &self.yell_nums,
                sx,
                sy,
                &self.palette,
                buffer,
            );
            draw_num(
                self.status.maxammo[i] as u32,
                x + ST_MAXAMMO0X * sx,
                max_ys[i] * sy,
                0,
                &self.yell_nums,
                sx,
                sy,
                &self.palette,
                buffer,
            );
        }
    }
}

impl SubsystemTrait for Statusbar {
    fn init<T: GameTraits + ConfigTraits>(&mut self, _game: &T) {}

    fn responder<T: GameTraits + ConfigTraits>(&mut self, _sc: KeyCode, _game: &mut T) -> bool {
        false
    }

    fn ticker<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) -> bool {
        self.status = game.player_status();
        self.faces.tick(&self.status);
        self.widescreen = game.config_value(ConfigKey::HudWidth) != 0;
        self.bar_mode = game.config_value(ConfigKey::HudSize) == 1;
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        let (sx, _) = hud_scale(buffer);
        if self.widescreen {
            self.x_ofs = 0.0;
            self.screen_width = buffer.size().width_f32();
        } else {
            self.x_ofs = (buffer.size().width_f32() - 320.0 * sx) / 2.0;
            self.screen_width = self.x_ofs + 320.0 * sx;
        }
        self.screen_height = buffer.size().height_f32();

        if self.bar_mode {
            self.draw_bar(buffer);
        } else {
            self.draw_fullscreen(buffer);
        }
    }
}

/// Decode a WadPatch into a flat RGBA pixel buffer (width × height).
/// Transparent pixels are 0x00000000.
fn decode_patch_to_rgba(patch: &WadPatch, palette: &WadPalette) -> Vec<u32> {
    let w = patch.width as usize;
    let h = patch.height as usize;
    let mut buf = vec![0u32; w * h];
    let mut col_idx = 0usize;

    for column in &patch.columns {
        if column.y_offset == 255 {
            col_idx += 1;
            continue;
        }
        if col_idx >= w {
            break;
        }
        for (row, &pal_idx) in column.pixels.iter().enumerate() {
            let y = column.y_offset as usize + row;
            if y < h {
                buf[y * w + col_idx] = palette.0[pal_idx];
            }
        }
    }
    buf
}
