use std::f32::consts::FRAC_PI_4;

use gameplay::glam::{Mat4, Vec3};
use golem::{Dimension::*, *};

use super::{ShaderDraw, GL_QUAD, GL_QUAD_INDICES};

pub struct Basic {
    _quad: [f32; 16],
    indices: [u32; 6],
    shader: ShaderProgram,
    _projection: Mat4,
    _look_at: Mat4,
    vb: VertexBuffer,
    eb: ElementBuffer,
}

impl Basic {
    pub fn new(ctx: &Context) -> Self {
        let mut shader = ShaderProgram::new(
            ctx,
            ShaderDescription {
                vertex_input: &[
                    Attribute::new("position", AttributeType::Vector(D2)),
                    Attribute::new("vert_uv", AttributeType::Vector(D2)),
                ],
                fragment_input: &[Attribute::new("frag_uv", AttributeType::Vector(D2))],
                uniforms: &[
                    // Standard view stuff
                    Uniform::new("projMat", UniformType::Matrix(D4)),
                    Uniform::new("viewMat", UniformType::Matrix(D4)),
                    Uniform::new("modelMat", UniformType::Matrix(D4)),
                    // The SDL bytes
                    Uniform::new("image", UniformType::Sampler2D),
                ],
                vertex_shader: VERT,
                fragment_shader: FRAG,
            },
        )
        .unwrap();
        shader.bind();
        shader.set_uniform("image", UniformValue::Int(1)).unwrap();

        let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
        shader
            .set_uniform("projMat", UniformValue::Matrix4(projection.to_cols_array()))
            .unwrap();

        let look_at = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 2.42),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );

        shader
            .set_uniform("viewMat", UniformValue::Matrix4(look_at.to_cols_array()))
            .unwrap();

        shader
            .set_uniform(
                "modelMat",
                UniformValue::Matrix4(Mat4::IDENTITY.to_cols_array()),
            )
            .unwrap();

        let mut vb = VertexBuffer::new(ctx).unwrap();
        let mut eb = ElementBuffer::new(ctx).unwrap();
        vb.set_data(&GL_QUAD);
        eb.set_data(&GL_QUAD_INDICES);

        Self {
            _quad: GL_QUAD,
            indices: GL_QUAD_INDICES,
            shader,
            _projection: projection,
            _look_at: look_at,
            vb,
            eb,
        }
    }
}

impl ShaderDraw for Basic {
    fn draw(&mut self, texture: &Texture) -> Result<(), GolemError> {
        let bind_point = std::num::NonZeroU32::new(1).unwrap();
        texture.set_active(bind_point);
        // self.ctx.clear();
        unsafe {
            self.shader.draw(
                &self.vb,
                &self.eb,
                0..self.indices.len(),
                GeometryMode::Triangles,
            )?;
        }
        Ok(())
    }
}

const VERT: &str = r#"
void main() {
    gl_Position = projMat * viewMat * modelMat * vec4(position, 0.0, 1.0);
    frag_uv = vert_uv;
}"#;

const FRAG: &str = r#"
void main() {
    vec4 colour = texture(image, frag_uv);
    gl_FragColor = colour;
}"#;
