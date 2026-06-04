//! Concatenated WGSL source strings, the single definition shared by the
//! pipeline builders and the offline naga validation test, so validated source
//! can never drift from compiled source. Shared functions (sky, sprite quad) are
//! prefixed onto the passes that use them.
//! Shared `BindGroupLayoutEntry`/`BindGroupEntry` constructors. Pure GPU
//! plumbing with no pass-specific state; deduped from scene/sprites/sky.

/// Scene pass: sky functions, then the scene shader.
pub const SCENE_SRC: &str = concat!(include_str!("sky_common.wgsl"), include_str!("scene.wgsl"));
/// Sky background pass: sky functions, then the sky shader.
pub const SKY_SRC: &str = concat!(include_str!("sky_common.wgsl"), include_str!("sky.wgsl"));
/// World sprite pass: quad table, then the sprite shader.
pub const SPRITE_SRC: &str = concat!(
    include_str!("sprite_quad_common.wgsl"),
    include_str!("sprite.wgsl")
);
/// Weapon psprite pass: quad table, then the psprite shader.
pub const PSPRITE_SRC: &str = concat!(
    include_str!("sprite_quad_common.wgsl"),
    include_str!("psprite.wgsl")
);
/// Voxel pass: self-contained (own face-corner table + camera/light structs).
pub const VOXEL_SRC: &str = include_str!("voxel.wgsl");

/// Uniform buffer binding.
pub fn bind_uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Read-only storage buffer binding.
pub fn bind_storage_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: true,
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// `texture_2d_array<f32>` binding, non-filterable (nearest atlas sampling).
pub fn bind_tex_array_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float {
                filterable: false,
            },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

/// `texture_2d<f32>` binding, filterable (sky sampling).
pub fn bind_tex_2d_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float {
                filterable: true,
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Fragment-stage sampler binding (`ty` picks filtering vs nearest).
pub fn bind_sampler_entry(
    binding: u32,
    ty: wgpu::SamplerBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(ty),
        count: None,
    }
}

/// Whole-buffer bind-group entry.
pub fn bind_buf_entry(binding: u32, buf: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buf.as_entire_binding(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse + validate every compiled shader source offline (no GPU): catches
    /// syntax and layout/type faults that otherwise only surface at runtime.
    #[test]
    fn shaders_validate() {
        for (name, src) in [
            ("scene", SCENE_SRC),
            ("sky", SKY_SRC),
            ("sprite", SPRITE_SRC),
            ("psprite", PSPRITE_SRC),
            ("voxel", VOXEL_SRC),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{name} parses: {e:?}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} validates: {e:?}"));
        }
    }
}
