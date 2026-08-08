//! Swept-sphere queries against the 3D BSP, for movement that must not consult
//! the blockmap.
//!
//! The tree holds only solid surfaces, so presence of geometry *is* blocking —
//! the open part of a two-sided linedef has no polygon and a sweep passes
//! through it. The exceptions are sky (not real geometry) and passable middle
//! textures, both handled in [`BSP3D::poly_blocks`].

use glam::{Vec2, Vec3};

use super::runtime::{BSP3D, is_leaf, leaf_index};
use crate::flags::LineDefFlags;

impl BSP3D {
    /// Leaf ids whose bounds overlap the box, appended to `out`.
    ///
    /// Gather once for a whole path and sweep against the result; the descent
    /// prunes on subtree AABBs alone, so it does not care where the tree
    /// switches from vertical to general partition planes.
    pub fn leaves_overlapping(&self, min: Vec3, max: Vec3, out: &mut Vec<usize>) {
        self.gather_leaves(self.root_node(), min, max, out);
    }

    fn gather_leaves(&self, node_id: u32, min: Vec3, max: Vec3, out: &mut Vec<usize>) {
        let Some(aabb) = self.get_node_aabb(node_id) else {
            return;
        };
        if aabb.min.cmpgt(max).any() || aabb.max.cmplt(min).any() {
            return;
        }
        if is_leaf(node_id) {
            out.push(leaf_index(node_id));
            return;
        }
        for child in self.nodes()[node_id as usize].children {
            self.gather_leaves(child, min, max, out);
        }
    }

    /// The first blocking polygon within `radius` of `point`, searching only
    /// `leaves`. `None` means a sphere there is clear of the level.
    pub fn blocking_poly_near(&self, point: Vec3, radius: f32, leaves: &[usize]) -> Option<usize> {
        let r_sq = radius * radius;
        leaves.iter().find_map(|&leaf| {
            self.leaf_poly_indices(leaf)
                .find(|&gi| self.poly_blocks(gi) && self.poly_within(gi, leaf, point, r_sq))
        })
    }

    /// Whether a polygon stops movement. Sky is a render artefact, and a middle
    /// texture on a two-sided line is walk-through unless the linedef blocks.
    fn poly_blocks(&self, gi: usize) -> bool {
        if self.poly_is_sky(gi) {
            return false;
        }
        if self.poly_is_masked_middle(gi) || self.poly_is_translucent(gi) {
            return self.linedef_blocks(gi);
        }
        true
    }

    fn linedef_blocks(&self, gi: usize) -> bool {
        self.polygons[gi]
            .linedef
            .as_ref()
            .is_some_and(|line| line.flags.contains(LineDefFlags::Blocking))
    }

    /// Whether a sphere at `point` of radius² `r_sq` touches polygon `gi`.
    ///
    /// A blocking linedef bars its whole opening, not just the slot its texture
    /// occupies, so such a wall is tested as its ground line over the leaf's
    /// full height — otherwise a railing would be teleported straight through.
    fn poly_within(&self, gi: usize, leaf: usize, point: Vec3, r_sq: f32) -> bool {
        if !self.poly_is_flat(gi) && self.linedef_blocks(gi) {
            return self.wall_line_within(gi, leaf, point, r_sq);
        }
        let verts = self.poly_vert_indices(gi);
        let Some(&first) = verts.first() else {
            return false;
        };
        let a = self.vertex_get(first);
        verts[1..].windows(2).any(|pair| {
            let (b, c) = (self.vertex_get(pair[0]), self.vertex_get(pair[1]));
            closest_on_triangle(point, a, b, c).distance_squared(point) <= r_sq
        })
    }

    /// Distance test against a wall's ground line, extended over the leaf's
    /// height rather than the polygon's own.
    fn wall_line_within(&self, gi: usize, leaf: usize, point: Vec3, r_sq: f32) -> bool {
        let aabb = &self.leaves[leaf].aabb;
        if point.z < aabb.min.z || point.z > aabb.max.z {
            return false;
        }
        let verts = self.poly_vert_indices(gi);
        let Some(&first) = verts.first() else {
            return false;
        };
        let a = self.vertex_get(first).truncate();
        // The wall's two distinct ground points; a vertical quad collapses to a
        // line in XY, so the furthest vertex from the first is the other end.
        let b = verts[1..]
            .iter()
            .map(|&i| self.vertex_get(i).truncate())
            .max_by(|p, q| p.distance_squared(a).total_cmp(&q.distance_squared(a)))
            .unwrap_or(a);
        closest_on_segment(point.truncate(), a, b).distance_squared(point.truncate()) <= r_sq
    }
}

/// The point of segment `a`→`b` nearest `p`.
fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq <= f32::EPSILON {
        return a;
    }
    a + ab * ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0)
}

/// The point of triangle `abc` nearest `p` (Ericson, *Real-Time Collision
/// Detection* §5.1.5): test the three vertex regions, then the three edge
/// regions, else project into the face.
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_point_lands_in_each_triangle_region() {
        let (a, b, c) = (
            Vec3::ZERO,
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        );
        // Above the face projects straight down onto it.
        assert!(
            closest_on_triangle(Vec3::new(1.0, 1.0, 5.0), a, b, c)
                .abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-5)
        );
        // Beyond a vertex clamps to that vertex.
        assert!(closest_on_triangle(Vec3::new(-3.0, -3.0, 0.0), a, b, c).abs_diff_eq(a, 1e-5));
        // Off the middle of an edge clamps onto the edge.
        assert!(
            closest_on_triangle(Vec3::new(2.0, -3.0, 0.0), a, b, c)
                .abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-5)
        );
    }
}
