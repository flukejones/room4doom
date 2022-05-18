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

unsafe fn load_char_patches(wad: &WadData) {
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

fn get_char_for(c: char) -> Option<&'static WadPatch> {
    unsafe {
        if !CHARS_INITIALISED {
            warn!("Character patches not initialised");
            return None;
        }
        CHARS.get((c as u8 - FONT_START) as usize)
    }
}

pub struct HUDString(Vec<&'static WadPatch>);

impl HUDString {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn add_char(&mut self, c: char) {
        if let Some(c) = get_char_for(c) {
            self.0.push(c);
        }
    }

    pub fn draw_line(&self) {}
}

#[cfg(test)]
mod tests {
    use crate::{get_char_for, load_char_patches, CHARS};
    use wad::WadData;

    #[test]
    fn load_and_check_chars() {
        let wad = WadData::new("../doom1.wad".into());
        unsafe { load_char_patches(&wad) };

        let l = get_char_for('!').unwrap();
        assert_eq!(l.name.as_str(), "STCFN033");

        let l = get_char_for('$').unwrap();
        assert_eq!(l.name.as_str(), "STCFN036");

        let d = get_char_for('D').unwrap();
        assert_eq!(d.name.as_str(), "STCFN068");
        let o = get_char_for('O').unwrap();
        assert_eq!(o.name.as_str(), "STCFN079");
        let o = get_char_for('O').unwrap();
        assert_eq!(o.name.as_str(), "STCFN079");
        let m = get_char_for('M').unwrap();
        assert_eq!(m.name.as_str(), "STCFN077");

        let l = get_char_for('_').unwrap();
        assert_eq!(l.name.as_str(), "STCFN095");
    }
}
