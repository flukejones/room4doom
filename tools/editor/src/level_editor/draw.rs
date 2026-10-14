//! Line/polygon drawing with the Line tool: the click chain, segment commit, and sector derivation when the draw finishes.

use std::collections::HashSet;

use editor_core::{
    LineKey, Name8, Sector, SideDef, add_edge, derive_sectors, ngon_points, rect_corners,
};

use super::{LevelEditorState, NEW_LINE_FLAGS, ON_SEGMENT_TOL_PX};
use crate::boundary::DrawShape;
use crate::level_editor::state::{Damage, Overlay, PolyChain, ShapeDraw};
use crate::undo::EditAction;

/// Click-on-chain-start/end radius (screen px); caller divides by zoom.
const CHAIN_CLOSE_PX: f32 = 5.0;

impl LevelEditorState {
    pub(super) fn poly_click(&mut self, world: [f32; 2], shift: bool) -> Damage {
        let point = self.chain_target(world);
        let Some(chain) = &self.poly else {
            let base = self.line_key_snapshot();
            self.poly = Some(PolyChain {
                points: vec![point],
                base,
            });
            self.refresh_chain_overlay(Some(point));
            return Damage::None;
        };

        let (start, prev) = (chain.points[0], *chain.points.last().expect("non-empty"));
        if point == prev {
            return self.end_poly(None);
        }
        if prev != start && point == start {
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

    fn chain_target(&mut self, world: [f32; 2]) -> [f32; 2] {
        let point = self.snap_point(world);
        if let Some(chain) = &self.poly {
            let radius = CHAIN_CLOSE_PX / self.camera.zoom_level();
            for target in [*chain.points.last().expect("non-empty"), chain.points[0]] {
                if (point[0] - target[0]).abs() <= radius && (point[1] - target[1]).abs() <= radius
                {
                    return target;
                }
            }
        }
        point
    }

    fn end_poly(&mut self, last: Option<[f32; 2]>) -> Damage {
        let Some(mut chain) = self.poly.take() else {
            return Damage::None;
        };
        self.overlay = Overlay::None;
        if let Some(last) = last {
            chain.points.push(last);
        }
        self.commit_chain(&chain.points, &chain.base)
    }

    /// The set of line keys present now; lines drawn later are the difference.
    fn line_key_snapshot(&self) -> HashSet<LineKey> {
        self.map
            .as_ref()
            .map_or_else(HashSet::new, |m| m.lines.keys().collect())
    }

    pub(super) fn commit_chain(&mut self, points: &[[f32; 2]], base: &HashSet<LineKey>) -> Damage {
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
            z: self.draw_brush.floor_h,
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
        let base = self.line_key_snapshot();
        let map = self.map.as_ref().expect("checked above");
        self.undo.record(EditAction::DrawLine, map);
        self.dirty = true;
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            self.append_edge(a, b);
        }
        self.finish_draw(&base)
    }

    /// Shape vertices: Rect = corner-to-corner; Triangle/N-gon = anchor centre, pointer radius.
    pub(super) fn shape_points(
        &mut self,
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

    pub(super) fn finish_draw(&mut self, base: &HashSet<LineKey>) -> Damage {
        let record = self.draw_brush.sector();
        let Some(map) = self.map.as_mut() else {
            return Damage::None;
        };
        let drawn: Vec<LineKey> = map.lines.keys().filter(|k| !base.contains(k)).collect();
        derive_sectors(map, &drawn, record);
        self.current_sector = None;
        self.draw_plane_z = None;
        Damage::Edited
    }

    pub fn hover_poly(&mut self, pos: [f32; 2]) -> Damage {
        let world = self.screen_to_world(pos);
        self.cursor_world = world;
        if self.poly.is_none() {
            return Damage::None;
        }
        let b = self.chain_target(world);
        let next = self
            .poly
            .as_ref()
            .map(|c| Overlay::Chain {
                pts: c.points.clone(),
                rubber: Some(b),
                z: self.draw_brush.floor_h,
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
                    z: self.draw_brush.floor_h,
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
            z: self.draw_brush.floor_h,
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
