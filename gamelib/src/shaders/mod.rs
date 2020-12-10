use golem::GolemError;

pub(crate) mod basic;
pub(crate) mod cgwg_crt;
pub(crate) mod lottes_crt;

const GL_QUAD: [f32; 16] = [
    // position         vert_uv
    -1.0, -1.0, 0.0, 1.0, // bottom left
    1.0, -1.0, 1.0, 1.0, // bottom right
    1.0, 1.0, 1.0, 0.0, // top right
    -1.0, 1.0, 0.0, 0.0, // top left
];

const GL_QUAD_INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];

pub(crate) trait Renderer {
    fn clear(&self);

    fn set_tex_filter(&self) -> Result<(), GolemError>;

    /// The input buffer/image of Doom
    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32));

    fn draw(&mut self) -> Result<(), GolemError>;
}
