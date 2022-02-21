use std::str::FromStr;

use golem::GolemError;

pub mod basic;
pub mod cgwg_crt;
pub mod lottes_crt;

#[derive(Debug, Clone, Copy)]
pub enum Shaders {
    Basic,
    Lottes,
    Cgwg,
}

impl FromStr for Shaders {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "basic" => Ok(Shaders::Basic),
            "lottes" => Ok(Shaders::Lottes),
            "cgwg" => Ok(Shaders::Cgwg),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Doh!")),
        }
    }
}

const GL_QUAD: [f32; 16] = [
    // position         vert_uv
    -1.0, -1.0, 0.0, 1.0, // bottom left
    1.0, -1.0, 1.0, 1.0, // bottom right
    1.0, 1.0, 1.0, 0.0, // top right
    -1.0, 1.0, 0.0, 0.0, // top left
];

const GL_QUAD_INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];

pub trait Drawer {
    fn clear(&self);

    fn set_tex_filter(&self) -> Result<(), GolemError>;

    /// The input buffer/image of Doom
    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32));

    fn draw(&mut self) -> Result<(), GolemError>;
}
