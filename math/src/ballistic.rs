//! The teleport aim curve, shared by the gameplay clip and the renderer draw.
//!
//! Both sides must trace the same parabola or the arc would land somewhere the
//! player did not aim at, so the curve lives here rather than in either.

use glam::Vec3;

/// Furthest a teleport reaches: 5 m at Doom's 32 units per metre.
pub const MAX_TELEPORT_RANGE: f32 = 160.0;

/// Doom's gravity in units per second squared — 1 unit per tic² at 35 tics.
pub const TELEPORT_GRAVITY: f32 = 1225.0;

/// √(`MAX_TELEPORT_RANGE` · `TELEPORT_GRAVITY`): the launch speed at which a 45°
/// aim just reaches maximum range.
pub const TELEPORT_SPEED: f32 = 442.72;

/// Flight time sampled before the arc is abandoned — a 45° launch's time to fall
/// back to launch height, by which point it has covered `MAX_TELEPORT_RANGE`.
pub const TELEPORT_FLIGHT_SECS: f32 = 0.511;

/// The aim curve at `t` seconds: launched from `origin` along unit `dir` at
/// `speed`, falling under [`TELEPORT_GRAVITY`]. Z is up.
#[inline]
pub fn ballistic_point(origin: Vec3, dir: Vec3, speed: f32, t: f32) -> Vec3 {
    let fall = 0.5 * TELEPORT_GRAVITY * t * t;
    origin + dir * speed * t - Vec3::Z * fall
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The consts are hand-derived from the range formula; the curve itself is
    /// the independent check on them.
    #[test]
    fn a_forty_five_degree_launch_reaches_max_range() {
        let dir = Vec3::new(
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        let landing = ballistic_point(Vec3::ZERO, dir, TELEPORT_SPEED, TELEPORT_FLIGHT_SECS);
        assert!(
            landing.z.abs() < 1.0,
            "the sampled flight time returns to launch height: {}",
            landing.z
        );
        assert!(
            (landing.x - MAX_TELEPORT_RANGE).abs() < 1.0,
            "and covers max range doing it: {} vs {MAX_TELEPORT_RANGE}",
            landing.x
        );
    }
}
