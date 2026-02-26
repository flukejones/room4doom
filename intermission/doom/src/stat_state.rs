use crate::{Intermission, SHOW_NEXT_LOC_DELAY, State, TICRATE, TITLE_Y};
use gamestate_traits::{DrawBuffer, GameMode};
use hud_util::{draw_num, draw_patch};

const SP_STATSX: f32 = 50.0;
const SP_STATSY: f32 = 50.0;
const SP_TIMEX: f32 = 16.0;
const SP_TIMEY: f32 = 200.0 - 32.0;

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

    pub(super) fn draw_level_finish_pixels(
        &self,
        x_offset: f32,
        sx: f32,
        sy: f32,
        pixels: &mut impl DrawBuffer,
    ) {
        let half = x_offset + 160.0 * sx;
        let mut y = TITLE_Y * sy;
        draw_patch(
            &self.patches.finish,
            half - self.patches.finish.width as f32 * sx / 2.0,
            y,
            sx,
            sy,
            &self.palette,
            pixels,
        );
        y += (5.0 * self.patches.finish.height as f32 * sy) / 4.0;
        let patch = self.get_this_level_name();
        draw_patch(
            patch,
            half - patch.width as f32 * sx / 2.0,
            y,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_percent(
        &self,
        p: u32,
        x: f32,
        y: f32,
        sx: f32,
        sy: f32,
        pixels: &mut impl DrawBuffer,
    ) {
        draw_patch(
            &self.patches.percent,
            x,
            y,
            sx,
            sy,
            &self.palette,
            pixels,
        );
        draw_num(
            p,
            x,
            y,
            0,
            &self.patches.nums,
            sx,
            sy,
            &self.palette,
            pixels,
        );
    }

    fn draw_time(
        &self,
        t: u32,
        mut x: f32,
        y: f32,
        sx: f32,
        sy: f32,
        buffer: &mut impl DrawBuffer,
    ) {
        let mut div = 1;
        if t <= 61 * 59 {
            loop {
                let n = (t / div) % 60;
                x = draw_num(
                    n,
                    x,
                    y,
                    1,
                    &self.patches.nums,
                    sx,
                    sy,
                    &self.palette,
                    buffer,
                ) - self.patches.colon.width as f32 * sx;
                div *= 60;

                if div == 60 || t / div != 0 {
                    draw_patch(&self.patches.colon, x, y, sx, sy, &self.palette, buffer);
                }

                if t / div == 0 {
                    break;
                }
            }
        }
    }

    pub(super) fn draw_stats_pixels(
        &mut self,
        x_ofs: f32,
        sx: f32,
        sy: f32,
        buffer: &mut impl DrawBuffer,
    ) {
        let bg_width = 320.0 * sx;
        let stats_x = SP_STATSX * sx;
        let stats_y = SP_STATSY * sy;
        let time_x = SP_TIMEX * sx;
        let time_y = SP_TIMEY * sy;

        // Background (fullscreen scale, centered)
        self.draw_bg(x_ofs, sx, sy, buffer);
        self.draw_animated_bg_pixels(x_ofs, sx, sy, buffer);
        self.draw_level_finish_pixels(x_ofs, sx, sy, buffer);

        let mut lh = (3.0 * self.patches.nums[0].height as f32 / 2.0) * sy;
        draw_patch(
            &self.patches.kills,
            x_ofs + stats_x,
            stats_y,
            sx,
            sy,
            &self.palette,
            buffer,
        );
        self.draw_percent(
            self.player_info.total_kills as u32,
            x_ofs + bg_width - stats_x,
            stats_y,
            sx,
            sy,
            buffer,
        );

        lh += lh;
        draw_patch(
            &self.patches.items,
            x_ofs + stats_x,
            stats_y + lh,
            sx,
            sy,
            &self.palette,
            buffer,
        );
        self.draw_percent(
            self.player_info.items_collected as u32,
            x_ofs + bg_width - stats_x,
            stats_y + lh,
            sx,
            sy,
            buffer,
        );

        lh += lh;
        draw_patch(
            &self.patches.sp_secret,
            x_ofs + stats_x,
            stats_y + lh,
            sx,
            sy,
            &self.palette,
            buffer,
        );
        self.draw_percent(
            self.player_info.secrets_found as u32,
            x_ofs + bg_width - stats_x,
            stats_y + lh,
            sx,
            sy,
            buffer,
        );

        draw_patch(
            &self.patches.time,
            x_ofs + time_x,
            time_y,
            sx,
            sy,
            &self.palette,
            buffer,
        );
        self.draw_time(
            self.player_info.level_time / TICRATE as u32,
            x_ofs + bg_width / 2.0 - time_x,
            time_y,
            sx,
            sy,
            buffer,
        );

        if self.level_info.episode < 3 {
            draw_patch(
                &self.patches.par,
                x_ofs + bg_width / 2.0 + time_x,
                time_y,
                sx,
                sy,
                &self.palette,
                buffer,
            );
            self.draw_time(
                self.level_info.partime as u32,
                x_ofs + bg_width - time_x,
                time_y,
                sx,
                sy,
                buffer,
            );
        }
    }
}
