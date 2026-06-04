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

/// OG Doom `R_PointOnSide`
///
/// raw 16.16 fixed-point side test for a directed
/// line from origin `(nx, ny)` along delta `(ndx, ndy)`, all in raw 16.16.
/// Pre-shifts the delta by `FRACBITS` before multiplying so long lines cannot
/// overflow the intermediate (a full `FixedMul` would). Returns 0 for the
/// front (right) side, 1 for back.
#[inline]
pub fn r_point_on_side_raw(x: i32, y: i32, nx: i32, ny: i32, ndx: i32, ndy: i32) -> usize {
    if ndx == 0 {
        return if x <= nx {
            (ndy > 0) as usize
        } else {
            (ndy < 0) as usize
        };
    }
    if ndy == 0 {
        return if y <= ny {
            (ndx < 0) as usize
        } else {
            (ndx > 0) as usize
        };
    }

    let dx = x.wrapping_sub(nx);
    let dy = y.wrapping_sub(ny);

    if (ndy ^ ndx ^ dx ^ dy) as u32 & 0x8000_0000 != 0 {
        return usize::from((ndy ^ dx) as u32 & 0x8000_0000 != 0);
    }

    let left = ((ndy >> 16) as i64 * dx as i64) >> 16;
    let right = (dy as i64 * (ndx >> 16) as i64) >> 16;
    usize::from(right >= left)
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

    // OG fast sign-bit decision (`p_maputl.c:P_PointOnDivlineSide`). This is NOT
    // equivalent to the FixedMul comparison for all inputs — it is the exact OG
    // path and demos depend on it. Omitting it flips the side for some mixed-sign
    // deltas, which silently changes hitscan bbox crossings and desyncs demos.
    let ldy = line.dy.to_fixed_raw();
    let ldx = line.dx.to_fixed_raw();
    let pdx = dx.to_fixed_raw();
    let pdy = dy.to_fixed_raw();
    if (ldy ^ ldx ^ pdx ^ pdy) < 0 {
        return usize::from((ldy ^ pdx) < 0);
    }

    // OG: left = FixedMul(line->dy >> 8, dx >> 8)
    //     right = FixedMul(dy >> 8, line->dx >> 8)
    #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
    {
        let left = ((line.dy.to_fixed_raw() >> 8) as i64 * (dx.to_fixed_raw() >> 8) as i64) >> 16;
        let right = ((dy.to_fixed_raw() >> 8) as i64 * (line.dx.to_fixed_raw() >> 8) as i64) >> 16;
        usize::from(right >= left)
    }
    #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
    {
        let half = FRACBITS / 2;
        let left = line.dy.shr(half).fixed_mul(dx.shr(half));
        let right = dy.shr(half).fixed_mul(line.dx.shr(half));
        if right < left { 0 } else { 1 }
    }
}

/// OG Doom `P_InterceptVector` — fixed-point intercept fraction.
///
/// Returns fixed-point fraction matching OG Doom's `>> 8` overflow
/// prevention and `FixedMul`/`FixedDiv` arithmetic.
#[inline]
pub fn intercept_vector(v2: &DivLineFixed, v1: &DivLineFixed) -> FixedT {
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

#[cfg(test)]
mod tests {
    use super::r_point_on_side_raw;

    const FU: i32 = 1 << 16;

    /// A long horizontal seg (full-linedef length, as the BSP rebuild emits)
    /// plus a point above it. A naive full `FixedMul` overflows i32 here and
    /// reports the wrong side; the pre-shifted OG path stays correct.
    #[test]
    fn long_seg_does_not_overflow() {
        // seg (0,0) -> (4000,0); point (2000, 64) is on the back (left) side.
        let side = r_point_on_side_raw(2000 * FU, 64 * FU, 0, 0, 4000 * FU, 0);
        assert_eq!(side, 1, "point above a +x seg is back side");
        // mirror: point below is front side.
        let side = r_point_on_side_raw(2000 * FU, -64 * FU, 0, 0, 4000 * FU, 0);
        assert_eq!(side, 0, "point below a +x seg is front side");
    }

    /// Degenerate axis-aligned segs hit the early-return branches.
    #[test]
    fn axis_aligned_segs() {
        // vertical seg (0,0)->(0,4000): point to the right is front (0).
        assert_eq!(
            r_point_on_side_raw(64 * FU, 2000 * FU, 0, 0, 0, 4000 * FU),
            0
        );
        assert_eq!(
            r_point_on_side_raw(-64 * FU, 2000 * FU, 0, 0, 0, 4000 * FU),
            1
        );
    }
}
