use game_config::GameMode;
use hud_util::hud_scale;
use render_common::DrawBuffer;

use super::Intermission;

impl Intermission {
    pub(crate) fn update_inter_text(&mut self) {
        self.inter_text.inc_current_char();
    }

    pub(crate) fn draw_inter_text(&mut self, buffer: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(buffer);
        let w = buffer.size().width();
        let h = buffer.size().height();

        if let Some(ref flat) = self.inter_text_bg {
            let pal = &self.palette;
            for tile_x in (0..w).step_by(64) {
                for tile_y in (0..h).step_by(64) {
                    for (y, col) in flat.data.chunks(64).enumerate() {
                        for (x, c) in col.iter().enumerate() {
                            let px = tile_x as usize + x;
                            let py = tile_y as usize + y;
                            if px < w as usize && py < h as usize {
                                buffer.set_pixel(px, py, pal.0[*c as usize]);
                            }
                        }
                    }
                }
            }
        }

        let x_ofs = (buffer.size().width_f32() - 320.0 * sx) / 2.0;
        self.inter_text
            .draw_pixels(x_ofs + 6.0 * sx, 6.0 * sy, &self.palette, buffer);
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
