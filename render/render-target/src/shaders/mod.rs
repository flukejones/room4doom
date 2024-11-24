use golem::{GolemError, Texture};
use nanoserde::{DeRon, SerRon};
use std::str::FromStr;

pub mod basic;
pub mod lottes_crt;
pub mod lottes_reduced;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq, DeRon, SerRon)]
pub enum Shaders {
    Lottes,
    LottesBasic,
    Basic,
}

impl Default for Shaders {
    fn default() -> Self {
        Self::Lottes
    }
}

impl FromStr for Shaders {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "lottes" => Ok(Shaders::Lottes),
            "lottesbasic" => Ok(Shaders::LottesBasic),
            "basic" => Ok(Shaders::Basic),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Doh!")),
        }
    }
}

const GL_QUAD: [f32; 16] = [
    // position         vert_uv
    -1.0, -1.0, 0.0, 1.0, // bottom left
    1.0, -1.0, 1.0, 1.0, // bottom right
    1.0, 1.0, 1.0, 0.0, // top right
    -1.0, 1.0, 0.0, 0.0, /* top left */
];

const GL_QUAD_INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];

pub trait ShaderDraw {
    fn draw(&mut self, texture: &Texture) -> Result<(), GolemError>;
}
