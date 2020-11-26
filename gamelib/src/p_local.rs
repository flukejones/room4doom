/// P_MOBJ
pub static ONFLOORZ: i32 = i32::MIN;
/// P_MOBJ
pub static ONCEILINGZ: i32 = i32::MAX;

pub static MAXHEALTH: i32 = 100;
pub static VIEWHEIGHT: i32 = 41;

pub static MAXRADIUS: f32 = 32.0;

pub const FRACBITS: i32 = 16;

/// The Doom `FRACUNIT` is `1 << FRACBITS`
pub const FRACUNIT: f32 = 65536.0; //(1 << FRACBITS) as f32;

pub const FRACUNIT_DIV4: f32 = 0.25;

/// Convert a Doom `fixed_t` fixed-point float to `f32`
pub const fn fixed_to_float(value: i32) -> f32 { value as f32 / FRACUNIT }

const DEG_TO_RAD: f32 = 0.017453292; //PI / 180.0;

/// Convert a BAM (Binary Angle Measure) to radians
pub const fn bam_to_radian(value: u32) -> f32 {
    (value as f32 * 8.38190317e-8) * DEG_TO_RAD
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    use super::bam_to_radian;

    #[test]
    fn convert_bam_to_rad() {
        // DOOM constants
        let ang45: u32 = 0x20000000;
        let ang90: u32 = 0x40000000;
        let ang180: u32 = 0x80000000;

        assert_eq!(bam_to_radian(ang45), FRAC_PI_4);
        assert_eq!(bam_to_radian(ang90), FRAC_PI_2);
        assert_eq!(bam_to_radian(ang180), PI);
    }
}
