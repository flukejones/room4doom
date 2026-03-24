use pic_data::PicData;
use render_common::{DrawBuffer, FUZZ_TABLE, RenderPspDef, RenderView, fuzz_darken};

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
/// Convert a distance/offset from Doom's 320-wide coordinate space to screen
/// pixels. Uses height-based scaling (same as menu/title screens) so that the
/// blit's CRT stretch produces correct proportions.
#[inline(always)]
fn scale_x(original: f32, screen_height: f32) -> f32 {
    original * (screen_height / ORIGINAL_HEIGHT)
}

/// Convert a distance/offset from Doom's 200-tall coordinate space to screen
/// pixels. CRT pixel aspect (1.2×) is handled by the blit layer.
#[inline(always)]
fn scale_y(original: f32, screen_height: f32) -> f32 {
    original * (screen_height / ORIGINAL_HEIGHT)
}

impl Software3D {
    /// Draw all active player weapon sprites (weapon + muzzle flash layers).
    /// Called after world sprite rendering so weapons draw on top of
    /// everything.
    pub(crate) fn draw_player_weapons(
        &mut self,
        view: &RenderView,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let sector_light = view.sector_lightlevel >> 4;
        // Weapons get +2 extra light (matching original Doom behaviour)
        let base_brightness = (sector_light + view.extralight + 2).min(15);

        for psp in view.psprites.iter() {
            if psp.active {
                self.draw_player_weapon_sprite(
                    psp,
                    base_brightness,
                    view.is_shadow,
                    pic_data,
                    buffer,
                );
            }
        }
    }

    /// Rasterize a single weapon sprite layer (weapon or muzzle flash).
    ///
    /// - Resolves sprite frame/rotation and patch
    /// - Scales from Doom's 320x200 coordinate space to current screen size
    /// - Handles horizontal flip, fullbright frames, and fuzz (shadow)
    ///   rendering
    fn draw_player_weapon_sprite(
        &mut self,
        psp: &RenderPspDef,
        base_brightness: usize,
        is_shadow: bool,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        if !psp.active {
            return;
        }

        let def = pic_data.sprite_def(psp.sprite);
        if def.frames.is_empty() {
            return;
        }

        let frame_index = (psp.frame & FF_FRAMEMASK) as usize;
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
        let screen_h = self.view_height as f32;

        // All positions computed as offsets from screen center, then scaled.
        // sx=160 means centered; subtract 160 to get offset from center.
        // left_offset anchors the sprite's origin within the patch.
        let offset_x = (psp.sx - ORIGINAL_HALF_WIDTH) - patch.left_offset as f32;
        // Round x1 to integer to ensure 1:1 texel-to-pixel mapping (no doubled columns)
        let x1 = (screen_w * 0.5 + scale_x(offset_x, screen_h)).round();
        let x2 = x1 + scale_x(sprite_cols as f32, screen_h);

        // texture_mid positions the sprite vertically in 200px space.
        // 100 = vertical center of 200px; top_offset anchors the sprite within the
        // patch. Position uses base scaling (no CRT stretch) so the anchor
        // stays at the correct screen location; only the sprite height is
        // CRT-stretched.
        let texture_mid = ORIGINAL_HALF_HEIGHT - (psp.sy - patch.top_offset as f32);
        let base_scale = screen_h / ORIGINAL_HEIGHT;
        // Round y1 to integer to ensure 1:1 texel-to-pixel mapping (no doubled rows)
        let y1 = (screen_h * 0.5 - texture_mid * base_scale).round();
        let y2 = y1 + scale_y(sprite_rows as f32, screen_h);

        // Early reject if entirely off-screen
        if x2 < 0.0 || x1 >= screen_w || y2 < 0.0 || y1 >= screen_h {
            return;
        }

        let brightness = if psp.frame & FF_FULLBRIGHT != 0 {
            15
        } else {
            base_brightness
        };

        // Scale weapon light with sector brightness, then add a couple of
        // steps so it's always slightly brighter than the proportional value.
        // brightness 0..15, colourmap index 0..47.
        const EXTRA: f32 = 3.0;
        let weapon_light_scale = ((brightness as f32 / 15.0) * 44.0 + EXTRA).min(47.0);
        let colourmap = if !is_shadow {
            Some(pic_data.base_colourmap(brightness, weapon_light_scale))
        } else {
            None
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

        let sprite_cols_f = sprite_cols as f32;
        let sprite_rows_f = sprite_rows as f32;

        let y1_i = y1 as i32;
        let x1_i = x1 as i32;
        let height = self.height as usize;

        for y in draw_y1..=draw_y2 {
            let tex_row = ((y as i32 - y1_i) as f32 * sprite_rows_f / quad_h) as usize;
            if tex_row >= sprite_rows {
                continue;
            }

            for x in draw_x1..=draw_x2 {
                let tex_col_raw = ((x as i32 - x1_i) as f32 * sprite_cols_f / quad_w) as usize;
                let tex_col = if flip {
                    sprite_cols.saturating_sub(1 + tex_col_raw)
                } else {
                    tex_col_raw
                };
                if tex_col >= sprite_cols {
                    continue;
                }

                let color_index = patch.data[tex_col][tex_row];
                if color_index == usize::MAX {
                    continue;
                }

                if let Some(colourmap) = colourmap {
                    let lit_index = colourmap[color_index];
                    if let Some(&color) = pic_data.palette().get(lit_index) {
                        buffer.set_pixel(x, y, color);
                    }
                } else {
                    let pitch = buffer.pitch();
                    let buf = buffer.buf_mut();
                    let offset = FUZZ_TABLE[self.fuzz_pos % FUZZ_TABLE.len()];
                    let src_y = (y as i32 + offset).clamp(0, height as i32 - 1) as usize;
                    buf[y * pitch + x] = fuzz_darken(buf[src_y * pitch + x]);
                    self.fuzz_pos += 1;
                }
            }
        }
    }
}
