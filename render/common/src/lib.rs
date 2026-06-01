pub mod wipe;

use level::LevelData;
use math::{Angle, Bam, FixedT};
use pic_data::PicData;

// =============================================================================
// Constants
// =============================================================================

/// OG Doom's original resolution.
const OG_WIDTH: f32 = 320.0;
const OG_HEIGHT: f32 = 200.0;

/// OG STBAR height in native 200px space.
pub const STBAR_HEIGHT: i32 = 32;

/// Classic Doom fuzz Y-offsets. The table cycles per-pixel to create the
/// spectre shimmer effect.
pub const FUZZ_TABLE: [i32; 50] = [
    1, -1, 1, -1, 1, 1, -1, 1, 1, -1, 1, 1, 1, -1, 1, 1, 1, -1, -1, -1, 1, -1, -1, -1, 1, 1, 1, 1,
    -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, -1, -1, -1, 1, 1, 1, 1, -1, 1, 1, -1, 1,
];

// --- Health vignette tuning ---

/// Tint colour red channel (0x00–0xFF). Green and blue are always 0.
const VIG_TINT_R: u32 = 0x30;
/// Max blend alpha at 99 HP (subtle edge tint).
const VIG_ALPHA_MIN: f32 = 0.55;
/// Max blend alpha at 0 HP (heavy vignette).
const VIG_ALPHA_MAX: f32 = 0.95;
/// Normalised radial distance where tint begins at 0 HP (0=center, 1=edge).
const VIG_START_DEATH: f32 = 0.1;
/// Normalised radial distance where tint begins at 99 HP (barely visible).
const VIG_START_FULL: f32 = 0.85;
/// Overhealth glow max alpha (at 200 HP).
const VIG_GLOW_ALPHA: f32 = 0.15;
/// Overhealth glow radial start (normalised distance from center).
const VIG_GLOW_START: f32 = 0.6;
/// Overhealth glow tint colour (warm gold).
const VIG_GLOW_R: u32 = 0xFF;
const VIG_GLOW_G: u32 = 0xD0;
const VIG_GLOW_B: u32 = 0x40;
/// Max radial distance (view corner) for the vignette LUT.
const VIG_DIST_MAX: f32 = 1.414;
/// Distance→alpha-table index scale: maps `[0, VIG_DIST_MAX]` onto `[0, 255]`.
const VIG_DIST_QUANT: f32 = 255.0 / VIG_DIST_MAX;

// =============================================================================
// Traits
// =============================================================================

pub trait GameRenderer {
    /// Render the 3D view from the given viewpoint.
    fn render_player_view(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &mut PicData,
    );

    /// Present the current buffer to screen.
    fn flip_and_present(&mut self);

    /// Get the framebuffer used for direct draw access.
    fn frame_buffer(&mut self) -> &mut impl DrawBuffer;

    /// Capture the current buffer as the wipe source (old frame).
    fn start_wipe(&mut self);

    /// Overdraw old-frame columns on the current buffer. Returns true when
    /// the melt is complete.
    fn do_wipe(&mut self) -> bool;

    /// Whether a wipe transition is currently in progress.
    fn is_wiping(&self) -> bool;

    fn buffer_size(&self) -> &BufferSize;

    /// Return the cached radial-distance LUT for the health vignette,
    /// rebuilding it (via [`GameRenderer::build_vignette_lut`]) when the view
    /// size changes. Impls own the cache storage.
    fn vignette_lut(&mut self) -> &[f32];

    /// Draw the cached health vignette for the current view. Impls hoist their
    /// disjoint framebuffer / LUT field borrows and call
    /// [`Self::blend_vignette`].
    fn draw_health_vignette(&mut self, health: i32);

    /// Build the per-pixel radial-distance LUT for the health vignette, sized
    /// to the current view. `lut[y * width + x]` is the normalised distance
    /// from the view centre — geometry only, so impls cache it and rebuild
    /// only when the view size changes.
    fn build_vignette_lut(&self) -> Vec<f32> {
        let size = self.buffer_size();
        let width = size.width_usize();
        let view_height = size.view_height_usize();
        let half_w = width as f32 * 0.5;
        let half_h = view_height as f32 * 0.5;
        let inv_half_w = 1.0 / half_w;
        let inv_half_h = 1.0 / half_h;

        let mut lut = vec![0.0f32; width * view_height];
        for y in 0..view_height {
            let dy = (y as f32 - half_h) * inv_half_h;
            let dy2 = dy * dy;
            let row = y * width;
            for x in 0..width {
                let dx = (x as f32 - half_w) * inv_half_w;
                lut[row + x] = (dx * dx + dy2).sqrt();
            }
        }
        lut
    }

    /// Apply a health-based vignette to the view area using a pre-built
    /// radial-distance `lut` (from [`GameRenderer::build_vignette_lut`], sized
    /// `width * view_height`).
    /// - 100 HP: no effect
    /// - 0–99 HP: dark red vignette creeping inward (stronger at lower health)
    /// - 101–200 HP: subtle gold glow at edges (stronger at higher overhealth)
    ///
    /// `self`-free so the caller can hoist disjoint `&mut buf` / `&lut`
    /// borrows from its own fields without the borrow checker conflating them.
    fn blend_vignette(
        buf: &mut [u32],
        dist_lut: &[f32],
        alpha_lut: &[u32; 256],
        pitch: usize,
        width: usize,
        view_height: usize,
        glow: bool,
    ) {
        if glow {
            for y in 0..view_height {
                let row = y * pitch;
                let lut_row = y * width;
                for x in 0..width {
                    let idx = (dist_lut[lut_row + x] * VIG_DIST_QUANT) as usize;
                    let a256 = alpha_lut[idx.min(255)];
                    if a256 == 0 {
                        continue;
                    }
                    let inv = 256 - a256;
                    let pixel = buf[row + x];
                    let new = glow_blend_pixel(pixel, inv, a256);
                    buf[row + x] = new;
                }
            }
        } else {
            // Tint is a convex blend (inv + a256 == 256, no per-channel
            // saturation), so R+B and G can be blended two-at-a-time via SWAR:
            // products stay within their byte lanes and never overflow u32.
            for y in 0..view_height {
                let row = y * pitch;
                let lut_row = y * width;
                for x in 0..width {
                    let idx = (dist_lut[lut_row + x] * VIG_DIST_QUANT) as usize;
                    let a256 = alpha_lut[idx.min(255)];
                    if a256 == 0 {
                        continue;
                    }
                    let inv = 256 - a256;
                    buf[row + x] = tint_blend_pixel(buf[row + x], inv, a256);
                }
            }
        }
    }

    /// Compute the `(start, max_alpha)` blend parameters for `health`, or
    /// `None` at 100 HP (no vignette).
    fn vignette_params(health: i32) -> Option<(f32, f32)> {
        if health == 100 {
            None
        } else if health < 100 {
            let t = health.max(0) as f32 / 100.0;
            Some((
                VIG_START_DEATH + VIG_START_FULL * t * t,
                VIG_ALPHA_MIN + (VIG_ALPHA_MAX - VIG_ALPHA_MIN) * (1.0 - t),
            ))
        } else {
            let excess = ((health - 100) as f32 / 100.0).min(1.0);
            Some((
                VIG_GLOW_START + (VIG_START_FULL - VIG_GLOW_START) * (1.0 - excess),
                VIG_GLOW_ALPHA * excess,
            ))
        }
    }
}

/// Convex damage-tint blend of one `0x00RRGGBB` pixel (R+B and G blended
/// two-at-a-time via SWAR; the red tint is added into R's slot). No per-channel
/// saturation is needed — `inv + a256 == 256`, so each channel stays ≤ 255 and
/// the byte-lane products never overflow `u32`. Output is `0xFFRRGGBB`.
#[inline]
fn tint_blend_pixel(pixel: u32, inv: u32, a256: u32) -> u32 {
    let rb = pixel & 0x00FF_00FF;
    let g = pixel & 0x0000_FF00;
    let rb = ((rb * inv + ((VIG_TINT_R * a256) << 16)) >> 8) & 0x00FF_00FF;
    let g = ((g * inv) >> 8) & 0x0000_FF00;
    0xFF00_0000 | rb | g
}

/// Additive overhealth-glow blend of one `0x00RRGGBB` pixel. `inv = 256 -
/// a256`. Per-channel `.min(255)` saturation (the glow is additive, not
/// convex) makes SWAR more costly than it saves, so this stays scalar. Output
/// is `0xFFRRGGBB`.
#[inline]
fn glow_blend_pixel(pixel: u32, inv: u32, a256: u32) -> u32 {
    let nr = (((pixel >> 16) & 0xFF) * inv + VIG_GLOW_R * a256) >> 8;
    let ng = (((pixel >> 8) & 0xFF) * inv + VIG_GLOW_G * a256) >> 8;
    let nb = ((pixel & 0xFF) * inv + VIG_GLOW_B * a256) >> 8;
    0xFF00_0000 | (nr.min(255) << 16) | (ng.min(255) << 8) | nb.min(255)
}

/// Build the 256-entry blend-weight (`a256`, 0..=256) table indexed by
/// quantised radial distance (`d * VIG_DIST_QUANT`). Entries at or below
/// `start` are 0 so those pixels are skipped by the caller.
pub fn build_vignette_alpha_lut(start: f32, max_alpha: f32) -> [u32; 256] {
    let inv_span = 1.0 / (VIG_DIST_MAX - start);
    let mut lut = [0u32; 256];
    for (idx, a) in lut.iter_mut().enumerate() {
        let d = idx as f32 / VIG_DIST_QUANT;
        if d <= start {
            continue;
        }
        let raw = ((d - start) * inv_span).clamp(0.0, 1.0);
        *a = (raw * max_alpha * 256.0) as u32;
    }
    lut
}

#[cfg(test)]
mod vignette_tests {
    /// `tint_blend_pixel` (SWAR convex blend) must match the scalar
    /// per-channel reference for every channel value and blend weight.
    #[test]
    fn tint_swar_matches_scalar() {
        for a256 in 0u32..=256 {
            let inv = 256 - a256;
            for pixel in (0u32..=0x00FF_FFFF).step_by(0x4321) {
                let pr = (pixel >> 16) & 0xFF;
                let pg = (pixel >> 8) & 0xFF;
                let pb = pixel & 0xFF;
                let nr = (pr * inv + super::VIG_TINT_R * a256) >> 8;
                let ng = (pg * inv) >> 8;
                let nb = (pb * inv) >> 8;
                let scalar = 0xFF00_0000 | (nr << 16) | (ng << 8) | nb;
                let swar = super::tint_blend_pixel(pixel, inv, a256);
                assert_eq!(swar, scalar, "pixel {pixel:#010X}, a256 {a256}");
            }
        }
    }

    /// `glow_blend_pixel` (u64 SWAR + per-lane saturation) must match the
    /// scalar `.min(255)` reference for every channel value and blend weight,
    /// including overflowing channels.
    #[test]
    fn glow_swar_matches_scalar() {
        for a256 in 0u32..=256 {
            let inv = 256 - a256;
            for pixel in (0u32..=0x00FF_FFFF).step_by(0x4321) {
                let pr = (pixel >> 16) & 0xFF;
                let pg = (pixel >> 8) & 0xFF;
                let pb = pixel & 0xFF;
                let nr = ((pr * inv + super::VIG_GLOW_R * a256) >> 8).min(255);
                let ng = ((pg * inv + super::VIG_GLOW_G * a256) >> 8).min(255);
                let nb = ((pb * inv + super::VIG_GLOW_B * a256) >> 8).min(255);
                let scalar = 0xFF00_0000 | (nr << 16) | (ng << 8) | nb;
                let swar = super::glow_blend_pixel(pixel, inv, a256);
                assert_eq!(swar, scalar, "pixel {pixel:#010X}, a256 {a256}");
            }
        }
    }
}

pub trait DrawBuffer {
    fn size(&self) -> &BufferSize;
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32);
    fn read_pixel(&self, x: usize, y: usize) -> u32;
    fn get_buf_index(&self, x: usize, y: usize) -> usize;
    fn pitch(&self) -> usize;
    fn buf_mut(&mut self) -> &mut [u32];
    fn debug_flip_and_present(&mut self);
}

// =============================================================================
// Types
// =============================================================================

/// Pre-resolved weapon sprite for rendering. Extracted from gameplay's PspDef
/// so render-common doesn't depend on StateData/SpriteNum.
#[derive(Clone, Copy)]
pub struct RenderPspDef {
    pub active: bool,
    pub sprite: usize,
    pub frame: u32,
    pub sx: f32,
    pub sy: f32,
}

impl Default for RenderPspDef {
    fn default() -> Self {
        Self {
            active: false,
            sprite: 0,
            frame: 0,
            sx: 0.0,
            sy: 0.0,
        }
    }
}

/// Snapshot of player/level state needed for one frame.
pub struct RenderView {
    /// Player world X position.
    pub x: FixedT,
    /// Player world Y position.
    pub y: FixedT,
    /// Player z (floor level of mobj).
    pub z: FixedT,
    /// Eye height (includes bobbing).
    pub viewz: FixedT,
    /// Base eye height above floor.
    pub viewheight: FixedT,
    /// Facing angle (BAM).
    pub angle: Angle<Bam>,
    /// Vertical look pitch in radians.
    pub lookdir: f32,
    /// Fixed colormap index (0 = off, >0 = invulnerability/light goggles).
    pub fixedcolormap: usize,
    /// Extra brightness from gun flash.
    pub extralight: usize,
    /// Whether the player mobj has the Shadow flag (partial invisibility).
    pub is_shadow: bool,
    /// Pre-resolved subsector index into `LevelData::subsectors()`.
    pub subsector_id: usize,
    /// Weapon overlay sprites.
    pub psprites: [RenderPspDef; 2],
    /// Sector light level at player's subsector (for weapon sprite lighting).
    pub sector_lightlevel: usize,
    /// Player mobj pointer for skipping own sprite in rendering.
    pub player_mobj_id: usize,
    /// Sub-tic interpolation fraction (0.0..1.0) for smooth rendering.
    pub frac: f32,
    /// Sub-tic interpolation fraction in fixed-point.
    pub frac_fp: FixedT,
    /// Monotonic game tic counter (for time-based effects like voxel spin).
    pub game_tic: u32,
}

/// Pre-computed buffer dimensions for fast 2.5D rendering.
#[derive(Clone, Copy)]
pub struct BufferSize {
    hi_res: bool,
    width_usize: usize,
    height_usize: usize,
    width: i32,
    height: i32,
    width_f32: f32,
    height_f32: f32,
    statusbar_height: i32,
}

impl BufferSize {
    pub const fn new(width: usize, height: usize) -> Self {
        Self {
            hi_res: height > 200,
            width_usize: width,
            height_usize: height,
            width: width as i32,
            height: height as i32,
            width_f32: width as f32,
            height_f32: height as f32,
            statusbar_height: 0,
        }
    }

    pub const fn hi_res(&self) -> bool {
        self.hi_res
    }
    pub const fn width(&self) -> i32 {
        self.width
    }
    pub const fn height(&self) -> i32 {
        self.height
    }
    pub const fn half_width(&self) -> i32 {
        self.width / 2
    }
    pub const fn half_height(&self) -> i32 {
        self.height / 2
    }
    pub const fn width_usize(&self) -> usize {
        self.width_usize
    }
    pub const fn height_usize(&self) -> usize {
        self.height_usize
    }
    pub const fn width_f32(&self) -> f32 {
        self.width_f32
    }
    pub const fn height_f32(&self) -> f32 {
        self.height_f32
    }
    pub const fn half_width_f32(&self) -> f32 {
        self.half_width() as f32
    }
    pub const fn half_height_f32(&self) -> f32 {
        self.half_height() as f32
    }

    pub fn set_statusbar_height(&mut self, h: i32) {
        self.statusbar_height = h;
    }
    pub const fn statusbar_height(&self) -> i32 {
        self.statusbar_height
    }
    pub const fn view_height(&self) -> i32 {
        self.height - self.statusbar_height
    }
    pub const fn view_height_usize(&self) -> usize {
        (self.height - self.statusbar_height) as usize
    }
    pub const fn view_height_f32(&self) -> f32 {
        (self.height - self.statusbar_height) as f32
    }
    pub const fn half_view_height(&self) -> i32 {
        self.view_height() / 2
    }
    pub const fn half_view_height_f32(&self) -> f32 {
        self.half_view_height() as f32
    }
}

// =============================================================================
// Projection
// =============================================================================

/// Derive FOV and focal length for a given buffer size, keeping the view
/// proportional to OG Doom's 320x200 at the specified base hfov (90°).
/// Scales with buffer height so hi-res (400p) works correctly.
/// Returns `(hfov, vfov, focal_length)` in radians / pixels.
pub fn og_projection(base_hfov: f32, buf_width: f32, buf_height: f32) -> (f32, f32, f32) {
    let scale = buf_height / OG_HEIGHT;
    let og_half_w = (OG_WIDTH / 2.0) * scale;
    let focal_length = og_half_w / (base_hfov / 2.0).tan();
    let hfov = 2.0 * ((buf_width / 2.0) / focal_length).atan();
    let vfov = 2.0 * ((buf_height / 2.0) / focal_length).atan();
    (hfov, vfov, focal_length)
}

// =============================================================================
// Pixel effects
// =============================================================================

/// Darken a pixel to ~6/16 brightness (approximates colourmap 6).
#[inline(always)]
pub fn fuzz_darken(pixel: u32) -> u32 {
    let r = (pixel >> 16) & 0xFF;
    let g = (pixel >> 8) & 0xFF;
    let b = pixel & 0xFF;
    let shift = 3;
    0xFF000000 | (((r * 6) >> shift) << 16) | (((g * 6) >> shift) << 8) | ((b * 6) >> shift)
}

/// Apply fuzz effect to a horizontal span of the framebuffer. For each pixel
/// in `x_start..=x_end`, reads the pixel at a Y-offset from `FUZZ_TABLE`,
/// darkens it, and writes to `(x, y)`. `fuzz_pos` is the running table index
/// and is updated in place so consecutive spans stay in phase.
pub fn fuzz_span(
    buf: &mut [u32],
    pitch: usize,
    height: usize,
    y: usize,
    x_start: usize,
    x_end: usize,
    fuzz_pos: &mut usize,
) {
    let row_base = y * pitch;
    for x in x_start..=x_end {
        let offset = FUZZ_TABLE[*fuzz_pos % FUZZ_TABLE.len()];
        let src_y = (y as i32 + offset).clamp(0, height as i32 - 1) as usize;
        let src_idx = src_y * pitch + x;
        let darkened = fuzz_darken(buf[src_idx]);
        buf[row_base + x] = darkened;
        *fuzz_pos += 1;
    }
}
