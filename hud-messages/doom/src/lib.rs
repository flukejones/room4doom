use gamestate_traits::{DrawBuffer, GameTraits, KeyCode, SubsystemTrait, TICRATE};
use hud_util::{HUD_STRING, HUDString, hud_scale, load_char_patches};
use wad::WadData;
use wad::types::WadPalette;

const COUNT_DOWN: i32 = 2 * TICRATE;

pub struct Messages {
    palette: WadPalette,
    screen_width: i32,
    screen_height: i32,
    lines: [HUDString; 4],
    /// Start of lines (`lines` is to be used circular)
    start: usize,
    /// End of lines (wraps around)
    current: usize,
    count_down: i32,
}

impl Messages {
    pub fn new(wad: &WadData) -> Self {
        // initialise
        load_char_patches(wad);
        let palette = wad.playpal_iter().next().unwrap();

        Self {
            palette,
            screen_width: 0,
            screen_height: 0,
            lines: [HUD_STRING; 4],
            start: 0,
            current: 0,
            count_down: COUNT_DOWN,
        }
    }

    pub fn add_line(&mut self, line: String) {
        self.current += 1;
        if self.current == self.lines.len() {
            self.current = 0;
        }
        self.lines[self.current].clear();
        self.lines[self.current].replace(line);
        //self.lines[self.current].set_draw_all();
        self.count_down = COUNT_DOWN;

        if self.start == self.current {
            // Shunt start over to make room for next line.
            self.start += 1;
            if self.start == self.lines.len() {
                self.start = 0;
            }
        }
    }

    pub fn pop_last(&mut self) {
        if self.start == self.current {
            return;
        }
        self.start += 1;
        if self.start == self.lines.len() {
            self.start = 0;
        }
    }

    pub fn draw_wrapped(&self, buffer: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(buffer);

        let x_ofs = (buffer.size().width_f32() - 320.0 * sx) / 2.0;
        let x = x_ofs + 10.0;
        let mut y = 2.0;
        let mut pos = self.start;
        loop {
            if pos >= self.lines.len() {
                pos = 0;
            }
            if self.lines[pos].line().is_empty() {
                if pos == self.current {
                    break;
                }
                pos += 1;
                continue;
            }

            self.lines[pos].draw(x, y, &self.palette, buffer);
            y += self.lines[pos].line_height() as f32 * sy + 1.0;

            if pos == self.current {
                break;
            }
            pos += 1;
        }
    }
}

impl SubsystemTrait for Messages {
    fn init(&mut self, _game: &impl GameTraits) {
        for l in self.lines.iter_mut() {
            l.clear();
        }
    }

    fn responder(&mut self, _sc: KeyCode, _game: &mut impl GameTraits) -> bool {
        false
    }

    fn ticker(&mut self, game: &mut impl GameTraits) -> bool {
        for l in self.lines.iter_mut() {
            if !l.line().is_empty() {
                l.inc_current_char();
            }
        }
        if let Some(msg) = game.player_msg_take() {
            self.add_line(msg.to_ascii_uppercase());
        }
        self.count_down -= 1;
        if self.count_down <= 0 {
            self.count_down = COUNT_DOWN;
            self.start = 0;
            self.current = 0;
            for l in self.lines.iter_mut() {
                l.clear();
            }
        }
        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        self.screen_width = buffer.size().width();
        self.screen_height = buffer.size().height();
        self.draw_wrapped(buffer);
    }
}

#[cfg(test)]
mod tests {
    use crate::Messages;
    use test_utils::doom1_wad_path;
    use wad::WadData;

    #[test]
    fn check_cycle_through_max() {
        let wad = WadData::new(&doom1_wad_path());

        let mut msgs = Messages::new(&wad);

        msgs.add_line("0".to_string());
        msgs.add_line("1".to_string());
        msgs.add_line("2".to_string());
        msgs.add_line("3".to_string());

        assert_eq!(msgs.lines[0].line(), "3");
        assert_eq!(msgs.lines[1].line(), "0");
        assert_eq!(msgs.lines[2].line(), "1");
        assert_eq!(msgs.lines[3].line(), "2");

        assert_eq!(msgs.lines[msgs.current].line(), "3");
        assert_eq!(msgs.lines[msgs.start].line(), "0");

        msgs.add_line("11".to_string());
        assert_eq!(msgs.lines[msgs.start].line(), "1");
        assert_eq!(msgs.lines[0].line(), "3");
        assert_eq!(msgs.lines[1].line(), "11");
        assert_eq!(msgs.lines[msgs.current].line(), "11");

        msgs.add_line("12".to_string());
        assert_eq!(msgs.lines[msgs.start].line(), "2");
        assert_eq!(msgs.lines[0].line(), "3");
        assert_eq!(msgs.lines[1].line(), "11");
        assert_eq!(msgs.lines[2].line(), "12");
        assert_eq!(msgs.lines[3].line(), "2");
        assert_eq!(msgs.lines[msgs.current].line(), "12");

        msgs.pop_last();
        assert_eq!(msgs.lines[msgs.start].line(), "3");

        while msgs.start != msgs.current {
            msgs.pop_last();
        }
        assert_eq!(msgs.lines[msgs.start].line(), "12");
        assert_eq!(msgs.lines[msgs.current].line(), "12");

        msgs.pop_last();
        assert_eq!(msgs.lines[msgs.start].line(), "12");
        assert_eq!(msgs.lines[msgs.current].line(), "12");
    }
}
