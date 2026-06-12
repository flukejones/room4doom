//! Map-editing core ([`LevelEditorState`]) and UI glue.

pub mod bvh;
pub mod clipboard;
pub mod draw;
pub mod edit;
pub mod move_sel;
pub mod pick3d;
pub mod preview;
pub mod remap;
pub mod sector;
pub mod select;
pub mod snap_index;
pub mod transform;
pub mod view;

pub use draw::default_sector;

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_4;
use std::mem;

use editor_core::geom::{SnapOptions, choose_snap, derive_thing_heights};
use editor_core::{
    ArenaKey as _, EditorMap, LineFlags, LineKey, Name8, Sector, SectorKey, ThingFlags, VertKey,
};

use crate::boundary::{DrawShape, SelectMode, Tool};
use crate::defaults::{DEFAULT_THING_KIND, DEFAULT_THING_OPTIONS, DEFAULT_THINGS};
use crate::level_editor::bvh::{MeshBvh, ThingLeaf};
use crate::level_editor::snap_index::SnapIndex;
use crate::level_editor::view::{PICK_RADIUS, vert_pair};
use crate::render::editor_camera::EditorCamera;
use crate::render::frame3d::Vert3D;
use crate::render::style::CanvasStyle;
use crate::render::view::{DEFAULT_GRID, WorldRect, snap};
use crate::state::{
    Damage, DragState, MapClipboard, Overlay, PolyChain, SectorFill, Selection, ShapeDraw,
    SkillFilter,
};
use crate::undo::UndoStack;

/// Screen pixels per wheel notch.
pub(super) const WHEEL_NOTCH_PX: f32 = 40.0;
pub(super) const NEW_LINE_FLAGS: LineFlags = LineFlags::BLOCKING;
/// Intersection-split tolerance (screen px); caller divides by zoom.
pub(super) const ON_SEGMENT_TOL_PX: f32 = 2.0;
/// Snap-to-vertex radius (screen px).
pub(super) const SNAP_VERTEX_PX: f32 = 8.0;
/// Snap-to-line radius (screen px).
pub(super) const SNAP_LINE_PX: f32 = 6.0;
/// Weld radius (world units).
pub(super) const VERTEX_WELD_DIST: f32 = 8.0;
/// World-unit tolerance shared by the heal pass and the audit scans.
pub(crate) const HEAL_TOL: f32 = 1.0;
/// Angle-snap quantisation for line drawing (45°).
pub(super) const ANGLE_SNAP_STEP: f32 = FRAC_PI_4;
pub(super) const GRID_STEPS: &[i32] = &[1, 2, 4, 8, 16, 32, 64];
pub(super) const DEFAULT_NGON_SIDES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThingTemplate {
    pub kind: i32,
    pub angle: i32,
    pub options: ThingFlags,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawBrush {
    pub floor_h: i32,
    pub ceil_h: i32,
    pub floor_flat: Name8,
    pub ceil_flat: Name8,
    pub wall_tex: Name8,
}

impl Default for DrawBrush {
    fn default() -> Self {
        let s = default_sector();
        Self {
            floor_h: s.floor_height,
            ceil_h: s.ceil_height,
            floor_flat: s.floor_flat,
            ceil_flat: s.ceil_flat,
            wall_tex: Name8::EMPTY,
        }
    }
}

impl DrawBrush {
    pub fn sector(&self) -> Sector {
        let base = default_sector();
        Sector {
            floor_height: self.floor_h,
            ceil_height: self.ceil_h,
            floor_flat: self.floor_flat,
            ceil_flat: self.ceil_flat,
            ..base
        }
    }
}

pub struct LevelEditorState {
    pub map: Option<EditorMap>,
    pub map_name: String,
    pub selection: Selection,
    pub tool: Tool,
    /// 2D framing + orbit camera, owned together so they cannot desync.
    pub camera: EditorCamera,
    pub grid: i32,
    pub overlays_visible: bool,
    pub snap: bool,
    pub snap_to_vertex: bool,
    pub snap_to_line: bool,
    /// Quantise drawn segments (and handle rotation) to angle steps.
    pub angle_snap: bool,
    /// Highlight void-facing sides in the warning colour.
    pub highlight_unenclosed: bool,
    pub drag: DragState,
    pub overlay: Overlay,
    pub poly: Option<PolyChain>,
    pub shape_draw: ShapeDraw,
    pub ngon_sides: u32,
    pub undo: UndoStack,
    pub thing_colors: HashMap<i32, [u8; 4]>,
    /// `[half_w, half_h]` per kind; absent kinds fall back to body-radius.
    pub thing_extents: HashMap<i32, [f32; 2]>,
    pub style: CanvasStyle,
    pub cursor_world: [f32; 2],
    /// Panel-open sector; `None` hides the panel.
    pub current_sector: Option<SectorKey>,
    pub draw_brush: DrawBrush,
    pub thing_template: ThingTemplate,
    pub sector_fill: SectorFill,
    pub skill_filter: SkillFilter,
    pub dirty: bool,
    /// Set when Sector tool samples; UI reads then clears.
    pub sampled_sector: Option<SectorKey>,
    pub clipboard: MapClipboard,
    /// Retained 3D mesh for ray-vs-mesh picking; refreshed on geometry rebuild.
    pub surface_mesh: Vec<Vert3D>,
    /// BVH over `surface_mesh` + thing billboards.
    pub bvh: MeshBvh,
    /// `{v1,v2}` → linedef key; used to classify mesh edges during pick.
    pub edge_lines: HashMap<(VertKey, VertKey), LineKey>,
    /// Grid-plane Z armed by the first 3D line-draw click; cleared on finish/tool change.
    pub draw_plane_z: Option<f32>,
    /// Snap-query buckets; the reconciler drops it on geometry change, `snap_point` rebuilds lazily.
    pub snap_index: Option<SnapIndex>,
}

impl Default for LevelEditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelEditorState {
    pub fn new() -> Self {
        let thing_colors = DEFAULT_THINGS.iter().map(|t| (t.kind, t.color)).collect();
        Self {
            map: None,
            map_name: String::new(),
            selection: Selection::default(),
            tool: Tool::Select(SelectMode::All),
            camera: EditorCamera::default(),
            grid: DEFAULT_GRID,
            overlays_visible: true,
            snap: true,
            snap_to_vertex: false,
            snap_to_line: false,
            angle_snap: false,
            highlight_unenclosed: false,
            drag: DragState::default(),
            overlay: Overlay::default(),
            poly: None,
            shape_draw: ShapeDraw::None,
            ngon_sides: DEFAULT_NGON_SIDES,
            undo: UndoStack::new(),
            thing_colors,
            thing_extents: HashMap::new(),
            style: CanvasStyle::default(),
            cursor_world: [0.0, 0.0],
            current_sector: None,
            draw_brush: DrawBrush::default(),
            thing_template: ThingTemplate {
                kind: DEFAULT_THING_KIND,
                angle: 90,
                options: DEFAULT_THING_OPTIONS,
            },
            sector_fill: SectorFill::default(),
            skill_filter: SkillFilter::default(),
            dirty: false,
            sampled_sector: None,
            clipboard: MapClipboard::default(),
            surface_mesh: Vec::new(),
            bvh: MeshBvh::default(),
            edge_lines: HashMap::new(),
            draw_plane_z: None,
            snap_index: None,
        }
    }

    /// Install a map. Caller must also call `SharedState::reset_map` to clear caches.
    pub fn load_map(&mut self, mut map: EditorMap, name: &str) -> Damage {
        self.camera.reset();
        if let Some(bounds) = map_bounds(&map) {
            self.camera.center_on(bounds);
            self.camera.settle();
        }
        // RON maps without z (or older saves) need floor Z derived.
        derive_thing_heights(&mut map);
        self.map = Some(map);
        self.map_name = name.to_owned();
        self.tool = Tool::Select(SelectMode::All);
        self.clear_gesture_state();
        self.selection.clear();
        self.current_sector = None;
        self.sampled_sector = None;
        // Fresh maps re-issue generation-0 keys, so the old pick mesh could alias them.
        self.surface_mesh = Vec::new();
        self.bvh = MeshBvh::default();
        self.edge_lines.clear();
        self.snap_index = None;
        self.dirty = false;
        Damage::Edited
    }

    /// Rebuild pick mesh without the renderer (headless/tests).
    #[cfg(test)]
    pub fn rebuild_pick_mesh(&mut self) {
        use crate::render::{frame3d, triangulate};
        self.surface_mesh = match &self.map {
            Some(map) => {
                let tris = triangulate::build_sector_tris(map);
                frame3d::build_surface(map, &tris, &HashMap::new()).0
            }
            None => Vec::new(),
        };
        self.rebuild_bvh();
    }

    /// Rebuild BVH and edge→linedef map from `surface_mesh` + things; also drops the snap buckets (headless callers mutate the map without the reconciler).
    pub fn rebuild_bvh(&mut self) {
        self.snap_index = None;
        let things = self
            .map
            .as_ref()
            .map_or_else(Vec::new, |map| thing_leaves(map, &self.thing_extents));
        self.bvh = MeshBvh::build(&self.surface_mesh, &things);
        self.edge_lines.clear();
        if let Some(map) = self.map.as_ref() {
            self.edge_lines
                .extend(map.lines.iter().map(|(k, l)| (vert_pair(l.v1, l.v2), k)));
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.clear_gesture_state();
    }

    /// Drop any in-flight drag/draw gesture; the state indexes the current map and tool.
    fn clear_gesture_state(&mut self) {
        self.drag = DragState::None;
        self.overlay = Overlay::None;
        self.poly = None;
        self.shape_draw = ShapeDraw::None;
        self.draw_plane_z = None;
    }

    /// Clamped to ≥3.
    pub fn set_ngon_sides(&mut self, sides: u32) {
        self.ngon_sides = sides.max(3);
    }

    /// Returns true if changed.
    fn set_grid_plane(&mut self, z: f32) -> bool {
        if (self.camera.grid_z() - z).abs() < f32::EPSILON {
            return false;
        }
        self.camera.set_grid_z(z);
        true
    }

    fn pick_mode(&self) -> Option<SelectMode> {
        match self.tool {
            Tool::Select(mode) => Some(mode),
            Tool::Sector => Some(SelectMode::Sector),
            _ => None,
        }
    }

    fn select_resolve(&mut self, pos: [f32; 2], mode: SelectMode, shift: bool) -> Damage {
        let Some(hit) = self.pick_3d_select(pos, mode) else {
            return if shift {
                Damage::None
            } else {
                self.clear_selection()
            };
        };
        self.cursor_world = [hit.world[0], hit.world[1]];
        self.camera.set_pivot(hit.world);
        let moved = self.set_grid_plane(hit.grid_z);
        let damage = if !hit.kind.matches_mode(mode) {
            if shift {
                Damage::None
            } else {
                self.clear_selection()
            }
        } else if matches!(self.tool, Tool::Sector) {
            self.sample_sector([hit.world[0], hit.world[1]])
        } else {
            self.select_pick(hit.kind, shift)
        };
        if moved {
            Damage::View.combine(damage)
        } else {
            damage
        }
    }

    pub fn editing_active(&self) -> bool {
        self.poly.is_some()
            || matches!(self.shape_draw, ShapeDraw::Anchored { .. })
            || matches!(
                self.drag,
                DragState::MoveSel { .. } | DragState::Transform { .. }
            )
    }

    pub fn tool_click(&mut self, pos: [f32; 2], shift: bool) -> Damage {
        if let Some(mode) = self.pick_mode() {
            return self.select_resolve(pos, mode, shift);
        }
        // First 3D line-draw click on a surface arms the editing plane to that Z.
        let mut plane_moved = false;
        if matches!(self.tool, Tool::Draw(DrawShape::Line))
            && self.poly.is_none()
            && self.draw_plane_z.is_none()
            && let Some(hit) = self.pick_3d(pos)
        {
            plane_moved = self.set_grid_plane(hit.grid_z);
            self.draw_plane_z = Some(hit.grid_z);
        }
        let world = self.screen_to_world(pos);
        self.cursor_world = world;
        let damage = match self.tool {
            Tool::Draw(DrawShape::Line) => self.poly_click(world, shift),
            Tool::Draw(shape) => self.shape_click(shape, world),
            Tool::Thing => self.place_thing(world),
            _ => Damage::None,
        };
        if plane_moved {
            Damage::View.combine(damage)
        } else {
            damage
        }
    }

    pub fn begin_tool_drag(&mut self, pos: [f32; 2], shift: bool) -> Damage {
        let world = self.screen_to_world(pos);
        self.cursor_world = world;
        match self.tool {
            Tool::Select(mode) => {
                // Transform handles win over element picking.
                if self.begin_transform_drag(pos) {
                    return Damage::None;
                }
                self.begin_select_drag(pos, mode, world, shift)
            }
            _ => Damage::None,
        }
    }

    pub fn drag_to(&mut self, pos: [f32; 2]) -> Damage {
        let world = self.screen_to_world(pos);
        self.cursor_world = world;
        match &self.drag {
            DragState::Rubber {
                start,
                ..
            } => {
                let a = *start;
                self.overlay = Overlay::Rubber {
                    a,
                    b: world,
                };
                Damage::Overlay
            }
            DragState::MoveSel {
                ..
            } => self.move_selection_to(world),
            DragState::Transform {
                ..
            } => self.transform_drag_to(world),
            DragState::None => Damage::None,
        }
    }

    pub fn end_drag(&mut self, pos: [f32; 2]) -> Damage {
        self.cursor_world = self.screen_to_world(pos);
        if matches!(self.drag, DragState::Transform { .. }) {
            return self.finish_transform(self.cursor_world);
        }
        if matches!(self.drag, DragState::MoveSel { .. }) {
            // A moved single line holds for the Move/Extrude choice; the UI commits or cancels.
            if self.line_drag_pending().is_some() {
                return Damage::Overlay;
            }
            let damage = self.finish_move();
            self.drag = DragState::None;
            return damage;
        }
        let drag = mem::take(&mut self.drag);
        match drag {
            DragState::Rubber {
                start,
                mode,
            } => {
                self.overlay = Overlay::None;
                match self.rubber_select(mode, start, self.cursor_world) {
                    Damage::None => Damage::Overlay,
                    damage => damage,
                }
            }
            _ => Damage::None,
        }
    }

    pub fn drawing_active(&self) -> bool {
        self.poly.is_some() || matches!(self.shape_draw, ShapeDraw::Anchored { .. })
    }

    /// Commit placed points (without the unplaced rubber segment) into geometry; Enter key + tool-switch path.
    pub fn cancel_gesture(&mut self) -> Damage {
        self.drag = DragState::None;
        self.overlay = Overlay::None;
        self.shape_draw = ShapeDraw::None;
        if let Some(chain) = self.poly.take() {
            return self.commit_chain(&chain.points, &chain.base);
        }
        self.draw_plane_z = None;
        Damage::None
    }

    /// Drop in-progress draw without committing geometry. Escape key path.
    pub fn discard_gesture(&mut self) -> Damage {
        let was_drawing = self.drawing_active() || !matches!(self.overlay, Overlay::None);
        self.clear_gesture_state();
        if was_drawing {
            Damage::Overlay
        } else {
            Damage::None
        }
    }

    fn snap_point(&mut self, world: [f32; 2]) -> [f32; 2] {
        let grid_snap = |w: [f32; 2]| [snap(w[0], self.grid), snap(w[1], self.grid)];
        // Angle rays anchor on the previous chain point (line draw only).
        let angle_from = if self.angle_snap {
            self.poly.as_ref().and_then(|c| c.points.last()).copied()
        } else {
            None
        };
        let Some(map) = &self.map else {
            return if self.snap { grid_snap(world) } else { world };
        };
        if !self.snap_to_vertex && !self.snap_to_line && angle_from.is_none() {
            return if self.snap { grid_snap(world) } else { world };
        }

        let vtol = SNAP_VERTEX_PX / self.camera.zoom_level();
        let ltol = SNAP_LINE_PX / self.camera.zoom_level();
        let radius = vtol.max(ltol);
        let index = self.snap_index.get_or_insert_with(|| SnapIndex::build(map));
        let verts = if self.snap_to_vertex {
            index.verts_near(world, radius)
        } else {
            Vec::new()
        };
        let lines = if self.snap_to_line {
            index.lines_near(world, ltol)
        } else {
            Vec::new()
        };
        choose_snap(
            world,
            &SnapOptions {
                grid: self.grid as f32,
                grid_on: self.snap,
                vertex_tol: vtol,
                line_tol: ltol,
                nearby_vertices: &verts,
                nearby_lines: &lines,
                angle_from,
                angle_step_rad: ANGLE_SNAP_STEP,
            },
        )
    }
}

/// Thing pick-leaves for the BVH: id = arena slot, half-extent from the kind cache.
pub(crate) fn thing_leaves(map: &EditorMap, extents: &HashMap<i32, [f32; 2]>) -> Vec<ThingLeaf> {
    map.things
        .iter()
        .map(|(k, t)| ThingLeaf {
            id: k.slot(),
            centre: [t.x as f32, t.y as f32],
            z: t.z as f32,
            half: extents
                .get(&t.kind)
                .copied()
                .unwrap_or([PICK_RADIUS, PICK_RADIUS]),
        })
        .collect()
}

pub fn map_bounds(map: &EditorMap) -> Option<WorldRect> {
    let first = map.vertices.values().next()?;
    let mut rect = WorldRect::point(first.x, first.y);
    for v in map.vertices.values() {
        rect = rect.union(WorldRect::point(v.x, v.y));
    }
    Some(rect)
}

#[cfg(test)]
mod tests;
