use std::f32;

#[cfg(feature = "hprof")]
use coarse_prof::profile;

/// Depth buffer for visibility testing with efficient polygon clipping
#[derive(Clone, Debug)]
pub struct DepthBuffer {
    /// Depth values for each pixel using 1/w convention
    /// Larger values indicate closer objects (0.0 = farthest)
    depths: Box<[f32]>,
    /// Screen dimensions
    width: usize,
    height: usize,
    /// Number of pixels that have been written to (depth > 0.0)
    covered_pixels: usize,
}

impl DepthBuffer {
    /// Create a new depth buffer with given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        let depths = vec![-1.0; size].into_boxed_slice();

        Self {
            depths,
            width,
            height,
            covered_pixels: 0,
        }
    }

    /// Reset the depth buffer for a new frame
    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("depth_buffer_reset");

        // Reset all depths to -1.0 (unwritten sentinel, any valid 1/w depth will be >
        // -1.0)
        self.depths.fill(-1.0);
        self.covered_pixels = 0;
    }

    /// Resize the depth buffer - recreates the buffer
    pub fn resize(&mut self, width: usize, height: usize) {
        let size = width * height;
        self.depths = vec![-1.0; size].into_boxed_slice();
        self.width = width;
        self.height = height;
        self.covered_pixels = 0;
    }

    /// Return true if every pixel has been written at least once.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.covered_pixels >= self.width * self.height
    }

    /// Read depth at pixel coordinates (unchecked). Returns stored depth
    /// (larger = closer).
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
        unsafe {
            let t = self.depths.get_unchecked_mut(index);
            if *t == -1.0 {
                self.covered_pixels += 1;
            }
            *t = depth;
        }
    }

    /// Test and set depth at pixel coordinates using 1/w convention (larger =
    /// closer) Returns true if the depth buffer was updated (same as
    /// before).
    #[inline]
    pub fn test_and_set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) -> bool {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        let index = y * self.width + x;
        unsafe {
            let t = self.depths.get_unchecked_mut(index);
            if depth > *t {
                // if previous was -1.0 (unwritten sentinel), increment covered count
                if *t == -1.0 {
                    self.covered_pixels += 1;
                }
                *t = depth;
                true
            } else {
                false
            }
        }
    }
}
