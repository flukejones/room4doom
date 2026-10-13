use game_config::GameMode;
use hud_util::{draw_flat, hud_scale};
use render_common::DrawBuffer;

use super::Intermission;

impl Intermission {
    pub(crate) fn update_inter_text(&mut self) {
        self.inter_text.inc_current_char();
    }

    pub(crate) fn draw_inter_text(&self, buffer: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(buffer);
        if let Some(flat) = &self.inter_text_bg {
            draw_flat(flat, &self.palette, buffer);
        }

        let [x_ofs, y_ofs] = buffer.ui_layout().origin;
        self.inter_text
            .draw_pixels(x_ofs + 6.0 * sx, y_ofs + 6.0 * sy, &self.palette, buffer);
    }

    pub(crate) fn skip_inter_text(&mut self) {
        if self.inter_text.is_at_end() {
            if self.mode == GameMode::Commercial || self.level_info.episode > 2 {
                self.init_no_state();
            } else {
                self.init_next_loc();
            }
        } else {
            self.inter_text.set_draw_all();
        }
    }
}
