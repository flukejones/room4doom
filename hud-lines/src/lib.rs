use gamestate_traits::{MachinationTrait, PixelBuf};
use log::warn;
use wad::{
    lumps::{WadPatch, WAD_PATCH},
    WadData,
};

const FONT_START: u8 = b'!';
const FONT_END: u8 = b'_';
const FONT_COUNT: u8 = FONT_END - FONT_START + 1;

static mut CHARS: [WadPatch; FONT_COUNT as usize] = [WAD_PATCH; FONT_COUNT as usize];
static mut CHARS_INITIALISED: bool = false;

pub unsafe fn load_char_patches(wad: &WadData) {
    if CHARS_INITIALISED {
        return;
    }
    //let mut chars = [WAD_PATCH; FONT_COUNT as usize];
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
        CHARS.get((c as u8 - FONT_START) as usize)
    }
}

pub struct HUDString {
    data: String,
    line_height: i32,
}

impl HUDString {
    pub fn new(wad: &WadData) -> Self {
        unsafe { load_char_patches(wad) };

        let line_height = get_patch_for_char('A').unwrap().width as i32;
        Self {
            data: String::new(),
            line_height,
        }
    }

    pub fn add_char(&mut self, c: char) {
        self.data.push(c);
        // let line_height = get_patch_for_char(c).unwrap().width  as i32;
        // if line_height > self.line_height {
        //     self.line_height = line_height;
        // }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn draw_wrapped(
        &self,
        mut x: i32,
        mut y: i32,
        machination: &impl MachinationTrait,
        pixels: &mut PixelBuf,
    ) -> Option<()> {
        let space_width = get_patch_for_char('_').unwrap().width as i32;
        let width = pixels.width() as i32;
        let height = pixels.height() as i32;
        let start_x = x;

        for (i, ch) in self.data.chars().enumerate() {
            if ch == ' ' {
                let mut len = 0;
                for c in self.data[i + 1..].chars() {
                    len += 1;
                    if c == ' ' {
                        break;
                    }
                }

                if x + len * space_width + space_width >= width {
                    x = start_x;
                    y += self.line_height + 1;
                } else {
                    x += space_width;
                }
                continue;
            }

            let patch = get_patch_for_char(ch).unwrap();
            if y + self.line_height >= height {
                warn!("HUD String to long for screen size");
                return None;
            }

            machination.draw_patch(patch, x, y - patch.height as i32 / 2, pixels);
            x += patch.width as i32 + 1;
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{get_patch_for_char, load_char_patches, CHARS};
    use wad::WadData;

    #[test]
    fn load_and_check_chars() {
        let wad = WadData::new("../doom1.wad".into());
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
