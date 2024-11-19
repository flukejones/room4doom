use gameplay::{Level, PicData, Player};

/// channels should match pixel format
const SOFT_PIXEL_CHANNELS: usize = 4;

#[derive(Clone, Copy)]
pub struct BufferSize {
    hi_res: bool,
    width_usize: usize,
    height_usize: usize,
    width: i32,
    height: i32,
    width_f32: f32,
    height_f32: f32,
}

impl BufferSize {
    pub const fn new(width: usize, height: usize) -> Self {
        Self {
            hi_res: height > 200,
            width_usize: width,
            height_usize: height,
            width: width as i32,
            height: height as i32,
            width_f32: width as f32,
            height_f32: height as f32,
        }
    }

    pub const fn hi_res(&self) -> bool {
        self.hi_res
    }

    // todo, need const traits stabilised
    pub const fn width(&self) -> i32 {
        self.width
    }

    pub const fn height(&self) -> i32 {
        self.height
    }

    pub const fn half_width(&self) -> i32 {
        self.width / 2
    }

    pub const fn half_height(&self) -> i32 {
        self.height / 2
    }

    pub const fn width_usize(&self) -> usize {
        self.width_usize
    }

    pub const fn height_usize(&self) -> usize {
        self.height_usize
    }

    pub const fn width_f32(&self) -> f32 {
        self.width_f32
    }

    pub const fn height_f32(&self) -> f32 {
        self.height_f32
    }

    pub const fn half_width_f32(&self) -> f32 {
        self.width_f32 / 2.0
    }

    pub const fn half_height_f32(&self) -> f32 {
        self.height_f32 / 2.0
    }
}

pub trait PixelBuffer {
    fn size(&self) -> &BufferSize;
    fn clear(&mut self);
    fn clear_with_colour(&mut self, colour: &[u8; SOFT_PIXEL_CHANNELS]);
    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; SOFT_PIXEL_CHANNELS]);
    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS];
    fn buf_mut(&mut self) -> &mut [u8];
    /// The pitch that should be added/subtracted to go up or down the Y while
    /// keeping X position
    fn pitch(&self) -> usize;
    /// Amount of colour channels, e.g: [R, G, B] == 3
    fn channels(&self) -> usize;
    /// Get an index point for this coord to copy a colour array too
    fn get_buf_index(&self, x: usize, y: usize) -> usize;
}

pub trait RenderTrait {
    /// Get the buffer currently being drawn to
    fn draw_buffer(&mut self) -> &mut impl PixelBuffer;

    /// Get the buffer that will be blitted to screen
    fn blit_buffer(&mut self) -> &mut impl PixelBuffer;

    /// Throw buffer1 at the screen
    fn blit(&mut self);

    /// for debug
    fn debug_blit_draw_buffer(&mut self);

    fn debug_clear(&mut self);

    fn clear(&mut self);

    fn flip(&mut self);

    /// Must do a blit after to show the results
    fn do_wipe(&mut self) -> bool;
}

pub trait PlayViewRenderer {
    /// Doom function name `R_RenderPlayerView`
    fn render_player_view(self: &mut Self, player: &Player, level: &Level, pic_data: &mut PicData);
}
