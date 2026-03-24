use super::{Intermission, MAP_POINTS, SHOW_NEXT_LOC_DELAY, State, TICRATE, TITLE_Y};
use game_config::GameMode;
use hud_util::draw_patch;
use render_common::DrawBuffer;
use wad::types::WadPatch;

impl Intermission {
    pub(super) fn init_next_loc(&mut self) {
        self.state = State::NextLoc;
        self.count = SHOW_NEXT_LOC_DELAY * TICRATE;

        self.init_animated_bg();
    }

    pub(super) fn update_show_next_loc(&mut self) {
        self.update_animated_bg();

        self.count -= 1;
        if self.count <= 0 {
            self.init_no_state();
        } else {
            self.pointer_on = (self.count & 31) < 20;
        }
    }

    pub(super) fn draw_on_lnode(
        &self,
        lv: usize,
        patch: &WadPatch,
        x_offset: f32,
        sx: f32,
        sy: f32,
        pixels: &mut impl DrawBuffer,
    ) {
        let ep = self.level_info.episode;
        let point = MAP_POINTS[ep][lv];

        let x = x_offset + point.0 as f32 * sx;
        let y = point.1 as f32 * sy - patch.top_offset as f32 * sy;

        draw_patch(patch, x, y, sx, sy, &self.palette, pixels);
    }

    pub(super) fn draw_enter_level_pixels(
        &self,
        x_offset: f32,
        sx: f32,
        sy: f32,
        buffer: &mut impl DrawBuffer,
    ) {
        let half = x_offset + 160.0 * sx;
        let mut y = TITLE_Y * sy;
        draw_patch(
            &self.patches.enter,
            half - self.patches.enter.width as f32 * sx / 2.0,
            y,
            sx,
            sy,
            &self.palette,
            buffer,
        );
        y += (5.0 * self.patches.enter.height as f32 * sy) / 4.0;
        self.get_enter_level_name()
            .draw_centered(half, y, sx, sy, &self.palette, buffer);
    }

    pub(super) fn draw_next_loc_pixels(
        &self,
        x_ofs: f32,
        sx: f32,
        sy: f32,
        buffer: &mut impl DrawBuffer,
    ) {
        // Background (fullscreen scale, centered)
        self.draw_bg(x_ofs, sx, sy, buffer);
        self.draw_animated_bg_pixels(x_ofs, sx, sy, buffer);

        // Location stuff only for episodes 1-3
        if self.mode != GameMode::Commercial && self.level_info.episode <= 2 {
            let last = if self.level_info.last == 8 {
                self.level_info.next - 1
            } else {
                self.level_info.next
            };

            for i in 0..last {
                self.draw_on_lnode(i, &self.yah_patches[2], x_ofs, sx, sy, buffer);
            }

            if self.level_info.didsecret {
                self.draw_on_lnode(8, &self.yah_patches[2], x_ofs, sx, sy, buffer);
            }

            if self.pointer_on {
                let next_level = self.level_info.next;
                self.draw_on_lnode(
                    next_level,
                    &self.yah_patches[self.yah_idx],
                    x_ofs,
                    sx,
                    sy,
                    buffer,
                );
            }
        }

        if self.mode != GameMode::Commercial || self.level_info.next != 30 {
            self.draw_enter_level_pixels(x_ofs, sx, sy, buffer);
        }
    }
}
