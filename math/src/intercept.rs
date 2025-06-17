use crate::DoomF32;

/// Used in path tracing for intercepts
/// Is divline + trace types
#[derive(Debug, Clone, Copy)]
pub struct Trace {
    pub x: DoomF32,
    pub y: DoomF32,
    pub dx: DoomF32,
    pub dy: DoomF32,
}

impl Trace {
    #[inline]
    pub fn new(x: DoomF32, y: DoomF32, dx: DoomF32, dy: DoomF32) -> Self {
        Self { x, y, dx, dy }
    }
}

/// Determine which side of the trace the vector point is on
#[inline]
pub fn point_on_side(trace: Trace, x: DoomF32, y: DoomF32) -> usize {
    let dx = x - trace.x;
    let dy = y - trace.y;

    if (dy * trace.dx) <= (trace.dy * dx) {
        // Front side
        return 0;
    }
    // Backside
    1
}

/// Returns the fractional intercept point along the first divline.
///
/// The lines can be pictured as arg1 being an infinite plane, and arg2 being
/// the line to check if intersected by the plane.
///
/// P_InterceptVector
#[inline]
pub fn intercept_vector(v2: Trace, v1: Trace) -> DoomF32 {
    // Doom does `v1->dy >> 8`, this is  x * 0.00390625
    let denominator = (v1.dy * v2.dx) - (v1.dx * v2.dy);
    if denominator == crate::doom_f32!(f32::EPSILON) {
        return crate::doom_f32!(-0.0);
    }
    let numerator = ((v1.x - v2.x) * v1.dy) + ((v2.y - v1.y) * v1.dx);
    numerator / denominator
}
