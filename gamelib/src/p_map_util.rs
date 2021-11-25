use crate::level_data::map_defs::{BBox, LineDef, SlopeType};
use glam::Vec2;
use std::f32::EPSILON;

#[derive(Default)]
pub(crate) struct PortalZ {
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

pub(crate) fn box_on_line_side(tmbox: &BBox, ld: &LineDef) -> i32 {
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

/// Produce a `LineContact` with the normal from movement->line, and the depth
/// of penetration taking in to account the radius.
///
/// Does some of `P_HitSlideLine`
#[inline]
pub(crate) fn line_slide_direction(
    origin: Vec2,
    momentum: Vec2,
    radius: f32,
    point1: Vec2,
    point2: Vec2,
) -> Option<Vec2> {
    let mxy = momentum.normalize() * radius;
    let move_to = origin + momentum + mxy;

    let lc = move_to - point1;
    let d = point2 - point1;
    let p = project_vec2(lc, d);

    let mxy_on_line = point1 + p;

    let lc = origin - point1;
    let p2 = project_vec2(lc, d);
    // point on line from starting point
    let origin_on_line = point1 + p2;

    if p.length() < d.length() && p.dot(d) > EPSILON {
        // line angle headng in direction we need to slide
        let mut slide_direction = (mxy_on_line - origin_on_line).normalize();
        if slide_direction.x().is_nan() || slide_direction.y().is_nan() {
            slide_direction = Vec2::default();
        }

        let mut vs_angle = mxy.angle_between(slide_direction).cos();
        if vs_angle.is_nan() {
            vs_angle = 0.0;
        }

        return Some(slide_direction * (vs_angle * momentum.length()));
    }
    None
}

#[inline]
pub(crate) fn line_line_intersection(origin: Vec2, moved: Vec2, ln1: Vec2, ln2: Vec2) -> bool {
    // cross product: lhs.x() * rhs.y() - lhs.y() * rhs.x()
    // dot product  : v1.x * v2.x + v1.y * v2.y
    let denominator = ((moved.x() - origin.x()) * (ln2.y() - ln1.y()))
        - ((moved.y() - origin.y()) * (ln2.x() - ln1.x()));
    let numerator1 = ((origin.y() - ln1.y()) * (ln2.x() - ln1.x()))
        - ((origin.x() - ln1.x()) * (ln2.y() - ln1.y()));
    let numerator2 = ((origin.y() - ln1.y()) * (moved.x() - origin.x()))
        - ((origin.x() - ln1.x()) * (moved.y() - origin.y()));

    if denominator == 0.0 {
        return numerator1 == 0.0 && numerator2 == 0.0;
    }

    let r = numerator1 / denominator;
    let s = numerator2 / denominator;

    return (r >= 0.0 && r <= 1.0) && (s >= 0.0 && s <= 1.0);
}

#[inline]
pub(crate) fn circle_to_line_intercept_basic(
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

    #[test]
    fn test_line_line_intersection() {
        let origin1 = Vec2::new(5.0, 1.0);
        let origin2 = Vec2::new(5.0, 10.0);
        let point1 = Vec2::new(1.0, 5.0);
        let point2 = Vec2::new(10.0, 5.0);
        assert!(line_line_intersection(origin1, origin2, point1, point2));

        let point1 = Vec2::new(5.0, 1.0);
        let point2 = Vec2::new(5.0, 10.0);
        assert!(line_line_intersection(origin1, origin2, point1, point2));

        let point1 = Vec2::new(4.0, 1.0);
        let point2 = Vec2::new(4.0, 10.0);
        assert!(!line_line_intersection(origin1, origin2, point1, point2));

        let origin1 = Vec2::new(1.0, 1.0);
        let origin2 = Vec2::new(10.0, 10.0);
        let point1 = Vec2::new(10.0, 1.0);
        let point2 = Vec2::new(1.0, 10.0);
        assert!(line_line_intersection(origin1, origin2, point1, point2));
    }
}
