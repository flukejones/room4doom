use crate::{Intermission, State, SHOW_NEXT_LOC_DELAY, TICRATE, TITLE_Y};
use game_traits::{util::draw_num, GameMode, MachinationTrait, PixelBuf};

const SCREEN_WIDTH: i32 = 320;
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

    pub(super) fn draw_level_finish(&self, buffer: &mut PixelBuf) {
        let mut y = TITLE_Y;
        self.draw_patch(
            &self.patches.finish,
            160 - self.patches.enter.width as i32 / 2,
            y,
            buffer,
        );
        y += (5 * self.patches.finish.height as i32) / 4;
        let ep = self.level_info.epsd as usize;
        let patch = &self.level_names[ep][self.level_info.last as usize];
        self.draw_patch(patch, 160 - patch.width as i32 / 2, y, buffer);
    }

    fn draw_percent(&self, p: u32, x: i32, y: i32, buffer: &mut PixelBuf) {
        self.draw_patch(&self.patches.percent, x, y, buffer);
        draw_num(p, x, y, 0, &self.patches.nums, self, buffer);
    }

    fn draw_time(&self, t: u32, mut x: i32, y: i32, buffer: &mut PixelBuf) {
        let mut div = 1;
        if t <= 61 * 59 {
            loop {
                let n = (t / div) % 60;
                x = draw_num(n, x, y, 1, &self.patches.nums, self, buffer)
                    - self.patches.colon.width as i32;
                div *= 60;

                if div == 60 || t / div != 0 {
                    self.draw_patch(&self.patches.colon, x, y, buffer);
                }

                if t / div == 0 {
                    break;
                }
            }
        }
    }

    pub(super) fn draw_stats(&mut self, buffer: &mut PixelBuf) {
        // Background
        self.draw_patch(&self.bg_patches[self.current_bg], 0, 0, buffer);
        self.draw_animated_bg(buffer);
        self.draw_level_finish(buffer);

        let mut lh = (3 * self.patches.nums[0].height / 2) as i32;
        self.draw_patch(&self.patches.kills, SP_STATSX, SP_STATSY, buffer);
        self.draw_percent(
            self.player_info.skills as u32,
            SCREEN_WIDTH - SP_STATSX,
            SP_STATSY,
            buffer,
        );

        lh += lh;
        self.draw_patch(&self.patches.items, SP_STATSX, SP_STATSY + lh, buffer);
        self.draw_percent(
            self.player_info.sitems as u32,
            SCREEN_WIDTH - SP_STATSX,
            SP_STATSY + lh,
            buffer,
        );

        lh += lh;
        self.draw_patch(&self.patches.sp_secret, SP_STATSX, SP_STATSY + lh, buffer);
        self.draw_percent(
            self.player_info.ssecret as u32,
            SCREEN_WIDTH - SP_STATSX,
            SP_STATSY + lh,
            buffer,
        );

        self.draw_patch(&self.patches.time, SP_TIMEX, SP_TIMEY, buffer);
        self.draw_time(
            self.player_info.stime / TICRATE as u32,
            SCREEN_WIDTH / 2 - SP_TIMEX,
            SP_TIMEY,
            buffer,
        );

        if self.level_info.epsd < 3 {
            self.draw_patch(
                &self.patches.par,
                SCREEN_WIDTH / 2 + SP_TIMEX,
                SP_TIMEY,
                buffer,
            );
            self.draw_time(
                self.level_info.partime as u32,
                SCREEN_WIDTH - SP_TIMEX,
                SP_TIMEY,
                buffer,
            );
        }
    }
}
