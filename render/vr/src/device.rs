//! wgpu objects on the runtime's Metal device.
//!
//! `XR_KHR_metal_enable` names the `MTLDevice` the application must render on,
//! so wgpu cannot create its own. `wgpu-hal` is an unstable surface — this is
//! the one module that depends on it, so a wgpu bump breaks here and nowhere
//! else.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandQueue, MTLDevice, MTLPixelFormat, MTLTexture, MTLTextureType};
use wgpu_hal::api::Metal;
use wgpu_hal::metal::{Device as MetalDevice, Queue as MetalQueue};
use wgpu_hal::{Adapter as _, Instance as _};

use crate::SWAPCHAIN_FORMAT;
use crate::session::VrError;

/// Builds a wgpu device/queue on the runtime's `MTLDevice`.
pub(crate) fn build_wgpu(
    queue: &Retained<ProtocolObject<dyn MTLCommandQueue>>,
) -> Result<(wgpu::Instance, wgpu::Device, wgpu::Queue), VrError> {
    let device = queue.device();
    // SAFETY: the retained queue owns this live runtime-selected Metal device.
    let hal_device =
        unsafe { MetalDevice::device_from_raw(device.clone(), wgpu::Features::empty()) };
    // SAFETY: as above; the timestamp period is Metal's nanosecond base.
    let hal_queue = unsafe { MetalQueue::queue_from_raw(queue.clone(), 1.0) };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = runtime_adapter(&instance, device.registryID())?;

    // SAFETY: the hal device and queue were built from the same MTLDevice
    // this adapter represents.
    let open = wgpu_hal::OpenDevice::<Metal> {
        device: hal_device,
        queue: hal_queue,
    };
    let (device, queue) = unsafe {
        adapter.create_device_from_hal(
            open,
            &wgpu::DeviceDescriptor {
                label: Some("vr"),
                required_features: wgpu::Features::empty(),
                // Real adapter limits: a per-eye headset target exceeds the
                // downlevel defaults.
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::Performance,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            },
        )
    }
    .map_err(VrError::Device)?;
    Ok((instance, device, queue))
}

fn runtime_adapter(instance: &wgpu::Instance, registry_id: u64) -> Result<wgpu::Adapter, VrError> {
    // SAFETY: this instance was created with the Metal backend enabled.
    let metal =
        unsafe { instance.as_hal::<Metal>() }.ok_or(VrError::NoMetalAdapter(registry_id))?;
    // SAFETY: no surface compatibility is required for OpenXR swapchains.
    for exposed in unsafe { metal.enumerate_adapters(None) } {
        // SAFETY: the exposed limits and empty features are supported by this adapter.
        let opened = unsafe {
            exposed.adapter.open(
                wgpu::Features::empty(),
                &exposed.capabilities.limits,
                &wgpu::MemoryHints::Performance,
            )
        }
        .map_err(VrError::MetalAdapter)?;
        if opened.device.raw_device().registryID() == registry_id {
            // SAFETY: this adapter belongs to the instance and matches the runtime's device.
            return Ok(unsafe { instance.create_adapter_from_hal::<Metal>(exposed) });
        }
    }
    Err(VrError::NoMetalAdapter(registry_id))
}

/// Wraps an sRGB runtime image through a shared UNORM view for already encoded pixels.
/// Null images, incompatible formats, and native view creation failures retain their context.
pub(crate) fn wrap_texture(
    device: &wgpu::Device,
    raw: *mut c_void,
    width: u32,
    height: u32,
) -> Result<wgpu::TextureView, VrError> {
    // SAFETY: the runtime owns the texture for the swapchain's lifetime;
    // retain takes our own reference for the wrapper.
    let texture: Retained<ProtocolObject<dyn MTLTexture>> =
        unsafe { Retained::retain(raw.cast()) }.ok_or(VrError::MissingSwapchainImage)?;
    let format = texture.pixelFormat();
    if format != MTLPixelFormat::BGRA8Unorm_sRGB {
        return Err(VrError::UnexpectedSwapchainImageFormat(format.0));
    }
    let texture = texture
        .newTextureViewWithPixelFormat(MTLPixelFormat::BGRA8Unorm)
        .ok_or(VrError::SwapchainImageView {
            format: format.0,
        })?;
    let copy_size = wgpu_hal::CopyExtent {
        width,
        height,
        depth: 1,
    };
    // SAFETY: the compatible view is 2D, single-layer, single-mip, and uses
    // the UNORM format declared below.
    let hal_texture = unsafe {
        MetalDevice::texture_from_raw(
            texture,
            SWAPCHAIN_FORMAT,
            MTLTextureType::Type2D,
            1,
            1,
            copy_size,
        )
    };
    let descriptor = wgpu::TextureDescriptor {
        label: Some("vr_eye"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SWAPCHAIN_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    // SAFETY: the hal texture was built from this device's backend and
    // matches the descriptor.
    let texture = unsafe { device.create_texture_from_hal::<Metal>(hal_texture, &descriptor) };
    Ok(texture.create_view(&wgpu::TextureViewDescriptor::default()))
}

#[cfg(test)]
mod tests {
    use std::slice;

    use objc2_metal::{
        MTLBuffer, MTLCreateSystemDefaultDevice, MTLResourceOptions, MTLStorageMode,
        MTLTextureDescriptor, MTLTextureUsage,
    };

    use super::*;

    #[test]
    fn encoded_colours_render_unchanged_into_srgb_swapchain_storage() {
        let metal = MTLCreateSystemDefaultDevice().expect("native Metal test needs a device");
        let commands = metal
            .newCommandQueue()
            .expect("Metal device creates a command queue");
        let (_instance, device, queue) =
            build_wgpu(&commands).expect("runtime Metal queue opens in wgpu");
        let buffer = metal
            .newBufferWithLength_options(256, MTLResourceOptions::StorageModeShared)
            .expect("Metal allocates shared test storage");
        let descriptor = MTLTextureDescriptor::new();
        descriptor.setTextureType(MTLTextureType::Type2D);
        descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm_sRGB);
        // SAFETY: a one-pixel target is within the device's texture limits.
        unsafe {
            descriptor.setWidth(1);
            descriptor.setHeight(1);
        }
        descriptor.setStorageMode(MTLStorageMode::Shared);
        descriptor.setUsage(
            MTLTextureUsage::RenderTarget
                | MTLTextureUsage::ShaderRead
                | MTLTextureUsage::PixelFormatView,
        );
        let texture = buffer
            .newTextureWithDescriptor_offset_bytesPerRow(&descriptor, 0, 256)
            .expect("Metal maps shared sRGB image");
        let view = wrap_texture(
            &device,
            (&*texture as *const ProtocolObject<dyn MTLTexture>)
                .cast_mut()
                .cast(),
            1,
            1,
        )
        .expect("runtime image wraps as a render target");
        for channel in [0_u8, 8, 16, 32, 64, 128, 192, 255] {
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let value = f64::from(channel) / 255.0;
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: value,
                                g: value,
                                b: value,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
            }
            queue.submit([encoder.finish()]);
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("submitted clear completes");
            // SAFETY: GPU completion precedes this read of the live shared allocation.
            let pixel =
                unsafe { slice::from_raw_parts(buffer.contents().as_ptr().cast::<u8>(), 4) };
            assert_eq!(
                pixel,
                &[channel, channel, channel, 255],
                "encoded channel {channel}"
            );
        }
    }
}
