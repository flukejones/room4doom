use sdl2::{gfx::primitives::DrawRenderer, render::Canvas, surface::Surface};
use std::f32::consts::{FRAC_PI_2, PI};
use wad::lumps::Segment;

use crate::{
    angle::{Angle, CLASSIC_DOOM_SCREEN_XTO_VIEW},
    player::Player,
    point_to_dist,
    r_bsp::BspCtrl,
    scale,
};

impl BspCtrl {
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
            println!("Bad R_RenderWallRange: {} to {}", start, stop);
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
        let hyp = point_to_dist(
            seg.start_vertex.x(),
            seg.start_vertex.y(),
            object.mobj.as_ref().unwrap().obj.xy,
        ); // verified correct
        let rw_distance = hyp * distangle.sin(); // COrrect??? Seems to be...

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = object.mobj.as_ref().unwrap().obj.angle;

        //m_ScreenXToAngle[i] = atan((m_HalfScreenWidth - i) / (float)m_iDistancePlayerToScreen) * 180 / PI;
        let visangle = view_angle
            + CLASSIC_DOOM_SCREEN_XTO_VIEW[start as usize] * PI / 180.0; // degress not rads
        let scale1 = scale(visangle, rw_normalangle, rw_distance, view_angle);

        let visangle = view_angle
            + CLASSIC_DOOM_SCREEN_XTO_VIEW[stop as usize] * PI / 180.0;
        let scale2 = scale(visangle, rw_normalangle, rw_distance, view_angle);

        // testing draws
        let rw_scalestep = (scale2 - scale1) / (stop - start) as f32;
        let z = object.viewz as i16;

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        let worldtop = seg.sidedef.sector.ceil_height - z;
        let worldbottom = seg.sidedef.sector.floor_height - z;

        // TODO: Texture stuff here
        //  midtexture = toptexture = bottomtexture = maskedtexture = 0;

        let topstep = -(worldtop as f32 * rw_scalestep);
        let mut topfrac = 100.0 - (worldtop as f32 * scale1);

        let bottomstep = -(worldbottom as f32 * rw_scalestep);
        let mut bottomfrac = 100.0 - (worldbottom as f32 * scale1);

        let alpha = 255;

        // testing lighting
        let mut lightnum =
            seg.linedef.front_sidedef.sector.light_level as u8 >> 4;
        if seg.start_vertex.y() == seg.end_vertex.y() {
            if lightnum > 5 {
                lightnum -= 5;
            }
        } else if seg.start_vertex.x() == seg.end_vertex.x() {
            if lightnum < 249 {
                lightnum += 5;
            }
        }

        let z = seg.sidedef.sector.floor_height.abs() as u8 / 2;

        let colour = sdl2::pixels::Color::RGBA(
            150 + lightnum - z as u8,
            50 + lightnum,
            50 + lightnum,
            alpha,
        );

        // R_RenderSegLoop
        let mut curr = start;
        while curr <= stop {
            canvas
                .line(
                    curr as i16,
                    topfrac as i16,
                    curr as i16,
                    bottomfrac as i16,
                    colour,
                )
                .ok();

            curr += 1;
            topfrac += topstep;
            bottomfrac += bottomstep;
            if curr < 0 {
                break;
            }
        }
    }
}
