//! Generic Doom bitmap drawing in buffer or original artwork coordinates.

use std::sync::OnceLock;

use log::warn;
use render_common::{DrawBuffer, UiViewport};
use wad::WadData;
use wad::types::{COLUMN_END, WAD_PATCH, WadFlat, WadPalette, WadPatch};

const FONT_START: u8 = b'!';
const FONT_END: u8 = b'_';
const FONT_COUNT: u8 = FONT_END - FONT_START + 1;
const FLAT_SIZE: usize = 64;

static CHARS: OnceLock<Vec<WadPatch>> = OnceLock::new();

/// Load numbered sprite patches matching `pattern` (e.g. `"STTNUM"`,
/// `"WINUM"`). Reads 10 lumps from `{pattern}{start}` through
/// `{pattern}{start+9}`.
pub fn load_num_sprites(pattern: &str, start: usize, wad: &WadData) -> [WadPatch; 10] {
    let mut nums: [WadPatch; 10] = [WAD_PATCH; 10];
    for (i, num) in nums.iter_mut().enumerate() {
        let p = i + start;
        let lump = wad.get_lump(&format!("{pattern}{p}")).unwrap();
        *num = WadPatch::from_lump(lump);
    }
    nums
}

/// Load the 6 status-bar key card sprites (`STKEYS0`..`STKEYS5`).
pub fn load_key_sprites(wad: &WadData) -> [WadPatch; 6] {
    std::array::from_fn(|i| {
        let lump = wad.get_lump(&format!("STKEYS{i}")).unwrap();
        WadPatch::from_lump(lump)
    })
}

pub fn load_char_patches(wad: &WadData) {
    CHARS.get_or_init(|| {
        let mut patches = Vec::with_capacity(FONT_COUNT as usize);
        for i in 0..FONT_COUNT {
            let f = i + FONT_START;
            if let Some(lump) = wad.get_lump(&format!("STCFN{f:0>3}")) {
                patches.push(WadPatch::from_lump(lump));
            } else {
                warn!("Missing STCFN{f:0>3}");
                patches.push(WadPatch::default());
            }
        }
        patches
    });
}

fn get_patch_for_char(c: char) -> Option<&'static WadPatch> {
    let chars = CHARS.get()?;
    if c == ' ' {
        return None;
    }
    chars.get((c as u8 - FONT_START) as usize)
}

/// Draw bitmap assets in original artwork coordinates through any pixel buffer.
/// The viewport chooses placement and text-flow bounds, without adding a clip.
pub struct UiCanvas<'a, B: DrawBuffer> {
    pixels: &'a mut B,
    palette: &'a WadPalette,
    viewport: UiViewport,
}

impl<'a, B: DrawBuffer> UiCanvas<'a, B> {
    pub fn new(pixels: &'a mut B, palette: &'a WadPalette, viewport: UiViewport) -> Self {
        Self {
            pixels,
            palette,
            viewport,
        }
    }

    pub fn patch(&mut self, patch: &WadPatch, at: [f32; 2]) {
        let [x, y] = self.viewport.point(at[0], at[1]);
        let [sx, sy] = self.viewport.scale;
        draw_patch(patch, x, y, sx, sy, self.palette, self.pixels);
    }

    /// Draw right-aligned digits and return their left edge in artwork coordinates.
    pub fn number(&mut self, value: u32, at: [f32; 2], pad: usize, digits: &[WadPatch]) -> f32 {
        let [x, y] = self.viewport.point(at[0], at[1]);
        let [sx, sy] = self.viewport.scale;
        let left = draw_num(value, x, y, pad, digits, sx, sy, self.palette, self.pixels);
        (left - self.viewport.origin[0]) / sx
    }

    pub fn text(&mut self, text: &str, at: [f32; 2]) {
        let [x, y] = self.viewport.point(at[0], at[1]);
        let [sx, sy] = self.viewport.scale;
        draw_text_line(text, x, y, sx, sy, self.palette, self.pixels);
    }

    pub fn tinted_text(&mut self, text: &str, at: [f32; 2], tint: u32) {
        let [x, y] = self.viewport.point(at[0], at[1]);
        let [sx, sy] = self.viewport.scale;
        draw_text_line_tinted(text, x, y, sx, sy, self.palette, tint, self.pixels);
    }

    pub fn wrapped_text(&mut self, text: &HUDString, at: [f32; 2]) -> Option<()> {
        let [x, y] = self.viewport.point(at[0], at[1]);
        text.draw_in(x, y, self.viewport, self.palette, self.pixels)
    }
}

/// Tile a UI flat with the same CRT-correct scale as patches and text.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "UI flat coordinates are quantized to texels"
)]
pub fn draw_flat(flat: &WadFlat, palette: &WadPalette, pixels: &mut impl DrawBuffer) {
    let layout = pixels.ui_layout();
    let (w, h) = (pixels.size().width_usize(), pixels.size().height_usize());
    for y in 0..h {
        let row = ((y as f32 - layout.origin[1]) / layout.scale[1])
            .floor()
            .rem_euclid(FLAT_SIZE as f32) as usize;
        for x in 0..w {
            let col = ((x as f32 - layout.origin[0]) / layout.scale[0])
                .floor()
                .rem_euclid(FLAT_SIZE as f32) as usize;
            pixels.set_pixel(x, y, palette.0[flat.data[row * FLAT_SIZE + col] as usize]);
        }
    }
}

/// Draw a WadPatch at (x, y) with separate X and Y pixel duplication scales.
///
/// `sx` controls column width, `sy` controls row height. Uses fractional
/// accumulation for both axes so the scaling is correct even at non-integer
/// scales (e.g. sx=0.833 for CRT-correct fullscreen patches).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "screen, input and game-state values deliberately retain their established numeric widths and rounding"
)]
pub fn draw_patch(
    patch: &WadPatch,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    palette: &WadPalette,
    pixels: &mut impl DrawBuffer,
) {
    let buf_w = pixels.size().width();
    let buf_h = pixels.size().height();
    let x_base = x - patch.left_offset as f32 * sx;
    let mut src_col: u32 = 0;

    for column in &patch.columns {
        let col_x_start = (x_base + src_col as f32 * sx).floor() as i32;
        let col_x_end = (x_base + (src_col + 1) as f32 * sx).floor() as i32;
        let col_y = y + column.y_offset as f32 * sy;

        for (src_row, p) in column.pixels.iter().enumerate() {
            let colour = palette.0[*p as usize];
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

        if column.y_offset == COLUMN_END {
            src_col += 1;
        }
    }
}

/// Draw a patch with a colour tint applied. `tint` is 0xRRGGBB where
/// each channel multiplies the palette colour (255 = full, 0 = black).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "screen, input and game-state values deliberately retain their established numeric widths and rounding"
)]
pub fn draw_patch_tinted(
    patch: &WadPatch,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    palette: &WadPalette,
    tint: u32,
    pixels: &mut impl DrawBuffer,
) {
    let buf_w = pixels.size().width();
    let buf_h = pixels.size().height();
    let x_base = x - patch.left_offset as f32 * sx;
    let tr = (tint >> 16) & 0xFF;
    let tg = (tint >> 8) & 0xFF;
    let tb = tint & 0xFF;
    let mut src_col: u32 = 0;

    for column in &patch.columns {
        let col_x_start = (x_base + src_col as f32 * sx).floor() as i32;
        let col_x_end = (x_base + (src_col + 1) as f32 * sx).floor() as i32;
        let col_y = y + column.y_offset as f32 * sy;

        for (src_row, p) in column.pixels.iter().enumerate() {
            let base = palette.0[*p as usize];
            let br = (base >> 16) & 0xFF;
            let bg = (base >> 8) & 0xFF;
            let bb = base & 0xFF;
            // Use max channel as luminance — preserves font shading
            let lum = br.max(bg).max(bb);
            let r = (lum * tr / 255).min(255);
            let g = (lum * tg / 255).min(255);
            let b = (lum * tb / 255).min(255);
            let colour = 0xFF000000 | (r << 16) | (g << 8) | b;
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

        if column.y_offset == COLUMN_END {
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

/// Draw tinted text. `tint` is 0xRRGGBB.
pub fn draw_text_line_tinted(
    text: &str,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    palette: &WadPalette,
    tint: u32,
    pixels: &mut impl DrawBuffer,
) -> f32 {
    let mut cx = x;
    for c in text.chars() {
        match get_patch_for_char(c) {
            Some(patch) => {
                draw_patch_tinted(patch, cx, y, sx, sy, palette, tint, pixels);
                cx += patch.width as f32 * sx + sx;
            }
            None => cx += 4.0 * sx,
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
        if let Some(p) = get_patch_for_char(c)
            && p.height as i32 > self.line_height
        {
            self.line_height = p.height as i32;
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
        x: f32,
        y: f32,
        palette: &WadPalette,
        pixels: &mut impl DrawBuffer,
    ) -> Option<()> {
        self.draw_in(x, y, pixels.ui_layout().screen(), palette, pixels)
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "font dimensions enter scaled drawing coordinates"
    )]
    fn draw_in(
        &self,
        mut x: f32,
        mut y: f32,
        viewport: UiViewport,
        palette: &WadPalette,
        pixels: &mut impl DrawBuffer,
    ) -> Option<()> {
        let [sx, sy] = viewport.scale;
        let width = viewport.right();
        let height = viewport.bottom();
        let start_x = x;

        for (i, ch) in self.data.char_indices() {
            if i > self.current_char {
                break;
            }

            // Word len check
            if ch == ' ' {
                let next_word = self.data[i + 1..].split_whitespace().next().unwrap_or("");
                let word_width: f32 = next_word
                    .chars()
                    .filter_map(get_patch_for_char)
                    .map(|p| p.width as f32 * sx)
                    .sum();
                if x + self.space_width as f32 * sx + word_width > width {
                    x = start_x;
                    y += self.line_height as f32 * sy;
                } else {
                    x += self.space_width as f32 * sx;
                }
                continue;
            }

            if ch == '\n' {
                x = start_x;
                y += self.line_height as f32 * sy;
                continue;
            }

            let patch = get_patch_for_char(ch).unwrap_or_else(|| panic!("Did not find {ch}"));
            if x > start_x && x + patch.width as f32 * sx > width {
                x = start_x;
                y += self.line_height as f32 * sy;
            }
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
        self.draw_pixels(x, y, palette, pixels)
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
