use crate::{Intermission, State, SHOW_NEXT_LOC_DELAY, TICRATE, TITLE_Y};
use game_traits::{GameMode, MachinationTrait, PixelBuf};

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

        self.count -= 1;
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

    fn draw_num(&self, p: u32, mut x: i32, y: i32, buffer: &mut PixelBuf) {
        let width = self.patches.nums[0].width as i32;
        let digits: Vec<u32> = p
            .to_string()
            .chars()
            .map(|d| d.to_digit(10).unwrap())
            .collect();
        for n in digits.iter().rev() {
            x -= width;
            let num = &self.patches.nums[*n as usize];
            self.draw_patch(num, x, y, buffer);
        }
    }

    fn draw_percent(&self, p: u32, x: i32, y: i32, buffer: &mut PixelBuf) {
        self.draw_patch(&self.patches.percent, x, y, buffer);
        self.draw_num(p, x, y, buffer);
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

        if self.level_info.epsd < 3 {
            self.draw_patch(
                &self.patches.par,
                SCREEN_WIDTH / 2 + SP_TIMEX,
                SP_TIMEY,
                buffer,
            );
        }
    }
}
