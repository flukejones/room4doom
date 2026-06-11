//! Runtime light/falloff parameters, the single source of truth for the Doom
//! diminishing-light model shared by the scene and sprite shaders (and the
//! CPU psprite path). Replaces the constants that were duplicated across the
//! three light sites. Uploaded as a uniform; `light_gamma` is menu-tunable.

/// Renderer-side tunables fed in per frame (e.g. from the config menu). Holds
/// only `light_gamma` today; grouped in a struct so `draw_view_gpu` does not
/// grow a new positional arg per future option.
#[derive(Clone, Copy)]
pub struct RenderConfig {
    /// Row->intensity exponent for the diminishing-light curve (≈0.5..1.5).
    pub light_gamma: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            light_gamma: DEFAULT_LIGHT_GAMMA,
        }
    }
}

/// Default row->intensity gamma when no config is supplied.
pub const DEFAULT_LIGHT_GAMMA: f32 = 1.2;
/// Max light band (0..15).
pub const LIGHT_LEVELS: usize = 15;
/// Darkest colourmap row; intensity = (1 - row/MAX_ROW)^gamma.
const MAX_ROW: f32 = 31.0;
/// 1/w -> colourmap-row distance scale (the shared software3d `LIGHT_SCALE`).
const DIST_SCALE: f32 = render_common::light::LIGHT_SCALE;
/// Max rows the near-distance boost can brighten by (`MAXLIGHTSCALE - 1`).
const DIST_ROWS_MAX: f32 = render_common::light::WEAPON_LIGHT_INDEX_MAX;

/// Light params as uploaded to the shaders. std140 uniform: 5×f32 (20) padded
/// to 32 bytes (16-aligned). Must byte-match the WGSL `LightParams` struct.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightParams {
    light_levels: f32,
    max_row: f32,
    light_gamma: f32,
    dist_scale: f32,
    dist_rows_max: f32,
    _pad: [f32; 3],
}

impl LightParams {
    /// Build from the runtime config; the fixed Doom constants come from this
    /// module so all three light paths read one source.
    pub fn new(config: &RenderConfig) -> Self {
        Self {
            light_levels: LIGHT_LEVELS as f32,
            max_row: MAX_ROW,
            light_gamma: config.light_gamma,
            dist_scale: DIST_SCALE,
            dist_rows_max: DIST_ROWS_MAX,
            _pad: [0.0; 3],
        }
    }

    /// Darkest colourmap row (for the CPU psprite light math).
    pub fn max_row(&self) -> f32 {
        self.max_row
    }

    /// Row->intensity gamma (for the CPU psprite light math).
    pub fn light_gamma(&self) -> f32 {
        self.light_gamma
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_params_layout() {
        // Must byte-match the WGSL std140 LightParams (32 bytes).
        assert_eq!(size_of::<LightParams>(), 32);
    }
}
