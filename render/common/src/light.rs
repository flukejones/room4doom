//! Shared diminishing-light constants for the 3D renderers (software3d +
//! wgpu3d), so the colourmap-scale magic numbers have one definition.

/// Doom colourmap scale-index count; `base_colourmap` clamps the index to
/// `MAXLIGHTSCALE - 1`. (Mirrors pic-data's private `MAXLIGHTSCALE`.)
pub const MAXLIGHTSCALE: i32 = 48;
/// Colourmap-index ceiling — the highest scale index `base_colourmap` accepts.
pub const WEAPON_LIGHT_INDEX_MAX: f32 = (MAXLIGHTSCALE - 1) as f32;
/// Extra colourmap-index boost so the held weapon reads brighter than its
/// sector.
pub const WEAPON_LIGHT_BOOST: f32 = 3.0;
/// Brightness-band span mapped across the index range; band=full + boost lands
/// exactly on the ceiling.
pub const WEAPON_LIGHT_INDEX_SPAN: f32 = WEAPON_LIGHT_INDEX_MAX - WEAPON_LIGHT_BOOST;
/// Light-level granularity (0..15 band).
pub const LIGHT_LEVELS: f32 = 15.0;

/// 1/w window the distance falloff spans (`LIGHT_MIN_Z`..`LIGHT_MAX_Z`).
const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);
/// 1/w -> colourmap-scale-index multiplier (Doom `8 * LIGHTLEVELS`).
pub const LIGHT_SCALE: f32 = LIGHT_RANGE * 8.0 * 16.0;
