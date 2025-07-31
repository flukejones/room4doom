use crate::{Intermission, SHOW_NEXT_LOC_DELAY, State, TICRATE, TITLE_Y};
use gamestate_traits::util::draw_num_pixels;
use gamestate_traits::{DrawBuffer, GameMode, SubsystemTrait};

const SCREEN_HEIGHT: i32 = 200;

const SP_STATSX: i32 = 50;
const SP_STATSY: i32 = 50;
const SP_TIMEX: i32 = 16;
const SP_TIMEY: i32 = SCREEN_HEIGHT - 32;

impl Intermission {
    pub(super) fn init_stats(&mut self) {
        self.pointer_on = false;
        self.state = State::StatCount;
        self.count = SHOW_NEXT_LOC_DELAY * TICRATE;

        self.init_animated_bg();
    }

    pub(super) fn update_stats(&mut self) {
        self.update_animated_bg();

        // self.count -= 1;
        if self.count <= 0 {
            if self.mode == GameMode::Commercial {
                self.init_no_state();
            } else {
                self.init_next_loc();
            }
        }
    }

    pub(super) fn draw_level_finish_pixels(&self, scale: i32, pixels: &mut impl DrawBuffer) {
        let half = pixels.size().width() / 2;
        let mut y = TITLE_Y * scale;
        self.draw_patch_pixels(
            &self.patches.finish,
            half - self.patches.enter.width as i32 * scale / 2,
            y,
            pixels,
        );
        y += (5 * self.patches.finish.height as i32) / 4 * scale;
        let patch = self.get_this_level_name();
        self.draw_patch_pixels(patch, half - patch.width as i32 * scale / 2, y, pixels);
    }

    fn draw_percent(&self, p: u32, x: i32, y: i32, pixels: &mut impl DrawBuffer) {
        self.draw_patch_pixels(&self.patches.percent, x, y, pixels);
        draw_num_pixels(p, x, y, 0, &self.patches.nums, self, pixels);
    }

    fn draw_time(&self, t: u32, mut x: i32, y: i32, scale: i32, buffer: &mut impl DrawBuffer) {
        let mut div = 1;
        if t <= 61 * 59 {
            loop {
                let n = (t / div) % 60;
                x = draw_num_pixels(n, x, y, 1, &self.patches.nums, self, buffer)
                    - self.patches.colon.width as i32 * scale;
                div *= 60;

                if div == 60 || t / div != 0 {
                    self.draw_patch_pixels(&self.patches.colon, x, y, buffer);
                }

                if t / div == 0 {
                    break;
                }
            }
        }
    }

    pub(super) fn draw_stats_pixels(&mut self, scale: i32, buffer: &mut impl DrawBuffer) {
        let width = buffer.size().width();
        let stats_x = SP_STATSX * scale;
        let stats_y = SP_STATSY * scale;
        let time_x = SP_TIMEX * scale;
        let time_y = SP_TIMEY * scale;

        // Background
        self.draw_patch_pixels(self.get_bg(), 0, 0, buffer);
        self.draw_animated_bg_pixels(scale, buffer);
        self.draw_level_finish_pixels(scale, buffer);

        let mut lh = (3 * self.patches.nums[0].height / 2) as i32;
        self.draw_patch_pixels(&self.patches.kills, stats_x, stats_y, buffer);
        self.draw_percent(
            self.player_info.total_kills as u32,
            width - stats_x,
            stats_y,
            buffer,
        );

        lh += lh;
        self.draw_patch_pixels(&self.patches.items, stats_x, stats_y + lh, buffer);
        self.draw_percent(
            self.player_info.items_collected as u32,
            width - stats_x,
            stats_y + lh,
            buffer,
        );

        lh += lh;
        self.draw_patch_pixels(&self.patches.sp_secret, stats_x, stats_y + lh, buffer);
        self.draw_percent(
            self.player_info.secrets_found as u32,
            width - stats_x,
            stats_y + lh,
            buffer,
        );

        self.draw_patch_pixels(&self.patches.time, time_x, time_y, buffer);
        self.draw_time(
            self.player_info.level_time / TICRATE as u32,
            width / 2 - time_x,
            time_y,
            scale,
            buffer,
        );

        if self.level_info.episode < 3 {
            self.draw_patch_pixels(&self.patches.par, width / 2 + time_x, time_y, buffer);
            self.draw_time(
                self.level_info.partime as u32,
                width - time_x,
                time_y,
                scale,
                buffer,
            );
        }
    }
}
