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
    /// View frustum bounds for clipping
    view_left: f32,
    view_right: f32,
    view_right_usize: usize,
    view_top: f32,
    view_bottom: f32,
    view_bottom_usize: usize,
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
            view_left: 0.0,
            view_right: width as f32,
            view_right_usize: width,
            view_top: 0.0,
            view_bottom: height as f32,
            view_bottom_usize: height,
            covered_pixels: 0,
        }
    }

    /// Reset the depth buffer for a new frame
    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("depth_buffer_reset");

        // Reset all depths to 0.0 (farthest possible in 1/w convention)
        self.depths.fill(-1.0);
        self.covered_pixels = 0;
    }

    /// Resize the depth buffer - recreates the buffer
    pub fn resize(&mut self, width: usize, height: usize) {
        let size = width * height;
        self.depths = vec![0.0; size].into_boxed_slice();
        self.width = width;
        self.height = height;
        self.view_left = 0.0;
        self.view_right = width as f32;
        self.view_top = 0.0;
        self.view_bottom = height as f32;
    }

    /// Set view frustum bounds for clipping
    pub fn set_view_bounds(&mut self, left: f32, right: f32, top: f32, bottom: f32) {
        self.view_left = left;
        self.view_right = right;
        self.view_top = top;
        self.view_bottom = bottom;
    }

    /// Return true if every pixel has been written at least once.
    #[inline]
    pub fn is_full(&self) -> bool {
        // TODO: figure out why the hell drawing isn't always to the edges.
        self.covered_pixels >= self.width * self.height - 50
    }

    /// Read depth at pixel coordinates (unchecked). Returns stored depth (larger = closer).
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
            *t = depth;
            self.covered_pixels += 1;
        }
    }

    /// Test and set depth at pixel coordinates using 1/w convention (larger = closer)
    /// Returns true if the depth buffer was updated (same as before).
    #[inline]
    pub fn test_and_set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) -> bool {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        let index = y * self.width + x;
        unsafe {
            let t = self.depths.get_unchecked_mut(index);
            if depth > *t {
                // if previous was zero (unset background) and we are setting to >0.0,
                // increment covered count
                if *t as i32 == -1 {
                    self.covered_pixels += 1;
                }
                *t = depth;
                true
            } else {
                false
            }
        }
    }

    pub fn is_bbox_covered(
        &self,
        x_min: usize,
        x_max: usize,
        y_min: usize,
        y_max: usize,
        sample_step: usize,
        poly_depth: f32,
    ) -> bool {
        let step = sample_step.max(1);
        // let right = x_max.min(self.view_right_usize);
        let y_max = y_max.min(self.view_bottom_usize);

        for y in (y_min..=y_max).step_by(step) {
            let row_base = y * self.width;
            for x in (x_min..x_max).step_by(step) {
                let idx = row_base + x;
                let current = unsafe { *self.depths.get_unchecked(idx) };
                if current < poly_depth {
                    return false;
                }
            }
        }
        true
    }
}
