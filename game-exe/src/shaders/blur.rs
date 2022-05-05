use std::f32::consts::FRAC_PI_4;

use glam::{Mat4, Vec3};
use golem::{Dimension::*, *};

use super::{Drawer, GL_QUAD, GL_QUAD_INDICES};

pub struct Basic<'c> {
    ctx: &'c Context,
    _quad: [f32; 16],
    indices: [u32; 6],
    shader: ShaderProgram,
    projection: Mat4,
    look_at: Mat4,
    texture: Texture,
    vb: VertexBuffer,
    eb: ElementBuffer,
}

impl<'c> Basic<'c> {
    pub fn new(ctx: &'c Context) -> Self {
        let shader = ShaderProgram::new(
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

        let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
        let look_at = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 2.42),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );

        let mut vb = VertexBuffer::new(ctx).unwrap();
        let mut eb = ElementBuffer::new(ctx).unwrap();
        vb.set_data(&GL_QUAD);
        eb.set_data(&GL_QUAD_INDICES);

        Self {
            ctx,
            _quad: GL_QUAD,
            indices: GL_QUAD_INDICES,
            shader,
            projection,
            look_at,
            texture: Texture::new(ctx).unwrap(),
            vb,
            eb,
        }
    }
}

impl<'c> Drawer for Basic<'c> {
    fn clear(&self) {
        self.ctx.set_clear_color(0.0, 0.0, 0.0, 1.0);
        self.ctx.clear();
    }

    fn set_tex_filter(&self) -> Result<(), GolemError> {
        self.texture.set_minification(TextureFilter::Nearest)?;
        self.texture.set_magnification(TextureFilter::Nearest)
    }

    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32)) {
        self.texture
            .set_image(Some(input), input_size.0, input_size.1, ColorFormat::RGBA);
    }

    fn draw(&mut self) -> Result<(), GolemError> {
        let bind_point = std::num::NonZeroU32::new(1).unwrap();
        self.texture.set_active(bind_point);

        self.shader.bind();

        self.shader.set_uniform("image", UniformValue::Int(1))?;

        self.shader.set_uniform(
            "projMat",
            UniformValue::Matrix4(self.projection.to_cols_array()),
        )?;
        self.shader.set_uniform(
            "viewMat",
            UniformValue::Matrix4(self.look_at.to_cols_array()),
        )?;
        self.shader.set_uniform(
            "modelMat",
            UniformValue::Matrix4(Mat4::identity().to_cols_array()),
        )?;

        self.ctx.clear();
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
