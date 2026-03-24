use crate::bam::{ANG90, ANG180, ANG270};
use crate::fixed_point::FixedT;

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
    let angle = (TANTOANGLE[slope_div(dy.0 as u32, dx.0 as u32) as usize].wrapping_add(ANG90))
        >> ANGLETOFINESHIFT;
    dx.fixed_div(fine_sin(angle << ANGLETOFINESHIFT))
}

/// OG Doom `SlopeDiv` — integer slope used to index `TANTOANGLE`.
#[inline]
fn slope_div(num: u32, den: u32) -> u32 {
    if den < 512 {
        return SLOPERANGE;
    }
    let ans = (num << 3) / (den >> 8);
    if ans <= SLOPERANGE { ans } else { SLOPERANGE }
}

/// OG Doom `R_PointToAngle2(0, 0, dx, dy)` — returns BAM angle from dx/dy.
///
/// Inputs are raw fixed-point i32 values. Uses octant dispatch with
/// `TANTOANGLE` LUT, matching OG Doom `r_main.c:R_PointToAngle` exactly.
#[inline]
pub fn r_point_to_angle(dx: FixedT, dy: FixedT) -> u32 {
    // slope_div operates on ratios so i32 truncation is fine
    let dxi = dx.to_fixed_raw();
    let dyi = dy.to_fixed_raw();
    if dxi == 0 && dyi == 0 {
        return 0;
    }

    if dxi >= 0 {
        if dyi >= 0 {
            let (x, y) = (dxi as u32, dyi as u32);
            if x > y {
                TANTOANGLE[slope_div(y, x) as usize]
            } else {
                ANG90
                    .wrapping_sub(1)
                    .wrapping_sub(TANTOANGLE[slope_div(x, y) as usize])
            }
        } else {
            let (x, y) = (dxi as u32, (-dyi) as u32);
            if x > y {
                0u32.wrapping_sub(TANTOANGLE[slope_div(y, x) as usize])
            } else {
                ANG270.wrapping_add(TANTOANGLE[slope_div(x, y) as usize])
            }
        }
    } else {
        let x = (-dxi) as u32;
        if dyi >= 0 {
            let y = dyi as u32;
            if x > y {
                ANG180
                    .wrapping_sub(1)
                    .wrapping_sub(TANTOANGLE[slope_div(y, x) as usize])
            } else {
                ANG90.wrapping_add(TANTOANGLE[slope_div(x, y) as usize])
            }
        } else {
            let y = (-dyi) as u32;
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
