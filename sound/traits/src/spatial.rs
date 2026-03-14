//! Shared spatial audio helpers used by all sound backends.

use std::f32::consts::TAU;
use std::fmt::Debug;

use glam::Vec2;

/// Maximum audible distance in map units
pub const MAX_DIST: f32 = 1666.0;
/// Number of simultaneous SFX mixer channels
pub const MIXER_CHANNELS: i32 = 32;
/// Doom MUS format magic bytes
pub const MUS_ID: [u8; 4] = [b'M', b'U', b'S', 0x1A];
/// Standard MIDI header magic bytes
pub const MID_ID: [u8; 4] = [b'M', b'T', b'h', b'd'];

/// Compute the angle from (x2,y2) to (x1,y1) in radians
pub fn point_to_angle_2(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x1 - x2;
    let y = y1 - y2;
    y.atan2(x)
}

/// Compute the signed angle between a listener direction and a point
pub fn angle_between(listener_angle: f32, other_x: f32, other_y: f32) -> f32 {
    let (y, x) = listener_angle.sin_cos();
    let v1 = Vec2::new(x, y);
    let other = Vec2::new(other_x, other_y);
    v1.angle_to(other)
}

/// Compute listener-relative angle to source in degrees (0-360, SDL2
/// convention)
pub fn listener_to_source_angle_deg(
    listener_x: f32,
    listener_y: f32,
    listener_angle: f32,
    source_x: f32,
    source_y: f32,
) -> f32 {
    let (y, x) = point_to_angle_2(source_x, source_y, listener_x, listener_y).sin_cos();
    let mut angle = angle_between(listener_angle, x, y);
    if angle.is_sign_negative() {
        angle += TAU;
    }
    360.0 - angle.to_degrees()
}

/// Euclidean distance between listener and source
pub fn dist_from_points(lx: f32, ly: f32, sx: f32, sy: f32) -> f32 {
    let dx = lx - sx;
    let dy = ly - sy;
    (dx * dx + dy * dy).sqrt()
}

/// Tracks position of a sound source or listener
#[derive(Debug, Default, Clone, Copy)]
pub struct SoundObject<S>
where
    S: Copy + Debug,
{
    /// Objects unique ID or hash
    pub uid: usize,
    /// The Sound effect this object has
    pub sfx: S,
    /// The world X coordinate
    pub x: f32,
    /// The world Y coordinate
    pub y: f32,
    /// Angle in radians
    pub angle: f32,
    /// Allocated mixer channel
    pub channel: i32,
    /// Playback priority
    pub priority: i32,
}
