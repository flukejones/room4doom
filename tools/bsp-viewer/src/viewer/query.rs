use glam::Vec2;

use super::MapViewerApp;
use super::geom::*;

#[derive(Clone, Copy, PartialEq)]
pub enum DragTool {
    None,
    LineProbe { start: Vec2 },
    RectSelect { start: Vec2 },
}

impl MapViewerApp {
    pub fn output_query(&self, ctx: &egui::Context, text: &str) {
        print!("{text}");
        let mut clipboard = String::new();
        clipboard.push_str(&format!("map: {}\n", self.data.map_name));
        clipboard.push_str(text);
        ctx.copy_text(clipboard);
    }

    pub fn query_line(&self, a: Vec2, b: Vec2) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        let _ = writeln!(buf, "--- LINE_PROBE ---");
        let _ = writeln!(
            buf,
            "line: ({:.1},{:.1}) -> ({:.1},{:.1})",
            a.x, a.y, b.x, b.y
        );

        let _ = writeln!(buf, "# subsectors");
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if line_intersects_polygon(a, b, &ss.vertices) {
                let s = &self.data.sectors[ss.sector_id];
                let _ = writeln!(
                    buf,
                    "ss={} sector={} floor={} ceil={}",
                    ss.index, ss.sector_id, s.floor_height, s.ceiling_height,
                );
            }
        }

        let _ = writeln!(buf, "# linedefs");
        for ld in &self.data.linedefs {
            if segments_intersect(a, b, ld.v1, ld.v2) {
                let _ = writeln!(
                    buf,
                    "ld={} v1=({:.1},{:.1}) v2=({:.1},{:.1}) two_sided={} front_sector={} back_sector={:?} special={} tag={}",
                    ld.index,
                    ld.v1.x,
                    ld.v1.y,
                    ld.v2.x,
                    ld.v2.y,
                    ld.is_two_sided,
                    ld.front_sector_id,
                    ld.back_sector_id,
                    ld.special,
                    ld.tag
                );
            }
        }

        let _ = writeln!(buf, "# divlines");
        for dl in &self.data.divlines {
            let len = dl.dir.length();
            if len < 1e-6 {
                continue;
            }
            let norm = dl.dir / len;
            let extent = 32768.0;
            let dl_a = dl.origin - norm * extent;
            let dl_b = dl.origin + norm * extent;
            if segments_intersect(a, b, dl_a, dl_b) {
                let _ = writeln!(
                    buf,
                    "divline: node={} origin=({:.1},{:.1}) dir=({:.1},{:.1})",
                    dl.index, dl.origin.x, dl.origin.y, dl.dir.x, dl.dir.y
                );
            }
        }

        let _ = writeln!(buf, "--- END ---");
        buf
    }

    pub fn query_rect(&self, lo: Vec2, hi: Vec2) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        let _ = writeln!(buf, "--- RECT_SELECT ---");
        let _ = writeln!(
            buf,
            "rect: ({:.1},{:.1}) -> ({:.1},{:.1})",
            lo.x, lo.y, hi.x, hi.y
        );

        let _ = writeln!(buf, "# subsectors");
        let mut selected_ss = Vec::new();
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if polygon_inside_rect(&ss.vertices, lo, hi) {
                let s = &self.data.sectors[ss.sector_id];
                let _ = writeln!(
                    buf,
                    "ss={} sector={} floor={} ceil={} verts={}",
                    ss.index,
                    ss.sector_id,
                    s.floor_height,
                    s.ceiling_height,
                    ss.vertices.len()
                );
                selected_ss.push(ss);
            }
        }

        let _ = writeln!(buf, "# polygons");
        for ss in &selected_ss {
            for (pi, poly) in ss.polygons.iter().enumerate() {
                let verts: String = poly
                    .vertices
                    .iter()
                    .map(|v| format!("({:.1},{:.1},{:.1})", v.x, v.y, v.z))
                    .collect::<Vec<_>>()
                    .join(" ");
                let _ = writeln!(
                    buf,
                    "ss={} poly={} kind={} verts={} [{}]",
                    ss.index,
                    pi,
                    poly.kind,
                    poly.vertices.len(),
                    verts
                );
            }
        }

        let _ = writeln!(buf, "# linedefs");
        for ld in &self.data.linedefs {
            if point_in_rect(ld.v1, lo, hi) && point_in_rect(ld.v2, lo, hi) {
                let _ = writeln!(
                    buf,
                    "ld={} v1=({:.1},{:.1}) v2=({:.1},{:.1}) two_sided={} front_sector={} back_sector={:?} special={} tag={}",
                    ld.index,
                    ld.v1.x,
                    ld.v1.y,
                    ld.v2.x,
                    ld.v2.y,
                    ld.is_two_sided,
                    ld.front_sector_id,
                    ld.back_sector_id,
                    ld.special,
                    ld.tag
                );
            }
        }

        let _ = writeln!(buf, "# divlines");
        for dl in &self.data.divlines {
            if point_in_rect(dl.origin, lo, hi) {
                let _ = writeln!(
                    buf,
                    "divline: node={} origin=({:.1},{:.1}) dir=({:.1},{:.1})",
                    dl.index, dl.origin.x, dl.origin.y, dl.dir.x, dl.dir.y
                );
            }
        }

        let _ = writeln!(buf, "# sectors");
        let mut seen_sectors = std::collections::BTreeSet::new();
        for ss in &self.data.subsectors {
            if ss.vertices.len() < 3 {
                continue;
            }
            if polygon_inside_rect(&ss.vertices, lo, hi) {
                seen_sectors.insert(ss.sector_id);
            }
        }
        for &sid in &seen_sectors {
            let s = &self.data.sectors[sid];
            let _ = writeln!(
                buf,
                "sector={} floor={} ceil={} light={} special={} tag={}",
                sid, s.floor_height, s.ceiling_height, s.light_level, s.special, s.tag,
            );
        }

        let _ = writeln!(buf, "--- END ---");
        buf
    }

    pub fn query_sector(&self, sector_id: usize) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        let _ = writeln!(buf, "--- SECTOR_QUERY ---");
        let s = &self.data.sectors[sector_id];
        let _ = writeln!(
            buf,
            "sector={} floor={} ceil={} light={} special={} tag={}",
            sector_id, s.floor_height, s.ceiling_height, s.light_level, s.special, s.tag,
        );

        let _ = writeln!(buf, "# subsectors");
        for ss in &self.data.subsectors {
            if ss.sector_id != sector_id {
                continue;
            }
            let _ = writeln!(buf, "ss={} verts={}", ss.index, ss.vertices.len());
        }

        let _ = writeln!(buf, "# linedefs");
        for ld in &self.data.linedefs {
            if ld.front_sector_id == sector_id || ld.back_sector_id == Some(sector_id) {
                let _ = writeln!(
                    buf,
                    "ld={} v1=({:.1},{:.1}) v2=({:.1},{:.1}) two_sided={} front_sector={} back_sector={:?} special={} tag={}",
                    ld.index,
                    ld.v1.x,
                    ld.v1.y,
                    ld.v2.x,
                    ld.v2.y,
                    ld.is_two_sided,
                    ld.front_sector_id,
                    ld.back_sector_id,
                    ld.special,
                    ld.tag
                );
            }
        }

        let _ = writeln!(buf, "--- END ---");
        buf
    }
}
