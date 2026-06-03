mod draw;
mod geom;
mod movers;
mod pick;
mod query;
mod render3d;

pub mod data;

pub use data::{ViewerData, extract_viewer_data};
use geom::*;
use query::DragTool;
pub use render3d::{Camera3D, Render3DMode, Renderer3D};

use egui::{Color32, Pos2, Vec2 as EVec2};
use glam::{Vec2, Vec3};
use level::LevelData;

pub(crate) struct ViewState {
    offset: EVec2,
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
    /// (subsector_index, edge_index_within_polygon)
    hovered_polygon_edge: Option<(usize, usize)>,
    pinned: bool,
    is_dragging: bool,
    drag_tool: DragTool,
    mode_3d: bool,
    render3d_mode: Render3DMode,
    cam: Camera3D,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            offset: EVec2::ZERO,
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
            is_dragging: false,
            drag_tool: DragTool::None,
            mode_3d: false,
            render3d_mode: Render3DMode::Textured,
            cam: Camera3D::default(),
        }
    }
}

pub(crate) struct MapViewerApp {
    pub data: ViewerData,
    pub state: ViewState,
    pub map_center: Vec2,
    /// Pinned level + 3D renderer, present only in the windowed `view` command.
    level: Option<std::pin::Pin<Box<LevelData>>>,
    renderer3d: Option<Renderer3D>,
    movers: movers::MoverState,
}

impl MapViewerApp {
    fn new(
        data: ViewerData,
        level: Option<std::pin::Pin<Box<LevelData>>>,
        renderer3d: Option<Renderer3D>,
        cam: Camera3D,
    ) -> Self {
        let map_center = (data.min + data.max) * 0.5;
        let state = ViewState {
            cam,
            ..ViewState::default()
        };
        Self {
            data,
            state,
            map_center,
            level,
            renderer3d,
            movers: movers::MoverState::default(),
        }
    }

    /// Render the 3D view into the central panel: handle free-fly input, drive
    /// the software renderer, blit its framebuffer as a texture.
    fn draw_3d(&mut self, ctx: &egui::Context, response: &egui::Response, painter: &egui::Painter) {
        let rect = response.rect;
        let (w, h) = (rect.width() as usize, rect.height() as usize);
        if w == 0 || h == 0 {
            return;
        }
        let (Some(level), Some(renderer)) = (&mut self.level, &mut self.renderer3d) else {
            return;
        };
        let level: &mut LevelData = unsafe { level.as_mut().get_unchecked_mut() };

        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            self.state.cam.yaw -= d.x * 0.005;
            self.state.cam.pitch = (self.state.cam.pitch - d.y * 0.005).clamp(-1.5, 1.5);
        }

        // Click (not drag) picks a surface; toggling a mover sector animates it.
        if response.clicked()
            && let Some(cursor) = response.interact_pointer_pos()
        {
            let px = cursor.x - rect.min.x;
            let py = cursor.y - rect.min.y;
            if let Some(hit) = pick::pick_sector(&level.bsp_3d, &self.state.cam, px, py, w, h) {
                // A door/lift's moving sector is the one *behind* the wall we
                // clicked, so prefer the wall's back/other sector, then front,
                // then the polygon's own sector.
                // The clicked sector carries the mover tag. For a floor/ceiling
                // poly that is the poly's own sector; for a wall the moving
                // sector is the one behind it (its back sector), so try that
                // first, then front, then the poly's sector.
                let mut candidates = Vec::new();
                if let Some(ld) = hit.linedef_id {
                    let line = &level.linedefs[ld];
                    if let Some(b) = line.backsector.as_ref().map(|s| s.num as usize) {
                        candidates.push(b);
                    }
                    candidates.push(line.frontsector.num as usize);
                }
                candidates.push(hit.sector_id);
                let mut tried = Vec::new();
                for sid in candidates {
                    if tried.contains(&sid) {
                        continue;
                    }
                    tried.push(sid);
                    if self.movers.toggle(sid, level) {
                        break;
                    }
                }
            }
        }

        // Advance any active mover lerps.
        if self.movers.has_groups() {
            let tick_dt = ctx.input(|i| i.stable_dt).min(0.1);
            if self.movers.tick(level, tick_dt) {
                ctx.request_repaint();
            }
        }

        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let speed = if ctx.input(|i| i.modifiers.shift) {
            1200.0
        } else {
            400.0
        } * dt;
        let fwd = self.state.cam.forward();
        let right = self.state.cam.right();
        let mut mv = Vec3::ZERO;
        ctx.input(|i| {
            if i.key_down(egui::Key::W) {
                mv += fwd;
            }
            if i.key_down(egui::Key::S) {
                mv -= fwd;
            }
            if i.key_down(egui::Key::D) {
                mv += right;
            }
            if i.key_down(egui::Key::A) {
                mv -= right;
            }
            if i.key_down(egui::Key::E) {
                mv.z += 1.0;
            }
            if i.key_down(egui::Key::Q) {
                mv.z -= 1.0;
            }
        });
        if mv.length_squared() > 0.0 {
            self.state.cam.pos += mv.normalize() * speed;
        }

        let tex = renderer.render(
            ctx,
            level,
            &self.state.cam,
            self.state.render3d_mode,
            (w, h),
        );
        painter.image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        ctx.request_repaint();
    }

    #[inline]
    pub fn map_to_screen(&self, map_pos: Vec2, vc: Pos2) -> Pos2 {
        Pos2::new(
            vc.x + (map_pos.x - self.map_center.x) * self.state.zoom + self.state.offset.x,
            vc.y - (map_pos.y - self.map_center.y) * self.state.zoom + self.state.offset.y,
        )
    }

    #[inline]
    pub fn screen_to_map(&self, sp: Pos2, vc: Pos2) -> Vec2 {
        Vec2::new(
            (sp.x - vc.x - self.state.offset.x) / self.state.zoom + self.map_center.x,
            -(sp.y - vc.y - self.state.offset.y) / self.state.zoom + self.map_center.y,
        )
    }

    fn fit_zoom(&mut self, viewport_size: EVec2) {
        let map_w = self.data.max.x - self.data.min.x;
        let map_h = self.data.max.y - self.data.min.y;
        if map_w > 0.0 && map_h > 0.0 {
            self.state.zoom = (viewport_size.x / map_w).min(viewport_size.y / map_h) * 0.9;
        }
    }

    fn handle_input(&mut self, response: &egui::Response) {
        let modifiers = response.ctx.input(|i| i.modifiers);
        let ctrl = modifiers.command;
        let shift = modifiers.shift;

        let is_primary_drag = response.dragged_by(egui::PointerButton::Primary);
        let primary_stopped = response.drag_stopped_by(egui::PointerButton::Primary);

        if is_primary_drag
            && ctrl
            && self.state.drag_tool == DragTool::None
            && let Some(pos) = response.interact_pointer_pos()
        {
            let vc = response.rect.center();
            let map_pos = self.screen_to_map(pos, vc);
            if shift {
                self.state.drag_tool = DragTool::RectSelect {
                    start: map_pos,
                };
            } else {
                self.state.drag_tool = DragTool::LineProbe {
                    start: map_pos,
                };
            }
        }

        if primary_stopped && self.state.drag_tool != DragTool::None {
            if let Some(pos) = response.interact_pointer_pos() {
                let vc = response.rect.center();
                let end = self.screen_to_map(pos, vc);
                match self.state.drag_tool {
                    DragTool::LineProbe {
                        start,
                    } => {
                        if (end - start).length() > 1.0 {
                            let text = self.query_line(start, end);
                            self.output_query(&response.ctx, &text);
                        }
                    }
                    DragTool::RectSelect {
                        start,
                    } => {
                        let lo = Vec2::new(start.x.min(end.x), start.y.min(end.y));
                        let hi = Vec2::new(start.x.max(end.x), start.y.max(end.y));
                        if (hi.x - lo.x) > 1.0 && (hi.y - lo.y) > 1.0 {
                            let text = self.query_rect(lo, hi);
                            self.output_query(&response.ctx, &text);
                        }
                    }
                    DragTool::None => {}
                }
            }
            self.state.drag_tool = DragTool::None;
        }

        if self.state.drag_tool == DragTool::None
            && (is_primary_drag || response.dragged_by(egui::PointerButton::Middle))
        {
            self.state.offset += response.drag_delta();
            self.state.is_dragging = true;
        }
        if primary_stopped || response.drag_stopped_by(egui::PointerButton::Middle) {
            self.state.is_dragging = false;
        }

        let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0
            && let Some(pointer_pos) = response.hover_pos()
        {
            let vc = response.rect.center();
            let mouse_map = self.screen_to_map(pointer_pos, vc);
            self.state.zoom *= 1.002_f32.powf(scroll);
            self.state.zoom = self.state.zoom.clamp(0.01, 200.0);
            let new_screen = self.map_to_screen(mouse_map, vc);
            self.state.offset.x += pointer_pos.x - new_screen.x;
            self.state.offset.y += pointer_pos.y - new_screen.y;
        }

        if response.clicked() && !self.state.is_dragging {
            if ctrl {
                if let Some(ss_id) = self.state.hovered_subsector
                    && let Some(ss) = self.data.subsectors.iter().find(|s| s.index == ss_id)
                {
                    let text = self.query_sector(ss.sector_id);
                    self.output_query(&response.ctx, &text);
                }
            } else if let Some(hovered) = self.state.hovered_subsector {
                if self.state.pinned && self.state.selected_subsector == Some(hovered) {
                    self.state.pinned = false;
                } else {
                    self.state.selected_subsector = Some(hovered);
                    self.state.pinned = true;
                }
            }
        }
    }

    fn update_hover(&mut self, vc: Pos2, pointer_pos: Option<Pos2>) {
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
        for ld in &self.data.linedefs {
            let dist = point_to_segment_dist(mouse_map, ld.v1, ld.v2);
            if dist < best_dist {
                best_dist = dist;
                self.state.hovered_linedef = Some(ld.index);
            }
        }

        if self.state.show_vertices {
            let vx_threshold = 6.0 / self.state.zoom;
            let mut best_vx_dist = vx_threshold;
            for vx in &self.data.vertices {
                let dist = (mouse_map - vx.pos).length();
                if dist < best_vx_dist {
                    best_vx_dist = dist;
                    self.state.hovered_vertex = Some(vx.index);
                }
            }
        }

        for ss in &self.data.subsectors {
            if ss.vertices.len() >= 3 && point_in_polygon(mouse_map, &ss.vertices) {
                self.state.hovered_subsector = Some(ss.index);
                break;
            }
        }

        if self.state.show_map_polygon_edges || self.state.show_polygon_fill {
            let edge_threshold = 4.0 / self.state.zoom;
            let mut best_edge_dist = edge_threshold;
            for ss in &self.data.subsectors {
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

impl eframe::App for MapViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.state.zoom == 1.0 && self.state.offset == EVec2::ZERO {
            let available = ctx.available_rect();
            let viewport_size = EVec2::new(available.width() - 210.0, available.height());
            self.fit_zoom(viewport_size);
        }

        egui::SidePanel::left("sidebar")
            .default_width(200.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_sidebar(ui);
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_gray(20)))
            .show(ctx, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                if self.state.mode_3d {
                    self.draw_3d(ctx, &response, &painter);
                } else {
                    let vc = response.rect.center();
                    self.update_hover(vc, response.hover_pos());
                    self.handle_input(&response);
                    self.draw_layers(&painter, vc);
                    self.draw_hover_overlay(&painter, response.rect);
                }
            });
    }
}

pub fn run(
    data: ViewerData,
    level: std::pin::Pin<Box<LevelData>>,
    renderer3d: Renderer3D,
    cam: Camera3D,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title(format!("bsp-viewer - {}", data.map_name)),
        ..Default::default()
    };
    eframe::run_native(
        "bsp-viewer",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(MapViewerApp::new(
                data,
                Some(level),
                Some(renderer3d),
                cam,
            )))
        }),
    )
    .unwrap();
}
