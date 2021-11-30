use crate::{DPtr, level_data::{level::{self, Level}, map_data::BSPTrace, map_defs::{BBox, LineDef, SlopeType}}, p_local::{BestSlide, Intercept, Trace}};
use glam::Vec2;
use std::f32::EPSILON;

#[derive(Default)]
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
        let back = line.backsector.as_ref().unwrap();

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

pub fn box_on_line_side(tmbox: &BBox, ld: &LineDef) -> i32 {
    let mut p1;
    let mut p2;

    match ld.slopetype {
        SlopeType::Horizontal => {
            p1 = (tmbox.top > ld.v1.y()) as i32;
            p2 = (tmbox.bottom > ld.v1.y()) as i32;
            if ld.delta.x() < 0.0 {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Vertical => {
            p1 = (tmbox.right > ld.v1.x()) as i32;
            p2 = (tmbox.left > ld.v1.x()) as i32;
            if ld.delta.y() < 0.0 {
                p1 ^= 1;
                p2 ^= 1;
            }
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
) -> Option<f32> {
    let lc = origin - point1;
    let d = point2 - point1;
    let p = project_vec2(lc, d);
    let nearest = point1 + p;

    if let Some(dist) = circle_point_intersect(origin, radius, nearest) {
        if p.length() < d.length() && p.dot(d) > EPSILON {
            return Some(dist);
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
fn circle_point_intersect(origin: Vec2, radius: f32, point: Vec2) -> Option<f32> {
    let dist = point - origin;
    let len = dist.length();
    if len < radius {
        return Some(radius - len);
    }
    None
}

#[inline]
pub fn unit_vec_from(rotation: f32) -> Vec2 {
    let (y, x) = rotation.sin_cos();
    Vec2::new(x, y)
}

pub fn path_traverse(origin: Vec2, endpoint: Vec2, level: &Level, trav: impl FnMut(&Intercept) -> bool) -> bool {
    let mut intercepts: Vec<Intercept> = Vec::with_capacity(20);
    let trace = Trace::new(origin, endpoint - origin);

    let mut bsp_trace = BSPTrace::new(origin, endpoint, level.map_data.start_node());
    bsp_trace.find_ssect_intercepts(&level.map_data);

    for n in bsp_trace.intercepted_nodes() {
        let segs = level.map_data.get_segments();
        let sub_sectors = level.map_data.get_subsectors();

        let ssect = &sub_sectors[*n as usize];
        let start = ssect.start_seg as usize;
        let end = start + ssect.seg_count as usize;
        for seg in &segs[start..end] {
            if seg.linedef.point_on_side(&origin) != seg.linedef.point_on_side(&endpoint) {
                // Add intercept
                // PIT_AddLineIntercepts
                if !add_line_intercepts(&trace, seg.linedef.clone(), &mut intercepts) {
                    // early out on first intercept?
                    return false;
                }
            }
        }
    }
    traverse_intercepts(&intercepts, 1.0, trav)
}

pub fn traverse_intercepts(intercepts: &Vec<Intercept>, max_frac: f32, mut trav: impl FnMut(&Intercept) -> bool) -> bool {
    let mut dist = f32::MAX;
    let mut intercept = &Intercept::default();
    for i in intercepts {
        if i.frac < dist {
            dist = i.frac;
            intercept = i;
        }
    }

    if dist > max_frac {
        return false;
    }

    // PTR_SlideTraverse checks if the line is blocking and sets the BestSlide
    if !trav(&intercept) {
        return false;
    }

    true
}

pub fn add_line_intercepts(trace: &Trace, line: DPtr<LineDef>, intercepts: &mut Vec<Intercept>) -> bool {
    let s1 = line.point_on_side(&trace.xy);
    let s2 = line.point_on_side(&(trace.xy + trace.dxy));

    if s1 == s2 {
        // line isn't crossed
        return true;
    }

    let dl = Trace::new(*line.v1, line.delta);
    let frac = intercept_vector (trace, &dl);

    if frac < 0.0 {
        return true; // behind the source
    }

    // if line.backsector.is_none() && frac < 1.0 {
    //     return false;
    // }

    // TODO: early out
    intercepts.push(Intercept{ frac, line: Some(line), thing: None } );
    true
}

/// P_InterceptVector
/// Returns the fractional intercept point
/// along the first divline.
/// This is only called by the addthings
/// and addlines traversers.
pub fn intercept_vector(v2: &Trace, v1: &Trace) -> f32 {
    // Does things with fixed-point like this without much explanation:
    // den = FixedMul (v1->dy>>8,v2->dx) - FixedMul(v1->dx>>8,v2->dy);
    // why the shift right by 8?
    let scale = 1.0;

    let denominator = (v1.dxy.y() * v2.dxy.x())
                         - (v1.dxy.x() * v2.dxy.y());
    let numerator1 = (v1.xy.x() - v2.xy.x()) * v1.dxy.y()
                         + (v2.xy.y() - v1.xy.y()) * v1.dxy.x();

    if denominator == EPSILON {
        return numerator1;
    }

    numerator1 / denominator
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
