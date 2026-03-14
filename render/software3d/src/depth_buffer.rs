use std::f32;

#[cfg(feature = "hprof")]
use coarse_prof::profile;

const TILE_SIZE: usize = 8;
/// Depth value written for sky pixels. Positive but smaller than any real
/// geometry 1/w, so solid surfaces always overwrite sky via the normal
/// `depth > old` test. Sky pixels do not count toward occlusion coverage.
pub const SKY_DEPTH: f32 = f32::EPSILON;

/// Depth buffer with single-level Hi-Z tile rejection.
///
/// Uses 1/w convention: larger values = closer to camera. Each tile tracks
/// the minimum depth of first-writes (farthest geometry drawn first). Since
/// rendering is front-to-back, first-writes set the farthest visible depth
/// per pixel. A polygon whose closest point is behind all first-write depths
/// in every overlapping tile is guaranteed fully occluded.
#[derive(Clone, Debug)]
pub struct DepthBuffer {
    depths: Box<[f32]>,
    width: usize,
    height: usize,
    covered_pixels: usize,

    tile_min_depth: Box<[f32]>,
    tile_covered: Box<[u16]>,
    tiles_x: usize,
    tiles_y: usize,
}

impl DepthBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        let tiles_x = (width + TILE_SIZE - 1) / TILE_SIZE;
        let tiles_y = (height + TILE_SIZE - 1) / TILE_SIZE;
        let tile_count = tiles_x * tiles_y;

        Self {
            depths: vec![-1.0; size].into_boxed_slice(),
            width,
            height,
            covered_pixels: 0,
            tile_min_depth: vec![f32::MAX; tile_count].into_boxed_slice(),
            tile_covered: vec![0; tile_count].into_boxed_slice(),
            tiles_x,
            tiles_y,
        }
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("depth_buffer_reset");

        self.depths.fill(-1.0);
        self.covered_pixels = 0;
        self.tile_min_depth.fill(f32::MAX);
        self.tile_covered.fill(0);
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.depths = vec![-1.0; width * height].into_boxed_slice();
        self.width = width;
        self.height = height;
        self.covered_pixels = 0;

        self.tiles_x = (width + TILE_SIZE - 1) / TILE_SIZE;
        self.tiles_y = (height + TILE_SIZE - 1) / TILE_SIZE;
        let tile_count = self.tiles_x * self.tiles_y;
        self.tile_min_depth = vec![f32::MAX; tile_count].into_boxed_slice();
        self.tile_covered = vec![0; tile_count].into_boxed_slice();
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.covered_pixels >= self.width * self.height
    }

    #[inline]
    pub fn peek_depth_unchecked(&self, x: usize, y: usize) -> f32 {
        let index = y * self.width + x;
        unsafe { *self.depths.get_unchecked(index) }
    }

    #[inline]
    pub fn set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        let index = y * self.width + x;
        let old = self.depths[index];
        if old == -1.0 {
            self.covered_pixels += 1;
            let tile_idx = (y / TILE_SIZE) * self.tiles_x + (x / TILE_SIZE);
            if depth < self.tile_min_depth[tile_idx] {
                self.tile_min_depth[tile_idx] = depth;
            }
            self.tile_covered[tile_idx] += 1;
        }
        self.depths[index] = depth;
    }

    #[inline]
    pub fn test_and_set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) -> bool {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        let index = y * self.width + x;
        let old = self.depths[index];
        if depth > old {
            if old == -1.0 {
                self.covered_pixels += 1;
                let tile_idx = (y / TILE_SIZE) * self.tiles_x + (x / TILE_SIZE);
                if depth < self.tile_min_depth[tile_idx] {
                    self.tile_min_depth[tile_idx] = depth;
                }
                self.tile_covered[tile_idx] += 1;
            }
            self.depths[index] = depth;
            true
        } else {
            false
        }
    }

    /// Write sky depth to a pixel. Only writes if the pixel is empty (-1.0).
    /// Does not increment coverage or update Hi-Z tiles — sky is a backdrop,
    /// not solid occlusion.
    #[inline]
    pub fn set_sky_depth_unchecked(&mut self, x: usize, y: usize) {
        let index = y * self.width + x;
        if self.depths[index] == -1.0 {
            self.depths[index] = SKY_DEPTH;
        }
    }

    /// Raw pointer to the depth buffer for bulk writes. Bypasses tile
    /// tracking and coverage counting — use only for span-based rendering
    /// where the depth values themselves are sufficient for per-pixel tests.
    #[inline]
    pub fn depths_raw_ptr(&mut self) -> *mut f32 {
        self.depths.as_mut_ptr()
    }

    /// Width of the depth buffer (for stride calculations).
    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Hi-Z occlusion test. Conservative: never produces false rejections.
    #[inline]
    pub fn is_occluded_hiz(
        &self,
        screen_min_x: usize,
        screen_min_y: usize,
        screen_max_x: usize,
        screen_max_y: usize,
        poly_max_depth: f32,
    ) -> bool {
        let tx0 = screen_min_x / TILE_SIZE;
        let ty0 = screen_min_y / TILE_SIZE;
        let tx1 = screen_max_x / TILE_SIZE;
        let ty1 = screen_max_y / TILE_SIZE;

        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let ti = ty * self.tiles_x + tx;
                let tile_w = TILE_SIZE.min(self.width - tx * TILE_SIZE);
                let tile_h = TILE_SIZE.min(self.height - ty * TILE_SIZE);
                let expected = (tile_w * tile_h) as u16;
                if self.tile_covered[ti] < expected {
                    return false;
                }
                if poly_max_depth > self.tile_min_depth[ti] {
                    return false;
                }
            }
        }

        true
    }
}
