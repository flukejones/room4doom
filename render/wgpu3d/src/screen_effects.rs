//! Full-screen gameplay colour effects applied to the scene texture before the
//! UI composites over it: the damage/bonus/radsuit player tint and the
//! invulnerability inverse-map. The UI (statusbar/HUD/menu) draws over the
//! tinted scene, so it stays untinted — matching vanilla, which tints the 3D
//! view but not the HUD.
//!
//! The effect math lives here (the Doom model: cshift colours from `pic-data`,
//! `INVERSECOLORMAP`); the GPU pass that consumes [`SceneEffects`] is hosted by
//! the backend's composite shader. The WGSL `SceneEffects` struct must byte-match
//! this one (asserted by the layout test).

use pic_data::{INVERSECOLORMAP, player_cshift};

/// GPU tint is stronger than the vanilla cshift maxima the CPU smooth-fade uses;
/// the pre-baked-RGBA scene reads the wash weaker, so scale it up (clamped).
const GPU_TINT_GAIN: f32 = 1.4;

/// Scene colour-effect parameters as uploaded to the composite shader. std140
/// uniform: `tint_rgb` (vec3) + `tint_pct` fill 16 bytes; `invert` +
/// `bleed_active` + pad fill the next 16. 32 bytes total.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneEffects {
    /// Tint colour (0..1 per channel).
    tint_rgb: [f32; 3],
    /// Tint blend fraction (0 = no tint).
    tint_pct: f32,
    /// 1.0 under Invulnerability (inverse map), else 0.0.
    invert: f32,
    /// 1.0 when the health-bleed columns should be drawn, else 0.0.
    bleed_active: f32,
    _pad: [f32; 2],
}

impl SceneEffects {
    /// Build from the live player state. The tint colour/strength come from the
    /// shared [`player_cshift`] (one definition for the CPU and GPU paths);
    /// invert is set when the player holds the invulnerability inverse colourmap
    /// (`fixedcolormap == INVERSECOLORMAP`).
    pub fn new(damagecount: i32, bonuscount: i32, radsuit: bool, fixedcolormap: usize) -> Self {
        let (tint, pct) = player_cshift(damagecount, bonuscount, radsuit);
        let tint_rgb = [
            ((tint >> 16) & 0xFF) as f32 / 255.0,
            ((tint >> 8) & 0xFF) as f32 / 255.0,
            (tint & 0xFF) as f32 / 255.0,
        ];
        let invert = if fixedcolormap == INVERSECOLORMAP as usize {
            1.0
        } else {
            0.0
        };
        Self {
            tint_rgb,
            tint_pct: (pct * GPU_TINT_GAIN).min(1.0),
            invert,
            bleed_active: 0.0,
            _pad: [0.0; 2],
        }
    }

    /// Flag the health-bleed columns as active (the backend drives the column
    /// buffer separately).
    pub fn set_bleed_active(&mut self, active: bool) {
        self.bleed_active = if active { 1.0 } else { 0.0 };
    }

    /// Raw bytes for the uniform upload (the backend hosts the GPU buffer and
    /// has no bytemuck dependency).
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_effects_layout() {
        // Must byte-match the WGSL std140 SceneEffects (32 bytes).
        assert_eq!(size_of::<SceneEffects>(), 32);
    }
}
