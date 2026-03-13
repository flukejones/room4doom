use crate::map_defs::Node;
use glam::Vec2;

impl Node {
    /// R_PointOnSide
    ///
    /// Determine with cross-product which side of a splitting line the point is
    /// on
    #[inline]
    pub const fn point_on_side(&self, v: &Vec2) -> usize {
        let dx = v.x - self.xy.x;
        let dy = v.y - self.xy.y;

        if (self.delta.y * dx) > (dy * self.delta.x) {
            return 0;
        }
        1
    }

    /// Returns (front_child_id, back_child_id) for the given point.
    /// Front is the child on the same side as the point (closer).
    pub fn front_back_children(&self, point: &Vec2) -> (u32, u32) {
        let side = self.point_on_side(point);
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
