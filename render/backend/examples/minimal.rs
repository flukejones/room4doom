//! Minimal [`RenderStack`] presentation lifecycle on the active winit backend.
//!
//! `cargo run -p render-backend --example minimal --features display-softbuffer,software3d`
//! (or `--features display-wgpu,software3d` for the GPU-upload path).
//!
//! Builds the window-backed stack, draws a moving gradient into the shared frame
//! via [`RenderStack::ui_frame`] each frame, and presents — the presentation
//! lifecycle minus the scene render (which needs a WAD). Exits after [`FRAMES`].

use std::sync::Arc;

use render_backend::{ActiveBackend, RenderStack, RenderType};
use render_common::DrawBuffer as _;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const FRAMES: u32 = 120;

#[cfg(not(any(feature = "display-softbuffer", feature = "display-wgpu")))]
compile_error!("the minimal example needs a winit backend: display-softbuffer or display-wgpu");

/// Build the active winit backend (softbuffer when wgpu is off, else wgpu).
fn make_backend(window: Arc<Window>) -> ActiveBackend<u32> {
    #[cfg(all(feature = "display-softbuffer", not(feature = "display-wgpu")))]
    {
        render_backend::new_softbuffer::<u32>(window)
    }
    #[cfg(feature = "display-wgpu")]
    {
        render_backend::new_wgpu::<u32>(window, true, Vec::new())
    }
}

struct App {
    stack: Option<RenderStack<u32>>,
    window: Option<Arc<Window>>,
    frame: u32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("render-backend minimal");
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );
        let backend = make_backend(window.clone());
        self.stack = Some(RenderStack::new(false, backend, RenderType::default()));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let Some(stack) = self.stack.as_mut() else {
                    return;
                };
                let buf = stack.ui_frame();
                let size = *buf.size();
                let (w, h) = (size.width_usize(), size.height_usize());
                let t = self.frame;
                for y in 0..h {
                    for x in 0..w {
                        let r = (x + t as usize) as u8;
                        let g = (y + t as usize) as u8;
                        buf.set_pixel(
                            x,
                            y,
                            0xFF00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8),
                        );
                    }
                }
                stack.present(false);

                self.frame += 1;
                if self.frame >= FRAMES {
                    event_loop.exit();
                } else if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to build event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        stack: None,
        window: None,
        frame: 0,
    };
    event_loop.run_app(&mut app).expect("event loop error");
}
