use std::f32::consts::PI;

use glam::Vec2;

#[inline]
pub fn radian_range(rad: f32) -> f32 {
    if rad < 0.0 {
        return rad + 2.0 * PI;
    } else if rad >= 2.0 * PI {
        return rad - 2.0 * PI;
    }
    rad
}

#[inline]
pub fn degree_range(deg: f32) -> f32 {
    if deg < 0.0 {
        return deg + 360.0;
    } else if deg >= 360.0 {
        return deg - 360.0;
    }
    deg
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

#[inline]
pub fn unit_vec_from(rotation: f32) -> Vec2 {
    let (y, x) = rotation.sin_cos();
    Vec2::new(x, y)
}
