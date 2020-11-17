use sdl2::{gfx::primitives::DrawRenderer, render::Canvas, surface::Surface};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use utils::radian_range;
use wad::{lumps::Segment, Vertex};

use crate::{
    angle::{Angle, CLASSIC_DOOM_SCREEN_XTO_VIEW},
    bsp::Bsp,
    player::Player,
};

impl Bsp {
    /// R_StoreWallRange - r_segs (required in r_bsp)
    pub fn store_wall_range(
        &self,
        start: i32,
        stop: i32,
        seg: &Segment,
        object: &Player,
        canvas: &mut Canvas<Surface>,
    ) {
        if start >= 320 || start > stop {
            //println!("Bad R_RenderWallRange: {} to {}", start, stop);
            return;
        }

        // let sidedef = seg.sidedef.clone();
        // let linedef = seg.linedef.clone();

        // mark the segment as visible for auto map
        // linedef->flags |= ML_MAPPED;

        let rw_normalangle = Angle::new(seg.angle_rads() + FRAC_PI_2);
        let offsetangle = rw_normalangle - self.get_rw_angle1(); // radians

        // Unrequired with full angle range
        // if offsetangle > FRAC_PI_2 {
        //     offsetangle = FRAC_PI_2;
        // }

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp =
            point_to_dist(seg.start_vertex.x(), seg.start_vertex.y(), object); // verified correct
        let rw_distance = hyp * distangle.sin(); // COrrect??? Seems to be...

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = object.rotation;

        //m_ScreenXToAngle[i] = atan((m_HalfScreenWidth - i) / (float)m_iDistancePlayerToScreen) * 180 / PI;
        let visangle = view_angle
            + CLASSIC_DOOM_SCREEN_XTO_VIEW[start as usize] * PI / 180.0; // degress not rads
        let scale1 = scale(visangle, rw_normalangle, rw_distance, view_angle);

        let visangle = view_angle
            + CLASSIC_DOOM_SCREEN_XTO_VIEW[stop as usize] * PI / 180.0;
        let scale2 = scale(visangle, rw_normalangle, rw_distance, view_angle);

        let steps = (scale2 - scale1) / (stop - start) as f32;
        let z = object.z as i16;
        let ceil = seg.sidedef.sector.ceil_height - z;
        let floor = seg.sidedef.sector.floor_height - z;

        let ceil_step = -(ceil as f32 * steps);
        let mut ceil_end = 100.0 - (ceil as f32 * scale1);

        let floor_step = -(floor as f32 * steps);
        let mut floor_start = 100.0 - (floor as f32 * scale1);

        let alpha = 255;
        let z = (z - seg.sidedef.sector.floor_height) as u8;
        let c = 100 - (ceil - floor) as u8 / 2;
        let colour =
            sdl2::pixels::Color::RGBA(255 - z, 200 - c, 200 - c, alpha);

        // 130
        let mut curr = start;
        while curr <= stop {
            canvas
                .line(
                    curr as i16,
                    ceil_end as i16,
                    curr as i16,
                    floor_start as i16,
                    colour,
                )
                .ok();

            curr += 1;
            ceil_end += ceil_step;
            floor_start += floor_step;
            if curr < 0 {
                break;
            }
        }

        self.test_fov_draw_solids(object, seg, canvas);
    }

    /// Translate the automap vertex to screen coords
    fn test_vertex_to_screen(
        &self,
        v: &Vertex,
        canvas: &mut Canvas<Surface>,
    ) -> (i16, i16) {
        let extents = self.get_map_extents();
        let scale = extents.automap_scale;
        let scr_height = canvas.surface().height() as f32;
        let scr_width = canvas.surface().width() as f32;

        let x_pad = (scr_width * scale - extents.width) / 2.0;
        let y_pad = (scr_height * scale - extents.height) / 2.0;

        let x_shift = -extents.min_vertex.x() + x_pad;
        let y_shift = -extents.min_vertex.y() + y_pad;
        (
            ((v.x() + x_shift) / scale) as i16,
            (scr_height - (v.y() + y_shift) / scale) as i16,
        )
    }

    /// automap fov
    pub fn test_fov_draw_solids(
        &self,
        object: &Player,
        seg: &Segment,
        canvas: &mut Canvas<Surface>,
    ) {
        let alpha = 255;
        let screen_start =
            self.test_vertex_to_screen(&seg.start_vertex, canvas);
        let screen_end = self.test_vertex_to_screen(&seg.end_vertex, canvas);

        let sector = &seg.sidedef.sector;
        if sector.floor_height > object.z as i16 {
            canvas
                .thick_line(
                    screen_start.0,
                    screen_start.1,
                    screen_end.0,
                    screen_end.1,
                    1,
                    sdl2::pixels::Color::RGBA(55, 170, 0, alpha),
                )
                .unwrap();
        } else if sector.floor_height < object.z as i16 {
            canvas
                .thick_line(
                    screen_start.0,
                    screen_start.1,
                    screen_end.0,
                    screen_end.1,
                    1,
                    sdl2::pixels::Color::RGBA(140, 240, 0, alpha),
                )
                .unwrap();
        } else {
            canvas
                .thick_line(
                    screen_start.0,
                    screen_start.1,
                    screen_end.0,
                    screen_end.1,
                    1,
                    sdl2::pixels::Color::RGBA(200, 200, 200, alpha),
                )
                .unwrap();
        }

        let player = self.test_vertex_to_screen(&object.xy, canvas);

        let (py, px) = object.rotation.sin_cos();
        let (lpy, lpx) = (object.rotation + PI / 4.0).sin_cos();
        let (rpy, rpx) = (object.rotation - PI / 4.0).sin_cos();
        let c = sdl2::pixels::Color::RGBA(20, 200, 200, alpha);
        canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (px * 25.0) as i16,
                player.1 - (py * 25.0) as i16,
                1,
                c,
            )
            .unwrap();
        canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (lpx * 500.0) as i16,
                player.1 - (lpy * 500.0) as i16,
                1,
                c,
            )
            .unwrap();
        canvas
            .thick_line(
                player.0,
                player.1,
                player.0 + (rpx * 500.0) as i16,
                player.1 - (rpy * 500.0) as i16,
                1,
                c,
            )
            .unwrap();
    }
}

// Verified correct
fn point_to_dist(x: f32, y: f32, object: &Player) -> f32 {
    let mut dx = (x - object.xy.x()).abs();
    let mut dy = (y - object.xy.y()).abs();

    if dy > dx {
        let temp = dx;
        dx = dy;
        dy = temp;
    }

    let dist = (dx.powi(2) + dy.powi(2)).sqrt();
    dist
}

/// R_ScaleFromGlobalAngle
// All should be in rads
fn scale(
    visangle: Angle,
    rw_normalangle: Angle,
    rw_distance: f32,
    view_angle: Angle,
) -> f32 {
    static MAX_SCALEFACTOR: f32 = 64.0;
    static MIN_SCALEFACTOR: f32 = 0.00390625;

    let anglea = radian_range(FRAC_PI_2 + visangle.rad() - view_angle.rad()); // CORRECT
    let angleb =
        radian_range(FRAC_PI_2 + visangle.rad() - rw_normalangle.rad()); // CORRECT

    let sinea = anglea.sin(); // not correct?
    let sineb = angleb.sin();

    //            projection
    //m_iDistancePlayerToScreen = m_HalfScreenWidth / HalfFOV.GetTanValue();
    let p = 160.0 / (FRAC_PI_4).tan();
    let num = p * sineb; // oof a bit
    let den = rw_distance * sinea;

    let mut scale = num / den;

    if scale > MAX_SCALEFACTOR {
        scale = MAX_SCALEFACTOR;
    } else if MIN_SCALEFACTOR > scale {
        scale = MIN_SCALEFACTOR;
    }
    scale
}
