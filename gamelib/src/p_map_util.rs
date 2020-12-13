use crate::level_data::map_defs::{LineDef, SlopeType};
use crate::p_map::{BOXBOTTOM, BOXLEFT, BOXRIGHT, BOXTOP};
use glam::Vec2;

pub(crate) fn box_on_line_side(tmbox: &[f32; 4], ld: &LineDef) -> i32 {
    let mut p1;
    let mut p2;

    match ld.slopetype {
        SlopeType::Horizontal => {
            p1 = (tmbox[BOXTOP] > ld.v1.y()) as i32;
            p2 = (tmbox[BOXBOTTOM] > ld.v1.y()) as i32;
            if ld.delta.x() < f32::EPSILON {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Vertical => {
            p1 = (tmbox[BOXRIGHT] > ld.v1.x()) as i32;
            p2 = (tmbox[BOXLEFT] > ld.v1.x()) as i32;
            if ld.delta.y() < f32::EPSILON {
                p1 ^= 1;
                p2 ^= 1;
            }
        }
        SlopeType::Positive => {
            p1 = ld.point_on_side(&Vec2::new(tmbox[BOXLEFT], tmbox[BOXTOP]))
                as i32;
            p2 = ld.point_on_side(&Vec2::new(tmbox[BOXRIGHT], tmbox[BOXBOTTOM]))
                as i32;
        }
        SlopeType::Negative => {
            p1 = ld.point_on_side(&Vec2::new(tmbox[BOXRIGHT], tmbox[BOXTOP]))
                as i32;
            p2 = ld.point_on_side(&Vec2::new(tmbox[BOXLEFT], tmbox[BOXBOTTOM]))
                as i32;
        }
    }

    if p1 == p2 {
        return p1;
    }
    -1
}
