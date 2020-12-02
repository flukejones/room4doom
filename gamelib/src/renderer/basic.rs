use std::f32::consts::FRAC_PI_4;

use glam::{Mat4, Vec3};
use golem::*;
use golem::{Dimension::*};

use super::Renderer;

pub(crate) struct Basic<'c> {
  ctx:        &'c Context,
  _quad:      [f32; 16],
  indices:    [u32; 6],
  shader:     ShaderProgram,
  projection: Mat4,
  look_at:    Mat4,
  texture:    Texture,
  vb:         VertexBuffer,
  eb:         ElementBuffer,
}

impl<'c> Basic<'c> {
  pub fn new(ctx: &'c Context) -> Self {
      let quad = [
          // position         vert_uv
          -1.0, -1.0, 0.0, 1.0, // bottom left
          1.0, -1.0, 1.0, 1.0, // bottom right
          1.0, 1.0, 1.0, 0.0, // top right
          -1.0, 1.0, 0.0, 0.0, // top left
      ];
      let indices = [0, 1, 2, 2, 3, 0];

      let shader = ShaderProgram::new(
        ctx,
        ShaderDescription {
            vertex_input:    &[
                Attribute::new("position", AttributeType::Vector(D2)),
                Attribute::new("vert_uv", AttributeType::Vector(D2)),
            ],
            fragment_input:  &[Attribute::new(
                "frag_uv",
                AttributeType::Vector(D2),
            )],
            uniforms:        &[
              // Standard view stuff
                Uniform::new("projMat", UniformType::Matrix(D4)),
                Uniform::new("viewMat", UniformType::Matrix(D4)),
                Uniform::new("modelMat", UniformType::Matrix(D4)),
              // The SDL bytes
                Uniform::new("image", UniformType::Sampler2D),
            ],
            vertex_shader:   r#" void main() {
                                gl_Position = projMat * viewMat * modelMat * vec4(position, 0.0, 1.0);
                                frag_uv = vert_uv;
                            }"#,
            fragment_shader: r#" void main() {
                                vec4 colour = texture(image, frag_uv);
                                gl_FragColor = colour;
                            }"#,
        },
    ).unwrap();

      let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
      let look_at = Mat4::look_at_rh(
          Vec3::new(0.0, 0.0, 2.5),
          Vec3::new(0.0, 0.0, 0.0),
          Vec3::new(0.0, 1.0, 0.0),
      );

      let mut vb = VertexBuffer::new(ctx).unwrap();
      let mut eb = ElementBuffer::new(ctx).unwrap();
      vb.set_data(&quad);
      eb.set_data(&indices);

      Self {
          ctx,
          _quad: quad,
          indices,
          shader,
          projection,
          look_at,
          texture: Texture::new(ctx).unwrap(),
          vb,
          eb,
      }
  }

  pub fn set_tex_filter(&mut self) -> Result<(), GolemError> {
      self.texture.set_minification(TextureFilter::Nearest)?;
      self.texture.set_magnification(TextureFilter::Linear)
  }
}

impl<'c> Renderer for Basic<'c> {
  fn draw(
      &mut self,
      input: &[u8],
      input_size: (u32, u32),
  ) -> Result<(), GolemError> {
      self.texture.set_image(
          Some(input),
          input_size.0,
          input_size.1,
          ColorFormat::RGBA,
      );

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

      let bind_point = std::num::NonZeroU32::new(1).unwrap();
      self.texture.set_active(bind_point);

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