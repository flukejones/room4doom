use gameplay::log::info;
use gamestate_traits::{
    sdl2::{
        pixels,
        rect::Rect,
        render::{Canvas, TextureCreator},
        surface,
        video::{Window, WindowContext},
    },
    RenderTarget,
};
use render_traits::{PixelBuffer, RenderType};

use crate::shaders::{
    self, basic::Basic, cgwg_crt::Cgwgcrt, lottes_crt::LottesCRT, Drawer, Shaders,
};

struct Shader {
    window: Window,
    shader: Box<dyn Drawer>,
}

struct Software {
    canvas: Canvas<Window>,
    tex_creator: TextureCreator<WindowContext>,
}

pub struct Blitter<'c> {
    _gl_ctx: &'c golem::Context,
    crop_rect: Rect,
    shader: Option<Shader>,
    soft: Option<Software>,
}

impl<'c> Blitter<'c> {
    pub fn new(shader_option: Option<Shaders>, gl_ctx: &'c golem::Context, window: Window) -> Self {
        // TODO: sort this block of stuff out
        let wsize = window.drawable_size();
        let ratio = wsize.1 as f32 * 1.333;
        let xp = (wsize.0 as f32 - ratio) / 2.0;
        let crop_rect = Rect::new(xp as i32, 0, ratio as u32, wsize.1);

        gl_ctx.set_viewport(
            crop_rect.x as u32,
            crop_rect.y as u32,
            crop_rect.width(),
            crop_rect.height(),
        );

        let mut soft = None;
        let mut shader = None;
        let post_process: Option<Box<dyn Drawer>> = if let Some(shader) = shader_option {
            match shader {
                Shaders::None => None,
                Shaders::Basic => Some(Box::new(Basic::new(gl_ctx))),
                Shaders::Lottes => Some(Box::new(LottesCRT::new(gl_ctx))),
                Shaders::LottesBasic => {
                    Some(Box::new(shaders::lottes_reduced::LottesCRT::new(gl_ctx)))
                }
                Shaders::Cgwg => Some(Box::new(Cgwgcrt::new(
                    gl_ctx,
                    crop_rect.width(),
                    crop_rect.height(),
                ))),
            }
        } else {
            None
        };

        if let Some(post_process) = post_process {
            info!("Using a post-process shader");
            // post_process.set_tex_filter().unwrap();
            shader = Some(Shader {
                window,
                shader: post_process,
            });
        } else {
            info!("No shader selectd, using pure software");
            let canvas = window.into_canvas().accelerated().build().unwrap();
            let tex_creator = canvas.texture_creator();
            soft = Some(Software {
                canvas,
                tex_creator,
            })
        }

        Self {
            _gl_ctx: gl_ctx,
            crop_rect,
            shader,
            soft,
        }
    }

    pub fn blit(&mut self, render_buffer: &mut RenderTarget) {
        if let Some(shader) = &mut self.shader {
            if matches!(render_buffer.render_type(), RenderType::SoftOpenGL) {
                let render_buffer = unsafe { render_buffer.soft_opengl_unchecked() };
                // shader.shader.clear();
                render_buffer.copy_softbuf_to_gl_texture();
                shader.shader.draw(render_buffer.gl_texture()).unwrap();
                shader.window.gl_swap_window();
            }
        } else if let Some(soft) = &mut self.soft {
            let w = render_buffer.width() as u32;
            let h = render_buffer.height() as u32;
            if matches!(render_buffer.render_type(), RenderType::Software) {
                let render_buffer = unsafe { render_buffer.software_unchecked() };
                let surf = surface::Surface::from_data(
                    render_buffer.read_softbuf_pixels(),
                    w,
                    h,
                    4 * w,
                    pixels::PixelFormatEnum::RGBA32,
                )
                .unwrap()
                .as_texture(&soft.tex_creator)
                .unwrap();
                soft.canvas.copy(&surf, None, Some(self.crop_rect)).unwrap();
                soft.canvas.present();
            }
        }
    }
}
