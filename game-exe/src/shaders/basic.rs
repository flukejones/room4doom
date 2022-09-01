use std::{error::Error, f32::consts::FRAC_PI_4};

use gameplay::glam::{Mat4, Vec3, Vec2};
use glow::{Context, HasContext, Shader};

use super::{Drawer, GL_QUAD, GL_QUAD_INDICES, set_uniform_f32, set_uniform_mat4, set_uniform_vec2};

pub struct Basic<'c> {
    ctx: &'c Context,
    _quad: [f32; 16],
    indices: [u32; 6],
    projection: Mat4,
    look_at: Mat4,
}

impl<'c> Basic<'c> {
    pub fn new(gl: &'c Context) -> Self {
        let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
        let look_at = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 2.42),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        unsafe {
            let vertex_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            gl.bind_vertex_array(Some(vertex_array));

            let program = gl.create_program().expect("Cannot create program");

            let shader_sources = [
                (glow::VERTEX_SHADER, VERT),
                (glow::FRAGMENT_SHADER, FRAG),
            ];
            let mut shaders = Vec::with_capacity(shader_sources.len());

            for (shader_type, shader_source) in shader_sources.iter() {
                let shader = gl
                    .create_shader(*shader_type)
                    .expect("Cannot create shader");
                gl.shader_source(shader, &shader_source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    panic!("{}", gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }

            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                panic!("{}", gl.get_program_info_log(program));
            }

            for shader in shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }

            gl.use_program(Some(program));

            set_uniform_mat4(&gl, program, "projMat", &projection.to_cols_array());
            set_uniform_mat4(&gl, program, "viewMat", &look_at.to_cols_array());
            set_uniform_mat4(&gl, program, "modelMat", &Mat4::IDENTITY.to_cols_array());
            set_uniform_vec2(gl, program, "position", &Vec2::default());
            set_uniform_vec2(gl, program, "vert_uv", &Vec2::default());

            // let mut vb = VertexBuffer::new(ctx).unwrap();
            // let mut eb = ElementBuffer::new(ctx).unwrap();
            // vb.set_data(&GL_QUAD);
            // eb.set_data(&GL_QUAD_INDICES);
        }

        Self {
            ctx: gl,
            _quad: GL_QUAD,
            indices: GL_QUAD_INDICES,
            projection,
            look_at,
        }
    }
}

impl<'c> Drawer for Basic<'c> {
    fn clear(&self) {
        unsafe {
            self.ctx.clear_color(0.0, 0.0, 0.0, 1.0);
            self.ctx.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    fn set_tex_filter(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32)) {}

    fn draw(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

const VERT: &str = r#"
#version 110
uniform mat4 projMat;
uniform mat4 viewMat;
uniform mat4 modelMat;
uniform vec2 position;
varying vec4 vert_uv;
varying vec4 frag_uv;

void main() {
    gl_Position = projMat * viewMat * modelMat * vec4(position, 0.0, 1.0);
    frag_uv = vert_uv;
}"#;

const FRAG: &str = r#"
#version 110
varying vec4 frag_uv;

void main() {
    vec4 colour = texture(image, frag_uv);
    gl_FragColor = colour;
}"#;
