use crate::level_data::map_defs::{BBox, LineDef, SlopeType};
use crate::renderer::bsp::point_to_angle_2;
use glam::Vec2;
use std::f32::consts::PI;
use std::f32::EPSILON;

#[derive(Default)]
pub(crate) struct PortalZ {
    pub top_z:     f32,
    pub bottom_z:  f32,
    pub range:     f32,
    pub low_point: f32,
}

impl PortalZ {
    pub fn new(line: &LineDef) -> Self {
        if line.backsector.is_none() {
            return Self::default();
        }

        let front = &line.frontsector;
        let back = line.backsector.as_ref().unwrap();

        let mut ww = PortalZ {
            top_z:     0.0,
            bottom_z:  0.0,
            range:     0.0,
            low_point: 0.0,
        };

        if front.ceilingheight < back.ceilingheight {
            ww.top_z = front.ceilingheight;
        } else {
            ww.top_z = back.ceilingheight;
        }

        if front.floorheight > back.floorheight {
            ww.bottom_z = front.floorheight;
            ww.low_point = back.floorheight;
        } else {
            ww.bottom_z = back.floorheight;
            ww.low_point = front.floorheight;
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
            if ld.delta.x() < f32::EPSILON {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Vertical => {
            p1 = (tmbox.right > ld.v1.x()) as i32;
            p2 = (tmbox.left > ld.v1.x()) as i32;
            if ld.delta.y() < f32::EPSILON {
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

#[derive(Debug)]
pub(crate) struct LineContact {
    pub penetration:     f32,
    pub normal:          Vec2,
    pub slide_dir:       Vec2,
    pub angle_delta:     f32,
    pub point_contacted: Option<Vec2>,
}

impl LineContact {
    pub fn new(
        penetration: f32,
        normal: Vec2,
        slide_dir: Vec2,
        angle_delta: f32,
        point_contacted: Option<Vec2>,
    ) -> Self {
        LineContact {
            penetration,
            normal,
            slide_dir,
            angle_delta,
            point_contacted,
        }
    }
}

/// Produce a `LineContact` with the normal from movement->line, and the depth
/// of penetration taking in to account the radius.
#[inline]
pub(crate) fn circle_to_seg_intersect(
    origin: Vec2,
    momentum: Vec2,
    radius: f32,
    point1: Vec2,
    point2: Vec2,
) -> Option<LineContact> {
    let move_to = origin + momentum;

    // TODO: move the final block out to new call so that we're not calling twice on ends
    if let Some(dist) = circle_point_intersect(move_to, radius, point1) {
        return Some(LineContact::new(
            dist,
            (point1 - move_to).normalize(),
            Vec2::default(),
            0.0,
            Some(point1),
        ));
    }
    if let Some(dist) = circle_point_intersect(move_to, radius, point2) {
        return Some(LineContact::new(
            dist,
            (point2 - move_to).normalize(),
            Vec2::default(),
            0.0,
            Some(point2),
        ));
    }

    let lc = move_to - point1;
    let d = point2 - point1;
    let p = project_vec2(lc, d);

    let nearest = point1 + p;

    if let Some(mut dist) = circle_point_intersect(move_to, radius, nearest) {
        if (p.length() < d.length() && p.dot(d) > EPSILON) {
            // TODO: save enough info to build this data later when really required
            let lc = origin - point1;
            let p = project_vec2(lc, d);
            // point on line from starting point
            let origin_on_line = point1 + p;
            // line angle headng in direction we need to slide
            let mut slide_direction = (nearest - origin_on_line).normalize();
            if slide_direction.x().is_nan() || slide_direction.y().is_nan() {
                slide_direction = Vec2::default();
            }

            let mut vs_angle =
                slide_direction.angle_between(move_to - origin).cos();
            if vs_angle.is_nan() {
                vs_angle = 0.0;
            }

            return Some(LineContact::new(
                dist,
                (nearest - move_to).normalize(),
                slide_direction,
                vs_angle,
                None,
            ));
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
fn circle_point_intersect(
    origin: Vec2,
    radius: f32,
    point: Vec2,
) -> Option<f32> {
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
    fn circle_seg_intersect() {
        let r = 1.0;
        let origin = Vec2::new(5.0, 5.0);
        let point1 = Vec2::new(3.0, 5.0);
        let point2 = Vec2::new(7.0, 4.0);
        assert!(circle_to_seg_intersect(origin, r, point1, point2).is_some());

        let point1 = Vec2::new(5.2, 9.0);
        let point2 = Vec2::new(4.0, 7.0);
        assert!(circle_to_seg_intersect(origin, r, point1, point2).is_none());

        let r = 3.0;
        assert!(circle_point_intersect(origin, r, point1).is_none());
        assert!(circle_point_intersect(origin, r, point2).is_some());
        assert!(circle_to_seg_intersect(origin, r, point1, point2).is_some());
    }
}
