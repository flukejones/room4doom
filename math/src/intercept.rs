use glam::Vec3;

/// Used in path tracing for intercepts
/// Is divline + trace types
#[derive(Debug, Clone, Copy)]
pub struct Trace {
    pub xyz: Vec3,
    pub dxyz: Vec3,
}

impl Trace {
    #[inline]
    pub const fn new(xyz: Vec3, dxyz: Vec3) -> Self {
        Self { xyz, dxyz }
    }
}

/// Determine which side of the trace the vector point is on
#[inline]
pub fn point_on_side(trace: Trace, v2: Vec3) -> usize {
    let dx = v2.x - trace.xyz.x;
    let dy = v2.y - trace.xyz.y;

    if (dy * trace.dxyz.x) <= (trace.dxyz.y * dx) {
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
pub fn intercept_vector(v2: Trace, v1: Trace) -> f32 {
    // Doom does `v1->dy >> 8`, this is  x * 0.00390625
    let denominator = (v1.dxyz.y * v2.dxyz.x) - (v1.dxyz.x * v2.dxyz.y);
    if denominator == f32::EPSILON {
        return -0.0;
    }
    let numerator = ((v1.xyz.x - v2.xyz.x) * v1.dxyz.y) + ((v2.xyz.y - v1.xyz.y) * v1.dxyz.x);
    numerator / denominator
}
