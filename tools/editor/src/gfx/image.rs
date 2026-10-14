//! Shared preview images and pixel buffers for native editor views.

use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiImageSize {
    pub width: u32,
    pub height: u32,
}

/// Owned straight-alpha RGBA pixels, used while composing asset previews.
#[derive(Clone, Debug, Default)]
pub struct UiPixels {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}
impl UiPixels {
    pub fn new(width: u32, height: u32) -> Self {
        let len = usize::try_from(width)
            .expect("image width")
            .checked_mul(usize::try_from(height).expect("image height"))
            .and_then(|pixels| pixels.checked_mul(4))
            .expect("image dimensions");
        Self {
            width,
            height,
            bytes: vec![0; len],
        }
    }
    pub fn make_mut_bytes(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    #[cfg(test)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Shared immutable image. Equality tracks identity for the GPUI image cache.
#[derive(Clone, Debug, Default)]
pub struct UiImage(Rc<ImageSource>);

#[derive(Debug)]
enum ImageSource {
    Pixels(UiPixels),
    #[cfg(target_os = "macos")]
    Surface(objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>),
}

impl Default for ImageSource {
    fn default() -> Self {
        Self::Pixels(UiPixels::default())
    }
}
impl PartialEq for UiImage {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl UiImage {
    pub fn from_rgba8(pixels: UiPixels) -> Self {
        Self(Rc::new(ImageSource::Pixels(pixels)))
    }
    pub fn size(&self) -> UiImageSize {
        match &*self.0 {
            ImageSource::Pixels(pixels) => UiImageSize {
                width: pixels.width,
                height: pixels.height,
            },
            #[cfg(target_os = "macos")]
            ImageSource::Surface(pixels) => UiImageSize {
                width: u32::try_from(objc2_core_video::CVPixelBufferGetWidth(pixels))
                    .expect("canvas width"),
                height: u32::try_from(objc2_core_video::CVPixelBufferGetHeight(pixels))
                    .expect("canvas height"),
            },
        }
    }
    pub fn width(&self) -> u32 {
        self.size().width
    }
    pub fn height(&self) -> u32 {
        self.size().height
    }
    pub fn as_bytes(&self) -> &[u8] {
        match &*self.0 {
            ImageSource::Pixels(pixels) => &pixels.bytes,
            #[cfg(target_os = "macos")]
            ImageSource::Surface(_) => &[],
        }
    }
    #[cfg(target_os = "macos")]
    pub fn from_surface(
        pixels: objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>,
    ) -> Self {
        Self(Rc::new(ImageSource::Surface(pixels)))
    }
    #[cfg(target_os = "macos")]
    pub fn surface(
        &self,
    ) -> Option<objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>> {
        match &*self.0 {
            ImageSource::Surface(pixels) => Some(pixels.clone()),
            ImageSource::Pixels(_) => None,
        }
    }
}
