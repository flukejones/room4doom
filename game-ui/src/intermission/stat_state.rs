use super::{Intermission, SHOW_NEXT_LOC_DELAY, State, TICRATE, TITLE_Y};
use game_config::GameMode;
use render_common::DrawBuffer;
use ui_draw::{UiCanvas, draw_patch};
use wad::umapinfo::TextOrClear;

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
            // Check UMAPINFO intertext before normal transition
            let map_name = self.map_name_for(self.level_info.episode, self.level_info.last);
            if let Some(TextOrClear::Text(t)) = self
                .umapinfo
                .as_ref()
                .and_then(|u| u.get(&map_name))
                .and_then(|e| e.inter_text.as_ref())
            {
                self.inter_text.replace(t.to_ascii_uppercase());
                self.state = State::InterText;
                return;
            }
            if self.mode == GameMode::Commercial {
                self.init_no_state();
            } else {
                self.init_next_loc();
            }
        }
    }

    pub(super) fn draw_level_finish_pixels(&self, pixels: &mut impl DrawBuffer) {
        let viewport = pixels.ui_layout().centered();
        let [sx, sy] = viewport.scale;
        let [half, mut y] = viewport.point(160.0, TITLE_Y);
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
        self.get_this_level_name()
            .draw_centered(half, y, sx, sy, &self.palette, pixels);
    }

    fn draw_time(&self, t: u32, mut x: f32, y: f32, canvas: &mut UiCanvas<'_, impl DrawBuffer>) {
        let mut div = 1;
        if t <= 61 * 59 {
            loop {
                let n = (t / div) % 60;
                x = canvas.number(n, [x, y], 1, &self.patches.nums)
                    - self.patches.colon.width as f32;
                div *= 60;
                if div == 60 || t / div != 0 {
                    canvas.patch(&self.patches.colon, [x, y]);
                }
                if t / div == 0 {
                    break;
                }
            }
        }
    }

    pub(super) fn draw_stats_pixels(&self, buffer: &mut impl DrawBuffer) {
        self.draw_bg(buffer);
        self.draw_animated_bg_pixels(buffer);
        self.draw_level_finish_pixels(buffer);
        let viewport = buffer.ui_layout().centered();
        let mut canvas = UiCanvas::new(buffer, &self.palette, viewport);
        let line_height = 1.5 * self.patches.nums[0].height as f32;
        for (label, value, row) in [
            (
                &self.patches.kills,
                self.player_info.total_kills as u32,
                0.0,
            ),
            (
                &self.patches.items,
                self.player_info.items_collected as u32,
                2.0,
            ),
            (
                &self.patches.sp_secret,
                self.player_info.secrets_found as u32,
                4.0,
            ),
        ] {
            let y = SP_STATSY + row * line_height;
            canvas.patch(label, [SP_STATSX, y]);
            canvas.patch(&self.patches.percent, [320.0 - SP_STATSX, y]);
            canvas.number(value, [320.0 - SP_STATSX, y], 0, &self.patches.nums);
        }
        canvas.patch(&self.patches.time, [SP_TIMEX, SP_TIMEY]);
        self.draw_time(
            self.player_info.level_time / TICRATE as u32,
            160.0 - SP_TIMEX,
            SP_TIMEY,
            &mut canvas,
        );
        if self.level_info.episode < 3 {
            canvas.patch(&self.patches.par, [160.0 + SP_TIMEX, SP_TIMEY]);
            self.draw_time(
                self.level_info.partime as u32,
                320.0 - SP_TIMEX,
                SP_TIMEY,
                &mut canvas,
            );
        }
    }
}
