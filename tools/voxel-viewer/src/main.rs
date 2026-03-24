#[cfg(feature = "hprof")]
use coarse_prof::profile;
use glam::{Mat4, Vec3};
use pic_data::voxel::kvx::VoxelModel;
use pic_data::voxel::slices::{self, VoxelSlices};
use pixels::{Pixels, SurfaceTexture};
use software3d::rasterizer::Rasterizer;
use software3d::voxel::collect::{VoxelCollectParams, collect_visible_slices};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const W: u32 = 640;
const H: u32 = 480;
const FOV_SCALE: f32 = H as f32 * 1.2;

struct Camera {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    focus: Vec3,
    auto_rotate: bool,
}

impl Camera {
    fn compute(&self, max_dim: f32) -> (Vec3, Vec3, Vec3, Vec3) {
        let dist = max_dim * 2.5 * self.zoom;
        // Camera orbits focus point
        let orbit = Vec3::new(
            self.yaw.cos() * self.pitch.cos() * dist,
            self.yaw.sin() * self.pitch.cos() * dist,
            self.pitch.sin() * dist,
        );
        let pos = self.focus + orbit;
        let look_dir = (self.focus - pos).normalize();
        let rt = Vec3::new(look_dir.y, -look_dir.x, 0.0).normalize();
        let up = rt.cross(look_dir);
        let pos = pos + rt * self.pan_x + up * self.pan_y;
        (pos, look_dir, rt, up)
    }

    fn view_proj(&self, _pos: Vec3, fwd: Vec3, _up: Vec3) -> Mat4 {
        // Eye at origin — collection pipeline handles camera translation
        let view = Mat4::look_to_rh(Vec3::ZERO, fwd, Vec3::Z);
        let vfov = 2.0 * (H as f32 * 0.5 / FOV_SCALE).atan();
        let proj = Mat4::perspective_rh(vfov, W as f32 / H as f32, 0.1, 1000.0);
        proj * view
    }
}

struct App {
    window: Option<Window>,
    pixels: Option<Pixels<'static>>,
    model: VoxelModel,
    slices: VoxelSlices,
    palette: [(u8, u8, u8); 256],
    palette_u32: [u32; 256],
    use_slices: bool,
    wireframe: u8,      // 0=off, 1=all quads, 2=collected only
    single_slice: bool, // view one slice at a time
    slice_index: usize, // current slice index on the selected axis
    vsync: bool,
    rasterizer: Rasterizer,
    camera: Camera,
    mouse_dragging: bool,
    last_mouse: (f64, f64),
    frames_since_print: u32,
    last_print: Instant,
    frame_time_min: f32,
    frame_time_max: f32,
    last_frame: Instant,
    #[cfg(feature = "hprof")]
    frame_count: u32,
}

impl App {
    fn render(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("render");
        let pixels = match self.pixels.as_mut() {
            Some(p) => p,
            None => return,
        };

        let frame = pixels.frame_mut();
        // Clear to dark grey
        for pixel in frame.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[30, 30, 35, 255]);
        }

        if self.camera.auto_rotate {
            self.camera.yaw += 0.02;
        }

        let hx = self.slices.xpivot;
        let hy = self.slices.ypivot;
        let hz = self.slices.zpivot;
        let max_dim = (self.model.xsiz as f32)
            .max(self.model.ysiz as f32)
            .max(self.model.zsiz as f32);
        let (cam_pos, fwd, rt, up) = self.camera.compute(max_dim);

        let mut rendered = 0u32;

        if !self.use_slices {
            #[cfg(feature = "hprof")]
            profile!("direct_render");
            let mut depth = vec![f32::MAX; (W * H) as usize];
            let cam = (cam_pos.x, cam_pos.y, cam_pos.z);
            let fwd_t = (fwd.x, fwd.y, fwd.z);
            let rt_t = (rt.x, rt.y, rt.z);
            let up_t = (up.x, up.y, up.z);

            // Splat stretch: project all 3 grid axis unit vectors onto the
            // camera's right and up vectors. Sum the absolute projections
            // to get the total screen coverage a unit cube edge needs.
            // Head-on (one axis aligned): sum ≈ 1.0. At 45°: sum ≈ 1.41.
            let axes: [Vec3; 3] = [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
            ];
            let mut sum_rt = 0.0f32;
            let mut sum_up = 0.0f32;
            for axis in &axes {
                sum_rt += axis.dot(rt).abs();
                sum_up += axis.dot(up).abs();
            }
            let splat_w_scale = sum_rt.max(1.0);
            let splat_h_scale = sum_up.max(1.0);

            for x in 0..self.model.xsiz {
                for y in 0..self.model.ysiz {
                    for z in 0..self.model.zsiz {
                        let color = self.model.get(x, y, z);
                        if color == 255 {
                            continue;
                        }
                        let wx = x as f32 - hx + 0.5;
                        let wy = y as f32 - hy + 0.5;
                        let wz = -(z as f32 - hz + 0.5);
                        if let Some((sx, sy, d)) = project(wx, wy, wz, cam, fwd_t, rt_t, up_t) {
                            let base = FOV_SCALE / d;
                            let sw = (base * splat_w_scale).ceil().max(1.0) as i32;
                            let sh = (base * splat_h_scale).ceil().max(1.0) as i32;
                            let (r, g, b) = self.palette[color as usize];
                            draw_rect(frame, &mut depth, sx, sy, d, sw, sh, r, g, b);
                            rendered += 1;
                        }
                    }
                }
            }
        } else {
            #[cfg(feature = "hprof")]
            profile!("slice_render");
            let view_proj = self.camera.view_proj(cam_pos, fwd, up);
            let half_w = W as f32 * 0.5;
            let half_h = H as f32 * 0.5;
            let identity_map: Vec<usize> = (0..256).collect();
            let colourmaps: Vec<&[usize]> = vec![&identity_map; 48];
            self.rasterizer.depth_buffer_mut().reset();
            let frame_u32: &mut [u32] = unsafe {
                std::slice::from_raw_parts_mut(frame.as_mut_ptr() as *mut u32, (W * H) as usize)
            };

            // Collect visible slices using the shared pipeline
            let params = VoxelCollectParams {
                base_pos: Vec3::ZERO,
                cos_a: 1.0,
                sin_a: 0.0,
                brightness: 15,
                player_pos: cam_pos,
                view_proj: &view_proj,
                screen_width: W,
                screen_height: H,
                is_shadow: false,
            };
            let mut voxel_slices = Vec::new();
            collect_visible_slices(
                &self.slices,
                &params,
                self.rasterizer.depth_buffer(),
                &mut voxel_slices,
            );

            // Single-slice mode: render only one quad on the dominant axis
            if self.single_slice {
                let abs_fwd = Vec3::new(fwd.x.abs(), fwd.y.abs(), fwd.z.abs());
                let dominant_axis = if abs_fwd.x >= abs_fwd.y && abs_fwd.x >= abs_fwd.z {
                    0usize
                } else if abs_fwd.y >= abs_fwd.z {
                    1
                } else {
                    2
                };
                let axis_quads = &self.slices.slices[dominant_axis];
                let axis_count = axis_quads.len();
                if axis_count > 0 {
                    self.slice_index = self.slice_index.min(axis_count - 1);
                    // Replace collected slices with just the one quad
                    voxel_slices.clear();
                    let quad = &axis_quads[self.slice_index];
                    // Render both sides of this quad
                    let hx = self.slices.xpivot;
                    let hy = self.slices.ypivot;
                    let hz = self.slices.zpivot;
                    for &(ref columns, d) in &[
                        (&quad.neg_columns, quad.depth),
                        (&quad.pos_columns, quad.depth + 1.0),
                    ] {
                        if columns.is_empty() {
                            continue;
                        }
                        let corner = |u: f32, v: f32| -> Vec3 {
                            match dominant_axis {
                                0 => Vec3::new(d - hx, u - hy, -(v - hz)),
                                1 => Vec3::new(u - hx, d - hy, -(v - hz)),
                                _ => Vec3::new(u - hx, v - hy, -(d - hz)),
                            }
                        };
                        let u0 = quad.min_u as f32;
                        let v0 = quad.min_v as f32;
                        let origin = corner(u0, v0);
                        let u_vec = corner(u0 + 1.0, v0) - origin;
                        let v_vec = corner(u0, v0 + 1.0) - origin;
                        use software3d::voxel::collect::VoxelSliceRef;
                        voxel_slices.push(VoxelSliceRef {
                            origin,
                            u_vec,
                            v_vec,
                            brightness: 15,
                            width: quad.width,
                            height: quad.height,
                            axis: dominant_axis as u8,
                            columns: &columns[..] as *const _,
                            depth: 0.0,
                            is_shadow: false,
                        });
                    }
                }
                let axis_name = ["X", "Y", "Z"][dominant_axis];
                if let Some(w) = &self.window {
                    w.set_title(&format!(
                        "Voxel Viewer [SLICE {} {}/{}]",
                        axis_name,
                        self.slice_index + 1,
                        axis_count
                    ));
                }
            }

            // Sort front-to-back for optimal depth rejection
            voxel_slices.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));

            for vq in &voxel_slices {
                let columns = unsafe { &*vq.columns };
                self.rasterizer.rasterize_voxel_texels(
                    vq.origin,
                    vq.u_vec,
                    vq.v_vec,
                    columns,
                    vq.width,
                    vq.height,
                    &view_proj,
                    cam_pos,
                    &colourmaps,
                    &self.palette_u32,
                    frame_u32,
                    W as usize,
                );
                rendered += 1;
            }

            // Wireframe overlay: w=1 all quads, w=2 collected (backface-culled) only
            if self.wireframe > 0 {
                if self.wireframe == 2 {
                    // Draw outlines of collected slices (same as rendered), coloured by axis
                    let axis_colors: [u32; 3] = [0xFF0000FF, 0xFF00FF00, 0xFFFF0000];
                    for vq in &voxel_slices {
                        let color = axis_colors[vq.axis as usize];
                        let w = vq.width as f32;
                        let h = vq.height as f32;
                        let corners_3d = [
                            vq.origin,
                            vq.origin + vq.u_vec * w,
                            vq.origin + vq.u_vec * w + vq.v_vec * h,
                            vq.origin + vq.v_vec * h,
                        ];
                        let proj: Vec<_> = corners_3d
                            .iter()
                            .map(|&p| {
                                let rel = p - cam_pos;
                                let c = view_proj * glam::Vec4::new(rel.x, rel.y, rel.z, 1.0);
                                if c.w > 0.0 {
                                    let iw = 1.0 / c.w;
                                    Some(((c.x + c.w) * half_w * iw, (c.w - c.y) * half_h * iw))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        for i in 0..4 {
                            if let (Some(a), Some(b)) = (proj[i], proj[(i + 1) % 4]) {
                                draw_line(frame_u32, W as i32, H as i32, a.0, a.1, b.0, b.1, color);
                            }
                        }
                    }
                } else {
                    // Draw all non-empty quads, coloured per axis
                    let axis_colors: [u32; 3] = [0xFF0000FF, 0xFF00FF00, 0xFFFF0000];
                    for axis in 0..3 {
                        let color = axis_colors[axis];
                        for quad in &self.slices.slices[axis] {
                            for &d in &[quad.depth, quad.depth + 1.0] {
                                let corner = |u: f32, v: f32| -> Vec3 {
                                    match axis {
                                        0 => Vec3::new(d - hx, u - hy, -(v - hz)),
                                        1 => Vec3::new(u - hx, d - hy, -(v - hz)),
                                        _ => Vec3::new(u - hx, v - hy, -(d - hz)),
                                    }
                                };
                                let u0 = quad.min_u as f32;
                                let v0 = quad.min_v as f32;
                                let u1 = u0 + quad.width as f32;
                                let v1 = v0 + quad.height as f32;
                                let corners_3d = [
                                    corner(u0, v0),
                                    corner(u1, v0),
                                    corner(u1, v1),
                                    corner(u0, v1),
                                ];
                                let proj: Vec<_> = corners_3d
                                    .iter()
                                    .map(|&p| {
                                        let rel = p - cam_pos;
                                        let c =
                                            view_proj * glam::Vec4::new(rel.x, rel.y, rel.z, 1.0);
                                        if c.w > 0.0 {
                                            let iw = 1.0 / c.w;
                                            Some((
                                                (c.x + c.w) * half_w * iw,
                                                (c.w - c.y) * half_h * iw,
                                            ))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                for i in 0..4 {
                                    if let (Some(a), Some(b)) = (proj[i], proj[(i + 1) % 4]) {
                                        draw_line(
                                            frame_u32, W as i32, H as i32, a.0, a.1, b.0, b.1,
                                            color,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        pixels.render().ok();

        let frame_ms = self.last_frame.elapsed().as_secs_f32() * 1000.0;
        self.last_frame = Instant::now();
        self.frame_time_min = self.frame_time_min.min(frame_ms);
        self.frame_time_max = self.frame_time_max.max(frame_ms);
        self.frames_since_print += 1;
        let elapsed = self.last_print.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            let fps = self.frames_since_print as f32 / elapsed;
            let avg_ms = elapsed * 1000.0 / self.frames_since_print as f32;
            let mode = if self.use_slices { "slices" } else { "splat" };
            eprintln!(
                "{mode}: {rendered} rendered | fps: {fps:.0} | {avg_ms:.2}ms avg, {:.2}ms min, {:.2}ms max",
                self.frame_time_min, self.frame_time_max,
            );
            self.frames_since_print = 0;
            self.frame_time_min = f32::MAX;
            self.frame_time_max = 0.0;
            self.last_print = Instant::now();
        }
    }
}

fn draw_line(buf: &mut [u32], w: i32, h: i32, x0: f32, y0: f32, x1: f32, y1: f32, color: u32) {
    let (mut x, mut y) = (x0 as i32, y0 as i32);
    let (ex, ey) = (x1 as i32, y1 as i32);
    let dx = (ex - x).abs();
    let dy = -(ey - y).abs();
    let sx = if x < ex { 1 } else { -1 };
    let sy = if y < ey { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x >= 0 && x < w && y >= 0 && y < h {
            buf[(y * w + x) as usize] = color;
        }
        if x == ex && y == ey {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn draw_rect(
    frame: &mut [u8],
    depth: &mut [f32],
    sx: f32,
    sy: f32,
    z: f32,
    w: i32,
    h: i32,
    r: u8,
    g: u8,
    b: u8,
) {
    let (x0, y0) = (sx as i32, sy as i32);
    for py in y0..y0 + h {
        if py < 0 || py >= H as i32 {
            continue;
        }
        for px in x0..x0 + w {
            if px < 0 || px >= W as i32 {
                continue;
            }
            let idx = py as usize * W as usize + px as usize;
            if z < depth[idx] {
                depth[idx] = z;
                let p = idx * 4;
                frame[p] = r;
                frame[p + 1] = g;
                frame[p + 2] = b;
            }
        }
    }
}

fn project(
    wx: f32,
    wy: f32,
    wz: f32,
    cam: (f32, f32, f32),
    fwd: (f32, f32, f32),
    rt: (f32, f32, f32),
    up: (f32, f32, f32),
) -> Option<(f32, f32, f32)> {
    let (dx, dy, dz) = (wx - cam.0, wy - cam.1, wz - cam.2);
    let z = dx * fwd.0 + dy * fwd.1 + dz * fwd.2;
    if z < 0.1 {
        return None;
    }
    let proj = FOV_SCALE / z;
    Some((
        W as f32 * 0.5 + (dx * rt.0 + dy * rt.1 + dz * rt.2) * proj,
        H as f32 * 0.5 - (dx * up.0 + dy * up.1 + dz * up.2) * proj,
        z,
    ))
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title(if self.use_slices {
                "Voxel Viewer [SLICES]"
            } else {
                "Voxel Viewer [DIRECT]"
            })
            .with_inner_size(LogicalSize::new(W, H))
            .with_resizable(true);
        let window = event_loop.create_window(attrs).unwrap();
        let phys = window.inner_size();
        let surface = SurfaceTexture::new(phys.width, phys.height, &window);
        let px = if self.vsync {
            Pixels::new(W, H, surface).unwrap()
        } else {
            pixels::PixelsBuilder::new(W, H, surface)
                .present_mode(pixels::wgpu::PresentMode::AutoNoVsync)
                .build()
                .unwrap()
        };
        self.pixels = Some(unsafe { std::mem::transmute(px) });
        self.window = Some(window);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                self.render();
                #[cfg(feature = "hprof")]
                {
                    self.frame_count += 1;
                    if self.frame_count % 120 == 0 {
                        coarse_prof::write(&mut std::io::stdout()).unwrap();
                    }
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(pixels) = self.pixels.as_mut() {
                    pixels.resize_surface(new_size.width, new_size.height).ok();
                }
            }
            WindowEvent::MouseInput {
                state,
                button,
                ..
            } => {
                if button == winit::event::MouseButton::Left {
                    self.mouse_dragging = state == winit::event::ElementState::Pressed;
                    self.camera.auto_rotate = false;
                }
            }
            WindowEvent::CursorMoved {
                position,
                ..
            } => {
                let (mx, my) = (position.x, position.y);
                if self.mouse_dragging {
                    let dx = (mx - self.last_mouse.0) as f32;
                    let dy = (my - self.last_mouse.1) as f32;
                    self.camera.yaw -= dx * 0.01;
                    self.camera.pitch = (self.camera.pitch + dy * 0.01).clamp(-1.5, 1.5);
                }
                self.last_mouse = (mx, my);
            }
            WindowEvent::MouseWheel {
                delta,
                ..
            } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.01,
                };
                if self.single_slice {
                    // Scroll through slices
                    if scroll > 0.0 && self.slice_index > 0 {
                        self.slice_index -= 1;
                    } else if scroll < 0.0 {
                        self.slice_index += 1;
                    }
                } else {
                    self.camera.zoom = (self.camera.zoom - scroll * 0.1).max(0.1);
                }
            }
            WindowEvent::KeyboardInput {
                event,
                ..
            } => {
                if event.state == winit::event::ElementState::Pressed {
                    use winit::keyboard::{Key, NamedKey};
                    match event.logical_key {
                        Key::Named(NamedKey::ArrowLeft) => self.camera.pan_x -= 1.0,
                        Key::Named(NamedKey::ArrowRight) => self.camera.pan_x += 1.0,
                        Key::Named(NamedKey::ArrowUp) => self.camera.pan_y += 1.0,
                        Key::Named(NamedKey::ArrowDown) => self.camera.pan_y -= 1.0,
                        Key::Character(ref c) => match c.as_str() {
                            "=" | "+" => self.camera.zoom = (self.camera.zoom - 0.1).max(0.1),
                            "-" => self.camera.zoom += 0.1,
                            "r" => self.camera.auto_rotate = !self.camera.auto_rotate,
                            "s" => self.use_slices = !self.use_slices,
                            "w" => self.wireframe = (self.wireframe + 1) % 3,
                            "d" => {
                                self.single_slice = !self.single_slice;
                                self.slice_index = 0;
                            }
                            _ => {}
                        },
                        Key::Named(NamedKey::Escape) => event_loop.exit(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: voxel-viewer <kvx-file> [--slices] [--no-vsync] [palette.lmp]");
        std::process::exit(1);
    }

    let use_slices = args.iter().any(|a| a == "--slices");
    let vsync = !args.iter().any(|a| a == "--no-vsync");
    let kvx_data = std::fs::read(&args[1]).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", args[1], e);
        std::process::exit(1);
    });
    let mut model = VoxelModel::load(&kvx_data).unwrap_or_else(|e| {
        eprintln!("Failed to parse KVX: {}", e);
        std::process::exit(1);
    });

    // Resolve palette: external Doom palette remaps grid indices,
    // KVX embedded palette scaled 6-bit→8-bit, else procedural fallback.
    let ext_pal = args
        .iter()
        .find(|a| a.ends_with(".lmp") || a.ends_with(".pal"))
        .and_then(|p| std::fs::read(p).ok());

    let mut palette = [(128u8, 128u8, 128u8); 256];
    if let Some(ref data) = ext_pal {
        if data.len() >= 768 {
            model.remap_to_doom_palette(data);
            for i in 0..256 {
                palette[i] = (data[i * 3], data[i * 3 + 1], data[i * 3 + 2]);
            }
            eprintln!("Using external Doom palette (remapped KVX indices)");
        }
    } else if let Some(ref kvx_pal) = model.palette {
        for i in 0..256.min(kvx_pal.len() / 3) {
            let (r, g, b) = (kvx_pal[i * 3], kvx_pal[i * 3 + 1], kvx_pal[i * 3 + 2]);
            palette[i] = (
                (r << 2) | (r >> 4),
                (g << 2) | (g >> 4),
                (b << 2) | (b >> 4),
            );
        }
        eprintln!("Using KVX embedded palette (6-bit scaled to 8-bit)");
    } else {
        for i in 0..256 {
            palette[i] = (
                ((i * 37 + 7) % 200 + 55) as u8,
                ((i * 53 + 13) % 200 + 55) as u8,
                ((i * 97 + 29) % 200 + 55) as u8,
            );
        }
        palette[0] = (0, 0, 0);
        eprintln!("No palette found, using procedural fallback");
    }

    let occupied = model.grid.iter().filter(|&&v| v != 255).count();
    eprintln!(
        "Loaded {}x{}x{}, {} occupied voxels, mode={}",
        model.xsiz,
        model.ysiz,
        model.zsiz,
        occupied,
        if use_slices { "slices" } else { "direct" }
    );

    let slices = slices::generate(&model);
    for (i, axis) in slices.slices.iter().enumerate() {
        let count = |cols: &[_]| -> usize {
            use pic_data::voxel::slices::VoxelColumn;
            let cols: &[VoxelColumn] = cols;
            cols.iter()
                .flat_map(|c| c.spans.iter())
                .map(|s| s.pixels.len())
                .sum()
        };
        let neg: usize = axis.iter().map(|q| count(&q.neg_columns)).sum();
        let pos: usize = axis.iter().map(|q| count(&q.pos_columns)).sum();
        eprintln!(
            "  {}: {} quads, neg={} px, pos={} px",
            ["X", "Y", "Z"][i],
            axis.len(),
            neg,
            pos
        );
    }

    // Build u32 palette (0xAABBGGRR little-endian = RGBA byte order)
    let mut palette_u32 = [0u32; 256];
    for i in 0..256 {
        let (r, g, b) = palette[i];
        palette_u32[i] = 0xFF000000 | (b as u32) << 16 | (g as u32) << 8 | r as u32;
    }

    // Focus midway between pivot origin and AABB center
    let hx = slices.xpivot;
    let hy = slices.ypivot;
    let hz = slices.zpivot;
    let focus = Vec3::new(
        model.xsiz as f32 * 0.5 - hx,
        model.ysiz as f32 * 0.5 - hy,
        hz - model.zsiz as f32 * 0.5,
    );

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App {
        window: None,
        pixels: None,
        model,
        slices,
        palette,
        palette_u32,
        use_slices,
        wireframe: 0,
        single_slice: false,
        slice_index: 0,
        vsync,
        rasterizer: Rasterizer::new(W, H),
        camera: Camera {
            yaw: 0.5,
            pitch: 0.4,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            focus,
            auto_rotate: true,
        },
        mouse_dragging: false,
        last_mouse: (0.0, 0.0),
        frames_since_print: 0,
        last_print: Instant::now(),
        frame_time_min: f32::MAX,
        frame_time_max: 0.0,
        last_frame: Instant::now(),
        #[cfg(feature = "hprof")]
        frame_count: 0,
    };
    event_loop.run_app(&mut app).unwrap();
}
