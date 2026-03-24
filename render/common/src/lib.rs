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

/// Apply a health-based vignette to the view area of the framebuffer.
/// - 100 HP: no effect
/// - 0–99 HP: dark red vignette creeping inward (stronger at lower health)
/// - 101–200 HP: subtle gold glow at edges (stronger at higher overhealth)
pub fn draw_health_vignette(
    buf: &mut [u32],
    pitch: usize,
    width: usize,
    view_height: usize,
    health: i32,
) {
    if health == 100 {
        return;
    }

    let half_w = width as f32 * 0.5;
    let half_h = view_height as f32 * 0.5;
    let inv_half_w = 1.0 / half_w;
    let inv_half_h = 1.0 / half_h;

    if health < 100 {
        let t = health.max(0) as f32 / 100.0;
        let intensity = 1.0 - t;
        let max_alpha = VIG_ALPHA_MIN + (VIG_ALPHA_MAX - VIG_ALPHA_MIN) * intensity;
        let start = VIG_START_DEATH + VIG_START_FULL * t * t;

        for y in 0..view_height {
            let dy = (y as f32 - half_h) * inv_half_h;
            let dy2 = dy * dy;
            let row = y * pitch;
            for x in 0..width {
                let dx = (x as f32 - half_w) * inv_half_w;
                let d = (dx * dx + dy2).sqrt();
                if d <= start {
                    continue;
                }
                let raw = ((d - start) / (1.414 - start)).min(1.0);
                let alpha = raw * max_alpha;
                let a256 = (alpha * 256.0) as u32;
                let inv = 256 - a256;
                let pixel = buf[row + x];
                let pr = (pixel >> 16) & 0xFF;
                let pg = (pixel >> 8) & 0xFF;
                let pb = pixel & 0xFF;
                let nr = (pr * inv + VIG_TINT_R * a256) >> 8;
                let ng = (pg * inv) >> 8;
                let nb = (pb * inv) >> 8;
                buf[row + x] = 0xFF000000 | (nr << 16) | (ng << 8) | nb;
            }
        }
    } else {
        let excess = ((health - 100) as f32 / 100.0).min(1.0);
        let max_alpha = VIG_GLOW_ALPHA * excess;
        let start = VIG_GLOW_START + (VIG_START_FULL - VIG_GLOW_START) * (1.0 - excess);

        for y in 0..view_height {
            let dy = (y as f32 - half_h) * inv_half_h;
            let dy2 = dy * dy;
            let row = y * pitch;
            for x in 0..width {
                let dx = (x as f32 - half_w) * inv_half_w;
                let d = (dx * dx + dy2).sqrt();
                if d <= start {
                    continue;
                }
                let raw = ((d - start) / (1.414 - start)).min(1.0);
                let alpha = raw * max_alpha;
                let a256 = (alpha * 256.0) as u32;
                let inv = 256 - a256;
                let pixel = buf[row + x];
                let pr = (pixel >> 16) & 0xFF;
                let pg = (pixel >> 8) & 0xFF;
                let pb = pixel & 0xFF;
                let nr = ((pr * inv + VIG_GLOW_R * a256) >> 8).min(255);
                let ng = ((pg * inv + VIG_GLOW_G * a256) >> 8).min(255);
                let nb = ((pb * inv + VIG_GLOW_B * a256) >> 8).min(255);
                buf[row + x] = 0xFF000000 | (nr << 16) | (ng << 8) | nb;
            }
        }
    }
}
