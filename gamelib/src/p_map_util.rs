use std::ptr::null_mut;

use crate::{
    level_data::{
        level::Level,
        map_data::BSPTrace,
        map_defs::{BBox, LineDef, SlopeType, SubSector},
    },
    p_local::{Intercept, Trace},
    p_map::{PT_ADDLINES, PT_EARLYOUT},
    p_map_object::{MapObject, MapObjectFlag},
    DPtr,
};
use glam::Vec2;

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
            p1 = (tmbox.top > ld.v1.y()) as i32;
            p2 = (tmbox.bottom > ld.v1.y()) as i32;
        }
        SlopeType::Vertical => {
            p1 = (tmbox.right > ld.v1.x()) as i32;
            p2 = (tmbox.left > ld.v1.x()) as i32;
        }
        SlopeType::Positive => {
            p1 = ld.point_on_side(&Vec2::new(tmbox.left, tmbox.top)) as i32;
            p2 = ld.point_on_side(&Vec2::new(tmbox.right, tmbox.bottom)) as i32;
        }
        SlopeType::Negative => {
            p1 = ld.point_on_side(&Vec2::new(tmbox.right, tmbox.top)) as i32;
            p2 = ld.point_on_side(&Vec2::new(tmbox.left, tmbox.bottom)) as i32;
        }
    }

    if p1 == p2 {
        return p1;
    }
    -1
}

#[inline]
pub fn cross(lhs: &Vec2, rhs: &Vec2) -> f32 {
    lhs.x() * rhs.y() - lhs.y() * rhs.x()
}

#[inline]
pub fn ray_to_line_intersect(
    origin: &Vec2,
    direction: f32,
    point1: &Vec2,
    point2: &Vec2,
) -> Option<f32> {
    let direction = unit_vec_from(direction);
    let v1 = *origin - *point1;
    let v2 = *point2 - *point1;
    let v3 = Vec2::new(-direction.y(), direction.x());
    let dot = v2.dot(v3);
    if dot.abs() < 0.000001 {
        return None;
    }
    let t1 = dot / cross(&v2, &v1);
    let t2 = v1.dot(v3) / dot;
    if t1 >= 0.0 && t2 >= 0.0 && t2 <= 1.0 {
        return Some(t1);
    }
    None
}

pub struct Slide {
    pub direction: Vec2,
    pub delta: f32,
}

#[inline]
pub fn circle_to_line_intercept_basic(
    origin: Vec2,
    radius: f32,
    point1: Vec2,
    point2: Vec2,
) -> Option<Vec2> {
    let lc = origin - point1;
    let d = point2 - point1;
    let p = project_vec2(lc, d);
    let nearest = point1 + p;

    if let Some(dist) = circle_point_intersect(origin, radius, nearest) {
        if p.length() < d.length() && p.dot(d) > f32::EPSILON {
            return Some((nearest - origin).normalize() * dist);
        }
    }
    None
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
pub fn circle_point_intersect(origin: Vec2, radius: f32, point: Vec2) -> Option<f32> {
    let dist = point - origin;
    let len = dist.length();
    if len < radius {
        return Some(len - radius);
    }
    None
}

#[inline]
pub fn unit_vec_from(rotation: f32) -> Vec2 {
    let (y, x) = rotation.sin_cos();
    Vec2::new(x, y)
}

pub fn path_traverse(
    origin: Vec2,
    endpoint: Vec2,
    flags: i32,
    line_to_line: bool,
    level: &mut Level,
    trav: impl FnMut(&Intercept) -> bool,
    bsp_trace: &mut BSPTrace,
) -> bool {
    let earlyout = flags & PT_EARLYOUT != 0;
    let mut intercepts: Vec<Intercept> = Vec::with_capacity(20);
    let trace = Trace::new(origin, endpoint - origin);

    let segs = &mut level.map_data.segments;
    let sub_sectors = &mut level.map_data.subsectors;

    level.valid_count += 1;
    for n in bsp_trace.intercepted_nodes() {
        let ssect = &sub_sectors[*n as usize];
        let start = ssect.start_seg as usize;
        let end = start + ssect.seg_count as usize;

        for seg in &mut segs[start..end] {
            if seg.linedef.valid_count == level.valid_count {
                continue;
            }
            seg.linedef.valid_count = level.valid_count;

            if flags & PT_ADDLINES != 0
                && !add_line_intercepts(
                    &trace,
                    seg.linedef.clone(),
                    &mut intercepts,
                    earlyout,
                    line_to_line,
                )
            {
                return false; // early out
            }
        }
    }
    traverse_intercepts(&mut intercepts, 1.0, trav)
}

pub fn traverse_intercepts(
    intercepts: &mut [Intercept],
    max_frac: f32,
    mut trav: impl FnMut(&Intercept) -> bool,
) -> bool {
    let mut intercept: *mut Intercept = unsafe { intercepts.get_unchecked_mut(0) };
    let mut intercepts = Vec::from(intercepts);
    let mut count = intercepts.len();

    while count != 0 {
        count -= 1;
        let mut dist = f32::MAX;

        for i in intercepts.iter_mut() {
            if i.frac <= dist {
                dist = i.frac;
                intercept = i;
            }
        }

        if dist > max_frac {
            return true;
        }

        unsafe {
            if !trav(&*intercept) {
                return false;
            }

            (*intercept).frac = f32::MAX;
        }
    }

    true
}

/// Check the line and add the intercept if valid
///
/// `line_to_line` is for "perfect" line-to-line collision (shot trace, use line etc)
pub fn add_line_intercepts(
    trace: &Trace,
    line: DPtr<LineDef>,
    intercepts: &mut Vec<Intercept>,
    earlyout: bool,
    line_to_line: bool,
) -> bool {
    let s1 = line.point_on_side(&trace.xy);
    let s2 = line.point_on_side(&(trace.xy + trace.dxy));

    if s1 == s2 {
        // line isn't crossed
        return true;
    }

    let dl = Trace::new(*line.v1, (*line.v2) - (*line.v1));
    let frac = if line_to_line {
        line_line_intersection(&dl, trace)
    } else {
        // check against line 'plane', good for slides, not much else
        line_line_intersection(trace, &dl)
    };
    // Skip if the trace doesn't intersect this line
    if frac.is_sign_negative() {
        return true;
    }

    if earlyout && frac < 1.0 && line.backsector.is_none() {
        return false;
    }

    // Only works if the angles are translated to 0-180
    // if line.backsector.is_none() && frac < 1.0 {
    //     return false;
    // }

    // TODO: early out
    intercepts.push(Intercept {
        frac,
        line: Some(line),
        thing: None,
    });
    true
}

// fn intercept_vector(v2: &Trace, v1: &Trace) -> f32 {
//     // Doom does `v1->dy >> 8`, this is  x * 0.00390625
//     let denominator = (v1.dxy.y() * v2.dxy.x()) - (v1.dxy.x() * v2.dxy.y());
//     if denominator == f32::EPSILON {
//         return 0.0;
//     }
//     let numerator = ((v1.xy.x() - v2.xy.x()) * v1.dxy.y()) + ((v2.xy.y() - v1.xy.y()) * v1.dxy.x());
//     numerator / denominator
// }

/// P_InterceptVector
/// Returns the fractional intercept point along the first divline.
///
/// The lines can be pictured as arg1 being an infinite plane, and arg2 being the line
/// to check if intersected by the plane.
///
/// Good for perfect line intersection test
fn line_line_intersection(v1: &Trace, v2: &Trace) -> f32 {
    let denominator = (v2.dxy.x() * v1.dxy.y()) - (v2.dxy.y() * v1.dxy.x());
    if denominator == 0.0 {
        return 0.0;
    }
    let numerator = ((v2.xy.y() - v1.xy.y()) * v2.dxy.x()) - ((v2.xy.x() - v1.xy.x()) * v2.dxy.y());
    numerator / denominator
}

#[cfg(test)]
mod tests {
    use crate::p_map_util::*;
    use glam::Vec2;

    #[test]
    fn circle_vec2_intersect() {
        let r = 1.0;
        let origin = Vec2::new(3.0, 5.0);
        let point = Vec2::new(2.5, 4.5);
        assert!(circle_point_intersect(origin, r, point).is_some());

        let point = Vec2::new(3.5, 5.5);
        assert!(circle_point_intersect(origin, r, point).is_some());

        let point = Vec2::new(2.0, 4.0);
        assert!(circle_point_intersect(origin, r, point).is_none());

        let point = Vec2::new(4.0, 7.0);
        let r = 2.5;
        assert!(circle_point_intersect(origin, r, point).is_some());
    }

    #[test]
    fn test_circle_to_line_intercept_basic() {
        let r = 5.0;
        let origin = Vec2::new(5.0, 7.0);
        let point1 = Vec2::new(1.0, 3.0);
        let point2 = Vec2::new(7.0, 20.0);
        assert!(circle_to_line_intercept_basic(origin, r, point1, point2).is_some());

        let r = 2.0;
        assert!(circle_to_line_intercept_basic(origin, r, point1, point2).is_none());
    }

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
}
