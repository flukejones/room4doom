mod canvas;
mod draw;
mod geom;
mod movers;
mod pick;
mod query;
mod render3d;
mod spatial;

pub mod data;

pub use canvas::Frame;
pub use data::{ViewerData, extract_viewer_data};
pub use render3d::{Camera3D, Render3DMode, Renderer3D};

use geom::{point_in_polygon, point_to_segment_dist};
use glam::{Vec2, Vec3};
use level::LevelData;
use query::DragTool;
use std::fmt::Write;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Primary,
    Middle,
}

#[derive(Clone, Copy)]
struct Drag {
    button: MouseButton,
    start: Vec2,
    last: Vec2,
    moved: bool,
    command: bool,
    shift: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Layer {
    Linedefs,
    Segments,
    Sectors,
    Aabbs,
    Divlines,
    Polygons,
    Fill,
    Vertices,
}
impl Layer {
    pub const ALL: [Self; 8] = [
        Self::Linedefs,
        Self::Segments,
        Self::Sectors,
        Self::Aabbs,
        Self::Divlines,
        Self::Polygons,
        Self::Fill,
        Self::Vertices,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Linedefs => "Linedefs",
            Self::Segments => "Segments",
            Self::Sectors => "Sector colours",
            Self::Aabbs => "Subsector AABBs",
            Self::Divlines => "Divlines (hover)",
            Self::Polygons => "Polygons",
            Self::Fill => "Polygon fill",
            Self::Vertices => "Vertices",
        }
    }
    fn flag(self, state: &mut ViewState) -> &mut bool {
        match self {
            Self::Linedefs => &mut state.show_linedefs,
            Self::Segments => &mut state.show_segments,
            Self::Sectors => &mut state.show_sectors,
            Self::Aabbs => &mut state.show_aabb,
            Self::Divlines => &mut state.show_divlines,
            Self::Polygons => &mut state.show_map_polygon_edges,
            Self::Fill => &mut state.show_polygon_fill,
            Self::Vertices => &mut state.show_vertices,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Control {
    View3D,
    RenderMode(Render3DMode),
    Layer(Layer),
    Unpin,
    Fit,
}

struct ViewState {
    offset: Vec2,
    zoom: f32,
    show_linedefs: bool,
    show_segments: bool,
    show_sectors: bool,
    show_aabb: bool,
    show_divlines: bool,
    show_map_polygon_edges: bool,
    show_polygon_fill: bool,
    show_vertices: bool,
    selected_subsector: Option<usize>,
    hovered_linedef: Option<usize>,
    hovered_subsector: Option<usize>,
    hovered_vertex: Option<usize>,
    hovered_polygon_edge: Option<(usize, usize)>,
    pinned: bool,
    drag_tool: DragTool,
    mode_3d: bool,
    render3d_mode: Render3DMode,
    cam: Camera3D,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            zoom: 1.0,
            show_linedefs: true,
            show_segments: false,
            show_sectors: false,
            show_aabb: false,
            show_divlines: false,
            show_map_polygon_edges: true,
            show_polygon_fill: false,
            show_vertices: false,
            selected_subsector: None,
            hovered_linedef: None,
            hovered_subsector: None,
            hovered_vertex: None,
            hovered_polygon_edge: None,
            pinned: false,
            drag_tool: DragTool::None,
            mode_3d: false,
            render3d_mode: Render3DMode::Textured,
            cam: Camera3D::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct HoverKey {
    line: Option<usize>,
    subsector: Option<usize>,
    vertex: Option<usize>,
    edge: Option<(usize, usize)>,
    camera: Option<Vec3>,
}

/// Map state, interactions, diagnostics, and rendering shared by every UI.
pub struct MapViewerApp {
    pub data: ViewerData,
    state: ViewState,
    map_center: Vec2,
    level: Option<std::pin::Pin<Box<LevelData>>>,
    renderer3d: Option<Renderer3D>,
    movers: movers::MoverState,
    pointer: Option<Vec2>,
    drag: Option<Drag>,
    held: [bool; 7],
    frame: Frame,
    paths: tiny_skia::PathBuilder,
    lines_index: spatial::SpatialIndex,
    vertices_index: spatial::SpatialIndex,
    subsectors_index: spatial::SpatialIndex,
    candidates: Vec<usize>,
    fitted: bool,
    dirty: bool,
    style: ui_common::theme::CanvasColors,
}

impl MapViewerApp {
    pub fn new(
        data: ViewerData,
        level: Option<std::pin::Pin<Box<LevelData>>>,
        renderer3d: Option<Renderer3D>,
        cam: Camera3D,
    ) -> Self {
        let map_center = (data.min + data.max) * 0.5;
        let lines_index = spatial::SpatialIndex::new(
            data.linedefs
                .iter()
                .map(|line| (line.v1.min(line.v2), line.v1.max(line.v2))),
        );
        let vertices_index =
            spatial::SpatialIndex::new(data.vertices.iter().map(|vertex| (vertex.pos, vertex.pos)));
        let subsectors_index =
            spatial::SpatialIndex::new(data.subsectors.iter().map(|ss| (ss.aabb_min, ss.aabb_max)));
        Self {
            lines_index,
            vertices_index,
            subsectors_index,
            candidates: Vec::new(),
            paths: tiny_skia::PathBuilder::new(),
            data,
            state: ViewState {
                cam,
                ..Default::default()
            },
            map_center,
            level,
            renderer3d,
            movers: movers::MoverState::default(),
            pointer: None,
            drag: None,
            held: [false; 7],
            frame: Frame::default(),
            fitted: false,
            dirty: true,
            style: ui_common::theme::resolve(
                ui_common::theme::DEFAULT_DARK_THEME,
                ui_common::theme::ThemeVariant::Dark,
            )
            .1,
        }
    }
    pub fn set_style(&mut self, style: ui_common::theme::CanvasColors) {
        if self.style != style {
            self.style = style;
            self.dirty = true;
        }
    }
    pub fn is_3d(&self) -> bool {
        self.state.mode_3d
    }
    pub fn render_mode(&self) -> Render3DMode {
        self.state.render3d_mode
    }
    pub fn pinned(&self) -> bool {
        self.state.pinned
    }
    pub fn interacting(&self) -> bool {
        self.drag.is_some()
    }
    pub fn layer_enabled(&self, layer: Layer) -> bool {
        match layer {
            Layer::Linedefs => self.state.show_linedefs,
            Layer::Segments => self.state.show_segments,
            Layer::Sectors => self.state.show_sectors,
            Layer::Aabbs => self.state.show_aabb,
            Layer::Divlines => self.state.show_divlines,
            Layer::Polygons => self.state.show_map_polygon_edges,
            Layer::Fill => self.state.show_polygon_fill,
            Layer::Vertices => self.state.show_vertices,
        }
    }
    pub fn control(&mut self, control: Control) {
        match control {
            Control::View3D => {
                self.state.mode_3d = !self.state.mode_3d;
                self.release_input();
            }
            Control::RenderMode(mode) => self.state.render3d_mode = mode,
            Control::Layer(layer) => {
                let flag = layer.flag(&mut self.state);
                *flag = !*flag;
            }
            Control::Unpin => self.state.pinned = false,
            Control::Fit => {
                self.state.offset = Vec2::ZERO;
                self.fit_zoom(self.viewport_size());
            }
        }
        self.update_hover(self.viewport_size() * 0.5, self.pointer);
        self.dirty = true;
    }
    pub fn key(&mut self, key: &str, down: bool) {
        let index = match key {
            "w" | "W" => 0,
            "s" | "S" => 1,
            "a" | "A" => 2,
            "d" | "D" => 3,
            "q" | "Q" => 4,
            "e" | "E" => 5,
            "shift" => 6,
            _ => return,
        };
        self.held[index] = down;
    }
    pub fn release_input(&mut self) {
        self.held.fill(false);
        self.dirty |= self.drag.is_some();
        self.drag = None;
        self.state.drag_tool = DragTool::None;
    }
    pub fn pointer_down(&mut self, button: MouseButton, pos: Vec2, command: bool, shift: bool) {
        self.pointer = Some(pos);
        self.update_hover(self.viewport_size() * 0.5, Some(pos));
        self.drag = Some(Drag {
            button,
            start: pos,
            last: pos,
            moved: false,
            command,
            shift,
        });
        self.dirty = true;
    }
    pub fn pointer_move(&mut self, pos: Option<Vec2>) -> bool {
        if pos == self.pointer && self.drag.is_none() {
            return false;
        }
        let before = (self.hover_key(), self.selection_key());
        self.pointer = pos;
        if let Some(pos) = pos
            && let Some(mut drag) = self.drag
        {
            let delta = pos - drag.last;
            drag.moved |= pos.distance(drag.start) > 3.0;
            if drag.moved && delta != Vec2::ZERO {
                self.dirty = true;
                if self.is_3d() && drag.button == MouseButton::Primary {
                    self.state.cam.yaw -= delta.x * 0.005;
                    self.state.cam.pitch =
                        (self.state.cam.pitch - delta.y * 0.005).clamp(-1.5, 1.5);
                } else if !self.is_3d() && drag.command && drag.button == MouseButton::Primary {
                    let start = self.screen_to_map(drag.start, self.viewport_size() * 0.5);
                    self.state.drag_tool = if drag.shift {
                        DragTool::RectSelect {
                            start,
                        }
                    } else {
                        DragTool::LineProbe {
                            start,
                        }
                    };
                } else if !self.is_3d() {
                    self.state.offset += delta;
                }
            }
            drag.last = pos;
            self.drag = Some(drag);
        }
        self.update_hover(self.viewport_size() * 0.5, pos);
        self.dirty |= before != (self.hover_key(), self.selection_key());
        self.dirty
    }
    /// Returns clipboard text when a click or completed probe produces a query.
    pub fn pointer_up(&mut self, button: MouseButton, pos: Vec2) -> Option<String> {
        let drag = self.drag.take()?;
        if drag.button != button {
            self.drag = Some(drag);
            return None;
        }
        let tool = std::mem::replace(&mut self.state.drag_tool, DragTool::None);
        self.dirty = true;
        let end = self.screen_to_map(pos, self.viewport_size() * 0.5);
        let query = match tool {
            DragTool::LineProbe {
                start,
            } if end.distance(start) > 1.0 => Some(self.query_line(start, end)),
            DragTool::RectSelect {
                start,
            } if (end.x - start.x).abs() > 1.0 && (end.y - start.y).abs() > 1.0 => {
                Some(self.query_rect(start.min(end), start.max(end)))
            }
            _ => None,
        };
        if let Some(text) = query {
            return Some(self.output_query(&text));
        }
        if !drag.moved && button == MouseButton::Primary {
            if self.is_3d() {
                self.pick_mover(pos);
            } else if drag.command {
                if let Some(ss) = self
                    .state
                    .hovered_subsector
                    .and_then(|id| self.data.subsectors.get(id))
                {
                    return Some(self.output_query(&self.query_sector(ss.sector_id)));
                }
            } else if let Some(id) = self.state.hovered_subsector {
                if self.state.pinned && self.state.selected_subsector == Some(id) {
                    self.state.pinned = false;
                } else {
                    self.state.selected_subsector = Some(id);
                    self.state.pinned = true;
                }
            }
        }
        None
    }
    pub fn scroll(&mut self, pos: Vec2, delta: f32) {
        if self.is_3d() {
            return;
        }
        let center = self.viewport_size() * 0.5;
        let anchor = self.screen_to_map(pos, center);
        self.state.zoom = (self.state.zoom * 1.002_f32.powf(delta)).clamp(0.01, 200.0);
        self.state.offset += pos - self.map_to_screen(anchor, center);
        self.update_hover(center, Some(pos));
        self.dirty = true;
    }
    pub fn advance(&mut self, dt: f32) {
        if !self.is_3d() {
            return;
        }
        let dt = dt.clamp(0.0, 0.1);
        let forward = self.state.cam.forward();
        let right = self.state.cam.right();
        let mut movement = Vec3::ZERO;
        for (held, direction) in
            self.held[..6]
                .iter()
                .zip([forward, -forward, -right, right, -Vec3::Z, Vec3::Z])
        {
            if *held {
                movement += direction;
            }
        }
        self.state.cam.pos +=
            movement.normalize_or_zero() * dt * if self.held[6] { 1200.0 } else { 400.0 };
        self.dirty |= movement != Vec3::ZERO && dt > 0.0;
        if self.movers.animating()
            && let Some(level) = &mut self.level
        {
            // SAFETY: the boxed level remains pinned for the viewer's lifetime.
            self.dirty |= self
                .movers
                .tick(unsafe { level.as_mut().get_unchecked_mut() }, dt);
        }
    }
    pub fn animating(&self) -> bool {
        self.is_3d() && (self.held[..6].iter().any(|held| *held) || self.movers.animating())
    }
    pub fn render_frame(&mut self, width: u32, height: u32) -> &mut Frame {
        let size = [width.clamp(1, 8192), height.clamp(1, 8192)];
        if size != self.frame.size {
            self.frame.size = size;
            self.update_hover(self.viewport_size() * 0.5, self.pointer);
            self.dirty = true;
        }
        if !self.fitted {
            self.fit_zoom(self.viewport_size());
            self.update_hover(self.viewport_size() * 0.5, self.pointer);
            self.fitted = true;
        }
        if self.dirty {
            if self.is_3d() {
                if let (Some(level), Some(renderer)) = (&self.level, &mut self.renderer3d) {
                    let pixels = renderer.render(
                        level,
                        &self.state.cam,
                        self.state.render3d_mode,
                        (size[0] as usize, size[1] as usize),
                    );
                    self.frame.pixels.clear();
                    self.frame
                        .pixels
                        .extend(pixels.iter().flat_map(|pixel| pixel.to_le_bytes()));
                }
            } else {
                let mut pixels = std::mem::take(&mut self.frame.pixels);
                pixels.resize(size[0] as usize * size[1] as usize * 4, 0);
                let mut paths = std::mem::take(&mut self.paths);
                let mut canvas = canvas::Canvas::new(
                    size[0],
                    size[1],
                    &mut pixels,
                    &mut paths,
                    canvas::Color(self.style.back),
                );
                self.draw_layers(&mut canvas, self.viewport_size() * 0.5);
                self.paths = paths;
                self.frame.pixels = pixels;
            }
            self.frame.revision = self.frame.revision.wrapping_add(1);
            self.dirty = false;
        }
        &mut self.frame
    }
    pub fn summary(&self) -> String {
        format!(
            "Sectors: {}\nLinedefs: {}\nSubsectors: {}",
            self.data.sectors.len(),
            self.data.linedefs.len(),
            self.data.subsectors.len()
        )
    }
    pub fn selection_key(&self) -> Option<usize> {
        self.state.selected_subsector
    }
    pub fn hover_key(&self) -> HoverKey {
        HoverKey {
            line: self.state.hovered_linedef,
            subsector: self.state.hovered_subsector,
            vertex: self.state.hovered_vertex,
            edge: self.state.hovered_polygon_edge,
            camera: self.is_3d().then_some(self.state.cam.pos),
        }
    }
    pub fn selection_info(&self) -> String {
        let Some(id) = self.state.selected_subsector else {
            return "Hover a subsector to select".into();
        };
        let mut info = String::new();
        self.write_subsector_info(id, "\n", &mut info);
        info
    }
    fn write_subsector_info(&self, id: usize, separator: &str, info: &mut String) {
        let ss = &self.data.subsectors[id];
        let sector = &self.data.sectors[ss.sector_id];
        let _ = write!(
            info,
            "Subsector: {}{separator}Sector: {}{separator}Floor: {}{separator}Ceil: {}{separator}Light: {}{separator}Special: {}{separator}Tag: {}",
            ss.index,
            ss.sector_id,
            sector.floor_height,
            sector.ceiling_height,
            sector.light_level,
            sector.special,
            sector.tag
        );
    }
    pub fn hover_info(&self) -> String {
        if self.is_3d() {
            let pos = self.state.cam.pos;
            return format!("Camera: ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z);
        }
        let mut info = String::new();
        if let Some(id) = self.state.hovered_subsector {
            self.write_subsector_info(id, "  ", &mut info);
            info.push('\n');
        }
        if let Some((ss, edge)) = self
            .state
            .hovered_polygon_edge
            .and_then(|(id, edge)| self.data.subsectors.get(id).map(|ss| (ss, edge)))
        {
            let a = ss.vertices[edge];
            let b = ss.vertices[(edge + 1) % ss.vertices.len()];
            let _ = writeln!(
                info,
                "Edge: ss{} e{}/{}  from ({:.1},{:.1}) to ({:.1},{:.1})  len {:.2}",
                ss.index,
                edge,
                ss.vertices.len(),
                a.x,
                a.y,
                b.x,
                b.y,
                a.distance(b)
            );
        }
        if let Some(line) = self
            .state
            .hovered_linedef
            .and_then(|id| self.data.linedefs.get(id))
        {
            let _ = writeln!(
                info,
                "LD: {}  front: {}  back: {:?}  sided: {}  special: {}  tag: {}",
                line.index,
                line.front_sector_id,
                line.back_sector_id,
                if line.is_two_sided { 2 } else { 1 },
                line.special,
                line.tag
            );
        }
        if let Some(vertex) = self
            .state
            .hovered_vertex
            .and_then(|id| self.data.vertices.get(id))
        {
            let _ = writeln!(
                info,
                "VX: {}  pos: ({:.1},{:.1})  linedefs: {:?}",
                vertex.index, vertex.pos.x, vertex.pos.y, vertex.linedef_ids
            );
        }
        info
    }
    fn pick_mover(&mut self, pos: Vec2) {
        let Some(level) = &mut self.level else { return };
        // SAFETY: moving sector surfaces preserves the pinned LevelData allocation.
        let level = unsafe { level.as_mut().get_unchecked_mut() };
        let Some(hit) = pick::pick_sector(
            &level.bsp_3d,
            &self.state.cam,
            pos.x,
            pos.y,
            self.frame.size[0] as usize,
            self.frame.size[1] as usize,
        ) else {
            return;
        };
        let line = hit.linedef_id.map(|id| &level.linedefs[id]);
        let candidates = [
            line.and_then(|line| line.backsector.as_ref().map(|sector| sector.num as usize)),
            line.map(|line| line.frontsector.num as usize),
            Some(hit.sector_id),
        ];
        for id in candidates.into_iter().flatten() {
            if self.movers.toggle(id, level) {
                break;
            }
        }
    }
    fn viewport_size(&self) -> Vec2 {
        Vec2::new(self.frame.size[0] as f32, self.frame.size[1] as f32)
    }
    fn map_to_screen(&self, pos: Vec2, center: Vec2) -> Vec2 {
        let relative = (pos - self.map_center) * self.state.zoom;
        center + Vec2::new(relative.x, -relative.y) + self.state.offset
    }
    fn screen_to_map(&self, pos: Vec2, center: Vec2) -> Vec2 {
        let relative = (pos - center - self.state.offset) / self.state.zoom;
        self.map_center + Vec2::new(relative.x, -relative.y)
    }
    fn fit_zoom(&mut self, size: Vec2) {
        let extent = self.data.max - self.data.min;
        if extent.min_element() > 0.0 {
            self.state.zoom = (size / extent).min_element() * 0.9;
        }
        self.dirty = true;
    }
    fn update_hover(&mut self, vc: Vec2, pointer_pos: Option<Vec2>) {
        if self.is_3d() {
            return;
        }
        self.state.hovered_linedef = None;
        self.state.hovered_subsector = None;
        self.state.hovered_vertex = None;
        self.state.hovered_polygon_edge = None;

        let Some(pointer) = pointer_pos else {
            return;
        };
        let mouse_map = self.screen_to_map(pointer, vc);

        let threshold = 5.0 / self.state.zoom;
        let mut best_dist = threshold;
        self.lines_index
            .query(mouse_map, threshold, &mut self.candidates);
        for &slot in &self.candidates {
            let ld = &self.data.linedefs[slot];
            let dist = point_to_segment_dist(mouse_map, ld.v1, ld.v2);
            if dist < best_dist {
                best_dist = dist;
                self.state.hovered_linedef = Some(ld.index);
            }
        }

        if self.state.show_vertices {
            let vx_threshold = 6.0 / self.state.zoom;
            let mut best_vx_dist = vx_threshold;
            self.vertices_index
                .query(mouse_map, vx_threshold, &mut self.candidates);
            for &slot in &self.candidates {
                let vx = &self.data.vertices[slot];
                let dist = (mouse_map - vx.pos).length();
                if dist < best_vx_dist {
                    best_vx_dist = dist;
                    self.state.hovered_vertex = Some(vx.index);
                }
            }
        }

        self.subsectors_index
            .query(mouse_map, 0.0, &mut self.candidates);
        for &slot in &self.candidates {
            let ss = &self.data.subsectors[slot];
            if ss.vertices.len() >= 3 && point_in_polygon(mouse_map, &ss.vertices) {
                self.state.hovered_subsector = Some(ss.index);
                break;
            }
        }

        if self.state.show_map_polygon_edges || self.state.show_polygon_fill {
            let edge_threshold = 4.0 / self.state.zoom;
            let mut best_edge_dist = edge_threshold;
            self.subsectors_index
                .query(mouse_map, edge_threshold, &mut self.candidates);
            for &slot in &self.candidates {
                let ss = &self.data.subsectors[slot];
                let n = ss.vertices.len();
                for i in 0..n {
                    let dist =
                        point_to_segment_dist(mouse_map, ss.vertices[i], ss.vertices[(i + 1) % n]);
                    if dist < best_edge_dist {
                        best_edge_dist = dist;
                        self.state.hovered_polygon_edge = Some((ss.index, i));
                    }
                }
            }
        }

        if !self.state.pinned {
            self.state.selected_subsector = self.state.hovered_subsector;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_hover_matches_exhaustive_geometry_and_idle_frames_retain_revision() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let level = test_utils::load_map(&test_utils::doom1_wad_path(), "E1M1");
        let data = extract_viewer_data("E1M1", &level, &wad);
        let mut app = MapViewerApp::new(data, None, None, Camera3D::default());
        app.control(Control::Layer(Layer::Vertices));
        app.render_frame(640, 480);
        let center = app.viewport_size() * 0.5;
        for zoom in [0.05, 1., 8.] {
            app.state.zoom = zoom;
            for y in (0_u16..480).step_by(17) {
                for x in (0_u16..640).step_by(19) {
                    let cursor = Vec2::new(f32::from(x), f32::from(y));
                    let point = app.screen_to_map(cursor, center);
                    let nearest = |items: &[(usize, f32)], radius: f32| {
                        items
                            .iter()
                            .fold((None, radius), |best, &(id, distance)| {
                                if distance < best.1 {
                                    (Some(id), distance)
                                } else {
                                    best
                                }
                            })
                            .0
                    };
                    let lines: Vec<_> = app
                        .data
                        .linedefs
                        .iter()
                        .map(|line| (line.index, point_to_segment_dist(point, line.v1, line.v2)))
                        .collect();
                    let vertices: Vec<_> = app
                        .data
                        .vertices
                        .iter()
                        .map(|vertex| (vertex.index, vertex.pos.distance(point)))
                        .collect();
                    let subsector = app
                        .data
                        .subsectors
                        .iter()
                        .find(|ss| ss.vertices.len() >= 3 && point_in_polygon(point, &ss.vertices))
                        .map(|ss| ss.index);
                    let mut edge = None;
                    let mut distance = 4. / zoom;
                    for ss in &app.data.subsectors {
                        for (i, &a) in ss.vertices.iter().enumerate() {
                            let d = point_to_segment_dist(
                                point,
                                a,
                                ss.vertices[(i + 1) % ss.vertices.len()],
                            );
                            if d < distance {
                                distance = d;
                                edge = Some((ss.index, i));
                            }
                        }
                    }
                    app.pointer_move(Some(cursor));
                    assert_eq!(app.state.hovered_linedef, nearest(&lines, 5. / zoom));
                    assert_eq!(app.state.hovered_vertex, nearest(&vertices, 6. / zoom));
                    assert_eq!(app.state.hovered_subsector, subsector);
                    assert_eq!(app.state.hovered_polygon_edge, edge);
                }
            }
        }
        app.pointer_move(None);
        app.pointer_move(Some(Vec2::splat(-1_000_000.)));
        let revision = app.render_frame(640, 480).revision;
        let pixels = std::mem::take(&mut app.frame.pixels);
        assert!(!app.pointer_move(Some(Vec2::splat(-999_999.))));
        assert_eq!(app.render_frame(640, 480).revision, revision);
        assert!(
            app.frame.pixels.is_empty(),
            "unchanged pixels already belong to presentation"
        );
        app.control(Control::Layer(Layer::Fill));
        let changed = app.render_frame(640, 480);
        assert_ne!(changed.revision, revision);
        assert_eq!(changed.pixels.len(), pixels.len());
        assert_ne!(changed.pixels, pixels);
        app.control(Control::View3D);
        let revision = app.render_frame(640, 480).revision;
        app.advance(0.016);
        assert_eq!(app.render_frame(640, 480).revision, revision);
        app.key("w", true);
        app.advance(0.016);
        assert!(app.animating());
        assert_ne!(app.render_frame(640, 480).revision, revision);
    }
}
