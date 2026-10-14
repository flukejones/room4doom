use super::canvas::{Canvas, Color, Stroke};
use glam::Vec2;

use super::MapViewerApp;
use super::query::DragTool;

fn flame_color(idx: u32, alpha: u8) -> Color {
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
    Color::from_rgba_unmultiplied(r, g, b, alpha)
}

impl MapViewerApp {
    pub fn draw_layers(&mut self, painter: &mut Canvas<'_>, vc: Vec2) {
        if self.state.show_sectors {
            for ss in &self.data.subsectors {
                if ss.vertices.len() < 3 {
                    continue;
                }
                let base = self.data.floor_texture_color[ss.sector_id];
                let v = self.data.floor_height_value[ss.sector_id];
                let peak = base.r().max(base.g()).max(base.b()).max(1) as f32;
                let scale = v * 255.0 / peak;
                let color = Color::from_rgb(
                    (base.r() as f32 * scale).min(255.0) as u8,
                    (base.g() as f32 * scale).min(255.0) as u8,
                    (base.b() as f32 * scale).min(255.0) as u8,
                );
                painter.polygon(
                    ss.vertices.iter().map(|&v| self.map_to_screen(v, vc)),
                    color,
                );
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
                painter.polygon(
                    ss.vertices.iter().map(|&v| self.map_to_screen(v, vc)),
                    color,
                );
                idx += 1;
            }
        }

        if self.state.show_map_polygon_edges {
            let mut edge = self.style.special;
            edge[3] = 200;
            let stroke = Stroke::new(1.0_f32, Color(edge));
            let draw_verts = self.state.show_vertices;
            let vert_color = Color(self.style.special);
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
        if let Some((ss_idx, edge_idx)) = self.state.hovered_polygon_edge
            && let Some(ss) = self.data.subsectors.iter().find(|s| s.index == ss_idx)
        {
            let n = ss.vertices.len();
            if edge_idx < n {
                let p1 = self.map_to_screen(ss.vertices[edge_idx], vc);
                let p2 = self.map_to_screen(ss.vertices[(edge_idx + 1) % n], vc);
                painter.line_segment([p1, p2], Stroke::new(3.0_f32, Color(self.style.selected)));
                painter.circle_filled(p1, 4.0, Color(self.style.selected));
                painter.circle_filled(p2, 4.0, Color(self.style.selected));
            }
        }

        if self.state.show_aabb {
            let sel = self.state.selected_subsector;
            let default_stroke =
                Stroke::new(0.5_f32, Color::from_rgba_unmultiplied(255, 100, 255, 60));
            let selected_stroke =
                Stroke::new(2.5_f32, Color::from_rgba_unmultiplied(255, 255, 0, 255));

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
                    (3.0, Color(self.style.selected))
                } else if !ld.is_two_sided {
                    (1.5, Color(self.style.one_sided))
                } else {
                    (1.2_f32, Color(self.style.two_sided))
                };
                painter.line_segment([p1, p2], Stroke::new(width, color));
            }
        }

        if self.state.show_segments {
            // Inset each seg toward its front side so the two segs of a
            // two-sided line are drawn as distinct parallel lines.
            for seg in &self.data.segments {
                let dir = (seg.v2 - seg.v1).normalize_or_zero();
                let normal = Vec2::new(-dir.y, dir.x); // points to the seg's left/front
                let inset = normal * 2.0;
                let p1 = self.map_to_screen(seg.v1 + inset, vc);
                let p2 = self.map_to_screen(seg.v2 + inset, vc);
                let color = if seg.back_sector_id.is_some() {
                    Color::from_rgb(80, 200, 255)
                } else {
                    Color::from_rgb(120, 255, 120)
                };
                painter.line_segment([p1, p2], Stroke::new(1.5_f32, color));
                // Arrowhead at v2 showing seg direction.
                let back = (p1 - p2).normalize_or_zero();
                let perp = Vec2::new(-back.y, back.x);
                painter.line_segment([p2, p2 + (back + perp) * 6.0], Stroke::new(1.5_f32, color));
                painter.line_segment([p2, p2 + (back - perp) * 6.0], Stroke::new(1.5_f32, color));
            }
        }

        if self.state.show_vertices {
            for vx in &self.data.vertices {
                let sp = self.map_to_screen(vx.pos, vc);
                let is_hovered = self.state.hovered_vertex == Some(vx.index);
                let (radius, color) = if is_hovered {
                    (5.0, Color::from_rgb(255, 220, 50))
                } else {
                    (2.5, Color::from_rgba_unmultiplied(180, 180, 255, 200))
                };
                painter.circle_filled(sp, radius, color);
                if is_hovered {
                    painter.circle_stroke(
                        sp,
                        radius + 2.0,
                        Stroke::new(1.5_f32, Color::from_rgba_unmultiplied(255, 220, 50, 120)),
                    );
                }
            }
        }

        if self.state.show_divlines
            && let Some(sel) = self.state.selected_subsector
            && let Some(path) = self.data.ss_divline_path.get(sel)
        {
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
                    Stroke::new(1.0_f32, Color::from_rgba_unmultiplied(0, 200, 255, alpha)),
                );
            }
        }

        self.draw_drag_overlay(painter, vc);
    }

    pub fn draw_drag_overlay(&self, painter: &mut Canvas<'_>, vc: Vec2) {
        let pointer = self.pointer;
        let Some(cursor) = pointer else { return };
        let end_map = self.screen_to_map(cursor, vc);

        match self.state.drag_tool {
            DragTool::LineProbe {
                start,
            } => {
                let sp1 = self.map_to_screen(start, vc);
                painter.line_segment(
                    [sp1, cursor],
                    Stroke::new(2.0_f32, Color::from_rgba_unmultiplied(255, 100, 0, 200)),
                );
            }
            DragTool::RectSelect {
                start,
            } => {
                let lo = Vec2::new(start.x.min(end_map.x), start.y.min(end_map.y));
                let hi = Vec2::new(start.x.max(end_map.x), start.y.max(end_map.y));
                let tl = self.map_to_screen(Vec2::new(lo.x, hi.y), vc);
                let br = self.map_to_screen(Vec2::new(hi.x, lo.y), vc);
                let (min, max) = (tl.min(br), tl.max(br));
                let stroke = Stroke::new(2.0_f32, Color::from_rgba_unmultiplied(255, 200, 0, 200));
                let fill = Color::from_rgba_unmultiplied(255, 200, 0, 20);
                painter.polygon(
                    [
                        Vec2::new(min.x, min.y),
                        Vec2::new(max.x, min.y),
                        Vec2::new(max.x, max.y),
                        Vec2::new(min.x, max.y),
                    ],
                    fill,
                );
                painter.line_segment([Vec2::new(min.x, min.y), Vec2::new(max.x, min.y)], stroke);
                painter.line_segment([Vec2::new(max.x, min.y), Vec2::new(max.x, max.y)], stroke);
                painter.line_segment([Vec2::new(max.x, max.y), Vec2::new(min.x, max.y)], stroke);
                painter.line_segment([Vec2::new(min.x, max.y), Vec2::new(min.x, min.y)], stroke);
            }
            DragTool::None => {}
        }
    }
}
