//! World-space framing helpers: bounding rect, grid snap, and view constants; camera transform lives on [`Camera`](super::camera3d::Camera).

/// Zoom multiplier per wheel notch, anchored at the cursor.
pub const WHEEL_ZOOM_FACTOR: f32 = 1.1;
/// Initial grid spacing in world units.
pub const DEFAULT_GRID: i32 = 8;

/// An axis-aligned rectangle in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl WorldRect {
    /// Rectangle covering a single point.
    pub fn point(x: f32, y: f32) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
}

/// Round half-away-from-zero to a multiple of `grid` (DoomEd snap); distinct from the kernel's plainer `snap_coord`.
pub fn snap(v: f32, grid: i32) -> f32 {
    let g = grid as f32;
    let cells = if v < 0.0 {
        (v / g - 0.5) as i32
    } else {
        (v / g + 0.5) as i32
    };
    (cells * grid) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_matches_doomed_rounding() {
        assert_eq!(snap(12.0, 8), 16.0);
        assert_eq!(snap(11.9, 8), 8.0);
        assert_eq!(snap(-12.0, 8), -16.0);
        assert_eq!(snap(-11.9, 8), -8.0);
        assert_eq!(snap(0.0, 8), 0.0);
        assert_eq!(snap(4.0, 8), 8.0);
        assert_eq!(snap(-4.0, 8), -8.0);
    }
}
