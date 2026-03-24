#[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
use crate::fixed_point::FRACBITS;
use crate::fixed_point::FixedT;
use glam::Vec2;

/// Used in path tracing for intercepts
/// Is divline + trace types
#[derive(Debug, Clone, Copy)]
pub struct Trace {
    pub xy: Vec2,
    pub dxy: Vec2,
}

impl Trace {
    #[inline]
    pub const fn new(xyz: Vec2, dxyz: Vec2) -> Self {
        Self {
            xy: xyz,
            dxy: dxyz,
        }
    }
}

/// Fixed-point divline for OG Doom-compatible intercept computation.
#[derive(Debug, Clone, Copy)]
pub struct DivLineFixed {
    pub x: FixedT,
    pub y: FixedT,
    pub dx: FixedT,
    pub dy: FixedT,
}

/// Determine which side of the trace the vector point is on
#[inline]
pub fn point_on_side(trace: Trace, v2: Vec2) -> usize {
    let dx = v2.x - trace.xy.x;
    let dy = v2.y - trace.xy.y;

    if (dy * trace.dxy.x) <= (trace.dxy.y * dx) {
        // Front side
        return 0;
    }
    // Backside
    1
}

/// OG Doom `P_PointOnDivlineSide` — fixed-point side test for divlines.
#[inline]
pub fn point_on_divline_side(x: FixedT, y: FixedT, line: &DivLineFixed) -> usize {
    let zero = FixedT::ZERO;
    if line.dx == zero {
        return if x <= line.x {
            (line.dy <= zero) as usize
        } else {
            (line.dy > zero) as usize
        };
    }
    if line.dy == zero {
        return if y <= line.y {
            (line.dx > zero) as usize
        } else {
            (line.dx <= zero) as usize
        };
    }

    let dx = x - line.x;
    let dy = y - line.y;

    // OG: left = FixedMul(line->dy >> 8, dx >> 8)
    //     right = FixedMul(dy >> 8, line->dx >> 8)
    #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
    {
        let left = ((line.dy.to_fixed_raw() >> 8) as i64 * (dx.to_fixed_raw() >> 8) as i64) >> 16;
        let right = ((dy.to_fixed_raw() >> 8) as i64 * (line.dx.to_fixed_raw() >> 8) as i64) >> 16;
        if right < left { 0 } else { 1 }
    }
    #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
    {
        let half = FRACBITS / 2;
        let left = line.dy.shr(half).fixed_mul(dx.shr(half));
        let right = dy.shr(half).fixed_mul(line.dx.shr(half));
        if right < left { 0 } else { 1 }
    }
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
    let denominator = (v1.dxy.y * v2.dxy.x) - (v1.dxy.x * v2.dxy.y);
    if denominator == f32::EPSILON {
        return -0.0;
    }
    let numerator = ((v1.xy.x - v2.xy.x) * v1.dxy.y) + ((v2.xy.y - v1.xy.y) * v1.dxy.x);
    numerator / denominator
}

/// OG Doom `P_InterceptVector` — fixed-point intercept fraction.
///
/// Returns fixed-point fraction matching OG Doom's `>> 8` overflow
/// prevention and `FixedMul`/`FixedDiv` arithmetic.
#[inline]
pub fn intercept_vector_fixed(v2: &DivLineFixed, v1: &DivLineFixed) -> FixedT {
    #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
    {
        let den_a = FixedT::from_fixed(v1.dy.to_fixed_raw() >> 8).fixed_mul(v2.dx);
        let den_b = FixedT::from_fixed(v1.dx.to_fixed_raw() >> 8).fixed_mul(v2.dy);
        let den = den_a - den_b;
        if den == FixedT::ZERO {
            return FixedT::ZERO;
        }
        let num_a = FixedT::from_fixed((v1.x - v2.x).to_fixed_raw() >> 8).fixed_mul(v1.dy);
        let num_b = FixedT::from_fixed((v2.y - v1.y).to_fixed_raw() >> 8).fixed_mul(v1.dx);
        (num_a + num_b).fixed_div(den)
    }
    #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
    {
        let half = FRACBITS / 2;
        let den = v1.dy.shr(half).fixed_mul(v2.dx) - v1.dx.shr(half).fixed_mul(v2.dy);
        if den == FixedT::ZERO {
            return FixedT::ZERO;
        }
        let num =
            (v1.x - v2.x).shr(half).fixed_mul(v1.dy) + (v2.y - v1.y).shr(half).fixed_mul(v1.dx);
        num.fixed_div(den)
    }
}
