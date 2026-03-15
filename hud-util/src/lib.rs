use std::sync::atomic::{AtomicBool, Ordering};

use gamestate_traits::DrawBuffer;
use log::warn;
use wad::WadData;
use wad::types::{WAD_PATCH, WadPalette, WadPatch};

const FONT_START: u8 = b'!';
const FONT_END: u8 = b'_';
const FONT_COUNT: u8 = FONT_END - FONT_START + 1;

static mut CHARS: [WadPatch; FONT_COUNT as usize] = [WAD_PATCH; FONT_COUNT as usize];
static mut CHARS_INITIALISED: AtomicBool = AtomicBool::new(false);

pub fn load_char_patches(wad: &WadData) {
    unsafe {
        #[allow(static_mut_refs)]
        if CHARS_INITIALISED.load(Ordering::Relaxed) {
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
        #[allow(static_mut_refs)]
        CHARS_INITIALISED.store(true, Ordering::Relaxed);
    }
}

fn get_patch_for_char(c: char) -> Option<&'static WadPatch> {
    unsafe {
        #[allow(static_mut_refs)]
        if !CHARS_INITIALISED.load(Ordering::Relaxed) {
            warn!("Character patches not initialised");
            return None;
        }
        if c == ' ' {
            return None;
        }
        #[allow(static_mut_refs)]
        CHARS.get((c as u8 - FONT_START) as usize)
    }
}

/// CRT stretch factor: 320x200 displayed as 320x240
pub const CRT_STRETCH: f32 = 240.0 / 200.0;

/// Returns (scale_x, scale_y) as floats.
/// scale_x = screen_height / 200.0 (standard Doom pixel scaling)
/// scale_y = scale_x * CRT_STRETCH (1.2x taller for CRT aspect)
pub fn hud_scale(pixels: &impl DrawBuffer) -> (f32, f32) {
    let sx = pixels.size().height_f32() / 200.0;
    let sy = sx * CRT_STRETCH;
    (sx, sy)
}

/// Returns (scale_x, scale_y) for full-screen 320x200 patches.
/// These must fit the buffer height while showing correct CRT aspect.
/// Y fills the buffer: scale_y = screen_height / 200.0
/// X is narrower to simulate CRT non-square pixels: scale_x = scale_y /
/// CRT_STRETCH
pub fn fullscreen_scale(pixels: &impl DrawBuffer) -> (f32, f32) {
    let sy = pixels.size().height_f32() / 200.0;
    let sx = sy / CRT_STRETCH;
    (sx, sy)
}

/// Draw a WadPatch at (x, y) with separate X and Y pixel duplication scales.
/// `sx` controls column width, `sy` controls row height. Uses fractional
/// accumulation for both axes so the scaling is correct even at non-integer
/// scales (e.g. sx=0.833 for CRT-correct fullscreen patches).
pub fn draw_patch(
    patch: &WadPatch,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    palette: &WadPalette,
    pixels: &mut impl DrawBuffer,
) {
    let buf_w = pixels.size().width() as i32;
    let buf_h = pixels.size().height() as i32;
    let x_base = x - patch.left_offset as f32 * sx;
    let mut src_col: u32 = 0;

    for column in patch.columns.iter() {
        let col_x_start = (x_base + src_col as f32 * sx).floor() as i32;
        let col_x_end = (x_base + (src_col + 1) as f32 * sx).floor() as i32;
        let col_y = y + column.y_offset as f32 * sy;

        for (src_row, p) in column.pixels.iter().enumerate() {
            let colour = palette.0[*p];
            let row_start = (col_y + src_row as f32 * sy).ceil() as i32;
            let row_end = (col_y + (src_row + 1) as f32 * sy).ceil() as i32;
            for row in row_start..row_end {
                if row < 0 || row >= buf_h {
                    continue;
                }
                for col in col_x_start..col_x_end {
                    if col < 0 || col >= buf_w {
                        continue;
                    }
                    pixels.set_pixel(col as usize, row as usize, colour);
                }
            }
        }

        if column.y_offset == 255 {
            src_col += 1;
        }
    }
}

/// Draw a number right-aligned at (x, y) with separate X and Y scales.
/// Returns the final x position (left edge of the drawn number).
pub fn draw_num(
    p: u32,
    mut x: f32,
    y: f32,
    pad: usize,
    nums: &[WadPatch],
    sx: f32,
    sy: f32,
    palette: &WadPalette,
    pixels: &mut impl DrawBuffer,
) -> f32 {
    let width = nums[0].width as f32 * sx;
    let digits: Vec<u32> = p
        .to_string()
        .chars()
        .map(|d| d.to_digit(10).unwrap())
        .collect();

    for n in digits.iter().rev() {
        x -= width;
        draw_patch(&nums[*n as usize], x, y, sx, sy, palette, pixels);
    }
    if digits.len() <= pad {
        for _ in 0..=pad - digits.len() {
            x -= width;
            draw_patch(&nums[0], x, y, sx, sy, palette, pixels);
        }
    }
    x
}

/// Draw `text` left-to-right from `(x, y)` using the HUD font.
///
/// Returns the x position after the last character. Spaces are rendered as a
/// fixed gap (`4 * sx`). Characters outside the font range are skipped.
pub fn draw_text_line(
    text: &str,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    palette: &WadPalette,
    pixels: &mut impl DrawBuffer,
) -> f32 {
    let mut cx = x;
    for c in text.chars() {
        match get_patch_for_char(c) {
            Some(patch) => {
                draw_patch(patch, cx, y, sx, sy, palette, pixels);
                cx += patch.width as f32 * sx + sx;
            }
            None => cx += 4.0 * sx, // space or unknown
        }
    }
    cx
}

/// Pixel width of `text` at horizontal scale `sx`, without drawing.
pub fn measure_text_line(text: &str, sx: f32) -> f32 {
    text.chars()
        .map(|c| match get_patch_for_char(c) {
            Some(p) => p.width as f32 * sx + sx,
            None => 4.0 * sx,
        })
        .sum()
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
        load_char_patches(wad);

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
        mut x: f32,
        mut y: f32,
        palette: &WadPalette,
        pixels: &mut impl DrawBuffer,
    ) -> Option<()> {
        let (sx, sy) = hud_scale(pixels);
        let width = pixels.size().width_f32();
        let height = pixels.size().height_f32();
        let start_x = x;

        for (i, ch) in self.data.chars().enumerate() {
            if i > self.current_char {
                break;
            }

            // Word len check
            if ch == ' ' {
                let mut len = 0.0;
                for c in self.data[i + 1..].chars() {
                    len += 1.0;
                    if c == ' ' {
                        break;
                    }
                }

                if x + len * self.space_width as f32 + self.space_width as f32 >= width {
                    x = start_x;
                    y += self.line_height as f32 * sy;
                } else {
                    x += self.space_width as f32;
                }
                continue;
            }

            if ch == '\n' {
                x = start_x;
                y += self.line_height as f32 * sy;
                continue;
            }

            let patch = get_patch_for_char(ch).unwrap_or_else(|| panic!("Did not find {ch}"));
            if y + self.line_height as f32 * sy >= height {
                warn!("HUD String to long for screen size");
                return None;
            }

            draw_patch(
                patch,
                x,
                y + self.line_height as f32 * sy - patch.height as f32 * sy,
                sx,
                sy,
                palette,
                pixels,
            );
            x += patch.width as f32 * sx;
        }
        Some(())
    }

    pub fn draw(
        &self,
        x: f32,
        y: f32,
        palette: &WadPalette,
        pixels: &mut impl DrawBuffer,
    ) -> Option<()> {
        self.draw_pixels(x, y, palette, pixels);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{get_patch_for_char, load_char_patches};
    use test_utils::doom1_wad_path;
    use wad::WadData;

    #[test]
    fn load_and_check_chars() {
        let wad = WadData::new(&doom1_wad_path());
        load_char_patches(&wad);

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
