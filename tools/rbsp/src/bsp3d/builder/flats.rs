//! Floor and ceiling polygon construction from the pre-carved convex subsector
//! polygons. Sky surfaces produce no flat (the sky pass + filler walls draw
//! them); winding is normalised so floors face +Z and ceilings face −Z.

use glam::{Vec2, Vec3};

use crate::bsp3d::input::Bsp3dInput;

use super::Bsp3dBuilder;
use super::types::{BuildKind, BuildPolygon, MIN_TRI_CROSS, vertex_shoelace};

impl Bsp3dBuilder {
    /// Create the floor and ceiling N-gon for one subsector from its carved
    /// polygon. Sky surfaces produce no polygon. Winding contract: floor CCW
    /// viewed from above (+Z normal), ceiling CW (−Z normal).
    ///
    /// Input polygon winding determines whether to reverse: rbsp emits CCW,
    /// older carve paths CW.
    pub(super) fn create_floor_ceiling_polygons(
        &mut self,
        input: &Bsp3dInput,
        ss_id: usize,
        sector_id: usize,
        polygon: &[Vec2],
    ) {
        if polygon.len() < 3 {
            return;
        }

        let sector = &input.sectors[sector_id];
        let skip_ceil = sector.sky_ceil;
        let skip_floor = sector.sky_floor;

        // Degenerate check via shoelace area.
        let shoelace: f32 = polygon
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f32>()
            + polygon.last().unwrap().x * polygon[0].y
            - polygon[0].x * polygon.last().unwrap().y;
        if shoelace.abs() < MIN_TRI_CROSS {
            return;
        }
        let input_is_ccw = shoelace > 0.0;

        if !skip_floor {
            let ordered: Vec<&Vec2> = if input_is_ccw {
                polygon.iter().collect()
            } else {
                polygon.iter().rev().collect()
            };
            let fv: Vec<usize> = ordered
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, sector.floor_z(v.x, v.y))))
                .collect();
            if fv.len() >= 3 && vertex_shoelace(&fv, &self.vertices) > 0.0 {
                let fi = self.polygons.len();
                self.polygons.push(BuildPolygon {
                    sector_id,
                    vertices: fv,
                    kind: BuildKind::Flat,
                    moves: false,
                });
                self.leaves[ss_id].polys.push(fi);
                self.leaves[ss_id].floor_polygons.push(fi);
            }
        }

        if !skip_ceil {
            let ordered: Vec<&Vec2> = if input_is_ccw {
                polygon.iter().rev().collect()
            } else {
                polygon.iter().collect()
            };
            let cv: Vec<usize> = ordered
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, sector.ceil_z(v.x, v.y))))
                .collect();
            if cv.len() < 3 || vertex_shoelace(&cv, &self.vertices) >= 0.0 {
                return;
            }
            let ci = self.polygons.len();
            self.polygons.push(BuildPolygon {
                sector_id,
                vertices: cv,
                kind: BuildKind::Flat,
                moves: false,
            });
            self.leaves[ss_id].polys.push(ci);
            self.leaves[ss_id].ceiling_polygons.push(ci);
        }
    }
}
