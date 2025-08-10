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
    view_top: f32,
    view_bottom: f32,
}

impl DepthBuffer {
    /// Create a new depth buffer with given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        let depths = vec![0.0; size].into_boxed_slice();

        Self {
            depths,
            width,
            height,
            view_left: 0.0,
            view_right: width as f32,
            view_top: 0.0,
            view_bottom: height as f32,
        }
    }

    /// Reset the depth buffer for a new frame
    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("depth_buffer_reset");

        // Reset all depths to 0.0 (farthest possible in 1/w convention)
        self.depths.fill(0.0);
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

    #[inline]
    pub fn set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        let index = y * self.width + x;
        unsafe {
            let t = self.depths.get_unchecked_mut(index);
            *t = depth;
        }
    }

    /// Test and set depth at pixel coordinates using 1/w convention (larger = closer)
    #[inline]
    pub fn test_and_set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) -> bool {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        let index = y * self.width + x;
        unsafe {
            let t = self.depths.get_unchecked_mut(index);
            if depth > *t {
                *t = depth;
                true
            } else {
                false
            }
        }
    }

    pub fn is_bbox_covered(
        &self,
        x_min: f32,
        x_max: f32,
        y_min: f32,
        y_max: f32,
        sample_step: usize,
        poly_depth: f32,
    ) -> bool {
        let step = sample_step.max(1);

        let left = x_min.max(self.view_left).max(0.0).floor() as isize;
        let right = x_max.min(self.view_right).max(0.0).ceil() as isize;
        let top = y_min.max(self.view_top).max(0.0).floor() as isize;
        let bottom = y_max.min(self.view_bottom).max(0.0).ceil() as isize;

        if left > right || top > bottom {
            return false;
        }

        let left = left as usize;
        let right = right as usize;
        let top = top as usize;
        let bottom = bottom as usize;

        for y in (top..=bottom).step_by(step) {
            let row_base = y * self.width;
            for x in (left..=right).step_by(step) {
                let idx = row_base + x;
                // safe because bbox clamped to view bounds / width/height
                let current = unsafe { *self.depths.get_unchecked(idx) };
                if current < poly_depth {
                    return false;
                }
            }
        }
        true
    }
}
