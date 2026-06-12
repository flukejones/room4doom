//! Line/polygon drawing with the Line tool: the click chain, segment commit,
//! and sector derivation when the draw finishes.

use editor_core::{Name8, Sector, SideDef, add_edge, derive_sectors, ngon_points, rect_corners};

use super::{LevelEditorState, NEW_LINE_FLAGS, ON_SEGMENT_TOL_PX};
use crate::boundary::DrawShape;
use crate::state::{Damage, Overlay, PolyChain, ShapeDraw};
use crate::undo::EditAction;

/// Click-on-chain-start/end radius (screen px); caller divides by zoom.
const CHAIN_CLOSE_PX: f32 = 5.0;

impl LevelEditorState {
    pub(super) fn poly_click(&mut self, world: [f32; 2], shift: bool) -> Damage {
        let point = self.snap_point(world);
        let Some(chain) = &self.poly else {
            let base = self.map.as_ref().map_or(0, |m| m.lines.len());
            self.poly = Some(PolyChain {
                points: vec![point],
                base,
            });
            self.refresh_chain_overlay(Some(point));
            return Damage::None;
        };

        let near = |a: [f32; 2], b: [f32; 2]| {
            let r = CHAIN_CLOSE_PX / self.camera.zoom_level();
            (a[0] - b[0]).abs() <= r && (a[1] - b[1]).abs() <= r
        };
        let (start, prev) = (chain.points[0], *chain.points.last().expect("non-empty"));
        if near(point, prev) {
            return self.end_poly(None);
        }
        if prev != start && near(point, start) {
            return self.end_poly(Some(start));
        }
        if shift {
            return self.end_poly(Some(point));
        }

        if let Some(chain) = &mut self.poly {
            chain.points.push(point);
        }
        self.refresh_chain_overlay(Some(point));
        Damage::None
    }

    fn end_poly(&mut self, last: Option<[f32; 2]>) -> Damage {
        let Some(mut chain) = self.poly.take() else {
            return Damage::None;
        };
        self.overlay = Overlay::None;
        if let Some(last) = last {
            chain.points.push(last);
        }
        self.commit_chain(&chain.points, chain.base)
    }

    pub(super) fn commit_chain(&mut self, points: &[[f32; 2]], base: usize) -> Damage {
        if points.len() < 2 || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_ref().expect("checked above");
        self.undo.record(EditAction::DrawLine, map);
        self.dirty = true;
        for pair in points.windows(2) {
            self.append_edge(pair[0], pair[1]);
        }
        self.finish_draw(base)
    }

    fn refresh_chain_overlay(&mut self, rubber: Option<[f32; 2]>) {
        let Some(chain) = &self.poly else {
            self.overlay = Overlay::None;
            return;
        };
        self.overlay = Overlay::Chain {
            pts: chain.points.clone(),
            rubber,
        };
    }

    fn append_edge(&mut self, a: [f32; 2], b: [f32; 2]) {
        let front = SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: self.draw_brush.wall_tex,
            sector: None,
        };
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        if let Some(map) = self.map.as_mut() {
            add_edge(map, a, b, front, NEW_LINE_FLAGS, tol);
        }
    }

    pub(super) fn commit_shape(&mut self, points: &[[f32; 2]]) -> Damage {
        if points.len() < 3 || self.map.is_none() {
            return Damage::None;
        }
        let base = self.map.as_ref().map_or(0, |m| m.lines.len());
        let map = self.map.as_ref().expect("checked above");
        self.undo.record(EditAction::DrawLine, map);
        self.dirty = true;
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            self.append_edge(a, b);
        }
        self.finish_draw(base)
    }

    /// Shape vertices: Rect = corner-to-corner; Triangle/N-gon = anchor centre, pointer radius.
    pub(super) fn shape_points(
        &self,
        shape: DrawShape,
        anchor: [f32; 2],
        pointer: [f32; 2],
    ) -> Vec<[f32; 2]> {
        let raw = match shape {
            DrawShape::Line => return Vec::new(),
            DrawShape::Rect => rect_corners(anchor, pointer).to_vec(),
            DrawShape::Triangle => ngon_points(anchor, pointer, 3),
            DrawShape::Ngon => ngon_points(anchor, pointer, self.ngon_sides.max(3)),
        };
        if self.snap {
            raw.into_iter().map(|p| self.snap_point(p)).collect()
        } else {
            raw
        }
    }

    pub(super) fn finish_draw(&mut self, base: usize) -> Damage {
        let record = self.draw_brush.sector();
        let Some(map) = self.map.as_mut() else {
            return Damage::None;
        };
        derive_sectors(map, base, record);
        self.current_sector = None;
        self.draw_plane_z = None;
        Damage::Geometry
    }

    pub fn hover_poly(&mut self, pos: [f32; 2]) -> Damage {
        let world = self.screen_to_world(pos);
        self.cursor_world = world;
        if self.poly.is_none() {
            return Damage::None;
        }
        let b = self.snap_point(world);
        let next = self
            .poly
            .as_ref()
            .map(|c| Overlay::Chain {
                pts: c.points.clone(),
                rubber: Some(b),
            })
            .expect("chain present");
        if self.overlay == next {
            return Damage::None;
        }
        self.overlay = next;
        if self.poly.as_ref().and_then(|c| c.points.last()).is_some() {
            Damage::Overlay
        } else {
            Damage::None
        }
    }

    pub(super) fn shape_click(&mut self, shape: DrawShape, world: [f32; 2]) -> Damage {
        match self.shape_draw {
            ShapeDraw::None => {
                let anchor = if self.snap {
                    self.snap_point(world)
                } else {
                    world
                };
                self.shape_draw = ShapeDraw::Anchored {
                    shape,
                    anchor,
                };
                self.overlay = Overlay::Poly {
                    pts: vec![anchor],
                };
                Damage::None
            }
            ShapeDraw::Anchored {
                shape,
                anchor,
            } => {
                let points = self.shape_points(shape, anchor, world);
                self.shape_draw = ShapeDraw::None;
                self.overlay = Overlay::None;
                self.commit_shape(&points)
            }
        }
    }

    pub fn shape_hover(&mut self, pos: [f32; 2]) -> Damage {
        let world = self.screen_to_world(pos);
        self.cursor_world = world;
        let ShapeDraw::Anchored {
            shape,
            anchor,
        } = self.shape_draw
        else {
            return Damage::None;
        };
        let mut pts = self.shape_points(shape, anchor, world);
        if pts.is_empty() {
            pts.push(anchor);
        }
        self.overlay = Overlay::Poly {
            pts,
        };
        Damage::Overlay
    }
}

pub fn default_sector() -> Sector {
    Sector {
        floor_height: 0,
        floor_flat: Name8::new("FLOOR4_8").expect("known-valid flat name"),
        ceil_height: 128,
        ceil_flat: Name8::new("CEIL3_5").expect("known-valid flat name"),
        light_level: 192,
        special: 0,
        tag: 0,
    }
}
