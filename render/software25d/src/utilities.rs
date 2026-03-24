use math::{ANG90, Angle, Bam, FixedT};

const MIN_DEN: FixedT = FixedT(1);

/// R_ScaleFromGlobalAngle
pub fn scale_from_view_angle(
    visangle: Angle<Bam>,
    rw_normalangle: Angle<Bam>,
    rw_distance: FixedT,
    view_angle: Angle<Bam>,
    screen_width_half: FixedT,
) -> FixedT {
    let ang90 = Angle::<Bam>::from_bam(ANG90);
    let anglea: Angle<Bam> = ang90 + (visangle - view_angle);
    let angleb: Angle<Bam> = ang90 + (visangle - rw_normalangle);
    let num = screen_width_half * angleb.sin_fixedt();
    let den = rw_distance * anglea.sin_fixedt();

    if den.doom_abs() < MIN_DEN {
        if num > FixedT::ZERO {
            FixedT::from(64)
        } else {
            FixedT::from(-64)
        }
    } else {
        (num / den).clamp(FixedT::from(-180), FixedT::from(180))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perpendicular_segment_edge_cases() {
        let screen_width_half = FixedT::from(160);
        let view_angle = Angle::<Bam>::from_bam(ANG90);
        let rw_normalangle = Angle::<Bam>::from_bam(ANG90);
        let visangle = Angle::<Bam>::from_bam(ANG90);
        let rw_distance = FixedT::ONE;

        let scale = scale_from_view_angle(
            visangle,
            rw_normalangle,
            rw_distance,
            view_angle,
            screen_width_half,
        );
        // Degenerate case: all angles equal, tiny distance. Result is clamped.
        assert!(scale.doom_abs() <= 180);
    }

    #[test]
    fn test_zero_distance() {
        let screen_width_half = FixedT::from(160);
        let view_angle = Angle::<Bam>::new(0.0);
        let rw_normalangle = Angle::<Bam>::new(0.0);
        let visangle = Angle::<Bam>::new(0.0);
        let rw_distance = FixedT::ZERO;

        let scale = scale_from_view_angle(
            visangle,
            rw_normalangle,
            rw_distance,
            view_angle,
            screen_width_half,
        );
        assert!(scale.doom_abs() <= 64);
    }

    #[test]
    fn test_angle_bounds() {
        let screen_width_half = FixedT::from(160);
        let view_angle = Angle::<Bam>::new(0.0);
        let rw_normalangle = Angle::<Bam>::from_bam(ANG90);
        let visangle = Angle::<Bam>::from_bam(ANG90);
        let rw_distance = FixedT::ONE;

        let scale = scale_from_view_angle(
            visangle,
            rw_normalangle,
            rw_distance,
            view_angle,
            screen_width_half,
        );
        // At distance 1.0 with these angles, scale is clamped to ±180
        assert!(scale.doom_abs() <= 180);
    }
}
