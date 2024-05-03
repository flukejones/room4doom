use gamestate_traits::{MachinationTrait, PixelBuffer};
use log::warn;
use wad::lumps::{WadPatch, WAD_PATCH};
use wad::WadData;

const FONT_START: u8 = b'!';
const FONT_END: u8 = b'_';
const FONT_COUNT: u8 = FONT_END - FONT_START + 1;

static mut CHARS: [WadPatch; FONT_COUNT as usize] = [WAD_PATCH; FONT_COUNT as usize];
static mut CHARS_INITIALISED: bool = false;

pub unsafe fn load_char_patches(wad: &WadData) {
    if CHARS_INITIALISED {
        return;
    }
    for i in 0..FONT_COUNT {
        let f = i + FONT_START;
        if let Some(lump) = wad.get_lump(&format!("STCFN{f:0>3}")) {
            CHARS[i as usize] = WadPatch::from_lump(lump);
        } else {
            warn!("Missing STCFN{f:0>3}");
        }
    }
    CHARS_INITIALISED = true;
}

fn get_patch_for_char(c: char) -> Option<&'static WadPatch> {
    unsafe {
        if !CHARS_INITIALISED {
            warn!("Character patches not initialised");
            return None;
        }
        if c == ' ' {
            return None;
        }
        CHARS.get((c as u8 - FONT_START) as usize)
    }
}

/// Specifically to help create static arrays of `WadPatch`
pub const HUD_STRING: HUDString = HUDString::default();

#[derive(Debug, Clone)]
pub struct HUDString {
    data: String,
    line_height: i32,
    current_char: usize,
    space_width: i32,
}

impl HUDString {
    const fn default() -> Self {
        Self {
            data: String::new(),
            line_height: 10,
            current_char: 0,
            space_width: 4,
        }
    }

    pub fn new(wad: &WadData) -> Self {
        unsafe { load_char_patches(wad) };

        Self {
            data: String::new(),
            line_height: 10,
            current_char: 0,
            space_width: 4,
        }
    }

    pub fn line_height(&self) -> i32 {
        self.line_height
    }

    pub fn line(&self) -> &str {
        &self.data
    }

    pub fn replace(&mut self, string: String) {
        self.data = string;
    }

    pub fn add_char(&mut self, c: char) {
        self.data.push(c);
        if let Some(p) = get_patch_for_char(c) {
            if p.height as i32 > self.line_height {
                self.line_height = p.height as i32;
            }
        }
    }

    pub fn inc_current_char(&mut self) {
        if self.current_char < self.data.len() {
            self.current_char += 1;
        }
    }

    pub fn is_at_end(&self) -> bool {
        self.current_char == self.data.len()
    }

    pub fn set_draw_all(&mut self) {
        self.current_char = self.data.len();
    }

    pub fn clear(&mut self) {
        self.current_char = 0;
        self.data.clear();
    }

    pub fn draw_pixels(
        &self,
        mut x: i32,
        mut y: i32,
        machination: &impl MachinationTrait,
        pixels: &mut dyn PixelBuffer,
    ) -> Option<()> {
        let f = pixels.size().height() / 200;
        let width = pixels.size().width();
        let height = pixels.size().height();
        let start_x = x;

        for (i, ch) in self.data.chars().enumerate() {
            if i > self.current_char {
                break;
            }

            // Word len check
            if ch == ' ' {
                let mut len = 0;
                for c in self.data[i + 1..].chars() {
                    len += 1;
                    if c == ' ' {
                        break;
                    }
                }

                if x + len * self.space_width + self.space_width >= width {
                    x = start_x;
                    y += self.line_height * f;
                } else {
                    x += self.space_width;
                }
                continue;
            }

            if ch == '\n' {
                x = start_x;
                y += self.line_height * f;
                continue;
            }

            let patch = get_patch_for_char(ch).unwrap_or_else(|| panic!("Did not find {ch}"));
            if y + self.line_height * f >= height {
                warn!("HUD String to long for screen size");
                return None;
            }

            machination.draw_patch_pixels(
                patch,
                x,
                y + self.line_height * f - patch.height as i32 * f,
                pixels,
            );
            x += patch.width as i32 * f;
        }
        Some(())
    }

    pub fn draw(
        &self,
        x: i32,
        y: i32,
        machination: &impl MachinationTrait,
        pixels: &mut dyn PixelBuffer,
    ) -> Option<()> {
        self.draw_pixels(x, y, machination, pixels);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{get_patch_for_char, load_char_patches};
    use wad::WadData;

    #[test]
    fn load_and_check_chars() {
        let wad = WadData::new("../../doom1.wad".into());
        unsafe { load_char_patches(&wad) };

        let l = get_patch_for_char('!').unwrap();
        assert_eq!(l.name.as_str(), "STCFN033");

        let l = get_patch_for_char('$').unwrap();
        assert_eq!(l.name.as_str(), "STCFN036");

        let d = get_patch_for_char('D').unwrap();
        assert_eq!(d.name.as_str(), "STCFN068");
        let o = get_patch_for_char('O').unwrap();
        assert_eq!(o.name.as_str(), "STCFN079");
        let o = get_patch_for_char('O').unwrap();
        assert_eq!(o.name.as_str(), "STCFN079");
        let m = get_patch_for_char('M').unwrap();
        assert_eq!(m.name.as_str(), "STCFN077");

        let l = get_patch_for_char('_').unwrap();
        assert_eq!(l.name.as_str(), "STCFN095");
    }
}
