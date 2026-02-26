use gameplay::{MapObjFlag, PicData, Player, PspDef};
use render_trait::DrawBuffer;

use crate::Software3D;

const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;

// Doom's weapon sprites use a 320x200 coordinate system:
// - sx: horizontal position, 160.0 = screen center
// - sy: vertical position, 32.0 = weapon raised (ready), 128.0 = weapon lowered
// - left_offset/top_offset: sprite anchor point within the patch
// - texture_mid formula: 100.0 - (sy - top_offset), where 100 = half of 200
const ORIGINAL_WIDTH: f32 = 320.0;
const ORIGINAL_HEIGHT: f32 = 200.0;
const ORIGINAL_HALF_WIDTH: f32 = ORIGINAL_WIDTH / 2.0; // 160.0
const ORIGINAL_HALF_HEIGHT: f32 = ORIGINAL_HEIGHT / 2.0; // 100.0
/// Doom rendered 320x200 but displayed on 4:3 CRT as 320x240,
/// making each pixel 1.2x taller than wide.
const CRT_STRETCH: f32 = 240.0 / 200.0;

/// Convert a distance/offset from Doom's 320-wide coordinate space to screen
/// pixels.
#[inline(always)]
fn scale_x(original: f32, screen_width: f32) -> f32 {
    original * (screen_width / ORIGINAL_WIDTH)
}

/// Convert a distance/offset from Doom's 200-tall coordinate space to screen
/// pixels, including the 1.2x CRT vertical stretch.
#[inline(always)]
fn scale_y(original: f32, screen_height: f32) -> f32 {
    original * (screen_height / ORIGINAL_HEIGHT) * CRT_STRETCH
}

impl Software3D {
    /// Draw all active player weapon sprites (weapon + muzzle flash layers).
    /// Called after world sprite rendering so weapons draw on top of
    /// everything.
    pub(super) fn draw_player_weapons(
        &mut self,
        player: &Player,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let mobj = match player.mobj() {
            Some(m) => m,
            None => return,
        };

        let sector_light = mobj.subsector.sector.lightlevel >> 4;
        // Weapons get +2 extra light (matching original Doom behaviour)
        let base_brightness = (sector_light + player.extralight + 2).min(15);
        let flags = mobj.flags;

        for psp in player.psprites.iter() {
            if psp.state.is_some() {
                self.draw_player_weapon_sprite(psp, base_brightness, flags, pic_data, buffer);
            }
        }
    }

    fn draw_player_weapon_sprite(
        &mut self,
        psp: &PspDef,
        base_brightness: usize,
        flags: u32,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let state = match psp.state {
            Some(s) => s,
            None => return,
        };

        let sprnum = state.sprite as u32 as usize;
        let def = pic_data.sprite_def(sprnum);
        if def.frames.is_empty() {
            return;
        }

        let frame_index = (state.frame & FF_FRAMEMASK) as usize;
        if frame_index >= def.frames.len() {
            return;
        }
        let frame = def.frames[frame_index];
        // Weapon sprites always use rotation 0
        let patch_index = frame.lump[0] as u32 as usize;
        let flip = frame.flip[0] != 0;

        let patch = pic_data.sprite_patch(patch_index);
        let sprite_cols = patch.data.len();
        if sprite_cols == 0 {
            return;
        }
        let sprite_rows = patch.data[0].len();
        if sprite_rows == 0 {
            return;
        }

        let screen_w = self.width as f32;
        let screen_h = self.height as f32;

        // All positions computed as offsets from screen center, then scaled.
        // sx=160 means centered; subtract 160 to get offset from center.
        // left_offset anchors the sprite's origin within the patch.
        let offset_x = (psp.sx - ORIGINAL_HALF_WIDTH) - patch.left_offset as f32;
        let x1 = screen_w * 0.5 + scale_x(offset_x, screen_w);
        let x2 = x1 + scale_x(sprite_cols as f32, screen_w);

        // texture_mid positions the sprite vertically in 200px space.
        // 100 = vertical center of 200px; top_offset anchors the sprite within the
        // patch. Position uses base scaling (no CRT stretch) so the anchor
        // stays at the correct screen location; only the sprite height is
        // CRT-stretched.
        let texture_mid = ORIGINAL_HALF_HEIGHT - (psp.sy - patch.top_offset as f32);
        let base_scale = screen_h / ORIGINAL_HEIGHT;
        let y1 = screen_h * 0.5 - texture_mid * base_scale;
        let y2 = y1 + scale_y(sprite_rows as f32, screen_h);

        // Early reject if entirely off-screen
        if x2 < 0.0 || x1 >= screen_w || y2 < 0.0 || y1 >= screen_h {
            return;
        }

        let brightness = if state.frame & FF_FULLBRIGHT != 0 {
            15
        } else {
            base_brightness
        };

        let is_shadow = flags & MapObjFlag::Shadow as u32 != 0;
        // Scale weapon light with sector brightness, then add a couple of
        // steps so it's always slightly brighter than the proportional value.
        // brightness 0..15, colourmap index 0..47.
        const EXTRA: f32 = 3.0;
        let weapon_light_scale = ((brightness as f32 / 15.0) * 44.0 + EXTRA).min(47.0);
        let colourmap = if is_shadow {
            pic_data.colourmap(33)
        } else {
            pic_data.base_colourmap(brightness, weapon_light_scale)
        };

        // Clamp draw region to screen bounds
        let draw_x1 = x1.max(0.0).ceil() as usize;
        let draw_x2 = (x2.min(screen_w - 1.0).floor() as usize).min(self.width as usize - 1);
        let draw_y1 = y1.max(0.0).ceil() as usize;
        let draw_y2 = (y2.min(screen_h - 1.0).floor() as usize).min(self.height as usize - 1);

        let quad_w = x2 - x1;
        let quad_h = y2 - y1;
        if quad_w < 1.0 || quad_h < 1.0 {
            return;
        }

        let inv_quad_w = 1.0 / quad_w;
        let inv_quad_h = 1.0 / quad_h;
        let sprite_cols_f = sprite_cols as f32;
        let sprite_rows_f = sprite_rows as f32;

        for y in draw_y1..=draw_y2 {
            let v = (y as f32 - y1) * inv_quad_h;
            let tex_row = if flip {
                (v * sprite_rows_f) as usize
            } else {
                (v * sprite_rows_f) as usize
            };
            if tex_row >= sprite_rows {
                continue;
            }

            for x in draw_x1..=draw_x2 {
                let u = (x as f32 - x1) * inv_quad_w;
                let tex_col = if flip {
                    // Mirror horizontally when flipped
                    let flipped_u = 1.0 - u;
                    (flipped_u * sprite_cols_f) as usize
                } else {
                    (u * sprite_cols_f) as usize
                };
                if tex_col >= sprite_cols {
                    continue;
                }

                let color_index = patch.data[tex_col][tex_row];
                if color_index == usize::MAX {
                    continue; // Transparent pixel
                }

                let lit_index = colourmap[color_index];
                if let Some(color) = pic_data.palette().get(lit_index) {
                    buffer.set_pixel(x, y, color);
                }
            }
        }
    }
}
