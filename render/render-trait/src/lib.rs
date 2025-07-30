use gameplay::{Level, PicData, Player};

/// channels should match pixel format
pub const SOFT_PIXEL_CHANNELS: usize = 4;

pub trait GameRenderer {
    // Core 3D rendering
    fn render_player_view(&mut self, player: &Player, level: &mut Level, pic_data: &mut PicData);

    // Buffer management & presentation
    fn flip_and_present(&mut self);
    fn flip(&mut self);

    /// Get the framebuffer used for direct draw access
    fn draw_buffer(&mut self) -> &mut impl DrawBuffer;

    // Screen effects
    fn do_wipe(&mut self) -> bool;

    fn buffer_size(&self) -> &BufferSize;
}

pub trait DrawBuffer {
    // Direct pixel access
    fn size(&self) -> &BufferSize;
    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; 4]);
    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS];
    fn get_buf_index(&self, x: usize, y: usize) -> usize;
    fn pitch(&self) -> usize;
    fn buf_mut(&mut self) -> &mut [u8]; // TODO: remove this
    fn debug_blit_draw_buffer(&mut self);
}

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
        self.half_width() as f32
    }

    pub const fn half_height_f32(&self) -> f32 {
        self.half_height() as f32
    }
}
