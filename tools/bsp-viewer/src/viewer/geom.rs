use egui::{Color32, Mesh, Pos2};
use glam::Vec2;
use std::sync::Arc;

pub fn filled_polygon(points: &[Pos2], color: Color32) -> egui::Shape {
    if points.len() < 3 {
        return egui::Shape::Noop;
    }
    let n = points.len();
    let centroid = Pos2::new(
        points.iter().map(|p| p.x).sum::<f32>() / n as f32,
        points.iter().map(|p| p.y).sum::<f32>() / n as f32,
    );
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centroid, color);
    for p in points {
        mesh.colored_vertex(*p, color);
    }
    let n = n as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    egui::Shape::Mesh(Arc::new(mesh))
}

pub fn point_to_segment_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.dot(ab);
    if len_sq < 1e-12 {
        return (p - a).length();
    }
    let t = (p - a).dot(ab) / len_sq;
    (p - (a + ab * t.clamp(0.0, 1.0))).length()
}

pub fn point_in_polygon(p: Vec2, verts: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = verts.len() - 1;
    for i in 0..verts.len() {
        let vi = verts[i];
        let vj = verts[j];
        if ((vi.y > p.y) != (vj.y > p.y))
            && (p.x < (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn point_in_rect(p: Vec2, lo: Vec2, hi: Vec2) -> bool {
    p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
}

pub fn polygon_inside_rect(verts: &[Vec2], lo: Vec2, hi: Vec2) -> bool {
    verts.iter().any(|&v| point_in_rect(v, lo, hi))
}

pub fn segments_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    let d1 = a2 - a1;
    let d2 = b2 - b1;
    let cross = d1.x * d2.y - d1.y * d2.x;
    if cross.abs() < 1e-10 {
        return false;
    }
    let d = b1 - a1;
    let t = (d.x * d2.y - d.y * d2.x) / cross;
    let u = (d.x * d1.y - d.y * d1.x) / cross;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

pub fn line_intersects_polygon(a: Vec2, b: Vec2, verts: &[Vec2]) -> bool {
    if point_in_polygon(a, verts) || point_in_polygon(b, verts) {
        return true;
    }
    let n = verts.len();
    for i in 0..n {
        if segments_intersect(a, b, verts[i], verts[(i + 1) % n]) {
            return true;
        }
    }
    false
}
