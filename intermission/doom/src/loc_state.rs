use crate::{Intermission, State, MAP_POINTS, SHOW_NEXT_LOC_DELAY, TICRATE, TITLE_Y};
use gamestate_traits::{GameMode, MachinationTrait, PixelBuffer};
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
        scale: i32,
        pixels: &mut dyn PixelBuffer,
    ) {
        let ep = self.level_info.episode;
        let point = MAP_POINTS[ep][lv];

        let x = point.0 - patch.left_offset as i32;
        let y = point.1 - patch.top_offset as i32;

        self.draw_patch_pixels(patch, x * scale, y * scale, pixels);
    }

    pub(super) fn draw_enter_level_pixels(&self, scale: i32, buffer: &mut dyn PixelBuffer) {
        let half = buffer.size().width() / 2;
        let mut y = TITLE_Y * scale;
        self.draw_patch_pixels(
            &self.patches.enter,
            half - self.patches.enter.width as i32 * scale / 2,
            y,
            buffer,
        );
        y += (5 * self.patches.enter.height as i32 * scale) / 4;
        let patch = self.get_enter_level_name();
        self.draw_patch_pixels(patch, half - patch.width as i32 * scale / 2, y, buffer);
    }

    pub(super) fn draw_next_loc_pixels(&self, scale: i32, buffer: &mut dyn PixelBuffer) {
        // Background
        self.draw_patch_pixels(self.get_bg(), 0, 0, buffer);
        self.draw_animated_bg_pixels(scale, buffer);

        // Location stuff only for episodes 1-3
        if self.mode != GameMode::Commercial && self.level_info.episode <= 2 {
            let last = if self.level_info.last == 8 {
                self.level_info.next - 1
            } else {
                self.level_info.next
            };

            for i in 0..last {
                self.draw_on_lnode(i, &self.yah_patches[2], scale, buffer);
            }

            if self.level_info.didsecret {
                self.draw_on_lnode(8, &self.yah_patches[2], scale, buffer);
            }

            if self.pointer_on {
                let next_level = self.level_info.next;
                self.draw_on_lnode(next_level, &self.yah_patches[self.yah_idx], scale, buffer);
            }
        }

        if self.mode != GameMode::Commercial || self.level_info.next != 30 {
            self.draw_enter_level_pixels(scale, buffer);
        }
    }
}
