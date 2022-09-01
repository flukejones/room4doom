use gameplay::glam::Vec2;
use glow::{HasContext, NativeProgram};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, error::Error};

pub mod basic;
// pub mod cgwg_crt;
// pub mod lottes_crt;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq, Serialize, Deserialize)]
pub enum Shaders {
    Lottes,
    Cgwg,
    None,
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
            "cgwg" => Ok(Shaders::Cgwg),
            "off" | "none" => Ok(Shaders::None),
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

    fn set_tex_filter(&self) -> Result<(), Box<dyn Error>>;

    /// The input buffer/image of Doom
    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32));

    fn draw(&mut self) -> Result<(), Box<dyn Error>>;
}

unsafe fn set_uniform_f32(gl: &glow::Context, program: NativeProgram, name: &str, value: f32) {
    let uniform_location = gl.get_uniform_location(program, name);
    // See also `uniform_n_i32`, `uniform_n_u32`, `uniform_matrix_4_f32_slice` etc.
    gl.uniform_1_f32(uniform_location.as_ref(), value)
}

unsafe fn set_uniform_mat4(gl: &glow::Context, program: NativeProgram, name: &str, value: &[f32]) {
    let uniform_location = gl.get_uniform_location(program, name);
    gl.uniform_matrix_4_f32_slice(uniform_location.as_ref(), false, value)
}

unsafe fn set_uniform_vec2(gl: &glow::Context, program: NativeProgram, name: &str, value: &Vec2) {
    let uniform_location = gl.get_uniform_location(program, name);
    gl.uniform_2_f32(uniform_location.as_ref(), value.x, value.y)
}