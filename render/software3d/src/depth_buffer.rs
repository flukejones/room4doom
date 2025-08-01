#[cfg(feature = "hprof")]
use coarse_prof::profile;

/// Depth buffer for visibility testing with efficient polygon clipping
#[derive(Clone, Debug)]
pub struct DepthBuffer {
    /// Depth values for each pixel (z-coordinate in view space)
    /// Negative values indicate closer objects (following view space
    /// convention)
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
        let depths = vec![f32::INFINITY; size].into_boxed_slice();

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

        // Reset all depths to infinity (farthest possible)
        self.depths.fill(f32::INFINITY);
    }

    /// Resize the depth buffer - recreates the buffer
    pub fn resize(&mut self, width: usize, height: usize) {
        let size = width * height;
        self.depths = vec![f32::INFINITY; size].into_boxed_slice();
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
    pub fn test_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) -> usize {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        // if x >= self.width || y >= self.height {
        //     return usize::MAX;
        // }

        let index = y * self.width + x;
        unsafe {
            let t = self.depths.get_unchecked_mut(index);
            if depth < *t { index } else { usize::MAX }
        }
    }

    /// Set depth at pixel coordinates (unchecked)
    #[inline]
    pub fn set_depth_unchecked(&mut self, depth: f32, index: usize) {
        #[cfg(feature = "hprof")]
        profile!("set_depth_unchecked");
        unsafe {
            *self.depths.get_unchecked_mut(index) = depth;
        }
    }
}
