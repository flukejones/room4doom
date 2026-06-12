//! Canvas gesture entry points: pan/zoom/orbit and canvas-setting toggles.

use std::cell::RefCell;

use super::bvh::Leaf;
use super::pick3d::{
    PickHit, PickKind, pick_mesh, point_ray_dist_sq, ray_hits_quad, ray_hits_tri, seg_ray_dist_sq,
};
use editor_core::{LineKey, ThingKey, VertKey};

use super::{GRID_STEPS, LevelEditorState, WHEEL_NOTCH_PX, map_bounds};
use crate::boundary::{SelectMode, SkillFilter};
use crate::render::camera3d::{Camera, Projection};
use crate::render::frame3d::{NO_VERT, SURFACE_CEIL, SURFACE_FLOOR};
use crate::render::view::WHEEL_ZOOM_FACTOR;
use crate::state::{Damage, SectorFill, SelItem};

/// Cylinder radius (world units) for edge picking.
pub(crate) const PICK_RADIUS: f32 = 3.0;
/// Wider than edge radius so vertex > edge snap priority holds.
pub(crate) const VERTEX_PICK_RADIUS: f32 = 8.0;

thread_local! {
    /// Reused per-hover; avoids an alloc on every hover.
    static PICK_LEAVES: RefCell<Vec<Leaf>> = const { RefCell::new(Vec::new()) };
}

/// Order-independent vertex-pair key for the edge→linedef lookup.
pub(crate) fn vert_pair(a: VertKey, b: VertKey) -> (VertKey, VertKey) {
    (a.min(b), a.max(b))
}

impl LevelEditorState {
    pub fn set_viewport(&mut self, w: f32, h: f32) -> Damage {
        self.camera.set_viewport(w, h);
        Damage::View
    }

    pub fn set_skill_filter(&mut self, filter: SkillFilter) -> Damage {
        if self.skill_filter == filter {
            return Damage::None;
        }
        self.skill_filter = filter;
        Damage::Repaint
    }

    pub fn pan(&mut self, dx: f32, dy: f32) -> Damage {
        self.camera.pan(dx, dy).into()
    }

    pub fn screen_to_world(&self, pos: [f32; 2]) -> [f32; 2] {
        self.camera.screen_to_world(pos)
    }

    /// Nearest mesh hit for draw-plane arming. Selection uses `pick_3d_select`.
    pub fn pick_3d(&self, pos: [f32; 2]) -> Option<PickHit> {
        let map = self.map.as_ref()?;
        let (origin, dir) = self.pick_ray(pos)?;
        let hit = pick_mesh(&self.surface_mesh, origin, dir)?;
        let v = self.surface_mesh[hit.tri];
        let kind = if v.surface == SURFACE_FLOOR || v.surface == SURFACE_CEIL {
            PickKind::Sector(map.sectors.key_at_slot(v.sector)?)
        } else {
            PickKind::Linedef(map.lines.key_at_slot(v.source)?)
        };
        Some(PickHit {
            kind,
            world: hit.world,
            grid_z: hit.world[2],
        })
    }

    /// Pick vertex > edge > thing > face; solid surfaces occlude. Grid-plane Z = hit Z.
    pub fn pick_3d_select(&self, pos: [f32; 2], mode: SelectMode) -> Option<PickHit> {
        let map = self.map.as_ref()?;
        let (origin, raw_dir) = self.pick_ray(pos)?;
        // Normalise so `t` is world-unit depth; radii compare directly.
        let len = (raw_dir[0] * raw_dir[0] + raw_dir[1] * raw_dir[1] + raw_dir[2] * raw_dir[2])
            .sqrt()
            .max(1e-6);
        let dir = [raw_dir[0] / len, raw_dir[1] / len, raw_dir[2] / len];

        let edge_lines = &self.edge_lines;
        let tris = &self.surface_mesh;
        let r2 = PICK_RADIUS * PICK_RADIUS;
        let vr2 = VERTEX_PICK_RADIUS * VERTEX_PICK_RADIUS;
        let mut vertex: Option<(f32, f32, VertKey, f32)> = None; // (dist², t, vert, gz)
        let mut edge: Option<(f32, f32, LineKey, f32)> = None; // (dist², t, line, gz)
        let mut face: Option<(f32, usize)> = None; // (t, first-vert index)
        let mut thing: Option<(f32, ThingKey, f32)> = None; // (t, thing, gz)

        // Gather at the widest tier so no narrower tier is accidentally pruned.
        PICK_LEAVES.with_borrow_mut(|leaves| {
            self.bvh.gather(origin, dir, VERTEX_PICK_RADIUS, leaves);
            for leaf in leaves.iter() {
                match *leaf {
                    Leaf::Tri {
                        tri: i,
                    } => {
                        let tri = [tris[i], tris[i + 1], tris[i + 2]];
                        if let Some(t) =
                            ray_hits_tri(origin, dir, tri[0].pos, tri[1].pos, tri[2].pos)
                            && face.is_none_or(|(bt, _)| t < bt)
                        {
                            face = Some((t, i));
                        }
                        if matches!(mode, SelectMode::Vertex | SelectMode::All) {
                            for c in &tri {
                                if c.vert == NO_VERT {
                                    continue;
                                }
                                let Some(vk) = map.vertices.key_at_slot(c.vert) else {
                                    continue;
                                };
                                let (d, t) = point_ray_dist_sq(c.pos, origin, dir);
                                if d <= vr2 && vertex.is_none_or(|b| d < b.0) {
                                    vertex = Some((d, t, vk, c.pos[2]));
                                }
                            }
                        }
                        // z-equal endpoints sharing a linedef = map edge (rejects vertical sides and ear-clip diagonals).
                        if matches!(mode, SelectMode::Line | SelectMode::All) {
                            for (m, n) in [(0, 1), (1, 2), (2, 0)] {
                                let (a, b) = (tri[m], tri[n]);
                                if a.vert == NO_VERT || b.vert == NO_VERT || a.pos[2] != b.pos[2] {
                                    continue;
                                }
                                let (Some(ka), Some(kb)) = (
                                    map.vertices.key_at_slot(a.vert),
                                    map.vertices.key_at_slot(b.vert),
                                ) else {
                                    continue;
                                };
                                let Some(&lk) = edge_lines.get(&vert_pair(ka, kb)) else {
                                    continue;
                                };
                                let (d, t) = seg_ray_dist_sq(a.pos, b.pos, origin, dir);
                                if d <= r2 && edge.is_none_or(|e| d < e.0) {
                                    edge = Some((d, t, lk, a.pos[2]));
                                }
                            }
                        }
                    }
                    Leaf::Thing {
                        id,
                        z,
                        half,
                        ..
                    } => {
                        if !matches!(mode, SelectMode::Thing | SelectMode::All) {
                            continue;
                        }
                        let Some(tk) = map.things.key_at_slot(id) else {
                            continue;
                        };
                        let t = &map.things[tk];
                        if !self.skill_filter.allows(t.options) {
                            continue;
                        }
                        if let Some(tt) =
                            self.thing_quad_hit([t.x as f32, t.y as f32], z, half, origin, dir)
                            && thing.is_none_or(|(bt, ..)| tt < bt)
                        {
                            thing = Some((tt, tk, z));
                        }
                    }
                }
            }
        });

        // Vert/edge within PICK_RADIUS of the nearest solid surface is unoccluded; in wireframe there are no fills, so faces never occlude.
        let face_t = face.map_or(f32::INFINITY, |(t, _)| t);
        let face_occludes = self.sector_fill != SectorFill::None;
        let occluder = if face_occludes { face_t } else { f32::INFINITY }
            .min(thing.map_or(f32::INFINITY, |(t, ..)| t))
            + PICK_RADIUS;

        if let Some((_, t, vk, gz)) = vertex
            && t <= occluder
        {
            return Some(hit(PickKind::Vertex(vk), ray_point(origin, dir, t), gz));
        }
        if let Some((_, t, lk, gz)) = edge
            && t <= occluder
        {
            return Some(hit(PickKind::Linedef(lk), ray_point(origin, dir, t), gz));
        }
        let thing_occluder = if face_occludes { face_t } else { f32::INFINITY };
        if let Some((tt, tk, gz)) = thing
            && tt <= thing_occluder
        {
            return Some(hit(PickKind::Thing(tk), ray_point(origin, dir, tt), gz));
        }
        let (ft, fi) = face?;
        let v = self.surface_mesh[fi];
        if matches!(mode, SelectMode::Sector | SelectMode::All) {
            let s = if v.surface == SURFACE_FLOOR || v.surface == SURFACE_CEIL {
                map.sectors.key_at_slot(v.sector)
            } else {
                map.lines
                    .key_at_slot(v.source)
                    .and_then(|lk| map.lines[lk].front.sector)
            };
            if let Some(s) = s {
                let gz = if v.surface == SURFACE_FLOOR || v.surface == SURFACE_CEIL {
                    v.pos[2]
                } else {
                    map.sectors[s].floor_height as f32
                };
                return Some(hit(PickKind::Sector(s), ray_point(origin, dir, ft), gz));
            }
        }
        None
    }

    fn pick_ray(&self, pos: [f32; 2]) -> Option<([f32; 3], [f32; 3])> {
        let [w, h] = self.camera.viewport();
        let ndc = [2.0 * pos[0] / w - 1.0, 1.0 - 2.0 * pos[1] / h];
        let aspect = w / h.max(1.0);
        self.render_camera().ray(ndc, aspect)
    }

    /// Ray-`t` for the thing's camera-facing billboard quad; BVH box is camera-invariant, precise quad built at resolve time.
    fn thing_quad_hit(
        &self,
        centre: [f32; 2],
        z: f32,
        half: [f32; 2],
        origin: [f32; 3],
        dir: [f32; 3],
    ) -> Option<f32> {
        let cam = self.render_camera();
        let right = cam.billboard_right();
        let up = cam.billboard_up();
        let [hw, hh] = half;
        let c = [centre[0], centre[1], z + hh];
        let r = [right[0] * hw, right[1] * hw, right[2] * hw];
        let u = [up[0] * hh, up[1] * hh, up[2] * hh];
        let quad = [
            [c[0] - r[0] - u[0], c[1] - r[1] - u[1], c[2] - r[2] - u[2]],
            [c[0] + r[0] - u[0], c[1] + r[1] - u[1], c[2] + r[2] - u[2]],
            [c[0] + r[0] + u[0], c[1] + r[1] + u[1], c[2] + r[2] + u[2]],
            [c[0] - r[0] + u[0], c[1] - r[1] + u[1], c[2] - r[2] + u[2]],
        ];
        ray_hits_quad(origin, dir, quad)
    }

    pub fn zoom_in(&mut self) -> Damage {
        self.camera.zoom_at_center(WHEEL_ZOOM_FACTOR).into()
    }

    pub fn zoom_out(&mut self) -> Damage {
        self.camera.zoom_at_center(1.0 / WHEEL_ZOOM_FACTOR).into()
    }

    /// Ctrl+scroll zoom anchored at cursor.
    pub fn scroll_zoom(&mut self, dy: f32, at: [f32; 2]) -> Damage {
        let factor = WHEEL_ZOOM_FACTOR.powf(dy / WHEEL_NOTCH_PX);
        self.camera.zoom(factor, at).into()
    }

    /// Pinch zoom; `factor` is the delta since the previous step.
    pub fn handle_pinch(&mut self, factor: f32, at: [f32; 2]) -> Damage {
        if factor <= 0.0 || (factor - 1.0).abs() < f32::EPSILON {
            return Damage::None;
        }
        self.camera.zoom(factor, at).into()
    }

    pub fn cycle_grid(&mut self) -> Damage {
        let at = GRID_STEPS.iter().position(|&g| g == self.grid);
        self.grid = GRID_STEPS[at.map_or(0, |i| (i + 1) % GRID_STEPS.len())];
        Damage::Repaint
    }

    /// Toggle overlay layer (grid + lines + vertices).
    pub fn toggle_overlays(&mut self) -> Damage {
        self.overlays_visible = !self.overlays_visible;
        Damage::View
    }

    pub fn set_overlays_visible(&mut self, on: bool) -> Damage {
        if on == self.overlays_visible {
            return Damage::None;
        }
        self.overlays_visible = on;
        Damage::View
    }

    pub fn set_grid(&mut self, grid: i32) -> Damage {
        let grid = grid.max(1);
        if grid == self.grid {
            return Damage::None;
        }
        self.grid = grid;
        Damage::Repaint
    }

    pub fn set_snap(&mut self, on: bool) -> Damage {
        self.snap = on;
        Damage::None
    }

    pub fn set_snap_to_vertex(&mut self, on: bool) -> Damage {
        self.snap_to_vertex = on;
        Damage::None
    }

    pub fn set_snap_to_line(&mut self, on: bool) -> Damage {
        self.snap_to_line = on;
        Damage::None
    }

    pub fn set_angle_snap(&mut self, on: bool) -> Damage {
        self.angle_snap = on;
        Damage::None
    }

    pub fn set_highlight_unenclosed(&mut self, on: bool) -> Damage {
        self.highlight_unenclosed = on;
        Damage::Repaint
    }

    /// Ease to top-down ortho, pivoting on the screen centre.
    pub fn reset_view_top_down(&mut self) -> Damage {
        let [w, h] = self.camera.viewport();
        let centre = self.view_centre_point([w * 0.5, h * 0.5]);
        self.camera.top_down_to(centre);
        Damage::View
    }

    fn view_centre_point(&self, pos: [f32; 2]) -> [f32; 3] {
        if let Some(world) = self
            .pick_ray(pos)
            .and_then(|(o, d)| pick_mesh(&self.surface_mesh, o, d).map(|h| h.world))
        {
            return world;
        }
        if let Some(b) = self.map.as_ref().and_then(map_bounds) {
            return [b.min_x.midpoint(b.max_x), b.min_y.midpoint(b.max_y), 0.0];
        }
        let w = self.screen_to_world(pos);
        [w[0], w[1], self.camera.grid_z()]
    }

    pub fn set_projection(&mut self, projection: Projection) -> Damage {
        self.camera.set_projection(projection).into()
    }

    pub fn projection(&self) -> Projection {
        self.camera.projection()
    }

    /// Set orbit pivot: selection centre → picked part → ground plane.
    pub fn orbit_start(&mut self, pos: [f32; 2]) {
        let pivot = self
            .selection_centre()
            .or_else(|| self.pick_3d_select(pos, SelectMode::All).map(|h| h.world))
            .unwrap_or_else(|| {
                let w = self.screen_to_world(pos);
                [w[0], w[1], self.camera.grid_z()]
            });
        self.camera.set_pivot(pivot);
    }

    fn selection_centre(&self) -> Option<[f32; 3]> {
        let map = self.map.as_ref()?;
        let mut sum = [0.0f32; 2];
        let mut n = 0u32;
        let mut add = |x: f32, y: f32| {
            sum[0] += x;
            sum[1] += y;
            n += 1;
        };
        for item in self.selection.items() {
            match *item {
                SelItem::Vertex(k) => {
                    if let Some(v) = map.vertices.get(k) {
                        add(v.x, v.y);
                    }
                }
                SelItem::Line(k) => {
                    if let Some(l) = map.lines.get(k) {
                        let (a, b) = (map.vertices[l.v1], map.vertices[l.v2]);
                        add((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                    }
                }
                SelItem::Thing(k) => {
                    if let Some(t) = map.things.get(k) {
                        add(t.x as f32, t.y as f32);
                    }
                }
                SelItem::Sector(_) => {}
            }
        }
        (n > 0).then(|| [sum[0] / n as f32, sum[1] / n as f32, self.camera.grid_z()])
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) -> Damage {
        self.camera.orbit(dx, dy).into()
    }

    pub fn ease_camera(&mut self) -> bool {
        self.camera.ease_tic()
    }

    pub fn zoom_to(&mut self, scale: f32) -> Damage {
        self.camera.zoom_to(scale).into()
    }

    pub fn zoom_fit(&mut self) -> Damage {
        let Some(bounds) = self.map.as_ref().and_then(map_bounds) else {
            return Damage::None;
        };
        self.camera.fit(bounds);
        Damage::View
    }

    pub fn render_camera(&self) -> Camera {
        self.camera.render_camera()
    }
}

fn hit(kind: PickKind, world: [f32; 3], grid_z: f32) -> PickHit {
    PickHit {
        kind,
        world,
        grid_z,
    }
}

fn ray_point(origin: [f32; 3], dir: [f32; 3], t: f32) -> [f32; 3] {
    [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ]
}
