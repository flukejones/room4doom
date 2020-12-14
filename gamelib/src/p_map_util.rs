use crate::level_data::map_defs::{BBox, LineDef, SlopeType};
use glam::Vec2;

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
