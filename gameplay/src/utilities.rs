//! Many helper functions related to traversing the map, crossing or finding
//! lines.

use std::f32::consts::PI;

use crate::thing::{MapObject, PT_ADDLINES, PT_ADDTHINGS, PT_EARLYOUT};

use crate::angle::Angle;
use crate::level::map_data::BSPTrace;
use crate::level::map_defs::{BBox, LineDef, SlopeType};
use crate::level::Level;
use crate::MapPtr;
use glam::Vec2;

const FRACBITS: i32 = 16;
const FRACUNIT: f32 = (1 << FRACBITS) as f32;
pub const FRACUNIT_DIV4: f32 = FRACUNIT / 4.0;

/// Convert a Doom `fixed_t` fixed-point float to `f32`
pub fn fixed_to_float(value: i32) -> f32 {
    value as f32 / FRACUNIT
}

const DEG_TO_RAD: f32 = PI / 180.0;

/// Convert a BAM (Binary Angle Measure) to radians
pub fn bam_to_radian(value: u32) -> f32 {
    (value as f32 * 8.381_903e-8) * DEG_TO_RAD
}

static mut RNDINDEX: usize = 0;
static mut PRNDINDEX: usize = 0;

pub const RNDTABLE: [i32; 256] = [
    0, 8, 109, 220, 222, 241, 149, 107, 75, 248, 254, 140, 16, 66, 74, 21, 211, 47, 80, 242, 154,
    27, 205, 128, 161, 89, 77, 36, 95, 110, 85, 48, 212, 140, 211, 249, 22, 79, 200, 50, 28, 188,
    52, 140, 202, 120, 68, 145, 62, 70, 184, 190, 91, 197, 152, 224, 149, 104, 25, 178, 252, 182,
    202, 182, 141, 197, 4, 81, 181, 242, 145, 42, 39, 227, 156, 198, 225, 193, 219, 93, 122, 175,
    249, 0, 175, 143, 70, 239, 46, 246, 163, 53, 163, 109, 168, 135, 2, 235, 25, 92, 20, 145, 138,
    77, 69, 166, 78, 176, 173, 212, 166, 113, 94, 161, 41, 50, 239, 49, 111, 164, 70, 60, 2, 37,
    171, 75, 136, 156, 11, 56, 42, 146, 138, 229, 73, 146, 77, 61, 98, 196, 135, 106, 63, 197, 195,
    86, 96, 203, 113, 101, 170, 247, 181, 113, 80, 250, 108, 7, 255, 237, 129, 226, 79, 107, 112,
    166, 103, 241, 24, 223, 239, 120, 198, 58, 60, 82, 128, 3, 184, 66, 143, 224, 145, 224, 81,
    206, 163, 45, 63, 90, 168, 114, 59, 33, 159, 95, 28, 139, 123, 98, 125, 196, 15, 70, 194, 253,
    54, 14, 109, 226, 71, 17, 161, 93, 186, 87, 244, 138, 20, 52, 123, 251, 26, 36, 17, 46, 52,
    231, 232, 76, 31, 221, 84, 37, 216, 165, 212, 106, 197, 242, 98, 43, 39, 175, 254, 145, 190,
    84, 118, 222, 187, 136, 120, 163, 236, 249,
];

pub fn p_random() -> i32 {
    unsafe {
        PRNDINDEX = (PRNDINDEX + 1) & 0xFF;
        RNDTABLE[PRNDINDEX]
    }
}

pub fn m_random() -> i32 {
    unsafe {
        RNDINDEX = (RNDINDEX + 1) & 0xFF;
        RNDTABLE[RNDINDEX]
    }
}

pub fn m_clear_random() {
    unsafe {
        // Not clearing this random as it's used only by screen wipe so far
        //RNDINDEX = 0;
        PRNDINDEX = 0;
    }
}

pub fn p_subrandom() -> i32 {
    let r = p_random();
    r - p_random()
}

/// Used in path tracing for intercepts
/// Is divline + trace types
#[derive(Debug, Clone, Copy)]
pub struct Trace {
    pub xy: Vec2,
    pub dxy: Vec2,
}

impl Trace {
    pub fn new(xy: Vec2, dxy: Vec2) -> Self {
        Self { xy, dxy }
    }
}

#[derive(Default, Clone, PartialEq)]
pub struct Intercept {
    pub frac: f32,
    pub line: Option<MapPtr<LineDef>>,
    pub thing: Option<MapPtr<MapObject>>,
}

impl PartialOrd for Intercept {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Intercept {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.frac < other.frac {
            std::cmp::Ordering::Less
        } else if self.frac > other.frac {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

impl Eq for Intercept {}

#[derive(Default)]
pub struct BestSlide {
    pub best_slide_frac: f32,
    pub second_slide_frac: f32,
    pub best_slide_line: Option<MapPtr<LineDef>>,
    pub second_slide_line: Option<MapPtr<LineDef>>,
}

impl BestSlide {
    pub fn new() -> Self {
        BestSlide {
            best_slide_frac: 1.0,
            ..Default::default()
        }
    }
}

/// Functions like `P_LineOpening`
#[derive(Default, Debug)]
pub struct PortalZ {
    /// The lowest ceiling of the portal line
    pub top_z: f32,
    /// The highest floor of the portal line
    pub bottom_z: f32,
    /// Range between `bottom_z` and `top_z`
    pub range: f32,
    /// The lowest floor of the portal line
    pub lowest_z: f32,
}

impl PortalZ {
    pub fn new(line: &LineDef) -> Self {
        if line.backsector.is_none() {
            return Self::default();
        }

        let front = &line.frontsector;
        let back = unsafe { line.backsector.as_ref().unwrap_unchecked() };

        let mut ww = PortalZ {
            top_z: 0.0,
            bottom_z: 0.0,
            range: 0.0,
            lowest_z: 0.0,
        };

        if front.ceilingheight < back.ceilingheight {
            ww.top_z = front.ceilingheight;
        } else {
            ww.top_z = back.ceilingheight;
        }

        if front.floorheight > back.floorheight {
            ww.bottom_z = front.floorheight;
            ww.lowest_z = back.floorheight;
        } else {
            ww.bottom_z = back.floorheight;
            ww.lowest_z = front.floorheight;
        }
        ww.range = ww.top_z - ww.bottom_z;

        ww
    }
}

/// Returns -1 if the line runs through the box at all
pub fn box_on_line_side(tmbox: &BBox, ld: &LineDef) -> i32 {
    let p1;
    let p2;

    match ld.slopetype {
        SlopeType::Horizontal => {
            p1 = (tmbox.top > ld.v1.y) as i32;
            p2 = (tmbox.bottom > ld.v1.y) as i32;
        }
        SlopeType::Vertical => {
            p1 = (tmbox.right > ld.v1.x) as i32;
            p2 = (tmbox.left > ld.v1.x) as i32;
        }
        SlopeType::Positive => {
            p1 = ld.point_on_side(Vec2::new(tmbox.left, tmbox.top)) as i32;
            p2 = ld.point_on_side(Vec2::new(tmbox.right, tmbox.bottom)) as i32;
        }
        SlopeType::Negative => {
            p1 = ld.point_on_side(Vec2::new(tmbox.right, tmbox.top)) as i32;
            p2 = ld.point_on_side(Vec2::new(tmbox.left, tmbox.bottom)) as i32;
        }
    }

    if p1 == p2 {
        return p1;
    }
    -1
}

pub fn point_to_angle_2(point1: Vec2, point2: Vec2) -> Angle {
    let x = point1.x - point2.x;
    let y = point1.y - point2.y;
    Angle::new(y.atan2(x))
}

pub fn path_traverse(
    origin: Vec2,
    endpoint: Vec2,
    flags: i32,
    level: &mut Level,
    trav: impl FnMut(&mut Intercept) -> bool,
    bsp_trace: &mut BSPTrace,
) -> bool {
    let earlyout = flags & PT_EARLYOUT != 0;
    let mut intercepts: Vec<Intercept> = Vec::with_capacity(20);
    let trace = Trace::new(origin, endpoint - origin);

    level.valid_count = level.valid_count.wrapping_add(1);
    for n in bsp_trace.intercepted_subsectors() {
        if flags & PT_ADDLINES != 0 {
            let start = level.map_data.subsectors_mut()[*n as usize].start_seg as usize;
            let end = start + level.map_data.subsectors_mut()[*n as usize].seg_count as usize;

            for seg in &mut level.map_data.segments_mut()[start..end] {
                if seg.linedef.valid_count == level.valid_count {
                    continue;
                }
                seg.linedef.valid_count = level.valid_count;

                if !add_line_intercepts(trace, seg.linedef.clone(), &mut intercepts, earlyout) {
                    return false; // early out
                }
            }
        }

        if flags & PT_ADDTHINGS != 0
            && !level.map_data.subsectors_mut()[*n as usize]
                .sector
                .run_mut_func_on_thinglist(|thing| {
                    add_thing_intercept(trace, &mut intercepts, thing, level.valid_count)
                })
        {
            return false; // early out
        }
    }

    intercepts.sort();

    traverse_intercepts(&mut intercepts, 1.0, trav)
}

pub fn traverse_intercepts(
    intercepts: &mut [Intercept],
    max_frac: f32,
    mut trav: impl FnMut(&mut Intercept) -> bool,
) -> bool {
    if intercepts.is_empty() {
        return false;
    }
    let mut intercept: *mut Intercept = unsafe { intercepts.get_unchecked_mut(0) };
    let mut intercepts = Vec::from(intercepts);
    let mut count = intercepts.len();

    while count != 0 {
        count -= 1;
        let mut dist = f32::MAX;

        for i in intercepts.iter_mut() {
            if i.frac < dist {
                dist = i.frac;
                intercept = i;
            }
        }

        if dist > max_frac {
            return true;
        }

        unsafe {
            if !trav(&mut *intercept) {
                return false;
            }

            (*intercept).frac = f32::MAX;
        }
    }
    true
}

/// Check the line and add the intercept if valid
///
/// `line_to_line` is for "perfect" line-to-line collision (shot trace, use line
/// etc)
pub fn add_line_intercepts(
    trace: Trace,
    line: MapPtr<LineDef>,
    intercepts: &mut Vec<Intercept>,
    earlyout: bool,
) -> bool {
    let s1 = point_on_side(trace, line.v1);
    let s2 = point_on_side(trace, line.v2);

    if s1 == s2 {
        // line isn't crossed
        return true;
    }

    let dl = Trace::new(line.v1, line.v2 - line.v1);
    let frac = intercept_vector(trace, dl);
    // Skip if the trace doesn't intersect this line
    if frac.is_sign_negative() {
        return true;
    }

    if earlyout && frac < 1.0 && line.backsector.is_none() {
        return false;
    }

    if line.backsector.is_none() && frac < 0.0 {
        return false;
    }

    // TODO: early out
    intercepts.push(Intercept {
        frac,
        line: Some(line),
        thing: None,
    });
    true
}

// TODO: needs a proper line-line intersection test.
fn add_thing_intercept(
    trace: Trace,
    intercepts: &mut Vec<Intercept>,
    thing: &mut MapObject,
    valid_count: usize,
) -> bool {
    if thing.valid_count == valid_count {
        // Already checked it
        return true;
    }
    thing.valid_count = valid_count;

    // Diagonals are too unrealiable for first check so use
    // Use the seg check to limit the range
    if !circle_seg_collide(thing.xy, thing.radius, trace.xy, trace.xy + trace.dxy) {
        return true;
    }
    // Get vector clockwise-perpendicular to trace
    let r = thing.radius;
    let p = Vec2::new(trace.xy.y, -trace.xy.x).normalize() * r;
    let v1 = thing.xy + p;
    let v2 = thing.xy - p;

    let dl = Trace::new(v1, v2 - v1);
    let frac = intercept_vector(trace, dl);

    // println!("Passing through {:?}, from x{},y{}, to x{},y{}, r{} f{}",
    // thing.kind, trace.xy.x, trace.xy.y, thing.xy.x, thing.xy.y, thing.radius,
    // frac);

    // Skip if the trace doesn't intersect this line
    if frac.is_sign_negative() {
        return true;
    }

    intercepts.push(Intercept {
        frac,
        line: None,
        thing: Some(MapPtr::new(thing)),
    });
    true
}

// Determine which side of the trace the vector point is on
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

/// Returns the fractional intercept point along the first divline.
///
/// The lines can be pictured as arg1 being an infinite plane, and arg2 being
/// the line to check if intersected by the plane.
///
/// P_InterceptVector
fn intercept_vector(v2: Trace, v1: Trace) -> f32 {
    // Doom does `v1->dy >> 8`, this is  x * 0.00390625
    let denominator = (v1.dxy.y * v2.dxy.x) - (v1.dxy.x * v2.dxy.y);
    if denominator == f32::EPSILON {
        return -0.0;
    }
    let numerator = ((v1.xy.x - v2.xy.x) * v1.dxy.y) + ((v2.xy.y - v1.xy.y) * v1.dxy.x);
    numerator / denominator
}

// #[inline]
// pub fn cross(lhs: &Vec2, rhs: &Vec2) -> f32 {
//     lhs.x * rhs.y - lhs.y * rhs.x
// }

/// True if the line segment from point1 to point2 penetrates the circle
#[inline]
pub fn circle_seg_collide(c_origin: Vec2, c_radius: f32, s_start: Vec2, s_end: Vec2) -> bool {
    let lc = c_origin - s_start;
    let d = s_end - s_start;
    let p = project_vec2(lc, d);
    let nearest = s_start + p;

    if circle_point_intersect(c_origin, c_radius, nearest)
        && p.length() < d.length()
        && p.dot(d) > f32::EPSILON
    {
        // return Some((nearest - c_origin).normalize() * dist);
        return true;
    }
    false
}

#[inline]
pub fn circle_line_collide(c_origin: Vec2, c_radius: f32, l_start: Vec2, l_end: Vec2) -> bool {
    let lc = c_origin - l_start;
    let p = project_vec2(lc, l_end - l_start);
    let nearest = l_start + p;

    circle_point_intersect(c_origin, c_radius, nearest)
}

fn project_vec2(this: Vec2, onto: Vec2) -> Vec2 {
    let d = onto.dot(onto);
    if d > 0.0 {
        let dp = this.dot(onto);
        return onto * (dp / d);
    }
    onto
}

#[inline]
pub fn circle_point_intersect(origin: Vec2, radius: f32, point: Vec2) -> bool {
    let dist = point - origin;
    let len = dist.length();
    if len < radius {
        return true; // Some(len - radius);
    }
    false
}

// #[inline]
// pub fn circle_circle_intersect(
//     origin: Vec2,
//     origin_radius: f32,
//     point: Vec2,
//     point_radius: f32,
// ) -> bool {
//     let dist = point - origin;
//     let len = dist.length();
//     if len < origin_radius + point_radius {
//         return true; // Some(len - radius);
//     }
//     false
// }

#[cfg(test)]
mod tests {
    use super::bam_to_radian;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    // use crate::play::utilities::{circle_point_intersect,
    // circle_to_line_intercept_basic};

    // #[test]
    // fn circle_vec2_intersect() {
    //     let r = 1.0;
    //     let origin = Vec2::new(3.0, 5.0);
    //     let point = Vec2::new(2.5, 4.5);
    //     assert!(circle_point_intersect(origin, r, point).is_some());

    //     let point = Vec2::new(3.5, 5.5);
    //     assert!(circle_point_intersect(origin, r, point).is_some());

    //     let point = Vec2::new(2.0, 4.0);
    //     assert!(circle_point_intersect(origin, r, point).is_none());

    //     let point = Vec2::new(4.0, 7.0);
    //     let r = 2.5;
    //     assert!(circle_point_intersect(origin, r, point).is_some());
    // }

    // #[test]
    // fn test_circle_to_line_intercept_basic() {
    //     let r = 5.0;
    //     let origin = Vec2::new(5.0, 7.0);
    //     let point1 = Vec2::new(1.0, 3.0);
    //     let point2 = Vec2::new(7.0, 20.0);
    //     assert!(circle_to_line_intercept_basic(origin, r, point1,
    // point2).is_some());

    //     let r = 2.0;
    //     assert!(circle_to_line_intercept_basic(origin, r, point1,
    // point2).is_none()); }

    // #[test]
    // fn test_line_line_intersection() {
    //     let origin1 = Vec2::new(5.0, 1.0);
    //     let origin2 = Vec2::new(5.0, 10.0);
    //     let point1 = Vec2::new(1.0, 5.0);
    //     let point2 = Vec2::new(10.0, 5.0);
    //     assert!(line_line_intersection(origin1, origin2, point1, point2));

    //     let point1 = Vec2::new(5.0, 1.0);
    //     let point2 = Vec2::new(5.0, 10.0);
    //     assert!(line_line_intersection(origin1, origin2, point1, point2));

    //     let point1 = Vec2::new(4.0, 1.0);
    //     let point2 = Vec2::new(4.0, 10.0);
    //     assert!(!line_line_intersection(origin1, origin2, point1, point2));

    //     let origin1 = Vec2::new(1.0, 1.0);
    //     let origin2 = Vec2::new(10.0, 10.0);
    //     let point1 = Vec2::new(10.0, 1.0);
    //     let point2 = Vec2::new(1.0, 10.0);
    //     assert!(line_line_intersection(origin1, origin2, point1, point2));
    // }

    #[test]
    #[allow(clippy::float_cmp)]
    fn convert_bam_to_rad() {
        // DOOM constants
        let ang45: u32 = 0x20000000;
        let ang90: u32 = 0x40000000;
        let ang180: u32 = 0x80000000;

        let one: u32 = 1 << 26;

        assert_eq!(bam_to_radian(ang45), FRAC_PI_4);
        assert_eq!(bam_to_radian(ang90), FRAC_PI_2);
        assert_eq!(bam_to_radian(ang180), PI);
        assert_eq!(bam_to_radian(one).to_degrees(), 5.625);
    }
}
