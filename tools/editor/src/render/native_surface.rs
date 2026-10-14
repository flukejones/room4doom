//! Share an IOSurface-backed render target with GPUI's Metal compositor.

use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFType};
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer,
    CVPixelBufferPool, kCVMetalTextureUsage, kCVPixelBufferHeightKey,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_32BGRA, kCVReturnSuccess,
};
use objc2_metal::{MTLPixelFormat, MTLTextureType, MTLTextureUsage};
use wgpu_hal::{api::Metal, metal::Device as MetalDevice};

pub(super) struct SurfaceCache {
    textures: CFRetained<CVMetalTextureCache>,
    pool: Option<(u32, u32, CFRetained<CVPixelBufferPool>)>,
}

pub(super) struct NativeTarget {
    pub texture: wgpu::Texture,
    pub pixels: CFRetained<CVPixelBuffer>,
    // CoreVideo requires this wrapper to live until the producer finishes.
    _native: CFRetained<CVMetalTexture>,
}

impl SurfaceCache {
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        // SAFETY: the canvas device was created with the Metal backend.
        let metal = unsafe { device.as_hal::<Metal>() }.ok_or("canvas device is not Metal")?;
        let usage = CFNumber::new_i64(
            i64::try_from((MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead).0)
                .expect("Metal texture usage fits i64"),
        );
        // SAFETY: the framework exports a process-lifetime dictionary key.
        let texture_attributes =
            CFDictionary::from_slices(&[unsafe { kCVMetalTextureUsage }], &[&*usage]);
        let mut cache = ptr::null_mut();
        // SAFETY: the device and out pointer are live for this call.
        let status = unsafe {
            CVMetalTextureCache::create(
                None,
                None,
                metal.raw_device(),
                Some(texture_attributes.as_opaque()),
                NonNull::from(&mut cache),
            )
        };
        if status != kCVReturnSuccess {
            return Err(format!("CoreVideo texture cache: {status}"));
        }
        let cache = NonNull::new(cache).ok_or("CoreVideo returned no texture cache")?;
        Ok(Self {
            // SAFETY: successful creation transfers one retained reference.
            textures: unsafe { CFRetained::from_raw(cache) },
            pool: None,
        })
    }

    pub fn target(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<NativeTarget, String> {
        // Release cache entries from completed frames. Live NativeTargets keep
        // their own wrappers until the producer has finished with them.
        self.textures.flush(0);
        if self
            .pool
            .as_ref()
            .is_none_or(|(w, h, _)| (*w, *h) != (width, height))
        {
            self.pool = Some((width, height, create_pool(width, height)?));
        }
        let mut pixels = ptr::null_mut();
        // CoreVideo only recycles buffers once both the scene and the GPU's
        // retained CVMetalTexture wrappers have released them.
        // SAFETY: the pool and out pointer remain live throughout this call.
        let status = unsafe {
            CVPixelBufferPool::create_pixel_buffer(
                None,
                &self.pool.as_ref().expect("initialized above").2,
                NonNull::from(&mut pixels),
            )
        };
        if status != kCVReturnSuccess {
            return Err(format!("canvas IOSurface: {status}"));
        }
        let pixels = NonNull::new(pixels).ok_or("CoreVideo returned no image buffer")?;
        // SAFETY: successful creation transfers one retained reference.
        let pixels = unsafe { CFRetained::from_raw(pixels) };
        let mut native = ptr::null_mut();
        // SAFETY: this cache belongs to the canvas device; BGRA is a packed
        // single-plane format with exactly these dimensions.
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &self.textures,
                &pixels,
                None,
                MTLPixelFormat::BGRA8Unorm,
                width as usize,
                height as usize,
                0,
                NonNull::from(&mut native),
            )
        };
        if status != kCVReturnSuccess {
            return Err(format!("canvas Metal surface: {status}"));
        }
        let native = NonNull::new(native).ok_or("CoreVideo returned no Metal wrapper")?;
        // SAFETY: successful creation transfers one retained reference.
        let native: CFRetained<CVMetalTexture> = unsafe { CFRetained::from_raw(native) };
        // Retain both the Metal object and its CoreVideo wrapper through the
        // producer's GPU work; the pixel buffer is then handed to GPUI.
        let metal =
            CVMetalTextureGetTexture(&native).ok_or("CoreVideo returned no Metal texture")?;
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        // SAFETY: CoreVideo created a single-mip, single-layer BGRA texture on
        // this device. Both renderer and compositor use this UNORM format.
        let imported = unsafe {
            MetalDevice::texture_from_raw(
                metal,
                wgpu::TextureFormat::Bgra8Unorm,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu_hal::CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
                None,
            )
        };
        let descriptor = wgpu::TextureDescriptor {
            label: Some("editor-gpui-surface"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        // SAFETY: the imported texture belongs to this Metal device and matches
        // the descriptor. Its native reference is owned by the wgpu wrapper.
        let texture = unsafe {
            device.create_texture_from_hal::<Metal>(
                imported,
                &descriptor,
                wgpu::TextureUses::UNINITIALIZED,
            )
        };
        Ok(NativeTarget {
            texture,
            pixels,
            _native: native,
        })
    }
}

fn create_pool(width: u32, height: u32) -> Result<CFRetained<CVPixelBufferPool>, String> {
    let properties = CFDictionary::<CFType, CFType>::empty();
    let w = CFNumber::new_i64(i64::from(width));
    let h = CFNumber::new_i64(i64::from(height));
    let format = CFNumber::new_i64(i64::from(kCVPixelFormatType_32BGRA));
    // SAFETY: these are process-lifetime framework dictionary keys.
    let keys = unsafe {
        [
            kCVPixelBufferIOSurfacePropertiesKey,
            kCVPixelBufferWidthKey,
            kCVPixelBufferHeightKey,
            kCVPixelBufferPixelFormatTypeKey,
        ]
    };
    let values: [&CFType; 4] = [
        properties.as_opaque().as_ref(),
        (*w).as_ref(),
        (*h).as_ref(),
        (*format).as_ref(),
    ];
    let attributes = CFDictionary::from_slices(&keys, &values);
    let mut pool = ptr::null_mut();
    // SAFETY: the attributes describe IOSurface-backed BGRA buffers;
    // the out pointer is valid for this call.
    let status = unsafe {
        CVPixelBufferPool::create(
            None,
            None,
            Some(attributes.as_opaque()),
            NonNull::from(&mut pool),
        )
    };
    if status != kCVReturnSuccess {
        return Err(format!("canvas pixel buffer pool: {status}"));
    }
    let pool = NonNull::new(pool).ok_or("CoreVideo returned no pixel buffer pool")?;
    // SAFETY: successful creation transfers one retained reference.
    Ok(unsafe { CFRetained::from_raw(pool) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_video::{
        CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferLockBaseAddress,
        CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    };

    #[test]
    fn native_targets_preserve_shared_pixels_across_frames_and_resize() {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(descriptor);
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                ..Default::default()
            }))
        else {
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .expect("native target test device");
        let mut cache = SurfaceCache::new(&device).expect("native texture cache");
        // An odd row width also exercises CoreVideo's padded storage.
        let target = cache.target(&device, 3, 2).expect("native render target");
        clear(&device, &queue, &target, wgpu::Color::RED);
        // The scene can retain an image after the producer has released its
        // target. Later paints and resizing must not overwrite that image.
        let pixels = target.pixels;
        drop(target.texture);
        drop(target._native);
        for (width, height, color, expected) in [
            (3, 2, wgpu::Color::GREEN, [0, 255, 0, 255]),
            (3, 2, wgpu::Color::BLUE, [255, 0, 0, 255]),
            (5, 1, wgpu::Color::WHITE, [255, 255, 255, 255]),
        ] {
            let next = cache.target(&device, width, height).expect("next target");
            clear(&device, &queue, &next, color);
            assert_pixels(&next.pixels, width as usize, height as usize, expected);
            assert_pixels(&pixels, 3, 2, [0, 0, 255, 255]);
        }
    }

    fn clear(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &NativeTarget,
        color: wgpu::Color,
    ) {
        let view = target.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("native target clear completes");
    }

    fn assert_pixels(pixels: &CVPixelBuffer, width: usize, height: usize, expected: [u8; 4]) {
        let flags = CVPixelBufferLockFlags::ReadOnly;
        assert_eq!(
            // SAFETY: the retained buffer is no longer in use by the producer.
            unsafe { CVPixelBufferLockBaseAddress(pixels, flags) },
            kCVReturnSuccess
        );
        let stride = CVPixelBufferGetBytesPerRow(pixels);
        let data = CVPixelBufferGetBaseAddress(pixels).cast::<u8>();
        assert!(!data.is_null());
        // SAFETY: successful read-only locking exposes height complete rows.
        let bytes = unsafe { std::slice::from_raw_parts(data, stride * height) };
        for row in bytes.chunks_exact(stride) {
            for pixel in row[..width * 4].as_chunks::<4>().0 {
                assert_eq!(*pixel, expected);
            }
        }
        assert_eq!(
            // SAFETY: use the same read-only flags as the successful lock above.
            unsafe { CVPixelBufferUnlockBaseAddress(pixels, flags) },
            kCVReturnSuccess
        );
    }
}
