mod draw;
mod geom;
mod query;

pub mod data;

pub use data::{ViewerData, extract_viewer_data};
use geom::*;
use query::DragTool;

use egui::{Color32, Pos2, Vec2 as EVec2};
use glam::Vec2;

pub(crate) struct ViewState {
    offset: EVec2,
    zoom: f32,
    show_linedefs: bool,
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
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            offset: EVec2::ZERO,
            zoom: 1.0,
            show_linedefs: true,
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
        }
    }
}

pub(crate) struct MapViewerApp {
    pub data: ViewerData,
    pub state: ViewState,
    pub map_center: Vec2,
}

impl MapViewerApp {
    fn new(data: ViewerData) -> Self {
        let map_center = (data.min + data.max) * 0.5;
        Self {
            data,
            state: ViewState::default(),
            map_center,
        }
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

        if is_primary_drag && ctrl && self.state.drag_tool == DragTool::None {
            if let Some(pos) = response.interact_pointer_pos() {
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

        if self.state.drag_tool == DragTool::None {
            if is_primary_drag || response.dragged_by(egui::PointerButton::Middle) {
                self.state.offset += response.drag_delta();
                self.state.is_dragging = true;
            }
        }
        if primary_stopped || response.drag_stopped_by(egui::PointerButton::Middle) {
            self.state.is_dragging = false;
        }

        let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(pointer_pos) = response.hover_pos() {
                let vc = response.rect.center();
                let mouse_map = self.screen_to_map(pointer_pos, vc);
                self.state.zoom *= 1.002_f32.powf(scroll);
                self.state.zoom = self.state.zoom.clamp(0.01, 200.0);
                let new_screen = self.map_to_screen(mouse_map, vc);
                self.state.offset.x += pointer_pos.x - new_screen.x;
                self.state.offset.y += pointer_pos.y - new_screen.y;
            }
        }

        if response.clicked() && !self.state.is_dragging {
            if ctrl {
                if let Some(ss_id) = self.state.hovered_subsector {
                    if let Some(ss) = self.data.subsectors.iter().find(|s| s.index == ss_id) {
                        let text = self.query_sector(ss.sector_id);
                        self.output_query(&response.ctx, &text);
                    }
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
                let vc = response.rect.center();

                self.update_hover(vc, response.hover_pos());
                self.handle_input(&response);
                self.draw_layers(&painter, vc);
                self.draw_hover_overlay(&painter, response.rect);
            });
    }
}

pub fn run(data: ViewerData) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title(format!("bsp-viewer - {}", data.map_name)),
        ..Default::default()
    };
    eframe::run_native(
        "bsp-viewer",
        options,
        Box::new(|_cc| Ok(Box::new(MapViewerApp::new(data)))),
    )
    .unwrap();
}
