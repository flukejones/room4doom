use crate::map_defs::Node;
use glam::Vec2;
use math::{FixedT, float_to_fixed, r_point_on_side_raw};

impl Node {
    /// R_PointOnSide (rendering) — raw f32 cross-product. For rendering only.
    #[inline]
    pub const fn point_on_side(&self, v: &Vec2) -> usize {
        let dx = v.x - self.xy.x;
        let dy = v.y - self.xy.y;

        if (self.delta.y * dx) > (dy * self.delta.x) {
            return 0;
        }
        1
    }

    /// OG Doom `R_PointOnSide` — 16.16 fixed-point side test matching OG
    /// integer arithmetic exactly. Used for gameplay subsector lookup.
    #[inline]
    pub fn point_on_side_fixed(&self, x: FixedT, y: FixedT) -> usize {
        r_point_on_side_raw(
            x.to_fixed_raw(),
            y.to_fixed_raw(),
            float_to_fixed(self.xy.x),
            float_to_fixed(self.xy.y),
            float_to_fixed(self.delta.x),
            float_to_fixed(self.delta.y),
        )
    }

    /// Returns (front_child_id, back_child_id) for the given point.
    /// Front is the child on the same side as the point (closer).
    pub fn front_back_children(&self, point: &Vec2) -> (u32, u32) {
        let side = self.point_on_side(point);
        (self.children[side], self.children[side ^ 1])
    }

    /// Fixed-point variant of `front_back_children` for gameplay subsector
    /// lookup. Matches OG Doom `R_PointOnSide` exactly.
    pub fn front_back_children_fixed(&self, x: FixedT, y: FixedT) -> (u32, u32) {
        let side = self.point_on_side_fixed(x, y);
        (self.children[side], self.children[side ^ 1])
    }

    #[inline]
    pub const fn point_in_bounds(&self, v: Vec2, side: usize) -> bool {
        if v.x > self.bboxes[side][0].x
            && v.x < self.bboxes[side][1].x
            && v.y < self.bboxes[side][0].y
            && v.y > self.bboxes[side][1].y
        {
            return true;
        }
        false
    }
}
