//! Canvas drawing colours and metrics. RGBA8, opaque.

/// One canvas color, RGBA8.
pub type Color = [u8; 4];

/// Vertex marker edge in screen pixels (DoomEd CPOINTDRAW).
pub const VERTEX_DRAW_PX: f32 = 7.0;
/// Length of the line-normal tick at line midpoints, in screen pixels.
pub const NORMAL_TICK_PX: f32 = 6.0;
/// Strong grid spacing in world units (flat alignment grid).
pub const TILE_GRID: i32 = 64;
/// Grid lines are skipped below this on-screen spacing.
pub const MIN_GRID_SPACING_PX: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasStyle {
    pub back: Color,
    pub grid: Color,
    pub tile: Color,
    pub selected: Color,
    pub point: Color,
    pub one_sided: Color,
    pub two_sided: Color,
    pub special: Color,
    pub thing: Color,
    /// Lines with a void-facing side, when the un-enclosed highlight is on.
    pub warning: Color,
}

impl Default for CanvasStyle {
    fn default() -> Self {
        Self {
            back: [0xff, 0xff, 0xff, 0xff],
            grid: [0xec, 0xec, 0xec, 0xff],
            tile: [0xd0, 0xd0, 0xd0, 0xff],
            selected: [0xe0, 0x20, 0x20, 0xff],
            point: [0x20, 0x20, 0x20, 0xff],
            one_sided: [0x00, 0x00, 0x00, 0xff],
            two_sided: [0x90, 0x90, 0x90, 0xff],
            special: [0x00, 0xa0, 0x00, 0xff],
            thing: [0x80, 0x30, 0xa0, 0xff],
            warning: [0xe0, 0x80, 0x00, 0xff],
        }
    }
}
