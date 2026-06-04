//! Shared voxel placement math: turns a thing's per-frame state plus its
//! [`VoxelSlices`] metadata into a world transform (position, yaw, brightness).
//!
//! Both renderers call this so the spin/bob/face-player/brightness behaviour
//! can't drift between them. Inputs are primitives (the caller extracts them
//! from its `MapObject`), so this crate stays free of a `gameplay` dependency.

use pic_data::VoxelSlices;

/// Vertical bob amplitude (map units) applied to spinning pickup voxels.
pub const VOXEL_BOB_RANGE: f32 = 6.0;

/// Per-frame thing state needed to place a voxel. `base_*` are already
/// interpolated (prev → curr by `frac`) by the caller.
pub struct VoxelTransformIn {
    pub base_x: f32,
    pub base_y: f32,
    pub base_z: f32,
    pub thing_angle_rad: f32,
    pub player_x: f32,
    pub player_y: f32,
    pub game_tic: u32,
    pub frac: f32,
    pub dropped: bool,
    pub fullbright: bool,
    pub light_level: usize,
    pub extralight: usize,
}

/// Resolved world placement for a voxel this frame.
pub struct VoxelTransform {
    /// World position with the spin bob folded into Z.
    pub pos: [f32; 3],
    /// Yaw to rotate the model by (radians).
    pub angle_rad: f32,
    /// Light band 0..=15.
    pub brightness: usize,
}

/// Light band ceiling (matches software3d / the GPU light model).
const MAX_BAND: usize = 15;

/// Compute a voxel's world transform from its slices metadata + per-frame state.
pub fn voxel_transform(vslices: &VoxelSlices, input: &VoxelTransformIn) -> VoxelTransform {
    let spin_rate = if input.dropped {
        vslices.dropped_spin
    } else {
        vslices.placed_spin
    };

    let (angle_rad, spin_bob) = if vslices.face_player {
        let dx = input.player_x - input.base_x;
        let dy = input.player_y - input.base_y;
        (dy.atan2(dx) + vslices.angle_offset, 0.0)
    } else if spin_rate != 0.0 {
        let spin_angle = spin_rate * (input.game_tic as f32 + input.frac);
        let bob = (1.0 - (spin_angle * 3.0).cos()) * 0.5 * VOXEL_BOB_RANGE;
        (spin_angle + vslices.angle_offset, bob)
    } else {
        (input.thing_angle_rad + vslices.angle_offset, 0.0)
    };

    let brightness = if input.fullbright {
        MAX_BAND
    } else {
        (input.light_level + input.extralight).min(MAX_BAND)
    };

    VoxelTransform {
        pos: [input.base_x, input.base_y, input.base_z + spin_bob],
        angle_rad,
        brightness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn slices(placed: f32, dropped: f32, face: bool, offset: f32) -> VoxelSlices {
        VoxelSlices {
            slices: [Vec::new(), Vec::new(), Vec::new()],
            xsiz: 1,
            ysiz: 1,
            zsiz: 1,
            xpivot: 0.0,
            ypivot: 0.0,
            zpivot: 0.0,
            angle_offset: offset,
            placed_spin: placed,
            dropped_spin: dropped,
            face_player: face,
        }
    }

    fn base_in() -> VoxelTransformIn {
        VoxelTransformIn {
            base_x: 0.0,
            base_y: 0.0,
            base_z: 10.0,
            thing_angle_rad: 0.5,
            player_x: 100.0,
            player_y: 0.0,
            game_tic: 0,
            frac: 0.0,
            dropped: false,
            fullbright: false,
            light_level: 8,
            extralight: 0,
        }
    }

    #[test]
    fn placed_vs_dropped_spin_selection() {
        let vs = slices(0.1, 0.2, false, 0.0);
        let mut input = base_in();
        input.game_tic = 10;
        let placed = voxel_transform(&vs, &input).angle_rad;
        input.dropped = true;
        let dropped = voxel_transform(&vs, &input).angle_rad;
        assert!((placed - 0.1 * 10.0).abs() < 1e-5);
        assert!((dropped - 0.2 * 10.0).abs() < 1e-5);
    }

    #[test]
    fn face_player_points_at_player_and_ignores_spin() {
        // Player to +X, voxel at origin -> yaw 0 (+ offset). Spin rates ignored.
        let vs = slices(0.5, 0.5, true, 0.0);
        let t = voxel_transform(&vs, &base_in());
        assert!(t.angle_rad.abs() < 1e-5);
        assert_eq!(t.pos[2], 10.0, "face-player voxels do not bob");
    }

    #[test]
    fn static_angle_uses_thing_angle_plus_offset() {
        let vs = slices(0.0, 0.0, false, 0.25);
        let t = voxel_transform(&vs, &base_in());
        assert!((t.angle_rad - (0.5 + 0.25)).abs() < 1e-5);
        assert_eq!(t.pos[2], 10.0);
    }

    #[test]
    fn spin_bobs_z() {
        let vs = slices(TAU / 100.0, 0.0, false, 0.0);
        let mut input = base_in();
        input.game_tic = 25; // quarter rotation -> cos(spin*3) varies, bob > 0
        let t = voxel_transform(&vs, &input);
        assert!(t.pos[2] != 10.0, "spinning voxel should bob");
        assert!(t.pos[2] >= 10.0 && t.pos[2] <= 10.0 + VOXEL_BOB_RANGE);
    }

    #[test]
    fn fullbright_pins_max_band() {
        let vs = slices(0.0, 0.0, false, 0.0);
        let mut input = base_in();
        input.light_level = 3;
        input.fullbright = true;
        assert_eq!(voxel_transform(&vs, &input).brightness, 15);
    }

    #[test]
    fn brightness_clamps_to_band_ceiling() {
        let vs = slices(0.0, 0.0, false, 0.0);
        let mut input = base_in();
        input.light_level = 12;
        input.extralight = 8;
        assert_eq!(voxel_transform(&vs, &input).brightness, 15);
    }
}
