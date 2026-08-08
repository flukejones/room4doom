//! wgpu objects on the runtime's Metal device.
//!
//! `XR_KHR_metal_enable` names the `MTLDevice` the application must render on,
//! so wgpu cannot create its own. `wgpu-hal` is an unstable surface — this is
//! the one module that depends on it, so a wgpu bump breaks here and nowhere
//! else.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandQueue, MTLDevice, MTLTexture, MTLTextureType};

use crate::SWAPCHAIN_FORMAT;
use crate::session::VrError;

/// Builds a wgpu device/queue on the runtime's `MTLDevice`.
pub(crate) fn build_wgpu(
    device: &Retained<ProtocolObject<dyn MTLDevice>>,
    queue: &Retained<ProtocolObject<dyn MTLCommandQueue>>,
) -> Result<(wgpu::Instance, wgpu::Device, wgpu::Queue), VrError> {
    // SAFETY: both handles are live and owned by the caller for longer than
    // the returned wgpu objects.
    let hal_device = unsafe {
        wgpu_hal::metal::Device::device_from_raw(device.clone(), wgpu::Features::empty())
    };
    // SAFETY: as above; the timestamp period is Metal's nanosecond base.
    let hal_queue = unsafe { wgpu_hal::metal::Queue::queue_from_raw(queue.clone(), 1.0) };

    // No surface is ever created from this instance — the runtime owns
    // presentation — so the display connection stays unset.
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|_| VrError::NoMetalDevice)?;

    // SAFETY: the hal device and queue were built from the same MTLDevice
    // this adapter represents.
    let open = wgpu_hal::OpenDevice::<wgpu_hal::api::Metal> {
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

/// Wraps one runtime `MTLTexture` as a wgpu texture view.
pub(crate) fn wrap_texture(
    device: &wgpu::Device,
    raw: *mut c_void,
    width: u32,
    height: u32,
) -> Option<wgpu::TextureView> {
    // SAFETY: the runtime owns the texture for the swapchain's lifetime;
    // retain takes our own reference for the wrapper.
    let texture: Retained<ProtocolObject<dyn MTLTexture>> =
        unsafe { Retained::retain(raw.cast()) }?;
    let copy_size = wgpu_hal::CopyExtent {
        width,
        height,
        depth: 1,
    };
    // SAFETY: the texture is 2D, single-layer, single-mip, and created with
    // the format below — the runtime's swapchain contract.
    let hal_texture = unsafe {
        wgpu_hal::metal::Device::texture_from_raw(
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
    let texture =
        unsafe { device.create_texture_from_hal::<wgpu_hal::api::Metal>(hal_texture, &descriptor) };
    Some(texture.create_view(&wgpu::TextureViewDescriptor::default()))
}
