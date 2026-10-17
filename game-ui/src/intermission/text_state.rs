use game_config::GameMode;
use render_common::DrawBuffer;
use ui_draw::{UiCanvas, draw_flat};

use super::Intermission;

impl Intermission {
    pub(crate) fn update_inter_text(&mut self) {
        self.inter_text.inc_current_char();
    }

    pub(crate) fn draw_inter_text(&self, buffer: &mut impl DrawBuffer) {
        if let Some(flat) = &self.inter_text_bg {
            draw_flat(flat, &self.palette, buffer);
        }

        let viewport = buffer.ui_layout().centered();
        UiCanvas::new(buffer, &self.palette, viewport).wrapped_text(&self.inter_text, [6.0, 6.0]);
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
