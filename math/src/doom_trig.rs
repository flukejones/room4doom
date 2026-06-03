use crate::bam::{ANG90, ANG180, ANG270};
use crate::fixed_point::{FixedT, UInner};

include!("og_trig_tables.rs");

#[cfg(feature = "fixed64hd")]
include!(concat!(env!("OUT_DIR"), "/hd_trig_tables.rs"));

/// `u32 BAM >> 19` gives fine index 0..8191.
pub const ANGLETOFINESHIFT: u32 = 19;

/// OG Doom `SLOPERANGE` — max index into `TANTOANGLE`.
const SLOPERANGE: u32 = 2048;

/// Returns `sin(bam)` as a `FixedT`.
#[inline]
pub fn fine_sin(bam: u32) -> FixedT {
    let idx = ((bam >> ANGLETOFINESHIFT) & 8191) as usize;
    #[cfg(feature = "fixed64hd")]
    {
        FixedT(FINESINE_HD[idx])
    }
    #[cfg(not(feature = "fixed64hd"))]
    {
        FixedT::from_fixed(FINESINE[idx])
    }
}

/// Returns `cos(bam)` as a `FixedT` (uses FINESINE at offset 2048).
#[inline]
pub fn fine_cos(bam: u32) -> FixedT {
    let idx = (((bam >> ANGLETOFINESHIFT) + 2048) & 8191) as usize;
    #[cfg(feature = "fixed64hd")]
    {
        FixedT(FINESINE_HD[idx])
    }
    #[cfg(not(feature = "fixed64hd"))]
    {
        FixedT::from_fixed(FINESINE[idx])
    }
}

/// OG `finesine[idx]` — raw fine-angle index lookup.
#[inline]
pub fn finesine(idx: usize) -> FixedT {
    #[cfg(feature = "fixed64hd")]
    {
        FixedT(FINESINE_HD[idx & 8191])
    }
    #[cfg(not(feature = "fixed64hd"))]
    {
        FixedT::from_fixed(FINESINE[idx & 8191])
    }
}

/// OG `finecosine[idx]` — fine-angle index lookup (offset by FINEANGLES/4).
#[inline]
pub fn finecosine(idx: usize) -> FixedT {
    #[cfg(feature = "fixed64hd")]
    {
        FixedT(FINESINE_HD[(idx + 2048) & 8191])
    }
    #[cfg(not(feature = "fixed64hd"))]
    {
        FixedT::from_fixed(FINESINE[(idx + 2048) & 8191])
    }
}

/// Returns `tan` for a fine angle index (0..4095).
#[inline]
pub fn fine_tan(fine_angle: usize) -> FixedT {
    #[cfg(feature = "fixed64hd")]
    {
        FixedT(FINETANGENT_HD[fine_angle & 4095])
    }
    #[cfg(not(feature = "fixed64hd"))]
    {
        FixedT::from_fixed(FINETANGENT[fine_angle & 4095])
    }
}

/// OG Doom `R_PointToDist` — exact hypotenuse via tantoangle LUT + inverse
/// sine.
#[inline]
pub fn r_point_to_dist(vx: FixedT, vy: FixedT, ox: FixedT, oy: FixedT) -> FixedT {
    let mut dx = (vx - ox).doom_abs();
    let mut dy = (vy - oy).doom_abs();
    if dy > dx {
        std::mem::swap(&mut dx, &mut dy);
    }
    if dx.is_zero() {
        return FixedT::ZERO;
    }
    let angle = (TANTOANGLE[slope_div(dy.0 as UInner, dx.0 as UInner) as usize]
        .wrapping_add(ANG90))
        >> ANGLETOFINESHIFT;
    dx.fixed_div(fine_sin(angle << ANGLETOFINESHIFT))
}

/// OG Doom `SlopeDiv` — integer slope used to index `TANTOANGLE`.
///
/// Returns `(num/den) * SLOPERANGE` clamped to `[0, SLOPERANGE]`. The result is
/// a pure ratio, invariant under uniform scaling of `num`/`den`, so the same
/// magic shifts hold across all precision modes. `num`/`den` are full-width
/// `UInner` so `num << 3` cannot overflow in the 64-bit fixed-point modes.
#[inline]
#[allow(
    clippy::unnecessary_cast,
    reason = "casts are real in the 64-bit precision modes"
)]
fn slope_div(num: UInner, den: UInner) -> u32 {
    if den < 512 {
        return SLOPERANGE;
    }
    let ans = (num << 3) / (den >> 8);

    if ans <= UInner::from(SLOPERANGE) {
        #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
        return ans;
        #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
        return ans as u32;
    }
    SLOPERANGE
}

/// OG Doom `R_PointToAngle2(0, 0, dx, dy)` — returns BAM angle from dx/dy.
///
/// Inputs are raw fixed-point i32 values. Uses octant dispatch with
/// `TANTOANGLE` LUT, matching OG Doom `r_main.c:R_PointToAngle` exactly.
#[inline]
pub fn r_point_to_angle(dx: FixedT, dy: FixedT) -> u32 {
    // Full-width raw bits: `slope_div` needs a true ratio plus correct octant
    // signs. `to_fixed_raw()` narrows to i32, corrupting both for deltas beyond
    // ±2^31 raw in the 64-bit fixed-point modes — use `raw()` instead.
    let dxi = dx.raw();
    let dyi = dy.raw();
    if dxi == 0 && dyi == 0 {
        return 0;
    }

    if dxi >= 0 {
        if dyi >= 0 {
            let (x, y) = (dxi as UInner, dyi as UInner);
            if x > y {
                TANTOANGLE[slope_div(y, x) as usize]
            } else {
                ANG90
                    .wrapping_sub(1)
                    .wrapping_sub(TANTOANGLE[slope_div(x, y) as usize])
            }
        } else {
            let (x, y) = (dxi as UInner, (-dyi) as UInner);
            if x > y {
                0u32.wrapping_sub(TANTOANGLE[slope_div(y, x) as usize])
            } else {
                ANG270.wrapping_add(TANTOANGLE[slope_div(x, y) as usize])
            }
        }
    } else {
        let x = (-dxi) as UInner;
        if dyi >= 0 {
            let y = dyi as UInner;
            if x > y {
                ANG180
                    .wrapping_sub(1)
                    .wrapping_sub(TANTOANGLE[slope_div(y, x) as usize])
            } else {
                ANG90.wrapping_add(TANTOANGLE[slope_div(x, y) as usize])
            }
        } else {
            let y = (-dyi) as UInner;
            if x > y {
                ANG180.wrapping_add(TANTOANGLE[slope_div(y, x) as usize])
            } else {
                ANG270
                    .wrapping_sub(1)
                    .wrapping_sub(TANTOANGLE[slope_div(x, y) as usize])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected BAM angles match OG Doom's `R_PointToAngle` octant build, which
    // lands one BAM below the ideal boundary on the half-open arms.
    const A_RIGHT: u32 = 0; // +x
    const A_UP: u32 = ANG90 - 1; // +y
    const A_LEFT: u32 = ANG180 - 1; // -x
    const A_DOWN: u32 = ANG270; // -y
    const A_DIAG: u32 = crate::bam::ANG45 - 1; // +x +y

    /// Cardinal and diagonal angles — octant dispatch is correct in every
    /// precision mode (these assertions run under default and 64-bit features).
    #[test]
    fn cardinal_and_diagonal() {
        assert_eq!(r_point_to_angle(FixedT::from(1), FixedT::from(0)), A_RIGHT);
        assert_eq!(r_point_to_angle(FixedT::from(0), FixedT::from(1)), A_UP);
        assert_eq!(r_point_to_angle(FixedT::from(-1), FixedT::from(0)), A_LEFT);
        assert_eq!(r_point_to_angle(FixedT::from(0), FixedT::from(-1)), A_DOWN);
        assert_eq!(r_point_to_angle(FixedT::from(1), FixedT::from(1)), A_DIAG);
        assert_eq!(r_point_to_angle(FixedT::from(0), FixedT::from(0)), 0);
    }

    /// Regression for the i32-truncation bug: in the 64-bit fixed-point modes a
    /// delta whose raw value exceeds the i32 range must still resolve to the
    /// same angle as the equivalent small delta. `40000 << FRACBITS` overflows
    /// i32 in those modes, so `to_fixed_raw()` (the old path) corrupted the
    /// octant and ratio. A large delta and a small delta of equal slope must
    /// agree.
    #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
    #[test]
    fn large_delta_beyond_i32() {
        let big = FixedT::from(40000);
        // Raw value must exceed i32 range, else the test proves nothing.
        assert!(big.raw() > i32::MAX as crate::fixed_point::Inner);
        // Same octant results as the unit-delta cardinal/diagonal cases.
        assert_eq!(r_point_to_angle(big, big), A_DIAG);
        assert_eq!(r_point_to_angle(big, FixedT::from(0)), A_RIGHT);
        assert_eq!(r_point_to_angle(FixedT::from(0), big), A_UP);
        assert_eq!(r_point_to_angle(-big, FixedT::from(0)), A_LEFT);
        assert_eq!(r_point_to_angle(FixedT::from(0), -big), A_DOWN);
    }
}
