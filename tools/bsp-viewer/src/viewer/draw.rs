use egui::{Color32, Pos2, Stroke};
use glam::Vec2;

use super::MapViewerApp;
use super::data::ViewerData;
use super::geom::filled_polygon;
use super::query::DragTool;

fn flame_color(idx: u32, alpha: u8) -> Color32 {
    let mut h = idx.wrapping_mul(0x9E3779B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85EBCA6B);
    h ^= h >> 13;
    let hue = (h % 60) as f32;
    let (r, g, b) = if hue < 20.0 {
        (255, (hue / 20.0 * 180.0) as u8, 0)
    } else if hue < 40.0 {
        (255, 180 + ((hue - 20.0) / 20.0 * 75.0) as u8, 0)
    } else {
        (255, 255, ((hue - 40.0) / 20.0 * 120.0) as u8)
    };
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

impl MapViewerApp {
    pub fn draw_layers(&mut self, painter: &egui::Painter, vc: Pos2) {
        if self.state.show_sectors {
            for ss in &self.data.subsectors {
                if ss.vertices.len() < 3 {
                    continue;
                }
                let base = self.data.floor_texture_color[ss.sector_id];
                let v = self.data.floor_height_value[ss.sector_id];
                let peak = base.r().max(base.g()).max(base.b()).max(1) as f32;
                let scale = v * 255.0 / peak;
                let color = Color32::from_rgb(
                    (base.r() as f32 * scale).min(255.0) as u8,
                    (base.g() as f32 * scale).min(255.0) as u8,
                    (base.b() as f32 * scale).min(255.0) as u8,
                );
                let points: Vec<Pos2> = ss
                    .vertices
                    .iter()
                    .map(|&v| self.map_to_screen(v, vc))
                    .collect();
                painter.add(filled_polygon(&points, color));
            }
        }

        if self.state.show_polygon_fill {
            let mut idx = 0u32;
            for ss in &self.data.subsectors {
                if ss.vertices.len() < 3 {
                    idx += 1;
                    continue;
                }
                let color = flame_color(idx, 160);
                let points: Vec<Pos2> = ss
                    .vertices
                    .iter()
                    .map(|&v| self.map_to_screen(v, vc))
                    .collect();
                painter.add(filled_polygon(&points, color));
                idx += 1;
            }
        }

        if self.state.show_map_polygon_edges {
            let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 60, 60, 200));
            let draw_verts = self.state.show_vertices;
            let vert_color = Color32::from_rgba_unmultiplied(220, 60, 60, 255);
            for ss in &self.data.subsectors {
                let n = ss.vertices.len();
                for i in 0..n {
                    let p1 = self.map_to_screen(ss.vertices[i], vc);
                    let p2 = self.map_to_screen(ss.vertices[(i + 1) % n], vc);
                    painter.line_segment([p1, p2], stroke);
                    if draw_verts {
                        painter.circle_filled(p1, 2.5, vert_color);
                    }
                }
            }
        }

        // Highlight hovered polygon edge
        if let Some((ss_idx, edge_idx)) = self.state.hovered_polygon_edge {
            if let Some(ss) = self.data.subsectors.iter().find(|s| s.index == ss_idx) {
                let n = ss.vertices.len();
                if edge_idx < n {
                    let p1 = self.map_to_screen(ss.vertices[edge_idx], vc);
                    let p2 = self.map_to_screen(ss.vertices[(edge_idx + 1) % n], vc);
                    painter.line_segment([p1, p2], Stroke::new(3.0, Color32::YELLOW));
                    painter.circle_filled(p1, 4.0, Color32::from_rgb(255, 220, 50));
                    painter.circle_filled(p2, 4.0, Color32::from_rgb(255, 220, 50));
                }
            }
        }

        if self.state.show_aabb {
            let sel = self.state.selected_subsector;
            let default_stroke =
                Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 100, 255, 60));
            let selected_stroke =
                Stroke::new(2.5, Color32::from_rgba_unmultiplied(255, 255, 0, 255));

            for ss in &self.data.subsectors {
                if ss.vertices.len() < 3 {
                    continue;
                }
                let stroke = if sel == Some(ss.index) {
                    selected_stroke
                } else {
                    default_stroke
                };
                let mn = ss.aabb_min;
                let mx = ss.aabb_max;
                let tl = self.map_to_screen(Vec2::new(mn.x, mx.y), vc);
                let tr = self.map_to_screen(Vec2::new(mx.x, mx.y), vc);
                let br = self.map_to_screen(Vec2::new(mx.x, mn.y), vc);
                let bl = self.map_to_screen(Vec2::new(mn.x, mn.y), vc);
                painter.line_segment([tl, tr], stroke);
                painter.line_segment([tr, br], stroke);
                painter.line_segment([br, bl], stroke);
                painter.line_segment([bl, tl], stroke);
            }
        }

        if self.state.show_linedefs {
            for ld in &self.data.linedefs {
                let p1 = self.map_to_screen(ld.v1, vc);
                let p2 = self.map_to_screen(ld.v2, vc);
                let (width, color) = if self.state.hovered_linedef == Some(ld.index) {
                    (3.0, Color32::YELLOW)
                } else if !ld.is_two_sided {
                    (1.5, Color32::WHITE)
                } else {
                    (1.2, Color32::from_rgb(230, 160, 60))
                };
                painter.line_segment([p1, p2], Stroke::new(width, color));
            }
        }

        if self.state.show_vertices {
            for vx in &self.data.vertices {
                let sp = self.map_to_screen(vx.pos, vc);
                let is_hovered = self.state.hovered_vertex == Some(vx.index);
                let (radius, color) = if is_hovered {
                    (5.0, Color32::from_rgb(255, 220, 50))
                } else {
                    (2.5, Color32::from_rgba_unmultiplied(180, 180, 255, 200))
                };
                painter.circle_filled(sp, radius, color);
                if is_hovered {
                    painter.circle_stroke(
                        sp,
                        radius + 2.0,
                        Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 220, 50, 120)),
                    );
                }
            }
        }

        if self.state.show_divlines {
            if let Some(sel) = self.state.selected_subsector {
                if let Some(path) = self.data.ss_divline_path.get(sel) {
                    for (depth, &dl_idx) in path.iter().enumerate() {
                        let dl = &self.data.divlines[dl_idx];
                        let len = dl.dir.length();
                        if len < 1e-6 {
                            continue;
                        }
                        let norm_dir = dl.dir / len;
                        let extent = 32768.0;
                        let p1 = dl.origin - norm_dir * extent;
                        let p2 = dl.origin + norm_dir * extent;
                        let sp1 = self.map_to_screen(p1, vc);
                        let sp2 = self.map_to_screen(p2, vc);
                        let alpha = (200 - (depth as u32 * 8).min(160)) as u8;
                        painter.line_segment(
                            [sp1, sp2],
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 200, 255, alpha)),
                        );
                    }
                }
            }
        }

        self.draw_drag_overlay(painter, vc);
    }

    pub fn draw_drag_overlay(&self, painter: &egui::Painter, vc: Pos2) {
        let pointer = painter.ctx().input(|i| i.pointer.interact_pos());
        let Some(cursor) = pointer else { return };
        let end_map = self.screen_to_map(cursor, vc);

        match self.state.drag_tool {
            DragTool::LineProbe {
                start,
            } => {
                let sp1 = self.map_to_screen(start, vc);
                painter.line_segment(
                    [sp1, cursor],
                    Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 100, 0, 200)),
                );
            }
            DragTool::RectSelect {
                start,
            } => {
                let lo = Vec2::new(start.x.min(end_map.x), start.y.min(end_map.y));
                let hi = Vec2::new(start.x.max(end_map.x), start.y.max(end_map.y));
                let tl = self.map_to_screen(Vec2::new(lo.x, hi.y), vc);
                let br = self.map_to_screen(Vec2::new(hi.x, lo.y), vc);
                let rect = egui::Rect::from_two_pos(tl, br);
                let stroke = Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 200, 0, 200));
                let fill = Color32::from_rgba_unmultiplied(255, 200, 0, 20);
                painter.add(filled_polygon(
                    &[
                        Pos2::new(rect.min.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.max.y),
                        Pos2::new(rect.min.x, rect.max.y),
                    ],
                    fill,
                ));
                painter.line_segment(
                    [
                        Pos2::new(rect.min.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.min.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.max.x, rect.min.y),
                        Pos2::new(rect.max.x, rect.max.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.max.x, rect.max.y),
                        Pos2::new(rect.min.x, rect.max.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(rect.min.x, rect.max.y),
                        Pos2::new(rect.min.x, rect.min.y),
                    ],
                    stroke,
                );
            }
            DragTool::None => {}
        }
    }

    pub fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading(&self.data.map_name);
        ui.label(format!("Sectors: {}", self.data.sectors.len()));
        ui.label(format!("Linedefs: {}", self.data.linedefs.len()));
        ui.label(format!("Subsectors: {}", self.data.subsectors.len()));
        ui.separator();

        ui.heading("Layers");
        ui.checkbox(&mut self.state.show_linedefs, "Linedefs");
        ui.checkbox(&mut self.state.show_sectors, "Sector colours");
        ui.checkbox(&mut self.state.show_aabb, "Subsector AABBs");
        ui.checkbox(&mut self.state.show_divlines, "Divlines (hover)");
        ui.checkbox(&mut self.state.show_map_polygon_edges, "Polygons");
        ui.checkbox(&mut self.state.show_polygon_fill, "Polygon fill");
        ui.checkbox(&mut self.state.show_vertices, "Vertices");
        ui.separator();

        ui.heading("Debug Tools");
        ui.label("Cmd+drag: line probe");
        ui.label("Cmd+Shift+drag: rect select");
        ui.label("Output printed to stdout and copied to clipboard");
        ui.separator();

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            if let Some(sel) = self.state.selected_subsector {
                if self.state.pinned {
                    if ui.button("Unpin selection").clicked() {
                        self.state.pinned = false;
                    }
                }
                Self::draw_subsector_info_static(&self.data, ui, sel);
            } else {
                ui.label("Hover a subsector to select");
            }
            let label = if self.state.pinned {
                "Selected (pinned)"
            } else {
                "Selected"
            };
            ui.colored_label(Color32::from_rgb(255, 160, 40), label);
            ui.separator();
        });
    }

    pub fn draw_hover_overlay(&self, painter: &egui::Painter, viewport_rect: egui::Rect) {
        let val = Color32::from_gray(210);
        let mut lines: Vec<Vec<(Color32, String)>> = Vec::new();

        if let Some(hovered) = self.state.hovered_subsector {
            lines.push(Self::build_ss_spans(&self.data, hovered));
        }

        if let Some((ss_idx, edge_idx)) = self.state.hovered_polygon_edge {
            if let Some(ss) = self.data.subsectors.iter().find(|s| s.index == ss_idx) {
                let n = ss.vertices.len();
                if edge_idx < n {
                    let v1 = ss.vertices[edge_idx];
                    let v2 = ss.vertices[(edge_idx + 1) % n];
                    let lbl = Color32::from_rgb(255, 255, 100);
                    let spans = vec![
                        (lbl, "Edge:".into()),
                        (val, format!(" ss{} e{}/{}", ss_idx, edge_idx, n)),
                        (lbl, "  from:".into()),
                        (val, format!("({:.1},{:.1})", v1.x, v1.y)),
                        (lbl, " to:".into()),
                        (val, format!("({:.1},{:.1})", v2.x, v2.y)),
                        (lbl, " len:".into()),
                        (val, format!("{:.2}", (v2 - v1).length())),
                    ];
                    lines.push(spans);
                }
            }
        }

        if let Some(lid) = self.state.hovered_linedef {
            if let Some(ld) = self.data.linedefs.get(lid) {
                let lbl = Color32::from_rgb(255, 220, 100);
                let back: String = ld.back_sector_id.map_or("none".into(), |b| b.to_string());
                let spans = vec![
                    (lbl, "LD:".into()),
                    (val, format!("{:<5}", lid)),
                    (lbl, " front:".into()),
                    (val, format!("{:<5}", ld.front_sector_id)),
                    (lbl, " back:".into()),
                    (val, format!("{:<8}", back)),
                    (lbl, " sided:".into()),
                    (
                        val,
                        format!("{:<6}", if ld.is_two_sided { "2" } else { "1" }),
                    ),
                    (lbl, " spc:".into()),
                    (val, format!("{:<5}", ld.special)),
                    (lbl, " tag:".into()),
                    (val, format!("{:<5}", ld.tag)),
                ];
                lines.push(spans);
            }
        }

        if let Some(vid) = self.state.hovered_vertex {
            if let Some(vx) = self.data.vertices.get(vid) {
                let lbl = Color32::from_rgb(180, 255, 180);
                let ld_list: String = vx
                    .linedef_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let spans = vec![
                    (lbl, "VX:".into()),
                    (val, format!("{:<5}", vid)),
                    (lbl, " pos:".into()),
                    (val, format!("({:.1},{:.1})", vx.pos.x, vx.pos.y)),
                    (lbl, "  ld:".into()),
                    (
                        val,
                        if ld_list.is_empty() {
                            "none".into()
                        } else {
                            ld_list
                        },
                    ),
                ];
                lines.push(spans);
            }
        }

        if lines.is_empty() {
            return;
        }

        let font = egui::FontId::monospace(13.0);
        let line_height = 18.0;
        let padding = 6.0;
        let total_height = lines.len() as f32 * line_height + padding * 2.0;

        let bg_rect = egui::Rect::from_min_max(
            Pos2::new(viewport_rect.min.x, viewport_rect.max.y - total_height),
            viewport_rect.max,
        );
        painter.rect_filled(bg_rect, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 210));

        for (i, spans) in lines.iter().enumerate() {
            let y = bg_rect.min.y + padding + i as f32 * line_height;
            let mut x = bg_rect.min.x + padding;
            for (color, text) in spans {
                let galley = painter.layout_no_wrap(text.clone(), font.clone(), *color);
                let w = galley.rect.width();
                painter.galley(Pos2::new(x, y), galley, *color);
                x += w;
            }
        }
    }

    fn build_ss_spans(data: &ViewerData, ss_id: usize) -> Vec<(Color32, String)> {
        let lbl = Color32::from_rgb(100, 200, 255);
        let val = Color32::from_gray(210);
        let mut spans = Vec::new();

        if let Some(ss) = data.subsectors.get(ss_id) {
            let sid = ss.sector_id;
            spans.push((lbl, "SS:".into()));
            spans.push((val, format!("{:<5}", ss_id)));
            spans.push((lbl, " sector:".into()));
            spans.push((val, format!("{:<5}", sid)));
            if let Some(s) = data.sectors.get(sid) {
                spans.push((lbl, " floor:".into()));
                spans.push((val, format!("{:<7}", s.floor_height)));
                spans.push((lbl, " ceil:".into()));
                spans.push((val, format!("{:<7}", s.ceiling_height)));
                spans.push((lbl, " light:".into()));
                spans.push((val, format!("{:<5}", s.light_level)));
                spans.push((lbl, " spc:".into()));
                spans.push((val, format!("{:<5}", s.special)));
                spans.push((lbl, " tag:".into()));
                spans.push((val, format!("{:<5}", s.tag)));
            }
        }
        spans
    }

    fn draw_subsector_info_static(data: &ViewerData, ui: &mut egui::Ui, ss_id: usize) {
        if let Some(ss) = data.subsectors.get(ss_id) {
            let sid = ss.sector_id;
            const W: usize = 12;
            ui.monospace(format!("{:<W$}{}", "Subsector:", ss_id));
            ui.monospace(format!("{:<W$}{}", "Sector:", sid));
            if let Some(s) = data.sectors.get(sid) {
                ui.monospace(format!("{:<W$}{}", "Floor:", s.floor_height));
                ui.monospace(format!("{:<W$}{}", "Ceil:", s.ceiling_height));
                ui.monospace(format!("{:<W$}{}", "Light:", s.light_level));
                ui.monospace(format!("{:<W$}{}", "Special:", s.special));
                ui.monospace(format!("{:<W$}{}", "Tag:", s.tag));
            }
        }
    }
}
