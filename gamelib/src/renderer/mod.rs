use golem::GolemError;

pub(crate) mod cgwg_crt;
pub(crate) mod lottes_crt;
pub(crate) mod basic;

pub(crate) trait Renderer {
  fn draw(
      &mut self,
      input: &[u8],
      input_size: (u32, u32),
  ) -> Result<(), GolemError>;
}